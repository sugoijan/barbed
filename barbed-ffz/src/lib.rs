use std::collections::HashMap;

use async_trait::async_trait;
use barbed_core::emotes::Emote;
#[cfg(any(test, feature = "reqwest-client"))]
use barbed_core::emotes::{
    EmoteId, EmoteImage, EmoteImageFormat, EmoteImageScale, EmoteModifier, EmoteModifierFlags,
    EmoteProvider, EmoteThemeMode,
};
#[cfg(any(test, feature = "reqwest-client"))]
use serde::Deserialize;
use thiserror::Error;

#[cfg(feature = "reqwest-client")]
const API_BASE: &str = "https://api.frankerfacez.com/v1";

#[derive(Debug, Error)]
pub enum FfzError {
    #[error("FFZ response failed to decode: {0}")]
    Json(#[from] serde_json::Error),
    #[cfg(feature = "reqwest-client")]
    #[error("FFZ request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("FFZ global emote sets were not seeded in the in-memory client")]
    MissingGlobal,
    #[error("FFZ room `{0}` was not seeded in the in-memory client")]
    MissingRoom(String),
    #[error("FFZ room response missing room payload")]
    MissingRoomPayload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfzEmoteSet {
    pub id: String,
    pub title: Option<String>,
    pub emotes: Vec<Emote>,
}

impl FfzEmoteSet {
    pub fn is_empty(&self) -> bool {
        self.emotes.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FfzGlobalEmoteSets {
    sets: HashMap<String, FfzEmoteSet>,
    default_set_ids: Vec<String>,
    user_scoped: HashMap<String, Vec<String>>,
}

impl FfzGlobalEmoteSets {
    pub fn is_empty(&self) -> bool {
        self.default_set_ids.is_empty() && self.user_scoped.is_empty()
    }

    pub fn default_sets(&self) -> Vec<FfzEmoteSet> {
        self.default_set_ids
            .iter()
            .filter_map(|id| self.sets.get(id).cloned())
            .collect()
    }

    pub fn sets_for_user(&self, user_id: &str) -> Vec<FfzEmoteSet> {
        self.user_scoped
            .iter()
            .filter_map(|(set_id, users)| {
                if users.iter().any(|candidate| candidate == user_id) {
                    self.sets.get(set_id).cloned()
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn user_scoped_summary(&self) -> Vec<(FfzEmoteSet, usize)> {
        self.user_scoped
            .iter()
            .filter_map(|(set_id, users)| {
                self.sets.get(set_id).cloned().map(|set| (set, users.len()))
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FfzRoomEmoteSets {
    pub room_id: String,
    pub twitch_id: String,
    pub sets: Vec<FfzEmoteSet>,
}

impl FfzRoomEmoteSets {
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }
}

#[async_trait]
pub trait FfzApi: Send + Sync {
    async fn global_emote_sets(&self) -> Result<FfzGlobalEmoteSets, FfzError>;

    async fn room_emote_sets_by_twitch_id(
        &self,
        twitch_id: &str,
    ) -> Result<FfzRoomEmoteSets, FfzError>;
}

#[derive(Clone, Default)]
pub struct InMemoryFfzApi {
    global: Option<FfzGlobalEmoteSets>,
    rooms: HashMap<String, FfzRoomEmoteSets>,
}

impl InMemoryFfzApi {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_global(mut self, sets: FfzGlobalEmoteSets) -> Self {
        self.global = Some(sets);
        self
    }

    pub fn insert_room(&mut self, twitch_id: impl Into<String>, room: FfzRoomEmoteSets) {
        self.rooms.insert(twitch_id.into(), room);
    }
}

#[async_trait]
impl FfzApi for InMemoryFfzApi {
    async fn global_emote_sets(&self) -> Result<FfzGlobalEmoteSets, FfzError> {
        self.global.clone().ok_or(FfzError::MissingGlobal)
    }

    async fn room_emote_sets_by_twitch_id(
        &self,
        twitch_id: &str,
    ) -> Result<FfzRoomEmoteSets, FfzError> {
        self.rooms
            .get(twitch_id)
            .cloned()
            .ok_or_else(|| FfzError::MissingRoom(twitch_id.to_string()))
    }
}

#[cfg(feature = "reqwest-client")]
#[derive(Clone)]
pub struct FfzClient {
    http: reqwest::Client,
}

#[cfg(feature = "reqwest-client")]
impl FfzClient {
    pub fn new() -> Result<Self, FfzError> {
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent("barbed/0.0.2")
                .build()?,
        })
    }
}

#[cfg(feature = "reqwest-client")]
#[async_trait]
impl FfzApi for FfzClient {
    async fn global_emote_sets(&self) -> Result<FfzGlobalEmoteSets, FfzError> {
        let body = self
            .http
            .get(format!("{API_BASE}/set/global"))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_global_sets_json(&body)
    }

    async fn room_emote_sets_by_twitch_id(
        &self,
        twitch_id: &str,
    ) -> Result<FfzRoomEmoteSets, FfzError> {
        let body = self
            .http
            .get(format!("{API_BASE}/room/id/{twitch_id}"))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_room_sets_json(&body)
    }
}

#[cfg(any(test, feature = "reqwest-client"))]
fn parse_global_sets_json(body: &str) -> Result<FfzGlobalEmoteSets, FfzError> {
    let response: GlobalSetsResponse = serde_json::from_str(body)?;
    let sets = response
        .sets
        .into_iter()
        .map(|(id, model)| (id, emote_set_from_model(model)))
        .collect();
    Ok(FfzGlobalEmoteSets {
        sets,
        default_set_ids: response
            .default_sets
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
        user_scoped: response.users,
    })
}

#[cfg(any(test, feature = "reqwest-client"))]
fn parse_room_sets_json(body: &str) -> Result<FfzRoomEmoteSets, FfzError> {
    let response: RoomResponse = serde_json::from_str(body)?;
    let room = response.room.ok_or(FfzError::MissingRoomPayload)?;
    Ok(FfzRoomEmoteSets {
        room_id: room.id,
        twitch_id: room.twitch_id.to_string(),
        sets: response
            .sets
            .into_values()
            .map(emote_set_from_model)
            .collect(),
    })
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct GlobalSetsResponse {
    #[serde(default)]
    default_sets: Vec<u64>,
    #[serde(default)]
    sets: HashMap<String, EmoteSetModel>,
    #[serde(default)]
    users: HashMap<String, Vec<String>>,
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct RoomResponse {
    room: Option<RoomModel>,
    #[serde(default)]
    sets: HashMap<String, EmoteSetModel>,
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct RoomModel {
    id: String,
    #[serde(rename = "twitch_id")]
    twitch_id: u64,
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct EmoteSetModel {
    id: u64,
    title: Option<String>,
    #[serde(default)]
    emoticons: Vec<EmoteModel>,
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct EmoteModel {
    id: u64,
    name: String,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    modifier: bool,
    #[serde(default)]
    modifier_flags: u32,
    #[serde(default)]
    urls: HashMap<String, Option<String>>,
    animated: Option<HashMap<String, Option<String>>>,
    mask: Option<HashMap<String, Option<String>>>,
    mask_animated: Option<HashMap<String, Option<String>>>,
}

#[cfg(any(test, feature = "reqwest-client"))]
fn emote_set_from_model(model: EmoteSetModel) -> FfzEmoteSet {
    FfzEmoteSet {
        id: model.id.to_string(),
        title: model.title,
        emotes: model.emoticons.into_iter().map(emote_from_model).collect(),
    }
}

#[cfg(any(test, feature = "reqwest-client"))]
fn emote_from_model(model: EmoteModel) -> Emote {
    let mut images = image_variants(&model.urls, EmoteImageFormat::Static);
    if let Some(animated) = &model.animated {
        images.extend(image_variants(animated, EmoteImageFormat::Animated));
    }
    let is_animated = images
        .iter()
        .any(|image| matches!(image.format, EmoteImageFormat::Animated));

    let mut emote = Emote::new(
        EmoteId::new(EmoteProvider::FrankerFaceZ, model.id.to_string()),
        model.name,
        is_animated,
        images,
    );

    if model.modifier {
        let flags = EmoteModifierFlags::from_bits_retain(model.modifier_flags);
        let mut mask_images = Vec::new();
        if let Some(mask) = &model.mask {
            mask_images.extend(image_variants(mask, EmoteImageFormat::Static));
        }
        if let Some(mask) = &model.mask_animated {
            mask_images.extend(image_variants(mask, EmoteImageFormat::Animated));
        }
        emote = emote.with_modifier(EmoteModifier {
            flags,
            raw_flags: model.modifier_flags,
            is_hidden: model.hidden || flags.contains(EmoteModifierFlags::HIDDEN),
            mask_images,
        });
    }

    emote
}

#[cfg(any(test, feature = "reqwest-client"))]
fn image_variants(
    images: &HashMap<String, Option<String>>,
    format: EmoteImageFormat,
) -> Vec<EmoteImage> {
    images
        .iter()
        .filter_map(|(scale_key, url)| {
            let url = url.clone()?;
            Some(EmoteImage {
                format: format.clone(),
                theme_mode: EmoteThemeMode::Light,
                scale: parse_scale(scale_key),
                url,
            })
        })
        .collect()
}

#[cfg(any(test, feature = "reqwest-client"))]
fn parse_scale(value: &str) -> EmoteImageScale {
    match value {
        "1" | "1.0" => EmoteImageScale::One,
        "2" | "2.0" => EmoteImageScale::Two,
        "3" | "3.0" => EmoteImageScale::Three,
        other => EmoteImageScale::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_fixture_preserves_default_and_user_scoped_sets() {
        let sets = parse_global_sets_json(include_str!("../tests/fixtures/global_sets.json"))
            .expect("global fixture should parse");
        assert_eq!(sets.default_sets().len(), 1);
        assert_eq!(sets.sets_for_user("42").len(), 1);
        assert_eq!(sets.user_scoped_summary().len(), 1);
    }

    #[test]
    fn room_fixture_preserves_modifier_metadata() {
        let room = parse_room_sets_json(include_str!("../tests/fixtures/room_sets.json"))
            .expect("room fixture should parse");
        let modifier = room.sets[0].emotes[0]
            .modifier()
            .expect("modifier metadata should exist");
        assert!(modifier.is_hidden);
        assert!(modifier.flags.contains(EmoteModifierFlags::RAINBOW));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn in_memory_api_returns_seeded_global_and_room_sets() {
        let mut api = InMemoryFfzApi::new().with_global(
            parse_global_sets_json(include_str!("../tests/fixtures/global_sets.json"))
                .expect("global fixture should parse"),
        );
        api.insert_room(
            "42",
            parse_room_sets_json(include_str!("../tests/fixtures/room_sets.json"))
                .expect("room fixture should parse"),
        );

        assert_eq!(
            api.global_emote_sets()
                .await
                .expect("global sets should exist")
                .default_sets()
                .len(),
            1
        );
        assert_eq!(
            api.room_emote_sets_by_twitch_id("42")
                .await
                .expect("room sets should exist")
                .sets
                .len(),
            1
        );
    }
}
