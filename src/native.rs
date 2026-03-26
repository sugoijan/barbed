use anyhow::{Context, Result, anyhow, bail};
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use time::OffsetDateTime;
use tokio::time::sleep;
use tracing::{debug, info, trace};

use crate::TwitchIdentity;
use crate::eventsub::{
    EventSubMessageType, EventSubStreamEvent, EventSubWebSocketSession,
    channel_chat_message_delete_subscription_request, channel_chat_message_subscription_request,
    decode_eventsub_websocket_message,
};
use crate::helix::{self, TwitchTokenExchange};
use crate::http::{HttpMethod, PreparedRequest, RawResponse};
use crate::oauth::{
    TokenValidation, TwitchAuthConfig, TwitchAuthOutcome, TwitchDeviceAuthorization,
    TwitchTokenState, device_code_request_with_scope, device_token_request, normalize_scopes,
    refresh_token_request, validate_token_request,
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
        let scope_string = normalize_scopes(scopes, self.config.default_scopes());
        let response = send_prepared_request(
            &self.http,
            device_code_request_with_scope(self.config.client_id(), &scope_string),
        )
        .await?;

        if response.status != 200 {
            bail!(
                "twitch device code request failed with status {}: {}",
                response.status,
                response.body
            );
        }

        let payload: DeviceCodeResponse = serde_json::from_str(&response.body)
            .context("failed to parse Twitch device code response")?;

        let now = OffsetDateTime::now_utc();
        TwitchDeviceAuthorization::new(
            payload.user_code,
            payload.verification_uri,
            payload.verification_uri_complete,
            payload.expires_in,
            payload.interval,
            payload.device_code,
            now,
        )
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
                device_token_request(self.config.client_id(), &authorization.device_code),
            )
            .await?;

            if response.status == 200 {
                let exchange: TwitchTokenExchange = serde_json::from_str(&response.body)
                    .context("failed to parse Twitch token exchange response")?;
                let identity = fetch_authenticated_user(
                    &self.http,
                    &exchange.access_token,
                    self.config.client_id(),
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
}

pub async fn send_prepared_request(
    http: &Client,
    prepared: PreparedRequest,
) -> Result<RawResponse> {
    let PreparedRequest {
        url,
        method,
        headers,
        body,
    } = prepared;

    let mut request = match method {
        HttpMethod::Get => http.get(&url),
        HttpMethod::Post => http.post(&url),
        HttpMethod::Delete => http.delete(&url),
    };
    for (key, value) in headers {
        request = request.header(key, value);
    }
    if let Some(body) = body {
        request = request.body(body);
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("failed to send request to {}", url))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok(RawResponse { status, body })
}

pub async fn validate_access_token(
    http: &Client,
    access_token: &str,
) -> Result<Option<TokenValidation>> {
    let response = send_prepared_request(http, validate_token_request(access_token)).await?;

    if response.status == 200 {
        let payload = serde_json::from_str::<TokenValidation>(&response.body)
            .context("failed to parse Twitch token validation response")?;
        return Ok(Some(payload));
    }

    if response.status == 401 {
        return Ok(None);
    }

    bail!(
        "Twitch token validation failed with status {}: {}",
        response.status,
        response.body
    );
}

pub async fn refresh_access_token(
    http: &Client,
    client_id: &str,
    refresh_token: &str,
) -> Result<Option<TwitchTokenState>> {
    let response =
        send_prepared_request(http, refresh_token_request(client_id, refresh_token)).await?;

    if response.status == 200 {
        let exchange: TwitchTokenExchange = serde_json::from_str(&response.body)
            .context("failed to parse Twitch refresh token response")?;
        return Ok(Some(TwitchTokenState {
            access_token: exchange.access_token,
            refresh_token: exchange
                .refresh_token
                .unwrap_or_else(|| refresh_token.to_string()),
            expires_in_seconds: exchange.expires_in,
            scope: exchange.scope.unwrap_or_default(),
            token_type: exchange.token_type.unwrap_or_else(|| "bearer".to_string()),
            linked_at_ms: now_ms(),
        }));
    }

    if response.status == 400 || response.status == 401 {
        return Ok(None);
    }

    bail!(
        "Twitch refresh token request failed with status {}: {}",
        response.status,
        response.body
    );
}

pub async fn fetch_authenticated_user(
    http: &Client,
    access_token: &str,
    client_id: &str,
) -> Result<TwitchIdentity> {
    let raw =
        send_prepared_request(http, helix::user_lookup_request(access_token, client_id)).await?;
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
        helix::user_lookup_by_login_request(access_token, client_id, login),
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

#[cfg(feature = "tokio-eventsub")]
use futures_util::{SinkExt, StreamExt};
#[cfg(feature = "tokio-eventsub")]
use tokio::net::TcpStream;
#[cfg(feature = "tokio-eventsub")]
use tokio_tungstenite::WebSocketStream;
#[cfg(feature = "tokio-eventsub")]
use tokio_tungstenite::connect_async;
#[cfg(feature = "tokio-eventsub")]
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
#[cfg(feature = "tokio-eventsub")]
use tokio_tungstenite::tungstenite::{Message as WsMessage, http::Uri};

#[cfg(feature = "tokio-eventsub")]
const EVENTSUB_WEBSOCKET_URL: &str = "wss://eventsub.wss.twitch.tv/ws";

#[cfg(feature = "tokio-eventsub")]
pub struct EventSubChatStream {
    ws: WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
    session: EventSubWebSocketSession,
}

#[cfg(feature = "tokio-eventsub")]
impl EventSubChatStream {
    pub fn session_id(&self) -> &str {
        &self.session.id
    }

    pub async fn next_event(&mut self) -> Result<Option<EventSubStreamEvent>> {
        while let Some(message) = self.ws.next().await {
            let message = message.context("EventSub WebSocket error")?;
            match message {
                WsMessage::Text(text) => {
                    let envelope = decode_eventsub_websocket_message(&text)
                        .context("failed to parse EventSub payload")?;
                    if let Some(event) = envelope
                        .stream_event()
                        .context("failed to decode EventSub stream event")?
                    {
                        return Ok(Some(event));
                    }
                }
                WsMessage::Ping(payload) => {
                    self.ws
                        .send(WsMessage::Pong(payload))
                        .await
                        .context("failed to reply to EventSub ping")?;
                }
                WsMessage::Pong(_) => {}
                WsMessage::Binary(_) => {
                    debug!("dropping unexpected binary EventSub payload");
                }
                WsMessage::Close(frame) => {
                    debug!(?frame, "EventSub WebSocket closed by server");
                    return Ok(None);
                }
                WsMessage::Frame(_) => {}
            }
        }

        Ok(None)
    }

    pub async fn close(&mut self) -> Result<()> {
        self.ws
            .close(None)
            .await
            .context("failed to close EventSub WebSocket")
    }
}

#[cfg(feature = "tokio-eventsub")]
pub async fn connect_chat_stream(
    http: &Client,
    config: ChatSubscriptionConfig,
) -> Result<EventSubChatStream> {
    let mut ws = connect_eventsub_websocket(
        EVENTSUB_WEBSOCKET_URL,
        &config.client_id,
        &config.access_token,
    )
    .await?;
    let session = wait_for_session_welcome(&mut ws).await?;
    debug!(
        session_id = %session.id,
        keepalive = session.keepalive_timeout_seconds,
        "received EventSub session welcome"
    );

    create_chat_subscription(http, &config, &session.id).await?;

    Ok(EventSubChatStream { ws, session })
}

#[cfg(feature = "tokio-eventsub")]
async fn connect_eventsub_websocket(
    endpoint: &str,
    client_id: &str,
    access_token: &str,
) -> Result<WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>> {
    ensure_rustls_crypto_provider()?;

    let uri: Uri = endpoint
        .parse()
        .context("failed to parse EventSub WebSocket URL")?;
    let mut request = uri
        .into_client_request()
        .context("failed to build EventSub websocket request")?;
    request.headers_mut().insert(
        "Client-Id",
        client_id.parse().context("invalid client id header")?,
    );
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {access_token}")
            .parse()
            .context("invalid authorization header")?,
    );

    let (ws, response) = connect_async(request)
        .await
        .context("failed to connect to EventSub WebSocket")?;
    trace!(status = ?response.status(), endpoint, "connected to EventSub");
    Ok(ws)
}

#[cfg(feature = "tokio-eventsub")]
fn ensure_rustls_crypto_provider() -> Result<()> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }

    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
        && rustls::crypto::CryptoProvider::get_default().is_none()
    {
        bail!("failed to install the Rustls process-level crypto provider");
    }

    Ok(())
}

