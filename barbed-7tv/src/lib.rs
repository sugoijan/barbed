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
const API_BASE: &str = "https://7tv.io/v3";

#[derive(Debug, Error)]
pub enum SevenTvError {
    #[error("7TV response failed to decode: {0}")]
    Json(#[from] serde_json::Error),
    #[cfg(feature = "reqwest-client")]
    #[error("7TV request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("7TV user `{0}` was not seeded in the in-memory client")]
    MissingUser(String),
    #[error("7TV emote set `{0}` was not seeded in the in-memory client")]
    MissingEmoteSet(String),
    #[error("7TV user had no associated Twitch emote set")]
    MissingDefaultEmoteSet,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SevenTvUser {
    pub default_emote_set_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SevenTvEmoteSet {
    pub id: String,
    pub name: String,
    pub emotes: Vec<Emote>,
}

#[async_trait]
pub trait SevenTvApi: Send + Sync {
    async fn global_emote_set(&self) -> Result<SevenTvEmoteSet, SevenTvError>;

    async fn user_by_twitch_id(&self, twitch_id: &str) -> Result<SevenTvUser, SevenTvError>;

    async fn emote_set(&self, set_id: &str) -> Result<SevenTvEmoteSet, SevenTvError>;

    async fn channel_emote_set_by_twitch_id(
        &self,
        twitch_id: &str,
    ) -> Result<SevenTvEmoteSet, SevenTvError> {
        let user = self.user_by_twitch_id(twitch_id).await?;
        let set_id = user
            .default_emote_set_id
            .ok_or(SevenTvError::MissingDefaultEmoteSet)?;
        self.emote_set(&set_id).await
    }
}

#[derive(Clone, Default)]
pub struct InMemorySevenTvApi {
    global_set: Option<SevenTvEmoteSet>,
    users: HashMap<String, SevenTvUser>,
    emote_sets: HashMap<String, SevenTvEmoteSet>,
}

impl InMemorySevenTvApi {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_global_set(mut self, set: SevenTvEmoteSet) -> Self {
        self.global_set = Some(set);
        self
    }

    pub fn insert_user(&mut self, twitch_id: impl Into<String>, user: SevenTvUser) {
        self.users.insert(twitch_id.into(), user);
    }

    pub fn insert_emote_set(&mut self, set: SevenTvEmoteSet) {
        self.emote_sets.insert(set.id.clone(), set);
    }
}

#[async_trait]
impl SevenTvApi for InMemorySevenTvApi {
    async fn global_emote_set(&self) -> Result<SevenTvEmoteSet, SevenTvError> {
        self.global_set
            .clone()
            .ok_or_else(|| SevenTvError::MissingEmoteSet("global".to_string()))
    }

    async fn user_by_twitch_id(&self, twitch_id: &str) -> Result<SevenTvUser, SevenTvError> {
        self.users
            .get(twitch_id)
            .cloned()
            .ok_or_else(|| SevenTvError::MissingUser(twitch_id.to_string()))
    }

    async fn emote_set(&self, set_id: &str) -> Result<SevenTvEmoteSet, SevenTvError> {
        self.emote_sets
            .get(set_id)
            .cloned()
            .ok_or_else(|| SevenTvError::MissingEmoteSet(set_id.to_string()))
    }
}

#[cfg(feature = "reqwest-client")]
#[derive(Clone)]
pub struct SevenTvClient {
    http: reqwest::Client,
}

#[cfg(feature = "reqwest-client")]
impl SevenTvClient {
    pub fn new() -> Result<Self, SevenTvError> {
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent("barbed/0.0.2")
                .build()?,
        })
    }
}

#[cfg(feature = "reqwest-client")]
#[async_trait]
impl SevenTvApi for SevenTvClient {
    async fn global_emote_set(&self) -> Result<SevenTvEmoteSet, SevenTvError> {
        let body = self
            .http
            .get(format!("{API_BASE}/emote-sets/global"))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_emote_set_json(&body)
    }

    async fn user_by_twitch_id(&self, twitch_id: &str) -> Result<SevenTvUser, SevenTvError> {
        let body = self
            .http
            .get(format!("{API_BASE}/users/twitch/{twitch_id}"))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_user_json(&body)
    }

