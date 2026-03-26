#[cfg(feature = "cloudflare-worker")]
use thiserror::Error;
#[cfg(feature = "cloudflare-worker")]
use wasm_bindgen::JsValue;
#[cfg(feature = "cloudflare-worker")]
use worker::{Fetch, Headers, Method, Request, RequestInit};

#[cfg(feature = "cloudflare-worker")]
use crate::eventsub::{CreateEventSubSubscriptionRequest, CreateEventSubSubscriptionResponse};
#[cfg(feature = "cloudflare-worker")]
use crate::helix::{
    self, HelixError, HttpMethod, PreparedRequest, RawResponse, TwitchTokenExchange,
};
#[cfg(feature = "cloudflare-worker")]
use crate::oauth::TwitchAuthOutcome;

#[cfg(feature = "cloudflare-worker")]
#[derive(Debug, Error)]
pub enum CloudflareWorkerError {
    #[error("cloudflare worker transport failed: {0}")]
    Transport(#[from] worker::Error),
    #[error(transparent)]
    Helix(#[from] HelixError),
}

/// Sends a [`PreparedRequest`] via the Cloudflare Workers `Fetch` API.
#[cfg(feature = "cloudflare-worker")]
pub async fn send_prepared_request(
    prepared: PreparedRequest,
) -> Result<RawResponse, worker::Error> {
    let headers = Headers::new();
    for (key, value) in &prepared.headers {
        headers.set(key, value)?;
    }
    let mut init = RequestInit::new();
    init.with_method(match prepared.method {
        HttpMethod::Get => Method::Get,
        HttpMethod::Post => Method::Post,
        HttpMethod::Delete => Method::Delete,
    });
    init.with_headers(headers);
    if let Some(body) = &prepared.body {
        init.with_body(Some(JsValue::from_str(body)));
    }
    let request = Request::new_with_init(&prepared.url, &init)?;
    let mut response = Fetch::Request(request).send().await?;
    Ok(RawResponse {
        status: response.status_code(),
        body: response.text().await.unwrap_or_default(),
    })
}

/// Exchanges an OAuth authorization code for tokens and fetches the user identity.
#[cfg(feature = "cloudflare-worker")]
pub async fn exchange_twitch_code(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
    now_ms: i64,
) -> Result<TwitchAuthOutcome, CloudflareWorkerError> {
    if client_id.is_empty() || client_secret.is_empty() {
        return Err(HelixError::MissingCredentials.into());
    }

    let token_req = helix::token_exchange_request(client_id, client_secret, code, redirect_uri);
    let token_raw = send_prepared_request(token_req).await?;
    let exchange = helix::parse_token_exchange(token_raw)?;

    let user_req = helix::user_lookup_request(&exchange.access_token, client_id);
    let user_raw = send_prepared_request(user_req).await?;
    let identity = helix::parse_user_lookup(user_raw)?;

    helix::build_auth_outcome(identity, exchange, now_ms).map_err(CloudflareWorkerError::from)
}

/// Refreshes an access token using a refresh token.
#[cfg(feature = "cloudflare-worker")]
pub async fn refresh_access_token(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<TwitchTokenExchange, CloudflareWorkerError> {
    if client_id.is_empty() || client_secret.is_empty() || refresh_token.is_empty() {
        return Err(HelixError::MissingCredentials.into());
    }

    let req = helix::token_refresh_request(client_id, client_secret, refresh_token);
    let raw = send_prepared_request(req).await?;
    helix::parse_token_refresh(raw).map_err(CloudflareWorkerError::from)
}

/// Creates an EventSub subscription via the Helix API.
#[cfg(feature = "cloudflare-worker")]
pub async fn create_eventsub_subscription(
    client_id: &str,
    access_token: &str,
    subscription: &CreateEventSubSubscriptionRequest,
) -> Result<CreateEventSubSubscriptionResponse, CloudflareWorkerError> {
    if client_id.is_empty() || access_token.is_empty() {
        return Err(HelixError::MissingCredentials.into());
    }

    let req = helix::create_eventsub_subscription_request(client_id, access_token, subscription)?;
    let raw = send_prepared_request(req).await?;
    helix::parse_create_eventsub_subscription(raw).map_err(CloudflareWorkerError::from)
}

/// Lists EventSub subscriptions for `channel.chat.message`.
#[cfg(feature = "cloudflare-worker")]
pub async fn list_eventsub_subscriptions(
    client_id: &str,
    access_token: &str,
) -> Result<CreateEventSubSubscriptionResponse, CloudflareWorkerError> {
    if client_id.is_empty() || access_token.is_empty() {
        return Err(HelixError::MissingCredentials.into());
    }

    let req = helix::list_eventsub_subscriptions_request(client_id, access_token);
    let raw = send_prepared_request(req).await?;
    helix::parse_list_eventsub_subscriptions(raw).map_err(CloudflareWorkerError::from)
}

/// Deletes an EventSub subscription by ID.
#[cfg(feature = "cloudflare-worker")]
pub async fn delete_eventsub_subscription(
    client_id: &str,
    access_token: &str,
    subscription_id: &str,
) -> Result<(), CloudflareWorkerError> {
    if client_id.is_empty() || access_token.is_empty() {
        return Err(HelixError::MissingCredentials.into());
    }

    let req = helix::delete_eventsub_subscription_request(client_id, access_token, subscription_id);
    let raw = send_prepared_request(req).await?;
    helix::parse_delete_eventsub_subscription(raw).map_err(CloudflareWorkerError::from)
}