#[cfg(feature = "tokio-eventsub")]
async fn wait_for_session_welcome(
    ws: &mut WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
) -> Result<EventSubWebSocketSession> {
    while let Some(message) = ws.next().await {
        let message = message.context("EventSub WebSocket error during welcome")?;
        match message {
            WsMessage::Text(text) => {
                let envelope = decode_eventsub_websocket_message(&text)
                    .context("failed to parse EventSub welcome payload")?;
                match envelope.message_type()? {
                    EventSubMessageType::SessionWelcome => {
                        return envelope
                            .session()
                            .cloned()
                            .ok_or_else(|| anyhow!("EventSub welcome payload missing session"));
                    }
                    EventSubMessageType::SessionReconnect => {
                        let url = envelope
                            .session()
                            .and_then(|session| session.reconnect_url.clone())
                            .unwrap_or_else(|| "<missing reconnect URL>".to_string());
                        bail!("EventSub requested reconnect before welcome: {url}");
                    }
                    EventSubMessageType::SessionDisconnect => {
                        let status = envelope
                            .session()
                            .and_then(|session| session.status.clone())
                            .unwrap_or_else(|| "disconnected".to_string());
                        let reason = envelope
                            .session()
                            .and_then(|session| session.disconnect_reason.clone());
                        bail!("EventSub disconnected before welcome (status {status}): {reason:?}");
                    }
                    _ => {}
                }
            }
            WsMessage::Ping(payload) => {
                ws.send(WsMessage::Pong(payload))
                    .await
                    .context("failed to reply to EventSub ping")?;
            }
            WsMessage::Close(frame) => {
                bail!("EventSub WebSocket closed before welcome (frame: {frame:?})");
            }
            _ => {}
        }
    }

    bail!("EventSub WebSocket closed before welcome message was received")
}

