use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::TwitchIdentity;
use crate::signing::{self, SigningError};

const TOKEN_REFRESH_SKEW_MS: i64 = 5 * 60 * 1_000;

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
