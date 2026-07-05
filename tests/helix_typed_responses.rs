//! Decodes representative Twitch documentation example payloads through the
//! generated typed response structs.

use barbed::helix::{HelixError, RawResponse, eventsub_endpoints, streams, users};

fn response(status: u16, body: &str) -> RawResponse {
    RawResponse {
        status,
        body: body.to_string(),
    }
}

#[test]
fn get_users_response_decodes_doc_example() {
    let body = r#"{
        "data": [
            {
                "id": "141981764",
                "login": "twitchdev",
                "display_name": "TwitchDev",
                "type": "",
                "broadcaster_type": "partner",
                "description": "Supporting third-party developers building Twitch integrations.",
                "profile_image_url": "https://static-cdn.jtvnw.net/jtv_user_pictures/8a6381c7-d0c0-4576-b179-38bd5ce1d6af-profile_image-300x300.png",
                "offline_image_url": "https://static-cdn.jtvnw.net/jtv_user_pictures/3f13ab61-ec78-4fe6-8481-8682cb3b0ac2-channel_offline_image-1920x1080.png",
                "view_count": 5980557,
                "email": "not-real@email.com",
                "created_at": "2016-12-14T20:32:28Z"
            }
        ]
    }"#;

    let decoded = users::GetUsersResponse::parse(response(200, body)).expect("should decode");
    assert_eq!(decoded.data.len(), 1);
    let user = &decoded.data[0];
    assert_eq!(user.id, "141981764");
    assert_eq!(user.login, "twitchdev");
    assert_eq!(user.display_name, "TwitchDev");
    assert_eq!(user.broadcaster_type, "partner");
    assert_eq!(user.created_at, "2016-12-14T20:32:28Z");
}

#[test]
fn get_streams_response_decodes_envelope_and_pagination() {
    let body = r#"{
        "data": [
            {
                "id": "123456789",
                "user_id": "98765",
                "user_login": "sandysanderman",
                "user_name": "SandySanderman",
                "game_id": "494131",
                "game_name": "Little Nightmares",
                "type": "live",
                "title": "hablamos y le damos a Little Nightmares 1",
                "tags": ["Español"],
                "viewer_count": 78365,
                "started_at": "2021-03-10T15:04:21Z",
                "language": "es",
                "thumbnail_url": "https://static-cdn.jtvnw.net/previews-ttv/live_user_auronplay-{width}x{height}.jpg",
                "tag_ids": [],
                "is_mature": false
            }
        ],
        "pagination": {
            "cursor": "eyJiIjp7IkN1cnNvciI6ImV5SnpJam8zT0RNMk5TNDBORFF4TlRjMU1UY3hOU3dpWkNJNlptRnNjMlVzSW5RaU9uUnlkV1Y5In19"
        }
    }"#;

    let decoded = streams::GetStreamsResponse::parse(response(200, body)).expect("should decode");
    assert_eq!(decoded.data.len(), 1);
    assert_eq!(decoded.data[0].user_login, "sandysanderman");
    assert_eq!(decoded.data[0].viewer_count, 78365);
    assert!(
        decoded
            .pagination
            .cursor
            .as_deref()
            .is_some_and(|cursor| { cursor.starts_with("eyJiIjp7") })
    );
}

#[test]
fn create_eventsub_subscription_response_expects_202() {
    let body = r#"{
        "data": [
            {
                "id": "26b1c993-bfcf-44d9-b876-379dacafe75a",
                "status": "enabled",
                "type": "channel.chat.message",
                "version": "1",
                "condition": {
                    "broadcaster_user_id": "1337",
                    "user_id": "9001"
                },
                "created_at": "2020-11-10T20:08:33.12345678Z",
                "transport": {
                    "method": "websocket",
                    "session_id": "AQoQexAWVYKSTIu4ec_2VAxyuhAB"
                },
                "cost": 0
            }
        ],
        "total": 1,
        "total_cost": 0,
        "max_total_cost": 10000
    }"#;

    let decoded =
        eventsub_endpoints::CreateEventsubSubscriptionResponse::parse(response(202, body))
            .expect("should decode");
    assert_eq!(decoded.total, 1);
    assert_eq!(decoded.max_total_cost, 10000);
    assert_eq!(decoded.data[0].r#type, "channel.chat.message");
    assert_eq!(decoded.data[0].transport.method, "websocket");
    assert_eq!(
        decoded.data[0].condition["broadcaster_user_id"],
        serde_json::json!("1337")
    );

    // The same payload with a 200 status must be rejected: Twitch documents 202.
    assert!(matches!(
        eventsub_endpoints::CreateEventsubSubscriptionResponse::parse(response(200, body)),
        Err(HelixError::ApiError { status: 200, .. })
    ));
}

#[test]
fn delete_eventsub_subscription_response_is_unit_on_204() {
    eventsub_endpoints::DeleteEventsubSubscriptionResponse::parse(response(204, ""))
        .expect("204 with empty body should parse");

    assert!(matches!(
        eventsub_endpoints::DeleteEventsubSubscriptionResponse::parse(response(
            404,
            r#"{"error":"Not Found"}"#
        )),
        Err(HelixError::ApiError { status: 404, .. })
    ));
}

#[test]
fn omitted_fields_fall_back_to_defaults() {
    let decoded = users::GetUsersResponse::parse(response(200, r#"{"data":[{"id":"42"}]}"#))
        .expect("partial objects should decode via defaults");
    assert_eq!(decoded.data[0].id, "42");
    assert_eq!(decoded.data[0].login, "");
    assert_eq!(decoded.data[0].view_count, 0);

    let empty = users::GetUsersResponse::parse(response(200, "{}"))
        .expect("empty envelope should decode via defaults");
    assert!(empty.data.is_empty());
}