#[cfg(feature = "tokio-eventsub")]
async fn create_chat_subscription(
    http: &Client,
    config: &ChatSubscriptionConfig,
    session_id: &str,
) -> Result<()> {
    create_subscription(
        http,
        config,
        &channel_chat_message_subscription_request(
            &config.broadcaster_id,
            &config.user_id,
            session_id,
        ),
    )
    .await?;
    if let Err(err) = create_subscription(
        http,
        config,
        &channel_chat_message_delete_subscription_request(
            &config.broadcaster_id,
            &config.user_id,
            session_id,
        ),
    )
    .await
    {
        debug!(
            ?err,
            "failed to subscribe to channel.chat.message_delete; continuing without delete events"
        );
    }
    Ok(())
}

#[cfg(feature = "tokio-eventsub")]
async fn create_subscription(
    http: &Client,
    config: &ChatSubscriptionConfig,
    subscription: &crate::eventsub::CreateEventSubSubscriptionRequest,
) -> Result<()> {
    let prepared = helix::create_eventsub_subscription_request(
        &config.client_id,
        &config.access_token,
        subscription,
    )
    .context("failed to serialize EventSub subscription request")?;
    let response = send_prepared_request(http, prepared).await?;

    if response.status == 202 {
        return Ok(());
    }

    match response.status {
        401 => Err(EventSubAuthError.into()),
        403 => {
            bail!(
                "Twitch reported insufficient permissions for EventSub subscription: {}",
                response.body
            );
        }
        _ => {
            bail!(
                "Twitch EventSub subscription request failed (status {}): {}",
                response.status,
                response.body
            );
        }
    }
}

fn now_ms() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp() * 1_000
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    expires_in: u64,
    #[serde(default)]
    interval: Option<u64>,
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

    #[cfg(feature = "tokio-eventsub")]
    #[test]
    fn rustls_provider_init_is_idempotent() {
        ensure_rustls_crypto_provider().expect("first provider init should succeed");
        ensure_rustls_crypto_provider().expect("second provider init should succeed");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    #[cfg(feature = "tokio-eventsub")]
    #[tokio::test(flavor = "current_thread")]
    async fn eventsub_connect_error_path_does_not_panic_for_missing_provider() {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            connect_eventsub_websocket("wss://127.0.0.1:1/ws", "test-client-id", "test-token"),
        )
        .await
        .expect("local websocket connect test should fail quickly");

        assert!(
            result.is_err(),
            "expected connect failure against a closed localhost port"
        );
    }

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
