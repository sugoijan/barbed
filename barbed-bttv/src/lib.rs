use std::collections::HashMap;

use async_trait::async_trait;
use barbed_core::emotes::Emote;
#[cfg(any(test, feature = "reqwest-client"))]
use barbed_core::emotes::{
    EmoteId, EmoteImage, EmoteImageFormat, EmoteImageScale, EmoteProvider, EmoteThemeMode,
};
#[cfg(any(test, feature = "reqwest-client"))]
use serde::Deserialize;
use thiserror::Error;

#[cfg(feature = "reqwest-client")]
const API_BASE: &str = "https://api.betterttv.net";
#[cfg(any(test, feature = "reqwest-client"))]
const CDN_BASE: &str = "https://cdn.betterttv.net/emote";

#[derive(Debug, Error)]
pub enum BttvError {
    #[error("BetterTTV response failed to decode: {0}")]
    Json(#[from] serde_json::Error),
    #[cfg(feature = "reqwest-client")]
    #[error("BetterTTV request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("BetterTTV channel `{0}` was not seeded in the in-memory client")]
    MissingChannel(String),
    #[error("BetterTTV global emotes were not seeded in the in-memory client")]
    MissingGlobal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BttvEmoteSet {
    pub id: String,
    pub name: String,
    pub emotes: Vec<Emote>,
}

impl BttvEmoteSet {
    pub fn is_empty(&self) -> bool {
        self.emotes.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BttvChannelEmoteSets {
    pub channel: Option<BttvEmoteSet>,
    pub shared: Option<BttvEmoteSet>,
}

impl BttvChannelEmoteSets {
    pub fn is_empty(&self) -> bool {
        self.channel.as_ref().is_none_or(BttvEmoteSet::is_empty)
            && self.shared.as_ref().is_none_or(BttvEmoteSet::is_empty)
    }

    pub fn all_emotes(&self) -> Vec<Emote> {
        let mut emotes = Vec::new();
        if let Some(set) = &self.channel {
            emotes.extend(set.emotes.iter().cloned());
        }
        if let Some(set) = &self.shared {
            emotes.extend(set.emotes.iter().cloned());
        }
        emotes
    }
}

#[async_trait]
pub trait BttvApi: Send + Sync {
    async fn global_emotes(&self) -> Result<BttvEmoteSet, BttvError>;

    async fn channel_emotes_by_twitch_id(
        &self,
        twitch_id: &str,
    ) -> Result<BttvChannelEmoteSets, BttvError>;
}

#[derive(Clone, Default)]
pub struct InMemoryBttvApi {
    global: Option<BttvEmoteSet>,
    channels: HashMap<String, BttvChannelEmoteSets>,
}

impl InMemoryBttvApi {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_global(mut self, set: BttvEmoteSet) -> Self {
        self.global = Some(set);
        self
    }

    pub fn insert_channel_sets(
        &mut self,
        twitch_id: impl Into<String>,
        sets: BttvChannelEmoteSets,
    ) {
        self.channels.insert(twitch_id.into(), sets);
    }
}

#[async_trait]
impl BttvApi for InMemoryBttvApi {
    async fn global_emotes(&self) -> Result<BttvEmoteSet, BttvError> {
        self.global.clone().ok_or(BttvError::MissingGlobal)
    }

    async fn channel_emotes_by_twitch_id(
        &self,
        twitch_id: &str,
    ) -> Result<BttvChannelEmoteSets, BttvError> {
        self.channels
            .get(twitch_id)
            .cloned()
            .ok_or_else(|| BttvError::MissingChannel(twitch_id.to_string()))
    }
}

#[cfg(feature = "reqwest-client")]
#[derive(Clone)]
pub struct BttvClient {
    http: reqwest::Client,
}

#[cfg(feature = "reqwest-client")]
impl BttvClient {
    pub fn new() -> Result<Self, BttvError> {
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent("barbed/0.0.2")
                .build()?,
        })
    }
}

#[cfg(feature = "reqwest-client")]
#[async_trait]
impl BttvApi for BttvClient {
    async fn global_emotes(&self) -> Result<BttvEmoteSet, BttvError> {
        let body = self
            .http
            .get(format!("{API_BASE}/3/cached/emotes/global"))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_global_emotes_json(&body)
    }

    async fn channel_emotes_by_twitch_id(
        &self,
        twitch_id: &str,
    ) -> Result<BttvChannelEmoteSets, BttvError> {
        let body = self
            .http
            .get(format!("{API_BASE}/3/cached/users/twitch/{twitch_id}"))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_channel_emotes_json(twitch_id, &body)
    }
}

#[cfg(any(test, feature = "reqwest-client"))]
fn parse_global_emotes_json(body: &str) -> Result<BttvEmoteSet, BttvError> {
    let models: Vec<EmoteModel> = serde_json::from_str(body)?;
    Ok(BttvEmoteSet {
        id: "global".to_string(),
        name: "BetterTTV Global".to_string(),
        emotes: models.into_iter().map(emote_from_model).collect(),
    })
}

#[cfg(any(test, feature = "reqwest-client"))]
fn parse_channel_emotes_json(
    twitch_id: &str,
    body: &str,
) -> Result<BttvChannelEmoteSets, BttvError> {
    let response: ChannelResponse = serde_json::from_str(body)?;
    Ok(BttvChannelEmoteSets {
        channel: (!response.channel_emotes.is_empty()).then(|| BttvEmoteSet {
            id: format!("channel:{twitch_id}"),
            name: "Channel Emotes".to_string(),
            emotes: response
                .channel_emotes
                .into_iter()
                .map(emote_from_model)
                .collect(),
        }),
        shared: (!response.shared_emotes.is_empty()).then(|| BttvEmoteSet {
            id: format!("shared:{twitch_id}"),
            name: "Shared Emotes".to_string(),
            emotes: response
                .shared_emotes
                .into_iter()
                .map(emote_from_model)
                .collect(),
        }),
    })
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct EmoteModel {
    id: String,
    code: String,
    #[serde(rename = "imageType", default)]
    image_type: String,
    #[serde(default)]
    animated: bool,
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct ChannelResponse {
    #[serde(rename = "channelEmotes", default)]
    channel_emotes: Vec<EmoteModel>,
    #[serde(rename = "sharedEmotes", default)]
    shared_emotes: Vec<EmoteModel>,
}

#[cfg(any(test, feature = "reqwest-client"))]
fn emote_from_model(model: EmoteModel) -> Emote {
    let is_animated = model.animated
        || matches!(
            model.image_type.to_ascii_lowercase().as_str(),
            "gif" | "webm"
        );
    let format = if is_animated {
        EmoteImageFormat::Animated
    } else {
        EmoteImageFormat::Static
    };
    let images = vec![
        EmoteImage {
            format: format.clone(),
            theme_mode: EmoteThemeMode::Light,
            scale: EmoteImageScale::One,
            url: format!("{CDN_BASE}/{}/1x", model.id),
        },
        EmoteImage {
            format: format.clone(),
            theme_mode: EmoteThemeMode::Light,
            scale: EmoteImageScale::Two,
            url: format!("{CDN_BASE}/{}/2x", model.id),
        },
        EmoteImage {
            format,
            theme_mode: EmoteThemeMode::Light,
            scale: EmoteImageScale::Three,
            url: format!("{CDN_BASE}/{}/3x", model.id),
        },
    ];

    Emote::new(
        EmoteId::new(EmoteProvider::BetterTtv, model.id),
        model.code,
        is_animated,
        images,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_fixture_builds_static_and_animated_emotes() {
        let set = parse_global_emotes_json(include_str!("../tests/fixtures/global_emotes.json"))
            .expect("global fixture should parse");
        assert_eq!(set.emotes.len(), 2);
        assert!(set.emotes.iter().any(|emote| emote.is_animated));
    }

    #[test]
    fn channel_fixture_preserves_channel_and_shared_sets() {
        let sets =
            parse_channel_emotes_json("42", include_str!("../tests/fixtures/channel_emotes.json"))
                .expect("channel fixture should parse");
        assert_eq!(
            sets.channel
                .as_ref()
                .expect("channel set should exist")
                .emotes
                .len(),
            1
        );
        assert_eq!(
            sets.shared
                .as_ref()
                .expect("shared set should exist")
                .emotes
                .len(),
            1
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn in_memory_api_round_trips_seeded_sets() {
        let mut api = InMemoryBttvApi::new().with_global(
            parse_global_emotes_json(include_str!("../tests/fixtures/global_emotes.json"))
                .expect("fixture should parse"),
        );
        api.insert_channel_sets(
            "42",
            parse_channel_emotes_json("42", include_str!("../tests/fixtures/channel_emotes.json"))
                .expect("fixture should parse"),
        );

        assert_eq!(
            api.global_emotes()
                .await
                .expect("global set should exist")
                .emotes
                .len(),
            2
        );
        assert_eq!(
            api.channel_emotes_by_twitch_id("42")
                .await
                .expect("channel sets should exist")
                .all_emotes()
                .len(),
            2
        );
    }
}
