//! Decodes representative EventSub notification payloads through the generated
//! typed structs via `EventSubWebSocketEnvelope::known_payload`.

use barbed::eventsub::{
    EventSubChatMessageType, EventSubMessageFragmentType, KnownEventSubPayload,
    decode_eventsub_websocket_message,
};

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

#[test]
fn chat_message_decodes_gif_fragment_reply_and_shared_chat_fields() {
    let payload = known_payload(
        "channel.chat.message",
        "1",
        serde_json::json!({
            "broadcaster_user_id": "1971641",
            "broadcaster_user_login": "streamer45",
            "broadcaster_user_name": "streamer45",
            "chatter_user_id": "4145994",
            "chatter_user_login": "viewer32",
            "chatter_user_name": "viewer32",
            "message_id": "cc106a89-1814-919d-454c-f4f2f970aae7",
            "message": {
                "text": "Hi chat [Y A Y Yes GIF by Djemilah Birnie] Cheer100",
                "fragments": [
                    { "type": "text", "text": "Hi chat ", "cheermote": null, "emote": null, "mention": null, "gif": null },
                    {
                        "type": "gif",
                        "text": "[Y A Y Yes GIF by Djemilah Birnie]",
                        "cheermote": null,
                        "emote": null,
                        "mention": null,
                        "gif": {
                            "id": "joSNxeswxuc74Juo8X",
                            "url": "https://media4.giphy.com/media/joSNxeswxuc74Juo8X/giphy.gif"
                        }
                    },
                    {
                        "type": "cheermote",
                        "text": " Cheer100",
                        "cheermote": { "prefix": "Cheer", "bits": 100, "tier": 100 },
                        "emote": null,
                        "mention": null,
                        "gif": null
                    }
                ]
            },
            "color": "#00FF7F",
            "badges": [ { "set_id": "moderator", "id": "1", "info": "" } ],
            "message_type": "text",
            "cheer": { "bits": 100 },
            "reply": {
                "parent_message_id": "b34ccfc7-4977-403a-8a94-33c6bac34fb8",
                "parent_message_body": "original message",
                "parent_user_id": "123",
                "parent_user_name": "Original",
                "parent_user_login": "original",
                "thread_message_id": "b34ccfc7-4977-403a-8a94-33c6bac34fb8",
                "thread_user_id": "123",
                "thread_user_name": "Original",
                "thread_user_login": "original"
            },
            "channel_points_custom_reward_id": null,
            "source_broadcaster_user_id": null,
            "source_broadcaster_user_login": null,
            "source_broadcaster_user_name": null,
            "source_message_id": null,
            "source_badges": null,
            "is_source_only": null
        }),
    );

    let KnownEventSubPayload::ChannelChatMessage1(message) = payload else {
        panic!("expected the hand-written chat message variant");
    };
    assert_eq!(message.message_type, EventSubChatMessageType::Text);
    assert_eq!(message.message.fragments.len(), 3);

    let gif = &message.message.fragments[1];
    assert_eq!(gif.fragment_type, EventSubMessageFragmentType::Gif);
    assert_eq!(
        gif.gif.as_ref().map(|gif| gif.id.as_str()),
        Some("joSNxeswxuc74Juo8X")
    );
    assert_eq!(
        message.message.fragments[2]
            .cheermote
            .as_ref()
            .map(|cheermote| cheermote.tier),
        Some(100)
    );
    assert_eq!(
        message
            .reply
            .as_ref()
            .map(|reply| reply.parent_user_login.as_str()),
        Some("original")
    );
    assert_eq!(message.channel_points_custom_reward_id, None);
    assert_eq!(message.source_broadcaster_user_id, None);
    assert!(
        message.source_badges.is_empty(),
        "explicit null should decode as no badges"
    );
    assert_eq!(message.is_source_only, None);
}

#[test]
fn chat_message_tolerates_gif_id_spelling_and_unknown_type_names() {
    let payload = known_payload(
        "channel.chat.message",
        "1",
        serde_json::json!({
            "broadcaster_user_id": "777",
            "chatter_user_id": "42",
            "chatter_user_login": "challenger",
            "chatter_user_name": "Challenger",
            "message_id": "chat-msg-2",
            "message_type": "some_future_message_type",
            "message": {
                "text": "[gif] ???",
                "fragments": [
                    {
                        "type": "gif",
                        "text": "[gif]",
                        "gif": { "gif_id": "abc123", "url": "https://example.invalid/abc123.gif" }
                    },
                    { "type": "hologram", "text": " ???" }
                ]
            }
        }),
    );

    let KnownEventSubPayload::ChannelChatMessage1(message) = payload else {
        panic!("expected the hand-written chat message variant");
    };
    assert_eq!(message.message_type, EventSubChatMessageType::Unknown);
    assert_eq!(
        message.message.fragments[0]
            .gif
            .as_ref()
            .map(|gif| gif.id.as_str()),
        Some("abc123")
    );
    assert_eq!(
        message.message.fragments[1].fragment_type,
        EventSubMessageFragmentType::Unknown
    );
}

