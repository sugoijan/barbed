use serde::Deserialize;
use thiserror::Error;

use crate::TwitchIdentity;
use crate::eventsub::{CreateEventSubSubscriptionRequest, CreateEventSubSubscriptionResponse};
use crate::http::form_body;
use crate::oauth::{TwitchAuthOutcome, TwitchTokenState};

#[derive(Debug, Error)]
pub enum HelixError {
    #[error("twitch API request failed with status {status}: {body}")]
    ApiError { status: u16, body: String },
    #[error("twitch API response failed to decode: {0}")]
    Json(#[from] serde_json::Error),
    #[error("twitch user lookup returned no users")]
    NoUsers,
    #[error("twitch token exchange omitted refresh token")]
    MissingRefreshToken,
    #[error("client_id or client_secret is not configured")]
    MissingCredentials,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Delete,
}

#[derive(Clone, Debug)]
pub struct PreparedRequest {
    pub url: String,
    pub method: HttpMethod,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RawResponse {
    pub status: u16,
    pub body: String,
}

// -- Twitch API response types --

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct TwitchTokenExchange {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u32>,
    pub scope: Option<Vec<String>>,
    pub token_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
struct TwitchUsersResponse {
    data: Vec<TwitchUserRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
struct TwitchUserRecord {
    id: String,
    login: String,
    display_name: String,
}

// -- Request builders --

pub fn token_exchange_request(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> PreparedRequest {
    PreparedRequest {
        url: "https://id.twitch.tv/oauth2/token".to_string(),
        method: HttpMethod::Post,
        headers: vec![(
            "Content-Type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        )],
        body: Some(form_body(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
        ])),
    }
}

pub fn token_refresh_request(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> PreparedRequest {
    PreparedRequest {
        url: "https://id.twitch.tv/oauth2/token".to_string(),
        method: HttpMethod::Post,
        headers: vec![(
            "Content-Type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        )],
        body: Some(form_body(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])),
    }
}

pub fn user_lookup_request(access_token: &str, client_id: &str) -> PreparedRequest {
    PreparedRequest {
        url: "https://api.twitch.tv/helix/users".to_string(),
        method: HttpMethod::Get,
        headers: vec![
            (
                "Authorization".to_string(),
                format!("Bearer {access_token}"),
            ),
            ("Client-Id".to_string(), client_id.to_string()),
        ],
        body: None,
    }
}

pub fn create_eventsub_subscription_request(
    client_id: &str,
    access_token: &str,
    subscription: &CreateEventSubSubscriptionRequest,
) -> Result<PreparedRequest, HelixError> {
    let body = serde_json::to_string(subscription)?;
    Ok(PreparedRequest {
        url: "https://api.twitch.tv/helix/eventsub/subscriptions".to_string(),
        method: HttpMethod::Post,
        headers: vec![
            (
                "Authorization".to_string(),
                format!("Bearer {access_token}"),
            ),
            ("Client-Id".to_string(), client_id.to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ],
        body: Some(body),
    })
}

pub fn list_eventsub_subscriptions_request(client_id: &str, access_token: &str) -> PreparedRequest {
    PreparedRequest {
        url: "https://api.twitch.tv/helix/eventsub/subscriptions?type=channel.chat.message"
            .to_string(),
        method: HttpMethod::Get,
        headers: vec![
            (
                "Authorization".to_string(),
                format!("Bearer {access_token}"),
            ),
            ("Client-Id".to_string(), client_id.to_string()),
        ],
        body: None,
    }
}

pub fn delete_eventsub_subscription_request(
    client_id: &str,
    access_token: &str,
    subscription_id: &str,
) -> PreparedRequest {
    PreparedRequest {
        url: format!(
            "https://api.twitch.tv/helix/eventsub/subscriptions?id={}",
            crate::http::percent_encode(subscription_id)
        ),
        method: HttpMethod::Delete,
        headers: vec![
            (
                "Authorization".to_string(),
                format!("Bearer {access_token}"),
            ),
            ("Client-Id".to_string(), client_id.to_string()),
        ],
        body: None,
    }
}

// -- Response parsers --

pub fn parse_token_exchange(response: RawResponse) -> Result<TwitchTokenExchange, HelixError> {
    if response.status != 200 {
        return Err(HelixError::ApiError {
            status: response.status,
            body: response.body,
        });
    }
    serde_json::from_str(&response.body).map_err(HelixError::from)
}

pub fn parse_token_refresh(response: RawResponse) -> Result<TwitchTokenExchange, HelixError> {
    parse_token_exchange(response)
}

pub fn parse_user_lookup(response: RawResponse) -> Result<TwitchIdentity, HelixError> {
    if response.status != 200 {
        return Err(HelixError::ApiError {
            status: response.status,
            body: response.body,
        });
    }
    let users: TwitchUsersResponse = serde_json::from_str(&response.body)?;
    let user = users.data.into_iter().next().ok_or(HelixError::NoUsers)?;
    Ok(TwitchIdentity::new(user.id, user.login, user.display_name))
}

/// Combines token exchange + user lookup into a [`TwitchAuthOutcome`].
///
/// Call this after sending both [`token_exchange_request`] and
/// [`user_lookup_request`] and parsing their responses.
pub fn build_auth_outcome(
    identity: TwitchIdentity,
    exchange: TwitchTokenExchange,
    now_ms: i64,
) -> Result<TwitchAuthOutcome, HelixError> {
    Ok(TwitchAuthOutcome {
        identity,
        tokens: TwitchTokenState {
            access_token: exchange.access_token,
            refresh_token: exchange
                .refresh_token
                .ok_or(HelixError::MissingRefreshToken)?,
            expires_in_seconds: exchange.expires_in,
            scope: exchange.scope.unwrap_or_default(),
            token_type: exchange.token_type.unwrap_or_else(|| "bearer".to_string()),
            linked_at_ms: now_ms,
        },
    })
}

pub fn parse_create_eventsub_subscription(
    response: RawResponse,
) -> Result<CreateEventSubSubscriptionResponse, HelixError> {
    if response.status != 202 {
        return Err(HelixError::ApiError {
            status: response.status,
            body: response.body,
        });
    }
    serde_json::from_str(&response.body).map_err(HelixError::from)
}

pub fn parse_list_eventsub_subscriptions(
    response: RawResponse,
) -> Result<CreateEventSubSubscriptionResponse, HelixError> {
    if response.status != 200 {
        return Err(HelixError::ApiError {
            status: response.status,
            body: response.body,
        });
    }
    serde_json::from_str(&response.body).map_err(HelixError::from)
}

pub fn parse_delete_eventsub_subscription(response: RawResponse) -> Result<(), HelixError> {
    if response.status != 204 {
        return Err(HelixError::ApiError {
            status: response.status,
            body: response.body,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_exchange_request_has_correct_structure() {
        let req = token_exchange_request("cid", "csecret", "authcode", "https://example.com/cb");
        assert_eq!(req.url, "https://id.twitch.tv/oauth2/token");
        assert_eq!(req.method, HttpMethod::Post);
        let body = req.body.unwrap();
        assert!(body.contains("client_id=cid"));
        assert!(body.contains("grant_type=authorization_code"));
        assert!(body.contains("code=authcode"));
    }

    #[test]
    fn token_refresh_request_has_correct_structure() {
        let req = token_refresh_request("cid", "csecret", "rtoken");
        assert_eq!(req.method, HttpMethod::Post);
        let body = req.body.unwrap();
        assert!(body.contains("grant_type=refresh_token"));
        assert!(body.contains("refresh_token=rtoken"));
    }

    #[test]
    fn user_lookup_request_has_auth_headers() {
        let req = user_lookup_request("my-token", "my-client");
        assert_eq!(req.method, HttpMethod::Get);
        assert!(
            req.headers
                .iter()
                .any(|(k, v)| k == "Authorization" && v == "Bearer my-token")
        );
        assert!(
            req.headers
                .iter()
                .any(|(k, v)| k == "Client-Id" && v == "my-client")
        );
    }

    #[test]
    fn parse_token_exchange_rejects_non_200() {
        let resp = RawResponse {
            status: 400,
            body: "bad request".to_string(),
        };
        assert!(matches!(
            parse_token_exchange(resp),
            Err(HelixError::ApiError { status: 400, .. })
        ));
    }

    #[test]
    fn parse_user_lookup_extracts_identity() {
        let resp = RawResponse {
            status: 200,
            body: r#"{"data":[{"id":"42","login":"tester","display_name":"Tester"}]}"#.to_string(),
        };
        let identity = parse_user_lookup(resp).expect("should parse");
        assert_eq!(identity.user_id, "42");
        assert_eq!(identity.login, "tester");
        assert_eq!(identity.display_name, "Tester");
    }

    #[test]
    fn parse_user_lookup_rejects_empty_data() {
        let resp = RawResponse {
            status: 200,
            body: r#"{"data":[]}"#.to_string(),
        };
        assert!(matches!(parse_user_lookup(resp), Err(HelixError::NoUsers)));
    }

    #[test]
    fn delete_eventsub_request_uses_query_param() {
        let req = delete_eventsub_subscription_request("cid", "tok", "sub-123");
        assert_eq!(req.method, HttpMethod::Delete);
        assert!(req.url.contains("id=sub-123"));
    }
}
