use serde::{Deserialize, Serialize};
use thiserror::Error;

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
}

impl EventSubMessageType {
    pub fn as_status_label(self) -> &'static str {
        match self {
            Self::Notification => "notification",
            Self::Revocation => "revocation",
            Self::SessionWelcome => "session_welcome",
            Self::SessionKeepalive => "session_keepalive",
            Self::SessionReconnect => "session_reconnect",
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

impl EventSubWebSocketEnvelope {
    pub fn message_type(&self) -> Result<EventSubMessageType, EventSubError> {
        parse_message_type(&self.metadata.message_type)
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
        let subscription_type = self
            .subscription()
            .map(|subscription| subscription.subscription_type.as_str())
            .or(self.metadata.subscription_type.as_deref());
        if subscription_type != Some("channel.chat.message") {
            return None;
        }
        let event = self.payload.event.clone()?;
        serde_json::from_value(event).ok()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubChatMessageText {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubChatMessage {
    pub broadcaster_user_id: String,
    pub chatter_user_id: String,
    pub chatter_user_login: String,
    pub chatter_user_name: String,
    pub message_id: String,
    pub message: EventSubChatMessageText,
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

pub fn chat_message_subscription_request(
    broadcaster_user_id: &str,
    session_id: &str,
) -> CreateEventSubSubscriptionRequest {
    CreateEventSubSubscriptionRequest {
        subscription_type: "channel.chat.message".to_string(),
        version: "1".to_string(),
        condition: EventSubCondition {
            broadcaster_user_id: Some(broadcaster_user_id.to_string()),
            moderator_user_id: None,
            user_id: Some(broadcaster_user_id.to_string()),
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
            envelope
                .session()
                .and_then(|session| session.reconnect_url.as_deref()),
            Some("wss://eventsub.wss.twitch.tv/ws?reconnect=abc123")
        );
    }

    #[test]
    fn chat_subscription_request_uses_websocket_transport() {
        let request = chat_message_subscription_request("777", "AQoSession");

        assert_eq!(request.subscription_type, "channel.chat.message");
        assert_eq!(request.version, "1");
        assert_eq!(
            request.condition.broadcaster_user_id.as_deref(),
            Some("777")
        );
        assert_eq!(request.condition.user_id.as_deref(), Some("777"));
        assert_eq!(request.transport.method.as_deref(), Some("websocket"));
        assert_eq!(request.transport.session_id.as_deref(), Some("AQoSession"));
    }
}
