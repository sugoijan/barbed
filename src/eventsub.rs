use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub const CHANNEL_CHAT_MESSAGE: &str = "channel.chat.message";
pub const CHANNEL_CHAT_MESSAGE_DELETE: &str = "channel.chat.message_delete";

#[derive(Debug, Error)]
pub enum EventSubError {
    #[error("unsupported eventsub message type: {0}")]
    UnsupportedMessageType(String),
    #[error("eventsub payload failed to decode: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventSubMessageType {
    Notification,
    Revocation,
    SessionWelcome,
    SessionKeepalive,
    SessionReconnect,
    SessionDisconnect,
}

impl EventSubMessageType {
    pub fn as_status_label(self) -> &'static str {
        match self {
            Self::Notification => "notification",
            Self::Revocation => "revocation",
            Self::SessionWelcome => "session_welcome",
            Self::SessionKeepalive => "session_keepalive",
            Self::SessionReconnect => "session_reconnect",
            Self::SessionDisconnect => "session_disconnect",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubCondition {
    #[serde(default)]
    pub broadcaster_user_id: Option<String>,
    #[serde(default)]
    pub moderator_user_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubTransport {
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubSubscription {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub subscription_type: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub condition: EventSubCondition,
    #[serde(default)]
    pub transport: Option<EventSubTransport>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubWebSocketSession {
    pub id: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub connected_at: Option<String>,
    #[serde(default)]
    pub keepalive_timeout_seconds: Option<u32>,
    #[serde(default)]
    pub reconnect_url: Option<String>,
    #[serde(default)]
    pub recovery_url: Option<String>,
    #[serde(default)]
    pub disconnect_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubMessageMetadata {
    pub message_id: String,
    pub message_type: String,
    pub message_timestamp: String,
    #[serde(default)]
    pub subscription_type: Option<String>,
    #[serde(default)]
    pub subscription_version: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EventSubPayload {
    #[serde(default)]
    pub session: Option<EventSubWebSocketSession>,
    #[serde(default)]
    pub subscription: Option<EventSubSubscription>,
    #[serde(default)]
    pub event: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventSubWebSocketEnvelope {
    pub metadata: EventSubMessageMetadata,
    pub payload: EventSubPayload,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubChatBadge {
    pub set_id: String,
    pub id: String,
    #[serde(default)]
    pub info: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubCheer {
    pub bits: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubCheermote {
    #[serde(default)]
    pub prefix: String,
    pub bits: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubMessageMention {
    pub user_id: String,
    pub user_name: String,
    pub user_login: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubMessageEmote {
    pub id: String,
    #[serde(default)]
    pub emote_set_id: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub format: Vec<String>,
    #[serde(default)]
    pub scale: Vec<String>,
    #[serde(default, rename = "theme_mode")]
    pub theme_modes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSubMessageFragmentType {
    Text,
    Emote,
    Mention,
    Cheermote,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubMessageFragment {
    #[serde(rename = "type")]
    pub fragment_type: EventSubMessageFragmentType,
    pub text: String,
    #[serde(default)]
    pub emote: Option<EventSubMessageEmote>,
    #[serde(default)]
    pub mention: Option<EventSubMessageMention>,
    #[serde(default)]
    pub cheermote: Option<EventSubCheermote>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubChatMessageText {
    pub text: String,
    #[serde(default)]
    pub fragments: Vec<EventSubMessageFragment>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubChatMessage {
    #[serde(default)]
    pub broadcaster_user_id: String,
    #[serde(default)]
    pub broadcaster_user_login: String,
    #[serde(default)]
    pub broadcaster_user_name: String,
    #[serde(default)]
    pub chatter_user_id: String,
    #[serde(default)]
    pub chatter_user_login: String,
    #[serde(default)]
    pub chatter_user_name: String,
    #[serde(default)]
    pub message_id: String,
    pub message: EventSubChatMessageText,
    #[serde(default)]
    pub cheer: Option<EventSubCheer>,
    #[serde(default)]
    pub badges: Vec<EventSubChatBadge>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub source_timestamp: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubChatMessageDeleted {
    #[serde(default)]
    pub broadcaster_user_id: String,
    #[serde(default)]
    pub target_user_id: Option<String>,
    #[serde(default)]
    pub message_id: String,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub source_timestamp: Option<OffsetDateTime>,
}

// Keep payloads inline to avoid forcing allocations in the public stream API.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventSubStreamEvent {
    ChatMessage(EventSubChatMessage),
    MessageDeleted(EventSubChatMessageDeleted),
    Keepalive,
    SessionReconnect {
        reconnect_url: String,
    },
    SessionDisconnect {
        status: String,
        reason: Option<String>,
    },
    Revocation {
        status: Option<String>,
        reason: Option<String>,
    },
}

trait HasSourceTimestamp {
    fn set_source_timestamp(&mut self, ts: Option<OffsetDateTime>);
}

impl HasSourceTimestamp for EventSubChatMessage {
    fn set_source_timestamp(&mut self, ts: Option<OffsetDateTime>) {
        self.source_timestamp = ts;
    }
}

impl HasSourceTimestamp for EventSubChatMessageDeleted {
    fn set_source_timestamp(&mut self, ts: Option<OffsetDateTime>) {
        self.source_timestamp = ts;
    }
}

impl EventSubWebSocketEnvelope {
    pub fn message_type(&self) -> Result<EventSubMessageType, EventSubError> {
        parse_message_type(&self.metadata.message_type)
    }

    pub fn message_timestamp(&self) -> Option<OffsetDateTime> {
        OffsetDateTime::parse(&self.metadata.message_timestamp, &Rfc3339).ok()
    }

    pub fn session(&self) -> Option<&EventSubWebSocketSession> {
        self.payload.session.as_ref()
    }

    pub fn subscription(&self) -> Option<&EventSubSubscription> {
        self.payload.subscription.as_ref()
    }

    pub fn broadcaster_user_id(&self) -> Option<&str> {
        self.subscription()?
            .condition
            .broadcaster_user_id
            .as_deref()
    }

    pub fn chat_message(&self) -> Option<EventSubChatMessage> {
        if self.subscription_type_str() != Some(CHANNEL_CHAT_MESSAGE) {
            return None;
        }
        self.deserialize_event()
    }

    pub fn chat_message_deleted(&self) -> Option<EventSubChatMessageDeleted> {
        if self.subscription_type_str() != Some(CHANNEL_CHAT_MESSAGE_DELETE) {
            return None;
        }
        self.deserialize_event()
    }

    fn deserialize_event<T: serde::de::DeserializeOwned + HasSourceTimestamp>(&self) -> Option<T> {
        let event = self.payload.event.clone()?;
        let mut value: T = serde_json::from_value(event).ok()?;
        value.set_source_timestamp(self.message_timestamp());
        Some(value)
    }

    fn subscription_type_str(&self) -> Option<&str> {
        self.subscription()
            .map(|s| s.subscription_type.as_str())
            .or(self.metadata.subscription_type.as_deref())
    }

    pub fn stream_event(&self) -> Result<Option<EventSubStreamEvent>, EventSubError> {
        let event = match self.message_type()? {
            EventSubMessageType::Notification => match self.subscription_type_str() {
                Some(CHANNEL_CHAT_MESSAGE) => self
                    .deserialize_event::<EventSubChatMessage>()
                    .map(EventSubStreamEvent::ChatMessage),
                Some(CHANNEL_CHAT_MESSAGE_DELETE) => self
                    .deserialize_event::<EventSubChatMessageDeleted>()
                    .map(EventSubStreamEvent::MessageDeleted),
                _ => None,
            },
            EventSubMessageType::Revocation => Some(EventSubStreamEvent::Revocation {
                status: self
                    .subscription()
                    .and_then(|subscription| subscription.status.clone()),
                reason: None,
            }),
            EventSubMessageType::SessionWelcome => None,
            EventSubMessageType::SessionKeepalive => Some(EventSubStreamEvent::Keepalive),
            EventSubMessageType::SessionReconnect => self
                .session()
                .and_then(|session| session.reconnect_url.clone())
                .map(|reconnect_url| EventSubStreamEvent::SessionReconnect { reconnect_url }),
            EventSubMessageType::SessionDisconnect => {
                Some(EventSubStreamEvent::SessionDisconnect {
                    status: self
                        .session()
                        .and_then(|session| session.status.clone())
                        .unwrap_or_else(|| "disconnected".to_string()),
                    reason: self
                        .session()
                        .and_then(|session| session.disconnect_reason.clone()),
                })
            }
        };
        Ok(event)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEventSubSubscriptionRequest {
    #[serde(rename = "type")]
    pub subscription_type: String,
    pub version: String,
    pub condition: EventSubCondition,
    pub transport: EventSubTransport,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEventSubSubscriptionResponse {
    #[serde(default)]
    pub data: Vec<EventSubSubscription>,
}

pub fn decode_eventsub_websocket_message(
    raw_body: &str,
) -> Result<EventSubWebSocketEnvelope, EventSubError> {
    serde_json::from_str(raw_body).map_err(EventSubError::from)
}

pub fn channel_chat_message_subscription_request(
    broadcaster_user_id: &str,
    user_id: &str,
    session_id: &str,
) -> CreateEventSubSubscriptionRequest {
    channel_chat_subscription_request(
        CHANNEL_CHAT_MESSAGE,
        broadcaster_user_id,
        user_id,
        session_id,
    )
}

pub fn channel_chat_message_delete_subscription_request(
    broadcaster_user_id: &str,
    user_id: &str,
    session_id: &str,
) -> CreateEventSubSubscriptionRequest {
    channel_chat_subscription_request(
        CHANNEL_CHAT_MESSAGE_DELETE,
        broadcaster_user_id,
        user_id,
        session_id,
    )
}

pub fn chat_message_subscription_request(
    broadcaster_user_id: &str,
    session_id: &str,
) -> CreateEventSubSubscriptionRequest {
    channel_chat_message_subscription_request(broadcaster_user_id, broadcaster_user_id, session_id)
}

pub fn chat_message_delete_subscription_request(
    broadcaster_user_id: &str,
    session_id: &str,
) -> CreateEventSubSubscriptionRequest {
    channel_chat_message_delete_subscription_request(
        broadcaster_user_id,
        broadcaster_user_id,
        session_id,
    )
}

fn channel_chat_subscription_request(
    subscription_type: &str,
    broadcaster_user_id: &str,
    user_id: &str,
    session_id: &str,
) -> CreateEventSubSubscriptionRequest {
    CreateEventSubSubscriptionRequest {
        subscription_type: subscription_type.to_string(),
        version: "1".to_string(),
        condition: EventSubCondition {
            broadcaster_user_id: Some(broadcaster_user_id.to_string()),
            moderator_user_id: None,
            user_id: Some(user_id.to_string()),
        },
        transport: EventSubTransport {
            method: Some("websocket".to_string()),
            session_id: Some(session_id.to_string()),
        },
    }
}

fn parse_message_type(value: &str) -> Result<EventSubMessageType, EventSubError> {
    match value {
        "notification" => Ok(EventSubMessageType::Notification),
        "revocation" => Ok(EventSubMessageType::Revocation),
        "session_welcome" => Ok(EventSubMessageType::SessionWelcome),
        "session_keepalive" => Ok(EventSubMessageType::SessionKeepalive),
        "session_reconnect" => Ok(EventSubMessageType::SessionReconnect),
        "session_disconnect" => Ok(EventSubMessageType::SessionDisconnect),
        other => Err(EventSubError::UnsupportedMessageType(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_fixture_exposes_session_metadata() {
        let envelope = decode_eventsub_websocket_message(include_str!(
            "../tests/fixtures/eventsub_ws_welcome.json"
        ))
        .expect("welcome fixture should decode");

        assert_eq!(
            envelope.message_type().expect("message type should parse"),
            EventSubMessageType::SessionWelcome
        );
        assert_eq!(
            envelope
                .session()
                .and_then(|session| session.keepalive_timeout_seconds),
            Some(30)
        );
        assert_eq!(
            envelope.session().map(|session| session.id.as_str()),
            Some("AQoQexAWVYKSTIu4ec_2VAxyuhAB")
        );
    }

    #[test]
    fn notification_fixture_yields_chat_message() {
        let envelope = decode_eventsub_websocket_message(include_str!(
            "../tests/fixtures/eventsub_ws_notification.json"
        ))
        .expect("notification fixture should decode");

        assert_eq!(
            envelope.message_type().expect("message type should parse"),
            EventSubMessageType::Notification
        );
        let chat_message = envelope
            .chat_message()
            .expect("chat message should be present");
        assert_eq!(chat_message.message.text, "!play");
        assert_eq!(chat_message.chatter_user_login, "challenger");
        assert_eq!(chat_message.source_timestamp, envelope.message_timestamp());
        assert_eq!(envelope.broadcaster_user_id(), Some("777"));
    }

    #[test]
    fn reconnect_fixture_exposes_reconnect_url() {
        let envelope = decode_eventsub_websocket_message(include_str!(
            "../tests/fixtures/eventsub_ws_reconnect.json"
        ))
        .expect("reconnect fixture should decode");

        assert_eq!(
            envelope.message_type().expect("message type should parse"),
            EventSubMessageType::SessionReconnect
        );
        assert_eq!(
            envelope.stream_event().expect("stream event should decode"),
            Some(EventSubStreamEvent::SessionReconnect {
                reconnect_url: "wss://eventsub.wss.twitch.tv/ws?reconnect=abc123".to_string()
            })
        );
    }

    #[test]
    fn delete_subscription_builder_uses_requested_user_id() {
        let request = channel_chat_message_delete_subscription_request("777", "42", "session-123");
        assert_eq!(request.subscription_type, "channel.chat.message_delete");
        assert_eq!(
            request.condition.broadcaster_user_id.as_deref(),
            Some("777")
        );
        assert_eq!(request.condition.user_id.as_deref(), Some("42"));
        assert_eq!(request.transport.session_id.as_deref(), Some("session-123"));
    }
}