    async fn emote_set(&self, set_id: &str) -> Result<SevenTvEmoteSet, SevenTvError> {
        let body = self
            .http
            .get(format!("{API_BASE}/emote-sets/{set_id}"))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_emote_set_json(&body)
    }
}

#[cfg(any(test, feature = "reqwest-client"))]
fn parse_user_json(body: &str) -> Result<SevenTvUser, SevenTvError> {
    let user: SevenTvUserModel = serde_json::from_str(body)?;
    Ok(SevenTvUser {
        default_emote_set_id: user.default_emote_set_id(),
    })
}

#[cfg(any(test, feature = "reqwest-client"))]
fn parse_emote_set_json(body: &str) -> Result<SevenTvEmoteSet, SevenTvError> {
    let model: EmoteSetModel = serde_json::from_str(body)?;
    Ok(SevenTvEmoteSet {
        id: model.id,
        name: model.name,
        emotes: model.emotes.into_iter().map(emote_from_model).collect(),
    })
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct SevenTvUserModel {
    #[serde(default)]
    emote_set: Option<SetRef>,
    #[serde(default)]
    connections: Vec<UserConnection>,
    #[serde(default)]
    emote_sets: Vec<SetRef>,
}

#[cfg(any(test, feature = "reqwest-client"))]
impl SevenTvUserModel {
    fn default_emote_set_id(&self) -> Option<String> {
        if let Some(set) = &self.emote_set {
            return Some(set.id.clone());
        }
        for connection in &self.connections {
            if connection.platform.eq_ignore_ascii_case("twitch") {
                if let Some(set) = &connection.emote_set {
                    return Some(set.id.clone());
                }
                if let Some(id) = &connection.emote_set_id {
                    return Some(id.clone());
                }
            }
        }
        self.emote_sets.first().map(|set| set.id.clone())
    }
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct UserConnection {
    platform: String,
    #[serde(default)]
    emote_set: Option<SetRef>,
    #[serde(default)]
    emote_set_id: Option<String>,
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct SetRef {
    id: String,
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct EmoteSetModel {
    id: String,
    name: String,
    emotes: Vec<SetEmote>,
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct SetEmote {
    id: String,
    name: String,
    data: EmoteData,
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct EmoteData {
    host: EmoteHost,
    #[serde(default)]
    animated: bool,
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct EmoteHost {
    url: String,
    #[serde(default)]
    files: Vec<HostFile>,
}

#[derive(Deserialize)]
#[cfg(any(test, feature = "reqwest-client"))]
struct HostFile {
    name: String,
}

#[cfg(any(test, feature = "reqwest-client"))]
fn emote_from_model(model: SetEmote) -> Emote {
    let format = if model.data.animated {
        EmoteImageFormat::Animated
    } else {
        EmoteImageFormat::Static
    };
    let images = if model.data.host.files.is_empty() {
        vec![EmoteImage {
            format: format.clone(),
            theme_mode: EmoteThemeMode::Light,
            scale: EmoteImageScale::One,
            url: model.data.host.url.clone(),
        }]
    } else {
        model
            .data
            .host
            .files
            .into_iter()
            .map(|file| EmoteImage {
                format: format.clone(),
                theme_mode: EmoteThemeMode::Light,
                scale: parse_scale_from_name(&file.name),
                url: format!(
                    "{}/{}",
                    model.data.host.url.trim_end_matches('/'),
                    file.name
                ),
            })
            .collect()
    };

    Emote::new(
        EmoteId::new(EmoteProvider::SevenTv, model.id),
        model.name,
        model.data.animated,
        images,
    )
}

#[cfg(any(test, feature = "reqwest-client"))]
fn parse_scale_from_name(name: &str) -> EmoteImageScale {
    if name.contains("1x") {
        EmoteImageScale::One
    } else if name.contains("2x") {
        EmoteImageScale::Two
    } else if name.contains("3x") {
        EmoteImageScale::Three
    } else {
        EmoteImageScale::Other(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_fixture_resolves_default_twitch_emote_set() {
        let user = parse_user_json(include_str!("../tests/fixtures/user_twitch.json"))
            .expect("user fixture should parse");
        assert_eq!(user.default_emote_set_id.as_deref(), Some("set-123"));
    }

    #[test]
    fn emote_set_fixture_builds_generic_emotes() {
        let set = parse_emote_set_json(include_str!("../tests/fixtures/emote_set.json"))
            .expect("set fixture should parse");
        assert_eq!(set.id, "set-123");
        assert_eq!(set.emotes.len(), 2);
        assert!(set.emotes.iter().any(|emote| emote.is_animated));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn in_memory_api_uses_seeded_user_to_resolve_channel_set() {
        let mut api = InMemorySevenTvApi::new();
        api.insert_user(
            "42",
            SevenTvUser {
                default_emote_set_id: Some("set-123".to_string()),
            },
        );
        api.insert_emote_set(
            parse_emote_set_json(include_str!("../tests/fixtures/emote_set.json"))
                .expect("fixture should parse"),
        );

        let set = api
            .channel_emote_set_by_twitch_id("42")
            .await
            .expect("channel set should resolve");
        assert_eq!(set.id, "set-123");
    }
}
