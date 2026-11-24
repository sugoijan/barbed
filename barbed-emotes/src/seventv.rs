//! 7TV's emote sets, from its v3 REST API.
//!
//! A channel's active set comes whole in the answer to
//! [`user_url`] (one request, no second one for the set), and the global set
//! at [`GLOBAL_SET_URL`]. Both are public and need no token.

use serde::Deserialize;
use serde_json::Value;

use crate::model::{
    Emote, EmoteId, EmoteImage, EmoteModifier, EmoteProvider, EmoteSet, ImageFormat, ParseError,
};
use crate::safe::{https, is_plain_id, is_plain_path, path_on};

/// The host 7TV's emote images are on.
pub const IMAGE_HOST: &str = "cdn.7tv.app";

/// 7TV's global emote set.
pub const GLOBAL_SET_URL: &str = "https://7tv.io/v3/emote-sets/global";

/// The address of the 7TV account connected to Twitch user `twitch_user_id`,
/// with its active emote set. It answers `404` for a channel without 7TV.
/// `None` for an id of another shape than Twitch's.
pub fn user_url(twitch_user_id: &str) -> Option<String> {
    is_plain_id(twitch_user_id).then(|| format!("https://7tv.io/v3/users/twitch/{twitch_user_id}"))
}

/// The address of emote set `set_id`.
pub fn emote_set_url(set_id: &str) -> Option<String> {
    is_plain_id(set_id).then(|| format!("https://7tv.io/v3/emote-sets/{set_id}"))
}

/// A set's `flags` bit on one of its emotes: drawn over the emote before it.
const ACTIVE_ZERO_WIDTH: i64 = 1 << 0;
/// An emote's own `flags` bit: zero-width wherever it is used.
const EMOTE_ZERO_WIDTH: i64 = 1 << 8;

#[derive(Deserialize)]
struct WireSet {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    emotes: Vec<Value>,
}

#[derive(Deserialize)]
struct WireActiveEmote {
    id: String,
    name: String,
    #[serde(default)]
    flags: i64,
    #[serde(default)]
    data: Option<WireEmoteData>,
}

#[derive(Default, Deserialize)]
struct WireEmoteData {
    #[serde(default)]
    flags: i64,
    #[serde(default)]
    animated: bool,
    #[serde(default)]
    host: Option<WireHost>,
}

#[derive(Deserialize)]
struct WireHost {
    url: String,
    #[serde(default)]
    files: Vec<WireFile>,
}

#[derive(Deserialize)]
struct WireFile {
    name: String,
    #[serde(default)]
    static_name: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
}

/// The scale and format of a file named `<n>x.<ext>` or `<n>x_static.<ext>`.
fn file_shape(name: &str) -> Option<(u8, ImageFormat)> {
    let (stem, extension) = name.split_once('.')?;
    let stem = stem.strip_suffix("_static").unwrap_or(stem);
    let scale: u8 = stem.strip_suffix('x')?.parse().ok()?;
    (1..=4)
        .contains(&scale)
        .then_some(())
        .and(ImageFormat::from_extension(extension))
        .map(|format| (scale, format))
}

fn emote_from_wire(value: Value) -> Option<Emote> {
    let wire: WireActiveEmote = serde_json::from_value(value).ok()?;
    if wire.name.is_empty() || !is_plain_id(&wire.id) {
        return None;
    }
    let data = wire.data.unwrap_or_default();
    // The folder its files are in: the answer's, when it is on 7TV's image
    // host; an answer naming another host is not believed.
    let base = match &data.host {
        Some(host) => path_on(IMAGE_HOST, &host.url)?.to_string(),
        None => format!("/emote/{}", wire.id),
    };
    let image = |name: &str, animated: bool, file: Option<&WireFile>| {
        let (scale, format) = file_shape(name)?;
        let path = format!("{base}/{name}");
        if !is_plain_path(&path) {
            return None;
        }
        let mut image = EmoteImage::new(https(IMAGE_HOST, &path), scale)
            .animated(animated)
            .with_format(format);
        if let Some(file) = file {
            image.width = file.width;
            image.height = file.height;
        }
        Some(image)
    };
    let files = data.host.map(|host| host.files).unwrap_or_default();
    let mut images = Vec::new();
    for file in &files {
        images.extend(image(&file.name, data.animated, Some(file)));
        // An animated emote's still frame, where the answer names one.
        if let Some(still) = file.static_name.as_deref()
            && data.animated
            && still != file.name
        {
            images.extend(image(still, false, Some(file)));
        }
    }
    if files.is_empty() {
        // An answer without a file list: the files every emote has.
        for scale in 1..=4 {
            images.extend(image(&format!("{scale}x.webp"), data.animated, None));
        }
    }
    if images.is_empty() {
        return None;
    }
    let mut emote = Emote::new(
        EmoteId::new(EmoteProvider::SevenTv, wire.id),
        wire.name,
        images,
    );
    emote.animated = data.animated;
    if wire.flags & ACTIVE_ZERO_WIDTH != 0 || data.flags & EMOTE_ZERO_WIDTH != 0 {
        emote.modifier = Some(EmoteModifier::overlay());
    }
    Some(emote)
}

