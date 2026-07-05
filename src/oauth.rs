use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
#[cfg(feature = "reqwest-client")]
use time::Duration as TimeDuration;
use time::OffsetDateTime;

use crate::TwitchIdentity;
use crate::http::{HttpMethod, PreparedRequest, append_query_params, form_post_request};
use crate::signing::{self, SigningError};

const TOKEN_REFRESH_SKEW_MS: i64 = 5 * 60 * 1_000;
const TWITCH_DEVICE_CODE_URL: &str = "https://id.twitch.tv/oauth2/device";
pub(crate) const TWITCH_TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const TWITCH_VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";
const TWITCH_REVOKE_URL: &str = "https://id.twitch.tv/oauth2/revoke";
const TWITCH_AUTHORIZE_URL: &str = "https://id.twitch.tv/oauth2/authorize";
const TWITCH_USERINFO_URL: &str = "https://id.twitch.tv/oauth2/userinfo";
const TWITCH_OPENID_CONFIGURATION_URL: &str =
    "https://id.twitch.tv/oauth2/.well-known/openid-configuration";
const TWITCH_OPENID_JWKS_URL: &str = "https://id.twitch.tv/oauth2/keys";
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OAuthTokenKind {
    App,
    User,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenIdConfiguration {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub userinfo_endpoint: String,
    #[serde(default)]
    pub id_token_signing_alg_values_supported: Vec<String>,
    #[serde(default)]
    pub claims_supported: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenIdUserInfo {
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
    pub iss: String,
    pub sub: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: Option<bool>,
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub preferred_username: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
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
    form_post_request(
        TWITCH_DEVICE_CODE_URL,
        &[("client_id", client_id), ("scope", scope_string)],
    )
}

pub fn device_token_request(client_id: &str, device_code: &str) -> PreparedRequest {
    form_post_request(
        TWITCH_TOKEN_URL,
        &[
            ("client_id", client_id),
            ("device_code", device_code),
            ("grant_type", DEVICE_GRANT_TYPE),
        ],
    )
}

pub fn refresh_token_request(client_id: &str, refresh_token: &str) -> PreparedRequest {
    form_post_request(
        TWITCH_TOKEN_URL,
        &[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
    )
}

pub fn validate_token_request(access_token: &str) -> PreparedRequest {
    PreparedRequest {
        url: TWITCH_VALIDATE_URL.to_string(),
        method: HttpMethod::Get,
        headers: vec![("Authorization".to_string(), format!("OAuth {access_token}"))],
        body: None,
    }
}

pub fn client_credentials_request(client_id: &str, client_secret: &str) -> PreparedRequest {
    form_post_request(
        TWITCH_TOKEN_URL,
        &[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("grant_type", "client_credentials"),
        ],
    )
}

pub fn revoke_token_request(client_id: &str, token: &str) -> PreparedRequest {
    form_post_request(
        TWITCH_REVOKE_URL,
        &[("client_id", client_id), ("token", token)],
    )
}

pub fn openid_configuration_request() -> PreparedRequest {
    PreparedRequest {
        url: TWITCH_OPENID_CONFIGURATION_URL.to_string(),
        method: HttpMethod::Get,
        headers: vec![],
        body: None,
    }
}

pub fn openid_jwks_request() -> PreparedRequest {
    PreparedRequest {
        url: TWITCH_OPENID_JWKS_URL.to_string(),
        method: HttpMethod::Get,
        headers: vec![],
        body: None,
    }
}

pub fn openid_userinfo_request(access_token: &str) -> PreparedRequest {
    PreparedRequest {
        url: TWITCH_USERINFO_URL.to_string(),
        method: HttpMethod::Get,
        headers: vec![(
            "Authorization".to_string(),
            format!("Bearer {access_token}"),
        )],
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
    let params = [
        ("client_id".to_string(), client_id.to_string()),
        ("redirect_uri".to_string(), redirect_uri.to_string()),
        ("response_type".to_string(), "code".to_string()),
        ("scope".to_string(), scopes.join(" ")),
        ("state".to_string(), signed_state),
    ];
    Ok(append_query_params(TWITCH_AUTHORIZE_URL, &params))
}

pub fn build_oidc_authorize_url(
    client_id: &str,
    redirect_uri: &str,
    scopes: &[&str],
    claims: Option<&serde_json::Value>,
    nonce: Option<&str>,
    response_type: &str,
) -> String {
    let mut params = vec![
        ("client_id".to_string(), client_id.to_string()),
        ("redirect_uri".to_string(), redirect_uri.to_string()),
        ("response_type".to_string(), response_type.to_string()),
        ("scope".to_string(), scopes.join(" ")),
    ];
    if let Some(nonce) = nonce {
        params.push(("nonce".to_string(), nonce.to_string()));
    }
    if let Some(claims) = claims {
        let claims = serde_json::to_string(claims).expect("OIDC claims JSON should serialize");
        params.push(("claims".to_string(), claims));
    }
    append_query_params(TWITCH_AUTHORIZE_URL, &params)
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

pub fn token_validation_due(last_validated_at_ms: Option<i64>, now_ms: i64) -> bool {
    match last_validated_at_ms {
        Some(last_validated_at_ms) => now_ms - last_validated_at_ms >= 60 * 60 * 1_000,
        None => true,
    }
}

pub fn sign_extension_jwt<T: Serialize>(secret: &[u8], claims: &T) -> Result<String, SigningError> {
    if secret.is_empty() {
        return Err(SigningError::MissingSigningSecret);
    }

    let header = serde_json::json!({
        "alg": "HS256",
        "typ": "JWT",
    });
    let header = encode_base64_url_no_pad(&serde_json::to_vec(&header)?);
    let claims = encode_base64_url_no_pad(&serde_json::to_vec(claims)?);
    let signing_input = format!("{header}.{claims}");
    let signature = signing::hmac_sha256(secret, &[signing_input.as_bytes()]);
    Ok(format!(
        "{signing_input}.{}",
        encode_base64_url_no_pad(&signature)
    ))
}

pub fn verify_extension_jwt<T: DeserializeOwned>(
    secret: &[u8],
    token: &str,
) -> Result<T, SigningError> {
    if secret.is_empty() {
        return Err(SigningError::MissingSigningSecret);
    }
    let (header, rest) = token.split_once('.').ok_or(SigningError::MalformedToken)?;
    let (claims, signature) = rest.split_once('.').ok_or(SigningError::MalformedToken)?;
    let signing_input = format!("{header}.{claims}");
    let expected =
        encode_base64_url_no_pad(&signing::hmac_sha256(secret, &[signing_input.as_bytes()]));
    if !signing::constant_time_eq(expected.as_bytes(), signature.as_bytes()) {
        return Err(SigningError::InvalidSignature);
    }
    let payload = decode_base64_url_no_pad(claims)?;
    serde_json::from_slice(&payload).map_err(SigningError::from)
}

fn encode_base64_url_no_pad(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn decode_base64_url_no_pad(value: &str) -> Result<Vec<u8>, SigningError> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| SigningError::MalformedToken)
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

    #[test]
    fn client_credentials_request_uses_expected_grant_type() {
        let request = client_credentials_request("client", "secret");
        assert_eq!(request.method, HttpMethod::Post);
        let body = request.body.expect("request should have a body");
        assert!(body.contains("grant_type=client_credentials"));
        assert!(body.contains("client_secret=secret"));
    }

    #[test]
    fn revoke_request_targets_revoke_endpoint() {
        let request = revoke_token_request("client", "token");
        assert_eq!(request.url, TWITCH_REVOKE_URL);
        assert_eq!(request.method, HttpMethod::Post);
    }

    #[test]
    fn extension_jwt_round_trips() {
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
        struct Claims {
            exp: i64,
            user_id: String,
        }

        let claims = Claims {
            exp: 1_741_515_200,
            user_id: "42".to_string(),
        };
        let token = sign_extension_jwt(b"super-secret", &claims).expect("jwt should sign");
        let verified: Claims =
            verify_extension_jwt(b"super-secret", &token).expect("jwt should verify");
        assert_eq!(verified, claims);
    }

    #[test]
    fn url_constants_match_checked_in_auth_catalog() {
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../twitch_catalog/auth.json"))
                .expect("auth.json should decode");
        let endpoints = catalog["endpoints"]
            .as_array()
            .expect("auth.json should list endpoints");

        let expected = [
            ("oauth_authorize", TWITCH_AUTHORIZE_URL),
            ("oauth_token", TWITCH_TOKEN_URL),
            ("oauth_device", TWITCH_DEVICE_CODE_URL),
            ("oauth_validate", TWITCH_VALIDATE_URL),
            ("oauth_revoke", TWITCH_REVOKE_URL),
            ("oidc_configuration", TWITCH_OPENID_CONFIGURATION_URL),
            ("oidc_keys", TWITCH_OPENID_JWKS_URL),
            ("oidc_userinfo", TWITCH_USERINFO_URL),
        ];

        assert_eq!(endpoints.len(), expected.len());
        for (id, url) in expected {
            let entry = endpoints
                .iter()
                .find(|endpoint| endpoint["id"] == id)
                .unwrap_or_else(|| panic!("auth.json should define endpoint `{id}`"));
            assert_eq!(entry["url"], url, "URL mismatch for `{id}`");
        }
    }

    #[test]
    fn token_validation_due_defaults_to_true_without_history() {
        assert!(token_validation_due(None, 1_700_000_000_000));
        assert!(!token_validation_due(
            Some(1_700_000_000_000),
            1_700_000_000_000 + 10_000
        ));
        assert!(token_validation_due(
            Some(1_700_000_000_000),
            1_700_000_000_000 + 3_700_000
        ));
    }
}
