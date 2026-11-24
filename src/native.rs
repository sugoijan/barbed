use anyhow::{Context, Result, anyhow, bail};
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use time::OffsetDateTime;
use tokio::time::sleep;
use tracing::{debug, info};

use crate::TwitchIdentity;
use crate::helix::{self, TwitchTokenExchange};
use crate::http::{HttpMethod, HttpResponse, PreparedRequest, RawResponse, ResponseHeader};
use crate::oauth::{
    DeviceCodeResponse, OpenIdConfiguration, OpenIdUserInfo, TokenValidation, TwitchAuthConfig,
    TwitchAuthOutcome, TwitchDeviceAuthorization, TwitchTokenState, client_credentials_request,
    device_code_request_with_scope, device_token_request, normalize_scopes,
    openid_configuration_request, openid_userinfo_request, refresh_token_request,
    revoke_token_request, validate_token_request,
};

#[derive(Clone)]
pub struct TwitchAuthClient {
    config: TwitchAuthConfig,
    http: Client,
}

impl TwitchAuthClient {
    pub fn new(config: TwitchAuthConfig) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("failed to construct reqwest client for Twitch auth")?;

        Ok(Self { config, http })
    }

    pub fn config(&self) -> &TwitchAuthConfig {
        &self.config
    }

    pub async fn start_device_code_flow(
        &self,
        scopes: &[String],
    ) -> Result<TwitchDeviceAuthorization> {
        let scope_string = normalize_scopes(scopes, &self.config.default_scopes);
        let payload: DeviceCodeResponse = send_json(
            &self.http,
            device_code_request_with_scope(&self.config.client_id, &scope_string),
            "Twitch device code",
        )
        .await?;

        TwitchDeviceAuthorization::from_response(payload, OffsetDateTime::now_utc())
            .map_err(|err| anyhow!("failed to construct Twitch device authorization: {err}"))
    }

    pub async fn complete_device_code_flow(
        &self,
        authorization: &TwitchDeviceAuthorization,
    ) -> Result<TwitchAuthOutcome> {
        let mut delay = authorization.interval;

        loop {
            if OffsetDateTime::now_utc() >= authorization.expires_at {
                bail!("device authorization expired before Twitch approval");
            }

            let response = send_prepared_request(
                &self.http,
                device_token_request(&self.config.client_id, &authorization.device_code),
            )
            .await?;

            if response.status == 200 {
                let exchange: TwitchTokenExchange = helix::parse_typed_response(response, 200)
                    .context("failed to parse Twitch token exchange response")?;
                let identity = fetch_authenticated_user(
                    &self.http,
                    &exchange.access_token,
                    &self.config.client_id,
                )
                .await?;
                return helix::build_auth_outcome(identity, exchange, now_ms())
                    .map_err(anyhow::Error::from);
            }

            let error =
                serde_json::from_str::<DeviceCodeErrorResponse>(&response.body).unwrap_or_default();
            let code = error.error.as_deref().or(error.message.as_deref());
            match code {
                Some("authorization_pending") => {
                    info!(
                        delay_secs = delay.as_secs(),
                        "waiting for Twitch authorization"
                    );
                    sleep(delay).await;
                }
                Some("slow_down") => {
                    debug!("received slow_down from Twitch; increasing poll interval");
                    delay += std::time::Duration::from_secs(5);
                    sleep(delay).await;
                }
                Some("access_denied") => bail!("Twitch authorization was denied by the user"),
                Some("expired_token") => {
                    bail!("device authorization expired before Twitch approval")
                }
                Some(other) => {
                    let description = error.description();
                    bail!("twitch token request failed: {other}{description}");
                }
                None => {
                    bail!(
                        "twitch token request failed (status {}): {}",
                        response.status,
                        error.description_or_body(&response.body)
                    );
                }
            }
        }
    }

    pub async fn request_app_access_token(
        &self,
        client_secret: &str,
    ) -> Result<TwitchTokenExchange> {
        send_json(
            &self.http,
            client_credentials_request(&self.config.client_id, client_secret),
            "Twitch app token",
        )
        .await
    }

    pub async fn revoke_token(&self, token: &str) -> Result<()> {
        let response = send_prepared_request(
            &self.http,
            revoke_token_request(&self.config.client_id, token),
        )
        .await?;
        if !(200..=299).contains(&response.status) {
            bail!(
                "twitch token revoke request failed with status {}: {}",
                response.status,
                response.body
            );
        }
        Ok(())
    }

    pub async fn fetch_openid_configuration(&self) -> Result<OpenIdConfiguration> {
        send_json(
            &self.http,
            openid_configuration_request(),
            "Twitch OpenID configuration",
        )
        .await
    }

    pub async fn fetch_openid_userinfo(&self, access_token: &str) -> Result<OpenIdUserInfo> {
        send_json(
            &self.http,
            openid_userinfo_request(access_token),
            "Twitch OpenID userinfo",
        )
        .await
    }
}

/// Sends a request, requires a 200 response, and decodes the JSON body.
async fn send_json<T: serde::de::DeserializeOwned>(
    http: &Client,
    prepared: PreparedRequest,
    what: &str,
) -> Result<T> {
    let response = send_prepared_request(http, prepared).await?;
    helix::parse_typed_response(response, 200).with_context(|| format!("{what} request failed"))
}