fn set_from_wire(wire: WireSet) -> EmoteSet {
    let mut set = EmoteSet {
        id: wire.id,
        name: wire.name,
        ..EmoteSet::default()
    };
    for value in wire.emotes {
        match emote_from_wire(value) {
            Some(emote) => set.emotes.push(emote),
            None => set.skipped += 1,
        }
    }
    set
}

/// Reads an emote set (`{"id", "name", "emotes": [...]}`): the answer of
/// [`GLOBAL_SET_URL`] or [`emote_set_url`].
///
/// Each emote's code is the set's name for it (an alias where the channel
/// renamed it). Its images are the files the answer lists, in the answer's
/// order: for every scale and format the animated file of an animated emote
/// followed by its still frame, or the one file of a still emote. A
/// zero-width emote is an [overlay](EmoteModifier::overlay).
pub fn parse_emote_set(body: &str) -> Result<EmoteSet, ParseError> {
    let value: Value = serde_json::from_str(body)?;
    if !value.is_object() {
        return Err(ParseError::Shape("a 7TV emote set is an object"));
    }
    Ok(set_from_wire(serde_json::from_value(value)?))
}

/// What [`user_url`] answered: a 7TV account connected to a Twitch channel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SevenTvUser {
    /// The channel's active emote set, where the answer carries it whole.
    pub emote_set: Option<EmoteSet>,
    /// The active set's id, to read with [`emote_set_url`] where
    /// [`emote_set`](Self::emote_set) is not there.
    pub emote_set_id: Option<String>,
}

impl SevenTvUser {
    /// The emotes of the channel's active set, none where the answer had no
    /// set.
    pub fn into_emotes(self) -> Vec<Emote> {
        self.emote_set.map(|set| set.emotes).unwrap_or_default()
    }
}

#[derive(Deserialize)]
struct WireUser {
    #[serde(default)]
    emote_set: Option<Value>,
    #[serde(default)]
    emote_set_id: Option<String>,
    #[serde(default)]
    connections: Vec<WireConnection>,
}

#[derive(Deserialize)]
struct WireConnection {
    #[serde(default)]
    platform: String,
    #[serde(default)]
    emote_set: Option<WireSetRef>,
    #[serde(default)]
    emote_set_id: Option<String>,
}

#[derive(Deserialize)]
struct WireSetRef {
    id: String,
}

