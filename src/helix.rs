use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::TwitchIdentity;
use crate::eventsub::{CreateEventSubSubscriptionRequest, CreateEventSubSubscriptionResponse};
use crate::http::{
    HttpResponse, PreparedRequestBuilder, ResponseMeta, append_query_params, form_body,
};
use crate::oauth::{TwitchAuthOutcome, TwitchTokenState};

const TWITCH_TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";

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
    #[error("endpoint {endpoint} requires an access token")]
    MissingAccessToken { endpoint: &'static str },
}

pub use crate::http::{HttpMethod, PreparedRequest, RawResponse};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndpointStability {
    Ga,
    New,
    Beta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HelixAuthKind {
    None,
    App,
    User,
    Either,
    ExtensionJwt,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelixEndpoint {
    pub id: &'static str,
    pub group: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub stability: EndpointStability,
    pub method: HttpMethod,
    pub path: &'static str,
    pub auth_kind: HelixAuthKind,
    pub scopes: &'static [&'static str],
    pub supports_pagination: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HelixEndpointRequest {
    path_params: Vec<(String, String)>,
    query_params: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    json_body: Option<serde_json::Value>,
}

impl HelixEndpointRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path_param(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.path_params.push((name.into(), value.into()));
        self
    }

    pub fn with_query_param(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.query_params.push((name.into(), value.into()));
        self
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn with_json_body(mut self, value: serde_json::Value) -> Self {
        self.json_body = Some(value);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HelixJsonResponse {
    pub meta: ResponseMeta,
    pub body: serde_json::Value,
}

impl HelixEndpoint {
    pub fn missing_scopes(&self, granted: &[String]) -> Vec<&'static str> {
        self.scopes
            .iter()
            .copied()
            .filter(|scope| !granted.iter().any(|granted_scope| granted_scope == scope))
            .collect()
    }

    pub fn prepare(
        &'static self,
        request: &HelixEndpointRequest,
        client_id: &str,
        access_token: Option<&str>,
    ) -> Result<PreparedRequest, HelixError> {
        if client_id.is_empty() {
            return Err(HelixError::MissingCredentials);
        }
        if matches!(
            self.auth_kind,
            HelixAuthKind::App | HelixAuthKind::User | HelixAuthKind::Either | HelixAuthKind::ExtensionJwt
        ) && access_token.is_none()
        {
            return Err(HelixError::MissingAccessToken { endpoint: self.id });
        }

        let mut path = self.path.to_string();
        for (name, value) in &request.path_params {
            let needle = format!("{{{name}}}");
            path = path.replace(&needle, &crate::http::percent_encode(value));
        }

        let base_url = format!("https://api.twitch.tv/helix{path}");
        let url = append_query_params(&base_url, &request.query_params);
        let mut builder = PreparedRequestBuilder::new(self.method.clone(), url)
            .header("Client-Id", client_id);
        if let Some(access_token) = access_token {
            builder = builder.header("Authorization", format!("Bearer {access_token}"));
        }
        for (name, value) in &request.headers {
            builder = builder.header(name.clone(), value.clone());
        }
        if let Some(json_body) = &request.json_body {
            builder = builder.json_body(json_body)?;
        }
        Ok(builder.build())
    }

    pub fn parse_json_response(
        &self,
        response: HttpResponse,
    ) -> Result<HelixJsonResponse, HelixError> {
        if !(200..=299).contains(&response.status) {
            return Err(HelixError::ApiError {
                status: response.status,
                body: response.body,
            });
        }
        let meta = response.meta();
        let body = if response.body.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(&response.body)?
        };
        Ok(HelixJsonResponse { meta, body })
    }
}

macro_rules! declare_generated_endpoint {
    ($request_name:ident, $response_name:ident, $endpoint_const:ident) => {
        #[derive(Clone, Debug, Default, PartialEq, Eq)]
        pub struct $request_name {
            inner: $crate::helix::HelixEndpointRequest,
        }

        impl $request_name {
            pub fn new() -> Self {
                Self {
                    inner: $crate::helix::HelixEndpointRequest::new(),
                }
            }

            pub fn with_path_param(
                mut self,
                name: impl Into<String>,
                value: impl Into<String>,
            ) -> Self {
                self.inner = self.inner.with_path_param(name, value);
                self
            }

            pub fn with_query_param(
                mut self,
                name: impl Into<String>,
                value: impl Into<String>,
            ) -> Self {
                self.inner = self.inner.with_query_param(name, value);
                self
            }

            pub fn with_header(
                mut self,
                name: impl Into<String>,
                value: impl Into<String>,
            ) -> Self {
                self.inner = self.inner.with_header(name, value);
                self
            }

            pub fn with_json_body(mut self, value: serde_json::Value) -> Self {
                self.inner = self.inner.with_json_body(value);
                self
            }

            pub fn endpoint(&self) -> &'static HelixEndpoint {
                &$endpoint_const
            }

            pub fn prepare(
                self,
                client_id: &str,
                access_token: Option<&str>,
            ) -> Result<$crate::http::PreparedRequest, $crate::helix::HelixError> {
                $endpoint_const.prepare(&self.inner, client_id, access_token)
            }
        }

        pub type $response_name = $crate::helix::HelixJsonResponse;
    };
}

#[path = "helix_generated.rs"]
pub mod generated;

pub use generated::{
    ads, analytics, bits, ccls as content_classification_labels, channel_points, channels, charity,
    chat, clips, conduits, entitlements, eventsub as eventsub_endpoints, extensions, games, goals,
    guest_star, hype_train, moderation, polls, predictions, raids, schedule, search, streams,
    subscriptions, tags, teams, users, videos, whispers,
};

pub static ALL_ENDPOINTS: &[&HelixEndpoint] = generated::ALL_ENDPOINTS;

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
        url: TWITCH_TOKEN_URL.to_string(),
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
        url: TWITCH_TOKEN_URL.to_string(),
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

pub fn user_lookup_by_login_request(
    access_token: &str,
    client_id: &str,
    login: &str,
) -> PreparedRequest {
    PreparedRequest {
        url: format!(
            "https://api.twitch.tv/helix/users?login={}",
            crate::http::percent_encode(login)
        ),
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
