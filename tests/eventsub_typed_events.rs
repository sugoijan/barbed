//! Decodes representative EventSub notification payloads through the generated
//! typed structs via `EventSubWebSocketEnvelope::known_payload`.

use barbed::eventsub::{KnownEventSubPayload, decode_eventsub_websocket_message};

fn envelope_json(subscription_type: &str, version: &str, event: serde_json::Value) -> String {
    serde_json::json!({
        "metadata": {
            "message_id": "msg-1",
            "message_type": "notification",
            "message_timestamp": "2026-07-06T00:00:00Z",
            "subscription_type": subscription_type,
            "subscription_version": version
        },
        "payload": {
            "subscription": {
                "id": "sub-1",
                "type": subscription_type,
                "version": version,
                "condition": {},
                "transport": { "method": "websocket", "session_id": "session-1" }
            },
            "event": event
        }
    })
    .to_string()
}

fn known_payload(
    subscription_type: &str,
    version: &str,
    event: serde_json::Value,
) -> KnownEventSubPayload {
    let envelope =
        decode_eventsub_websocket_message(&envelope_json(subscription_type, version, event))
            .expect("envelope should decode");
    envelope
        .known_payload()
        .expect("payload should decode to a known typed variant")
}

#[test]
fn custom_reward_redemption_resolves_shared_reward() {
    let payload = known_payload(
        "channel.channel_points_custom_reward_redemption.add",
        "1",
        serde_json::json!({
            "id": "17fa2df1-ad76-4804-bfa5-a40ef63efe63",
            "broadcaster_user_id": "1337",
            "broadcaster_user_login": "cool_user",
            "broadcaster_user_name": "Cool_User",
            "user_id": "9001",
            "user_login": "cooler_user",
            "user_name": "Cooler_User",
            "user_input": "pogchamp",
            "status": "unfulfilled",
            "reward": {
                "id": "92af127c-7326-4483-a52b-b0da0be61c01",
                "title": "title",
                "cost": 100,
                "prompt": "reward prompt"
            },
            "redeemed_at": "2020-07-15T17:16:03.17106713Z"
        }),
    );

    let KnownEventSubPayload::ChannelChannelPointsCustomRewardRedemptionAdd1(event) = payload
    else {
        panic!("expected the typed custom reward redemption variant");
    };
    assert_eq!(event.user_login, "cooler_user");
    assert_eq!(event.reward.title, "title");
    assert_eq!(event.reward.cost, 100);
    assert!(
        event
            .source_timestamp
            .is_some_and(|ts| ts.unix_timestamp() > 0),
        "source_timestamp should be stamped from the envelope"
    );
}

#[test]
fn goal_begin_treats_explicit_null_as_default() {
    let payload = known_payload(
        "channel.goal.begin",
        "1",
        serde_json::json!({
            "id": "12345-cool-event",
            "broadcaster_user_id": "141981764",
            "broadcaster_user_name": "TwitchDev",
            "broadcaster_user_login": "twitchdev",
            "type": "follower",
            "description": "Follow me!",
            "current_amount": 100,
            "target_amount": 220,
            "started_at": "2021-07-15T17:16:03.17106713Z",
            "ended_at": null
        }),
    );

    let KnownEventSubPayload::ChannelGoalBegin1(event) = payload else {
        panic!("expected the typed goal begin variant");
    };
    assert_eq!(event.r#type, "follower");
    assert_eq!(event.current_amount, 100);
    assert_eq!(event.target_amount, 220);
    assert_eq!(event.ended_at, "", "explicit null should decode as default");
}

#[test]
fn automod_message_hold_v2_decodes_nested_message() {
    let payload = known_payload(
        "automod.message.hold",
        "2",
        serde_json::json!({
            "broadcaster_user_id": "1337",
            "broadcaster_user_login": "cool_user",
            "broadcaster_user_name": "Cool_User",
            "user_id": "9001",
            "user_login": "chatter",
            "user_name": "Chatter",
            "message_id": "message-id-123",
            "message": {
                "text": "wow, what a message",
                "fragments": [
                    { "type": "text", "text": "wow, what a message" }
                ]
            },
            "reason": "automod",
            "held_at": "2024-01-01T00:00:00Z"
        }),
    );

    let KnownEventSubPayload::AutomodMessageHold2(event) = payload else {
        panic!("expected the typed automod message hold v2 variant");
    };
    assert_eq!(event.message.text, "wow, what a message");
    assert_eq!(event.message.fragments.len(), 1);
}

#[test]
fn poll_begin_decodes_shared_choices_and_voting() {
    let payload = known_payload(
        "channel.poll.begin",
        "1",
        serde_json::json!({
            "id": "1243456",
            "broadcaster_user_id": "1337",
            "broadcaster_user_login": "cool_user",
            "broadcaster_user_name": "Cool_User",
            "title": "Aren't shoes just really hard socks?",
            "choices": [
                { "id": "123", "title": "Yeah!", "bits_votes": 0, "channel_points_votes": 0, "votes": 0 },
                { "id": "124", "title": "No!", "bits_votes": 0, "channel_points_votes": 0, "votes": 0 }
            ],
            "bits_voting": { "is_enabled": true, "amount_per_vote": 10 },
            "channel_points_voting": { "is_enabled": true, "amount_per_vote": 10 },
            "started_at": "2020-07-15T17:16:03.17106713Z",
            "ends_at": "2020-07-15T17:21:03.17106713Z"
        }),
    );

    let KnownEventSubPayload::ChannelPollBegin1(event) = payload else {
        panic!("expected the typed poll begin variant");
    };
    assert_eq!(event.choices.len(), 2);
    assert_eq!(event.choices[0].title, "Yeah!");
    assert!(event.bits_voting.is_enabled);
}

#[test]
fn chat_message_known_payload_is_stamped() {
    let envelope =
        decode_eventsub_websocket_message(include_str!("fixtures/eventsub_ws_notification.json"))
            .expect("chat notification fixture should decode");

    let payload = envelope
        .known_payload()
        .expect("chat message should decode to a known payload");
    let KnownEventSubPayload::ChannelChatMessage1(message) = payload else {
        panic!("expected the hand-written chat message variant");
    };
    assert_eq!(message.source_timestamp, envelope.message_timestamp());
    assert!(message.source_timestamp.is_some());
}

#[test]
fn typed_payload_round_trips_modulo_source_timestamp() {
    let original = known_payload(
        "channel.goal.begin",
        "1",
        serde_json::json!({
            "id": "12345",
            "broadcaster_user_id": "1",
            "broadcaster_user_name": "b",
            "broadcaster_user_login": "b",
            "type": "follower",
            "description": "d",
            "current_amount": 1,
            "target_amount": 2,
            "started_at": "2021-07-15T17:16:03Z"
        }),
    );

    let serialized = serde_json::to_string(&original).expect("should serialize");
    let mut round_tripped: KnownEventSubPayload =
        serde_json::from_str(&serialized).expect("should deserialize");
    if let (
        KnownEventSubPayload::ChannelGoalBegin1(restored),
        KnownEventSubPayload::ChannelGoalBegin1(original_event),
    ) = (&mut round_tripped, &original)
    {
        assert_eq!(
            restored.source_timestamp, None,
            "serde(skip) drops the stamp"
        );
        restored.source_timestamp = original_event.source_timestamp;
    } else {
        panic!("expected goal begin variants");
    }
    assert_eq!(round_tripped, original);
}