/// Reads the answer of [`user_url`]: the connection's active set with its
/// emotes. The answer of `/v3/users/<7tv id>` is read too, which names the
/// Twitch connection's set by id only.
pub fn parse_user(body: &str) -> Result<SevenTvUser, ParseError> {
    let value: Value = serde_json::from_str(body)?;
    if !value.is_object() {
        return Err(ParseError::Shape("a 7TV user is an object"));
    }
    let wire: WireUser = serde_json::from_value(value)?;
    let emote_set = match wire.emote_set {
        Some(set) if set.is_object() => Some(set_from_wire(serde_json::from_value(set)?)),
        _ => None,
    };
    let emote_set_id = emote_set
        .as_ref()
        .and_then(|set| set.id.clone())
        .or(wire.emote_set_id)
        .or_else(|| {
            wire.connections
                .into_iter()
                .filter(|connection| connection.platform.eq_ignore_ascii_case("twitch"))
                .find_map(|connection| {
                    connection
                        .emote_set
                        .map(|set| set.id)
                        .or(connection.emote_set_id)
                })
        });
    Ok(SevenTvUser {
        emote_set,
        emote_set_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ImageRequest, ModifierTarget};

    const WEBP: ImageRequest<'static> = ImageRequest {
        scale: 2,
        animated: true,
        theme: crate::model::Theme::Dark,
        formats: &[ImageFormat::WebP],
    };

    fn codes(set: &EmoteSet) -> Vec<&str> {
        set.emotes.iter().map(|emote| emote.code.as_str()).collect()
    }

    #[test]
    fn a_channels_set_comes_with_its_user() {
        let user = parse_user(include_str!("../tests/fixtures/7tv-user.json")).expect("a user");
        assert_eq!(
            user.emote_set_id.as_deref(),
            Some("60b39e943e203cc169dfc106")
        );
        let set = user.emote_set.expect("its set");
        assert_eq!(set.name.as_deref(), Some("Streamer's Emotes"));
        assert_eq!(codes(&set), ["catJAM", "wideHappy"]);
        assert_eq!(set.skipped, 0);
        // catJAM is animated: its animated WebP, and its still frame when
        // that is asked for.
        let cat = &set.emotes[0];
        assert!(cat.animated);
        assert_eq!(
            cat.image(&WEBP).unwrap().url,
            "https://cdn.7tv.app/emote/60ae7316f7c927fad14e6ca2/2x.webp"
        );
        let still = ImageRequest {
            animated: false,
            ..WEBP
        };
        let frame = cat.image(&still).unwrap();
        assert!(frame.url.ends_with("/2x_static.webp") && !frame.animated);
        // wideHappy is still: its one file, with the size the answer gave.
        let wide = &set.emotes[1];
        assert!(!wide.animated);
        let image = wide.image(&WEBP).unwrap();
        assert!(image.url.ends_with("/2x.webp"), "{}", image.url);
        assert_eq!((image.width, image.height), (Some(168), Some(56)));
    }

    #[test]
    fn the_global_set_keeps_aliases_and_marks_zero_width_emotes() {
        let set =
            parse_emote_set(include_str!("../tests/fixtures/7tv-global.json")).expect("a set");
        assert_eq!(set.name.as_deref(), Some("Global Emotes"));
        // The set's name for an emote, not the emote's own (`catJam2`).
        assert_eq!(codes(&set), ["Clap", "catJAM", "ZeroCover"]);
        // The 2x WebP among the AVIF and GIF files beside it.
        assert!(
            set.emotes[0]
                .image(&WEBP)
                .unwrap()
                .url
                .ends_with("/2x.webp")
        );
        // Four scales of three formats, each with a still frame.
        assert_eq!(set.emotes[0].images.len(), 24);
        let zero = &set.emotes[2];
        let modifier = zero.modifier.as_ref().expect("an overlay");
        assert_eq!(modifier.target, ModifierTarget::Preceding);
        assert!(!modifier.is_hidden() && !zero.is_standalone() && zero.is_visible());
        assert!(set.emotes[0].is_standalone());
        // A still emote's files have no separate still frame.
        assert_eq!(zero.images.len(), 12);
    }

    #[test]
    fn images_stay_on_7tvs_host() {
        let body = r#"{"emote_set":{"id":"s","emotes":[
            {"id":"a1","name":"Up","data":{"host":{"url":"//cdn.7tv.app/../../auth/attempts/X/open","files":[{"name":"2x.webp"}]}}},
            {"id":"a2","name":"Esc","data":{"host":{"url":"//cdn.7tv.app/emote/%2e%2e","files":[{"name":"2x.webp"}]}}},
            {"id":"a3","name":"Away","data":{"host":{"url":"//evil.example/emote/a3","files":[{"name":"2x.webp"}]}}},
            {"id":"../a4","name":"BadId"},
            {"id":"a5","name":"OddFile","data":{"host":{"url":"//cdn.7tv.app/emote/a5","files":[{"name":"../2x.webp"},{"name":"9x.webp"},{"name":"2x.exe"}]}}},
            {"id":"a6","name":"Fine","data":{"host":{"url":"//cdn.7tv.app/emote/a6","files":[{"name":"2x.webp"}]}}},
            {"id":"a7","name":"NoHost"}
        ]}}"#;
        let set = parse_user(body).expect("a user").emote_set.expect("a set");
        assert_eq!(codes(&set), ["Fine", "NoHost"]);
        assert_eq!(set.skipped, 5);
        // Without a file list: the WebP files every emote has.
        assert_eq!(
            set.emotes[1].image(&WEBP).unwrap().url,
            "https://cdn.7tv.app/emote/a7/2x.webp"
        );
    }

    #[test]
    fn a_user_without_a_whole_set_names_it() {
        let user = parse_user(include_str!("../tests/fixtures/7tv-user-connections.json"))
            .expect("a user");
        assert!(user.emote_set.is_none());
        assert_eq!(user.emote_set_id.as_deref(), Some("set-123"));
        let none = parse_user(r#"{"id":"1","emote_set":null}"#).expect("a user");
        assert_eq!(none, SevenTvUser::default());
        assert!(none.into_emotes().is_empty());
        assert!(parse_user("[]").is_err());
        assert!(parse_emote_set("not json").is_err());
    }

    #[test]
    fn addresses_take_only_plain_ids() {
        assert_eq!(
            user_url("1971641").as_deref(),
            Some("https://7tv.io/v3/users/twitch/1971641")
        );
        assert_eq!(user_url("../../me"), None);
        assert_eq!(
            emote_set_url("set-123").as_deref(),
            Some("https://7tv.io/v3/emote-sets/set-123")
        );
    }
}