/// Like [`send_json`], but statuses in `absent_statuses` yield `Ok(None)`.
async fn send_json_or_none<T: serde::de::DeserializeOwned>(
    http: &Client,
    prepared: PreparedRequest,
    absent_statuses: &[u16],
    what: &str,
) -> Result<Option<T>> {
    let response = send_prepared_request(http, prepared).await?;
    if absent_statuses.contains(&response.status) {
        return Ok(None);
    }
    helix::parse_typed_response(response, 200)
        .map(Some)
        .with_context(|| format!("{what} request failed"))
}

async fn dispatch(http: &Client, prepared: PreparedRequest) -> Result<reqwest::Response> {
    let PreparedRequest {
        url,
        method,
        headers,
        body,
    } = prepared;

    let mut request = match method {
        HttpMethod::Get => http.get(&url),
        HttpMethod::Post => http.post(&url),
        HttpMethod::Put => http.put(&url),
        HttpMethod::Patch => http.patch(&url),
        HttpMethod::Delete => http.delete(&url),
    };
    for (key, value) in headers {
        request = request.header(key, value);
    }
    if let Some(body) = body {
        request = request.body(body);
    }
    request
        .send()
        .await
        .with_context(|| format!("failed to send request to {url}"))
}

pub async fn send_prepared_request(
    http: &Client,
    prepared: PreparedRequest,
) -> Result<RawResponse> {
    Ok(send_prepared_request_with_metadata(http, prepared)
        .await?
        .into_raw_response())
}

pub async fn send_prepared_request_with_metadata(
    http: &Client,
    prepared: PreparedRequest,
) -> Result<HttpResponse> {
    let response = dispatch(http, prepared).await?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| ResponseHeader {
            name: name.as_str().to_string(),
            value: value.to_str().unwrap_or_default().to_string(),
        })
        .collect();
    let body = response.text().await.unwrap_or_default();
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

/// Validates a token; `Ok(None)` means Twitch rejected it.
pub async fn validate_access_token(
    http: &Client,
    access_token: &str,
) -> Result<Option<TokenValidation>> {
    send_json_or_none(
        http,
        validate_token_request(access_token),
        &[401],
        "Twitch token validation",
    )
    .await
}

/// Refreshes a token; `Ok(None)` means Twitch rejected the refresh token.
pub async fn refresh_access_token(
    http: &Client,
    client_id: &str,
    refresh_token: &str,
) -> Result<Option<TwitchTokenState>> {
    let exchange: Option<TwitchTokenExchange> = send_json_or_none(
        http,
        refresh_token_request(client_id, refresh_token),
        &[400, 401],
        "Twitch refresh token",
    )
    .await?;
    exchange
        .map(|exchange| exchange.into_token_state(Some(refresh_token), now_ms()))
        .transpose()
        .map_err(anyhow::Error::from)
}

pub async fn fetch_authenticated_user(
    http: &Client,
    access_token: &str,
    client_id: &str,
) -> Result<TwitchIdentity> {
    let raw =
        send_prepared_request(http, helix::user_lookup_request(access_token, client_id)?).await?;
    helix::parse_user_lookup(raw).map_err(anyhow::Error::from)
}

pub async fn fetch_user_by_login(
    http: &Client,
    access_token: &str,
    client_id: &str,
    login: &str,
) -> Result<TwitchIdentity> {
    let raw = send_prepared_request(
        http,
        helix::user_lookup_by_login_request(access_token, client_id, login)?,
    )
    .await?;
    helix::parse_user_lookup(raw).map_err(anyhow::Error::from)
}

#[derive(Clone)]
pub struct ChatSubscriptionConfig {
    pub client_id: String,
    pub access_token: String,
    pub broadcaster_id: String,
    pub user_id: String,
}

#[derive(Debug, Error)]
#[error("Twitch rejected the provided EventSub credentials")]
pub struct EventSubAuthError;

/// The EventSub WebSocket chat stream.
#[cfg(feature = "tokio-eventsub")]
#[path = "native_eventsub.rs"]
mod eventsub_stream;
#[cfg(feature = "tokio-eventsub")]
pub use eventsub_stream::{EventSubChatStream, connect_chat_stream};

fn now_ms() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp() * 1_000
}

#[derive(Debug, Default, Deserialize)]
struct DeviceCodeErrorResponse {
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

impl DeviceCodeErrorResponse {
    fn description(&self) -> String {
        if let Some(desc) = &self.error_description {
            format!(" ({desc})")
        } else if let Some(message) = &self.message {
            format!(" ({message})")
        } else {
            String::new()
        }
    }

    fn description_or_body(&self, body: &str) -> String {
        let description = self.description();
        if description.is_empty() {
            body.to_string()
        } else {
            description
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_request_send_supports_form_posts() {
        let request = crate::oauth::device_token_request("client", "device");
        assert_eq!(request.method, HttpMethod::Post);
        assert!(
            request
                .body
                .expect("body should exist")
                .contains("device_code=device")
        );
    }
}