#[test]
fn chat_message_delete_decodes_login_and_display_names() {
    let payload = known_payload(
        "channel.chat.message_delete",
        "1",
        serde_json::json!({
            "broadcaster_user_id": "1337",
            "broadcaster_user_name": "Cool_User",
            "broadcaster_user_login": "cool_user",
            "target_user_id": "7734",
            "target_user_name": "Uncool_viewer",
            "target_user_login": "uncool_viewer",
            "message_id": "ab24e0b0-2260-4bac-94e4-05eedd4ecd0e"
        }),
    );

    let KnownEventSubPayload::ChannelChatMessageDelete1(deleted) = payload else {
        panic!("expected the hand-written chat message delete variant");
    };
    assert_eq!(deleted.broadcaster_user_login, "cool_user");
    assert_eq!(deleted.target_user_login.as_deref(), Some("uncool_viewer"));
    assert_eq!(deleted.target_user_name.as_deref(), Some("Uncool_viewer"));
}

#[test]
fn chat_notification_decodes_gifted_drops_summary() {
    let payload = known_payload(
        "channel.chat.notification",
        "1",
        serde_json::json!({
            "broadcaster_user_id": "1971641",
            "broadcaster_user_login": "streamer45",
            "broadcaster_user_name": "streamer45",
            "chatter_user_id": "49912639",
            "chatter_user_login": "viewer23",
            "chatter_user_name": "viewer23",
            "chatter_is_anonymous": false,
            "color": "",
            "badges": [],
            "system_message": "3 viewers received a gifted Drop!",
            "message_id": "d62235c8-47ea-4cbe-8ba7-6ab8d2b4ae7d",
            "message": { "text": "", "fragments": [] },
            "notice_type": "gifted_drops_summary",
            "sub": null,
            "resub": null,
            "sub_gift": null,
            "community_sub_gift": null,
            "gift_paid_upgrade": null,
            "prime_paid_upgrade": null,
            "pay_it_forward": null,
            "raid": null,
            "unraid": null,
            "announcement": null,
            "bits_badge_tier": null,
            "charity_donation": null,
            "watch_streak": null,
            "modiversary": null,
            "gifted_drops_summary": { "recipient_count": 3 },
            "shared_chat_gifted_drops_summary": null
        }),
    );

    let KnownEventSubPayload::ChannelChatNotification1(event) = payload else {
        panic!("expected the typed chat notification variant");
    };
    assert_eq!(event.notice_type, "gifted_drops_summary");
    assert_eq!(event.gifted_drops_summary.recipient_count, 3);
    assert!(event.shared_chat_gifted_drops_summary.is_null());
}

#[test]
fn custom_power_up_redemption_resolves_shared_custom_power_up() {
    let payload = known_payload(
        "channel.custom_power_up_redemption.add",
        "1",
        serde_json::json!({
            "id": "17fa2df1-ad76-4804-bfa5-a40ef63efe63",
            "broadcaster_user_id": "1337",
            "broadcaster_user_login": "cool_user",
            "broadcaster_user_name": "Cool_User",
            "user_id": "9001",
            "user_login": "cooler_user",
            "user_name": "Cooler_User",
            "user_input": "make it rain",
            "status": "unfulfilled",
            "custom_power_up": {
                "id": "92af127c-7326-4483-a52b-b0da0be61c01",
                "title": "Rain",
                "bits": 100,
                "prompt": "Summon the rain"
            },
            "redeemed_at": "2026-05-19T17:16:03.17106713Z"
        }),
    );

    let KnownEventSubPayload::ChannelCustomPowerUpRedemptionAdd1(event) = payload else {
        panic!("expected the typed custom power-up redemption variant");
    };
    // The docs type this field by a "Custom Power Up" link; the scraper must
    // keep resolving it to the shared struct instead of `serde_json::Value`.
    assert_eq!(event.custom_power_up.title, "Rain");
    assert_eq!(event.custom_power_up.bits, 100);
}
