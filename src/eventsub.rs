use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use time::{Duration, error::Parse as TimeParseError};

use crate::emotes::{
    Emote, EmoteId, EmoteImage, EmoteImageFormat, EmoteImageScale, EmoteProvider, EmoteThemeMode,
};
pub use crate::helix::EndpointStability;

pub const CHANNEL_CHAT_MESSAGE: &str = "channel.chat.message";
pub const CHANNEL_CHAT_MESSAGE_DELETE: &str = "channel.chat.message_delete";

#[derive(Debug, Error)]
pub enum EventSubError {
    #[error("unsupported eventsub message type: {0}")]
    UnsupportedMessageType(String),
    #[error("eventsub payload failed to decode: {0}")]
    Json(#[from] serde_json::Error),
    #[error("eventsub webhook headers are incomplete")]
    MissingWebhookHeaders,
    #[error("eventsub webhook signature did not match")]
    InvalidWebhookSignature,
    #[error("eventsub webhook timestamp is stale")]
    StaleWebhookTimestamp,
    #[error("eventsub webhook message was already processed")]
    DuplicateWebhookMessage,
    #[error("eventsub webhook timestamp failed to parse: {0}")]
    Timestamp(#[from] TimeParseError),
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
    #[serde(flatten)]
    pub extra: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubTransport {
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub callback: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub conduit_id: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, String>,
}

impl EventSubTransport {
    pub fn websocket(session_id: impl Into<String>) -> Self {
        Self {
            method: Some("websocket".to_string()),
            session_id: Some(session_id.into()),
            ..Self::default()
        }
    }

    pub fn webhook(callback: impl Into<String>, secret: impl Into<String>) -> Self {
        Self {
            method: Some("webhook".to_string()),
            callback: Some(callback.into()),
            secret: Some(secret.into()),
            ..Self::default()
        }
    }

    pub fn conduit(conduit_id: impl Into<String>) -> Self {
        Self {
            method: Some("conduit".to_string()),
            conduit_id: Some(conduit_id.into()),
            ..Self::default()
        }
    }
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
pub struct EventSubWebhookEnvelope {
    #[serde(default)]
    pub challenge: Option<String>,
    pub subscription: EventSubSubscription,
    #[serde(default)]
    pub event: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventSubWebhookMessageType {
    Notification,
    Verification,
    Revocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventSubWebhookHeaders {
    pub message_id: String,
    pub message_type: EventSubWebhookMessageType,
    pub message_timestamp: String,
    pub message_signature: String,
    pub subscription_type: Option<String>,
    pub subscription_version: Option<String>,
    pub message_retry: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubSubscriptionDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub subscription_type: &'static str,
    pub version: &'static str,
    pub stability: EndpointStability,
    pub description: &'static str,
}

pub trait EventSubReplayStore: Send + Sync {
    fn remember_message(
        &self,
        message_id: &str,
        seen_at: OffsetDateTime,
    ) -> Result<bool, EventSubError>;
}

/// Replay store that forgets message ids once they fall outside its retention
/// window, so memory stays bounded on long-running webhook servers.
#[derive(Clone)]
pub struct InMemoryEventSubReplayStore {
    state: Arc<Mutex<ReplayState>>,
    retention: Duration,
}

#[derive(Default)]
struct ReplayState {
    seen: HashSet<String>,
    /// Ids in arrival order with their `seen_at`; expired ids leave from the front.
    order: VecDeque<(OffsetDateTime, String)>,
}

impl Default for InMemoryEventSubReplayStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryEventSubReplayStore {
    /// Twitch tells receivers to reject notifications older than ten minutes,
    /// so ids are remembered for that long by default.
    pub const DEFAULT_RETENTION: Duration = Duration::minutes(10);

    pub fn new() -> Self {
        Self::with_retention(Self::DEFAULT_RETENTION)
    }

    /// `retention` must be at least the `max_age` given to
    /// [`verify_and_decode_webhook_message`]; a replay older than the retention
    /// window but still within `max_age` would otherwise pass.
    pub fn with_retention(retention: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(ReplayState::default())),
            retention,
        }
    }
}

impl EventSubReplayStore for InMemoryEventSubReplayStore {
    fn remember_message(
        &self,
        message_id: &str,
        seen_at: OffsetDateTime,
    ) -> Result<bool, EventSubError> {
        let mut state = self
            .state
            .lock()
            .expect("in-memory EventSub replay store lock poisoned");
        if let Some(cutoff) = seen_at.checked_sub(self.retention) {
            while let Some((at, _)) = state.order.front()
                && *at < cutoff
            {
                let (_, expired) = state.order.pop_front().expect("front was just observed");
                state.seen.remove(&expired);
            }
        }
        if !state.seen.insert(message_id.to_string()) {
            return Ok(false);
        }
        state.order.push_back((seen_at, message_id.to_string()));
        Ok(true)
    }
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
    #[serde(default)]
    pub tier: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubMessageMention {
    pub user_id: String,
    pub user_name: String,
    pub user_login: String,
}

/// Metadata of a `gif` message fragment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubMessageGif {
    /// The reference table names this `id` while the change log announced it
    /// as `gif_id`; both spellings decode.
    #[serde(alias = "gif_id")]
    pub id: String,
    /// Full asset URL; Twitch requires it to be used unmodified.
    pub url: String,
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

impl EventSubMessageEmote {
    pub fn to_emote(&self, code: impl Into<String>) -> Emote {
        let formats = parse_or(
            &self.format,
            &[EmoteImageFormat::Static],
            EmoteImageFormat::parse,
        );
        let themes = parse_or(
            &self.theme_modes,
            &[EmoteThemeMode::Light, EmoteThemeMode::Dark],
            EmoteThemeMode::parse,
        );
        let scales = parse_or(&self.scale, &[EmoteImageScale::One], EmoteImageScale::parse);
        let (themes, scales) = (&themes, &scales);

        let images = formats
            .iter()
            .flat_map(|format| {
                themes.iter().flat_map(move |theme| {
                    scales.iter().map(move |scale| {
                        EmoteImage::new(
                            format.clone(),
                            scale.clone(),
                            format!(
                                "https://static-cdn.jtvnw.net/emoticons/v2/{}/{}/{}/{}",
                                self.id,
                                format.as_str(),
                                theme.as_str(),
                                scale.as_str()
                            ),
                        )
                        .with_theme(theme.clone())
                    })
                })
            })
            .collect();

        Emote::new(
            EmoteId::new(EmoteProvider::Twitch, self.id.clone()),
            code,
            images,
        )
    }
}

/// Parses each value, or returns `default` when Twitch omitted the list.
fn parse_or<T: Clone>(values: &[String], default: &[T], parse: fn(&str) -> T) -> Vec<T> {
    if values.is_empty() {
        default.to_vec()
    } else {
        values.iter().map(|value| parse(value)).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSubMessageFragmentType {
    Text,
    Emote,
    Mention,
    Cheermote,
    Gif,
    /// Any fragment type this crate does not know yet.
    #[serde(other)]
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
    #[serde(default)]
    pub gif: Option<EventSubMessageGif>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubChatMessageText {
    pub text: String,
    #[serde(default)]
    pub fragments: Vec<EventSubMessageFragment>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSubChatMessageType {
    #[default]
    Text,
    ChannelPointsHighlighted,
    ChannelPointsSubOnly,
    UserIntro,
    PowerUpsMessageEffect,
    PowerUpsGigantifiedEmote,
    /// Any message type this crate does not know yet.
    #[serde(other)]
    Unknown,
}

/// Reply metadata of a chat message; present only when the message replies to
/// another one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubChatReply {
    #[serde(default)]
    pub parent_message_id: String,
    #[serde(default)]
    pub parent_message_body: String,
    #[serde(default)]
    pub parent_user_id: String,
    #[serde(default)]
    pub parent_user_name: String,
    #[serde(default)]
    pub parent_user_login: String,
    #[serde(default)]
    pub thread_message_id: String,
    #[serde(default)]
    pub thread_user_id: String,
    #[serde(default)]
    pub thread_user_name: String,
    #[serde(default)]
    pub thread_user_login: String,
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
    pub message_type: EventSubChatMessageType,
    #[serde(default)]
    pub cheer: Option<EventSubCheer>,
    #[serde(default)]
    pub badges: Vec<EventSubChatBadge>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub reply: Option<EventSubChatReply>,
    #[serde(default)]
    pub channel_points_custom_reward_id: Option<String>,
    /// Shared chat only: the channel the message was sent from when it is not
    /// the broadcaster's own. The `source_*` fields are `None` otherwise.
    #[serde(default)]
    pub source_broadcaster_user_id: Option<String>,
    #[serde(default)]
    pub source_broadcaster_user_login: Option<String>,
    #[serde(default)]
    pub source_broadcaster_user_name: Option<String>,
    #[serde(default)]
    pub source_message_id: Option<String>,
    /// Twitch sends `null` outside shared chat; that decodes as an empty list.
    #[serde(default, deserialize_with = "null_default")]
    pub source_badges: Vec<EventSubChatBadge>,
    #[serde(default)]
    pub is_source_only: Option<bool>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub source_timestamp: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSubChatMessageDeleted {
    #[serde(default)]
    pub broadcaster_user_id: String,
    #[serde(default)]
    pub broadcaster_user_login: String,
    #[serde(default)]
    pub broadcaster_user_name: String,
    #[serde(default)]
    pub target_user_id: Option<String>,
    #[serde(default)]
    pub target_user_login: Option<String>,
    #[serde(default)]
    pub target_user_name: Option<String>,
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

/// Treats an explicit JSON `null` like an absent field; EventSub payloads use
/// `null` liberally for optional values.
pub(crate) fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
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
        generated::decode_typed_event(self.payload.event.as_ref(), self.message_timestamp())
    }

    fn subscription_type_str(&self) -> Option<&str> {
        self.subscription()
            .map(|s| s.subscription_type.as_str())
            .or(self.metadata.subscription_type.as_deref())
    }

    pub fn known_payload(&self) -> Option<KnownEventSubPayload> {
        generated::decode_known_payload(
            self.subscription_type_str()?,
            self.subscription()
                .and_then(|subscription| subscription.version.as_deref())
                .or(self.metadata.subscription_version.as_deref()),
            self.payload.event.as_ref(),
            self.message_timestamp(),
        )
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

impl EventSubWebhookEnvelope {
    pub fn known_payload(
        &self,
        source_timestamp: Option<OffsetDateTime>,
    ) -> Option<KnownEventSubPayload> {
        generated::decode_known_payload(
            &self.subscription.subscription_type,
            self.subscription.version.as_deref(),
            self.event.as_ref(),
            source_timestamp,
        )
    }
}

impl EventSubWebhookHeaders {
    pub fn from_pairs<'a, I>(pairs: I) -> Result<Self, EventSubError>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut message_id = None;
        let mut message_type = None;
        let mut message_timestamp = None;
        let mut message_signature = None;
        let mut subscription_type = None;
        let mut subscription_version = None;
        let mut message_retry = None;

        for (name, value) in pairs {
            if name.eq_ignore_ascii_case("Twitch-Eventsub-Message-Id") {
                message_id = Some(value.to_string());
            } else if name.eq_ignore_ascii_case("Twitch-Eventsub-Message-Type") {
                message_type = Some(parse_webhook_message_type(value)?);
            } else if name.eq_ignore_ascii_case("Twitch-Eventsub-Message-Timestamp") {
                message_timestamp = Some(value.to_string());
            } else if name.eq_ignore_ascii_case("Twitch-Eventsub-Message-Signature") {
                message_signature = Some(value.to_string());
            } else if name.eq_ignore_ascii_case("Twitch-Eventsub-Subscription-Type") {
                subscription_type = Some(value.to_string());
            } else if name.eq_ignore_ascii_case("Twitch-Eventsub-Subscription-Version") {
                subscription_version = Some(value.to_string());
            } else if name.eq_ignore_ascii_case("Twitch-Eventsub-Message-Retry") {
                message_retry = Some(value.to_string());
            }
        }

        Ok(Self {
            message_id: message_id.ok_or(EventSubError::MissingWebhookHeaders)?,
            message_type: message_type.ok_or(EventSubError::MissingWebhookHeaders)?,
            message_timestamp: message_timestamp.ok_or(EventSubError::MissingWebhookHeaders)?,
            message_signature: message_signature.ok_or(EventSubError::MissingWebhookHeaders)?,
            subscription_type,
            subscription_version,
            message_retry,
        })
    }

    pub fn message_timestamp(&self) -> Result<OffsetDateTime, EventSubError> {
        OffsetDateTime::parse(&self.message_timestamp, &Rfc3339).map_err(EventSubError::from)
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GenericCreateEventSubSubscriptionRequest {
    #[serde(rename = "type")]
    pub subscription_type: String,
    pub version: String,
    pub condition: serde_json::Value,
    pub transport: EventSubTransport,
}

pub fn decode_eventsub_websocket_message(
    raw_body: &str,
) -> Result<EventSubWebSocketEnvelope, EventSubError> {
    serde_json::from_str(raw_body).map_err(EventSubError::from)
}

pub fn decode_eventsub_webhook_message(
    raw_body: &str,
) -> Result<EventSubWebhookEnvelope, EventSubError> {
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

pub fn generic_subscription_request(
    subscription_type: impl Into<String>,
    version: impl Into<String>,
    condition: serde_json::Value,
    transport: EventSubTransport,
) -> GenericCreateEventSubSubscriptionRequest {
    GenericCreateEventSubSubscriptionRequest {
        subscription_type: subscription_type.into(),
        version: version.into(),
        condition,
        transport,
    }
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
            user_id: Some(user_id.to_string()),
            ..Default::default()
        },
        transport: EventSubTransport::websocket(session_id.to_string()),
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

fn parse_webhook_message_type(value: &str) -> Result<EventSubWebhookMessageType, EventSubError> {
    match value {
        "notification" => Ok(EventSubWebhookMessageType::Notification),
        "webhook_callback_verification" => Ok(EventSubWebhookMessageType::Verification),
        "revocation" => Ok(EventSubWebhookMessageType::Revocation),
        other => Err(EventSubError::UnsupportedMessageType(other.to_string())),
    }
}

pub fn compute_webhook_signature(
    secret: &str,
    headers: &EventSubWebhookHeaders,
    raw_body: &str,
) -> String {
    let signature = crate::signing::hmac_sha256(
        secret.as_bytes(),
        &[
            headers.message_id.as_bytes(),
            headers.message_timestamp.as_bytes(),
            raw_body.as_bytes(),
        ],
    );
    format!("sha256={}", hex::encode(signature))
}

pub fn verify_webhook_signature(
    secret: &str,
    headers: &EventSubWebhookHeaders,
    raw_body: &str,
) -> bool {
    crate::signing::constant_time_eq(
        compute_webhook_signature(secret, headers, raw_body).as_bytes(),
        headers.message_signature.as_bytes(),
    )
}

pub fn webhook_timestamp_is_fresh(
    headers: &EventSubWebhookHeaders,
    now: OffsetDateTime,
    max_age: Duration,
) -> Result<bool, EventSubError> {
    Ok(timestamp_is_fresh(
        headers.message_timestamp()?,
        now,
        max_age,
    ))
}

fn timestamp_is_fresh(timestamp: OffsetDateTime, now: OffsetDateTime, max_age: Duration) -> bool {
    now >= timestamp && now - timestamp <= max_age
}

pub fn verify_and_decode_webhook_message(
    secret: &str,
    headers: &EventSubWebhookHeaders,
    raw_body: &str,
    now: OffsetDateTime,
    max_age: Duration,
    replay_store: Option<&dyn EventSubReplayStore>,
) -> Result<EventSubWebhookEnvelope, EventSubError> {
    if !verify_webhook_signature(secret, headers, raw_body) {
        return Err(EventSubError::InvalidWebhookSignature);
    }
    let timestamp = headers.message_timestamp()?;
    if !timestamp_is_fresh(timestamp, now, max_age) {
        return Err(EventSubError::StaleWebhookTimestamp);
    }
    if let Some(replay_store) = replay_store
        && !replay_store.remember_message(&headers.message_id, timestamp)?
    {
        return Err(EventSubError::DuplicateWebhookMessage);
    }
    decode_eventsub_webhook_message(raw_body)
}

#[path = "eventsub_generated.rs"]
mod generated;

pub use generated::*;
pub static ALL_EVENTSUB_SUBSCRIPTIONS: &[EventSubSubscriptionDefinition] =
    generated::ALL_SUBSCRIPTIONS;

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

    #[test]
    fn twitch_eventsub_emote_converts_to_generic_emote() {
        let emote = EventSubMessageEmote {
            id: "25".to_string(),
            emote_set_id: None,
            owner_id: None,
            format: vec!["static".to_string(), "animated".to_string()],
            scale: vec!["1.0".to_string(), "2.0".to_string()],
            theme_modes: vec!["light".to_string()],
        }
        .to_emote("Kappa");

        assert_eq!(emote.code, "Kappa");
        assert!(emote.is_animated());
        assert_eq!(emote.images.len(), 4);
    }

    #[test]
    fn webhook_headers_parse_case_insensitively() {
        let headers = EventSubWebhookHeaders::from_pairs([
            ("twitch-eventsub-message-id", "msg-1"),
            ("Twitch-Eventsub-Message-Type", "notification"),
            ("Twitch-Eventsub-Message-Timestamp", "2024-01-01T00:00:00Z"),
            ("Twitch-Eventsub-Message-Signature", "sha256=abc"),
            ("Twitch-Eventsub-Subscription-Type", "channel.follow"),
            ("Twitch-Eventsub-Subscription-Version", "2"),
        ])
        .expect("headers should parse");

        assert_eq!(headers.message_id, "msg-1");
        assert_eq!(
            headers.message_type,
            EventSubWebhookMessageType::Notification
        );
        assert_eq!(headers.subscription_type.as_deref(), Some("channel.follow"));
    }

    #[test]
    fn replay_store_forgets_ids_past_retention() {
        let store = InMemoryEventSubReplayStore::with_retention(Duration::minutes(1));
        let start = OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("valid timestamp");
        let remember = |id: &str, at: OffsetDateTime| {
            store
                .remember_message(id, at)
                .expect("in-memory store should not fail")
        };

        assert!(remember("a", start));
        assert!(!remember("a", start));
        assert!(remember("b", start + Duration::seconds(30)));
        // `a` is now older than the window and forgotten; `b` is still inside it.
        assert!(remember("a", start + Duration::seconds(90)));
        assert!(!remember("b", start + Duration::seconds(90)));
    }

    #[test]
    fn webhook_signature_round_trips_and_replay_store_rejects_duplicates() {
        let raw_body = r#"{
            "subscription": {
                "id": "sub-1",
                "type": "channel.follow",
                "version": "2",
                "condition": {
                    "broadcaster_user_id": "777"
                },
                "transport": {
                    "method": "webhook",
                    "callback": "https://example.com"
                }
            },
            "event": {
                "user_id": "42"
            }
        }"#;

        let headers = EventSubWebhookHeaders {
            message_id: "msg-1".to_string(),
            message_type: EventSubWebhookMessageType::Notification,
            message_timestamp: "2024-01-01T00:00:05Z".to_string(),
            message_signature: String::new(),
            subscription_type: Some("channel.follow".to_string()),
            subscription_version: Some("2".to_string()),
            message_retry: None,
        };
        let signed_headers = EventSubWebhookHeaders {
            message_signature: compute_webhook_signature("secret", &headers, raw_body),
            ..headers
        };
        assert!(verify_webhook_signature(
            "secret",
            &signed_headers,
            raw_body
        ));

        let store = InMemoryEventSubReplayStore::new();
        let envelope = verify_and_decode_webhook_message(
            "secret",
            &signed_headers,
            raw_body,
            OffsetDateTime::parse("2024-01-01T00:00:10Z", &Rfc3339).expect("time should parse"),
            Duration::seconds(30),
            Some(&store),
        )
        .expect("first delivery should verify");
        assert_eq!(envelope.subscription.subscription_type, "channel.follow");

        assert!(matches!(
            verify_and_decode_webhook_message(
                "secret",
                &signed_headers,
                raw_body,
                OffsetDateTime::parse("2024-01-01T00:00:10Z", &Rfc3339).expect("time should parse"),
                Duration::seconds(30),
                Some(&store),
            ),
            Err(EventSubError::DuplicateWebhookMessage)
        ));
    }
}
