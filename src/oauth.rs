use std::time::Duration;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
#[cfg(feature = "reqwest-client")]
use time::Duration as TimeDuration;
use time::OffsetDateTime;

use crate::TwitchIdentity;
use crate::http::{HttpMethod, PreparedRequest, form_body};
use crate::signing::{self, SigningError};

const TOKEN_REFRESH_SKEW_MS: i64 = 5 * 60 * 1_000;
const TWITCH_DEVICE_CODE_URL: &str = "https://id.twitch.tv/oauth2/device";
const TWITCH_TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const TWITCH_VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";
const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
#[cfg(feature = "reqwest-client")]
const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwitchTokenState {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in_seconds: Option<u32>,
    pub scope: Vec<String>,
    pub token_type: String,
    pub linked_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwitchAuthOutcome {
    pub identity: TwitchIdentity,
    pub tokens: TwitchTokenState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwitchAuthConfig {
    pub client_id: String,
    pub default_scopes: Vec<String>,
}

impl TwitchAuthConfig {
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            default_scopes: vec![
                "chat:read".to_string(),
                "bits:read".to_string(),
                "channel:read:redemptions".to_string(),
                "moderator:read:chatters".to_string(),
                "user:read:chat".to_string(),
            ],
        }
    }

    pub fn with_default_scopes<I, S>(mut self, scopes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.default_scopes = scopes.into_iter().map(Into::into).collect();
        self
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn default_scopes(&self) -> &[String] {
        &self.default_scopes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwitchDeviceAuthorization {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_at: OffsetDateTime,
    pub interval: Duration,
    pub(crate) device_code: String,
}

impl TwitchDeviceAuthorization {
    #[cfg(feature = "reqwest-client")]
    pub(crate) fn new(
        user_code: String,
        verification_uri: String,
        verification_uri_complete: Option<String>,
        expires_in_seconds: u64,
        interval_seconds: Option<u64>,
        device_code: String,
        now: OffsetDateTime,
    ) -> Result<Self, SigningError> {
        let expires_at = now
            .checked_add(TimeDuration::seconds(expires_in_seconds as i64))
            .ok_or(SigningError::MalformedToken)?;
        Ok(Self {
            user_code,
            verification_uri,
            verification_uri_complete,
            expires_at,
            interval: Duration::from_secs(interval_seconds.unwrap_or(DEFAULT_POLL_INTERVAL_SECS)),
            device_code,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenValidation {
    pub client_id: String,
    pub login: Option<String>,
    pub user_id: Option<String>,
    pub scopes: Vec<String>,
    pub expires_in: u64,
}

pub fn token_expires_at_ms(tokens: &TwitchTokenState) -> Option<i64> {
    tokens
        .expires_in_seconds
        .map(|expires_in| tokens.linked_at_ms + i64::from(expires_in) * 1_000)
}

pub fn should_refresh_twitch_token(tokens: &TwitchTokenState, now_ms: i64) -> bool {
    token_expires_at_ms(tokens)
        .is_some_and(|expires_at_ms| now_ms >= expires_at_ms - TOKEN_REFRESH_SKEW_MS)
}

pub fn refreshed_twitch_token_state(
    previous: &TwitchTokenState,
    access_token: String,
    refresh_token: Option<String>,
    expires_in_seconds: Option<u32>,
    scope: Option<Vec<String>>,
    token_type: Option<String>,
    now_ms: i64,
) -> TwitchTokenState {
    TwitchTokenState {
        access_token,
        refresh_token: refresh_token.unwrap_or_else(|| previous.refresh_token.clone()),
        expires_in_seconds,
        scope: scope.unwrap_or_else(|| previous.scope.clone()),
        token_type: token_type.unwrap_or_else(|| previous.token_type.clone()),
        linked_at_ms: now_ms,
    }
}

pub fn device_code_request(client_id: &str, scopes: &[String]) -> PreparedRequest {
    let scope_string = normalize_scopes(scopes, &[]);
    device_code_request_with_scope(client_id, &scope_string)
}

pub fn device_code_request_with_scope(client_id: &str, scope_string: &str) -> PreparedRequest {
    PreparedRequest {
        url: TWITCH_DEVICE_CODE_URL.to_string(),
        method: HttpMethod::Post,
        headers: vec![(
            "Content-Type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        )],
        body: Some(form_body(&[
            ("client_id", client_id),
            ("scope", scope_string),
        ])),
    }
}

pub fn device_token_request(client_id: &str, device_code: &str) -> PreparedRequest {
    PreparedRequest {
        url: TWITCH_TOKEN_URL.to_string(),
        method: HttpMethod::Post,
        headers: vec![(
            "Content-Type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        )],
        body: Some(form_body(&[
            ("client_id", client_id),
            ("device_code", device_code),
            ("grant_type", DEVICE_GRANT_TYPE),
        ])),
    }
}

pub fn refresh_token_request(client_id: &str, refresh_token: &str) -> PreparedRequest {
    PreparedRequest {
        url: TWITCH_TOKEN_URL.to_string(),
        method: HttpMethod::Post,
        headers: vec![(
            "Content-Type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        )],
        body: Some(form_body(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])),
    }
}

pub fn validate_token_request(access_token: &str) -> PreparedRequest {
    PreparedRequest {
        url: TWITCH_VALIDATE_URL.to_string(),
        method: HttpMethod::Get,
        headers: vec![("Authorization".to_string(), format!("OAuth {access_token}"))],
        body: None,
    }
}

/// Trait for OAuth state payloads that carry an expiration timestamp.
///
/// Consumers define their own state struct (embedding whatever app-specific
/// fields they need) and implement this trait so that
/// [`verify_oauth_state`] can check expiration.
pub trait OAuthStatePayload: Serialize + DeserializeOwned {
    fn expires_at_ms(&self) -> i64;
}

/// Builds a Twitch OAuth2 authorize URL.
///
/// Returns the full URL as a `String`. The `state` parameter is HMAC-signed
/// and embedded in the query string. The consumer defines the state struct
/// (implementing [`Serialize`]) with whatever fields they need.
pub fn build_authorize_url<S: Serialize>(
    client_id: &str,
    redirect_uri: &str,
    scopes: &[&str],
    state: &S,
    signing_secret: &str,
) -> Result<String, SigningError> {
    let signed_state = signing::sign_payload(signing_secret, state)?;
    let scope_joined = scopes.join(" ");

    let mut url = String::from("https://id.twitch.tv/oauth2/authorize?");
    url.push_str("client_id=");
    url.push_str(&crate::http::percent_encode(client_id));
    url.push_str("&redirect_uri=");
    url.push_str(&crate::http::percent_encode(redirect_uri));
    url.push_str("&response_type=code");
    url.push_str("&scope=");
    url.push_str(&crate::http::percent_encode(&scope_joined));
    url.push_str("&state=");
    url.push_str(&crate::http::percent_encode(&signed_state));

    Ok(url)
}

/// Verifies and decodes a signed OAuth state parameter.
///
/// Returns `Err(SigningError::Expired)` if the current time exceeds the
/// payload's expiration.
pub fn verify_oauth_state<S: OAuthStatePayload>(
    signing_secret: &str,
    state_token: &str,
    now_ms: i64,
) -> Result<S, SigningError> {
    let claims: S = signing::verify_signed_payload(signing_secret, state_token)?;
    if now_ms > claims.expires_at_ms() {
        return Err(SigningError::Expired);
    }
    Ok(claims)
}

pub(crate) fn normalize_scopes(overrides: &[String], defaults: &[String]) -> String {
    let mut scopes: Vec<String> = if overrides.is_empty() {
        defaults.to_vec()
    } else {
        overrides.to_vec()
    };
    scopes.sort();
    scopes.dedup();
    scopes.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-oauth-secret";

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct TestState {
        role: String,
        expires_at_ms: i64,
    }

    impl OAuthStatePayload for TestState {
        fn expires_at_ms(&self) -> i64 {
            self.expires_at_ms
        }
    }

    #[test]
    fn refresh_window_opens_before_token_expiry() {
        let tokens = TwitchTokenState {
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            expires_in_seconds: Some(3_600),
            scope: vec!["user:read:chat".to_string()],
            token_type: "bearer".to_string(),
            linked_at_ms: 1_741_515_200_000,
        };

        assert!(!should_refresh_twitch_token(&tokens, 1_741_518_000_000));
        assert!(should_refresh_twitch_token(&tokens, 1_741_518_500_000));
    }

    #[test]
    fn refreshed_token_state_preserves_previous_refresh_token_when_omitted() {
        let previous = TwitchTokenState {
            access_token: "old-access".to_string(),
            refresh_token: "old-refresh".to_string(),
            expires_in_seconds: Some(3_600),
            scope: vec!["user:read:chat".to_string()],
            token_type: "bearer".to_string(),
            linked_at_ms: 1_741_515_200_000,
        };

        let refreshed = refreshed_twitch_token_state(
            &previous,
            "new-access".to_string(),
            None,
            Some(7_200),
            None,
            None,
            1_741_515_300_000,
        );

        assert_eq!(refreshed.access_token, "new-access");
        assert_eq!(refreshed.refresh_token, "old-refresh");
        assert_eq!(refreshed.expires_in_seconds, Some(7_200));
        assert_eq!(refreshed.scope, previous.scope);
    }

    #[test]
    fn authorize_url_round_trips_state() {
        let state = TestState {
            role: "viewer".to_string(),
            expires_at_ms: 1_741_515_800_000,
        };

        let url = build_authorize_url(
            "client-id",
            "https://example.com/callback",
            &["user:read:chat"],
            &state,
            SECRET,
        )
        .expect("should build url");

        assert!(url.starts_with("https://id.twitch.tv/oauth2/authorize?"));
        assert!(url.contains("client_id=client-id"));
        assert!(url.contains("response_type=code"));

        // Extract the state param and verify it round-trips
        let state_param = url
            .split("&state=")
            .nth(1)
            .expect("state param should exist");
        let decoded_state =
            crate::http::percent_decode(state_param).expect("state should percent-decode");
        let verified: TestState = verify_oauth_state(SECRET, &decoded_state, 1_741_515_200_000)
            .expect("state should verify");
        assert_eq!(verified.role, "viewer");
    }

    #[test]
    fn expired_oauth_state_is_rejected() {
        let state = TestState {
            role: "streamer".to_string(),
            expires_at_ms: 1_741_515_200_100,
        };
        let signed = signing::sign_payload(SECRET, &state).expect("should sign");

        assert!(matches!(
            verify_oauth_state::<TestState>(SECRET, &signed, 1_741_515_200_101),
            Err(SigningError::Expired)
        ));
    }
}
