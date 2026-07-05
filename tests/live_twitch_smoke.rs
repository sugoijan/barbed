#![cfg(feature = "reqwest-client")]

use std::env;

use barbed::native::{TwitchAuthClient, validate_access_token};
use barbed::oauth::TwitchAuthConfig;

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("missing required env var: {name}"))
}

#[tokio::test]
#[ignore = "requires TWITCH_TEST_CLIENT_ID and TWITCH_TEST_CLIENT_SECRET"]
async fn app_token_and_openid_configuration_smoke() {
    let client_id = required_env("TWITCH_TEST_CLIENT_ID");
    let client_secret = required_env("TWITCH_TEST_CLIENT_SECRET");
    let auth_client = TwitchAuthClient::new(TwitchAuthConfig::new(client_id.clone()))
        .expect("client should build");

    let configuration = auth_client
        .fetch_openid_configuration()
        .await
        .expect("openid configuration should load");
    assert_eq!(configuration.issuer, "https://id.twitch.tv/oauth2");
    assert_eq!(
        configuration.userinfo_endpoint,
        "https://id.twitch.tv/oauth2/userinfo"
    );

    let app_token = auth_client
        .request_app_access_token(&client_secret)
        .await
        .expect("app token should be issued");
    assert_eq!(app_token.token_type.as_deref(), Some("bearer"));

    let http = reqwest::Client::new();
    let validation = validate_access_token(&http, &app_token.access_token)
        .await
        .expect("validation request should succeed")
        .expect("fresh app token should validate");
    assert_eq!(validation.client_id, client_id);
}
