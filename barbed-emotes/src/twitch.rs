//! Twitch's own emotes.
//!
//! A chat message already says which of its words are Twitch emotes (an
//! EventSub message's fragments, an IRC message's `emotes` tag), by id. An
//! id is all an image needs: every emote is served from one template,
//! `https://static-cdn.jtvnw.net/emoticons/v2/<id>/<format>/<theme>/<scale>`.
//!
//! The emote lists Helix returns (Get Global Emotes, Get Channel Emotes, Get
//! Emote Sets) need a token, so a browser or a game asks for them through its
//! own server; [`parse_emotes`] reads all three.

use serde::Deserialize;
use serde_json::Value;

use crate::model::{
    Emote, EmoteId, EmoteImage, EmoteProvider, EmoteSet, ImageFormat, ParseError, Theme,
};
use crate::safe::{https, is_plain_id};

/// The host Twitch's emote images are on.
pub const IMAGE_HOST: &str = "static-cdn.jtvnw.net";

/// Helix's Get Global Emotes. It needs an app or a user token.
pub const GLOBAL_EMOTES_URL: &str = "https://api.twitch.tv/helix/chat/emotes/global";

/// The size steps Twitch serves: `1.0`, `2.0` and `3.0`.
pub const SCALES: [u8; 3] = [1, 2, 3];

/// The address of emote `id`'s image: its still PNG, or its animated GIF.
/// `None` for an id of another shape than Twitch's
/// ([`is_plain_id`]) or a scale Twitch does not serve.
pub fn image_url(id: &str, animated: bool, theme: Theme, scale: u8) -> Option<String> {
    if !is_plain_id(id) || !SCALES.contains(&scale) {
        return None;
    }
    let format = if animated { "animated" } else { "static" };
    Some(https(
        IMAGE_HOST,
        &format!("/emoticons/v2/{id}/{format}/{}/{scale}.0", theme.as_str()),
    ))
}

/// Emote `id`'s image ([`image_url`]) with what is known of it.
pub fn image(id: &str, animated: bool, theme: Theme, scale: u8) -> Option<EmoteImage> {
    let format = if animated {
        ImageFormat::Gif
    } else {
        ImageFormat::Png
    };
    Some(
        EmoteImage::new(image_url(id, animated, theme, scale)?, scale)
            .animated(animated)
            .with_format(format)
            .with_theme(theme),
    )
}

/// Twitch emote `id`, written `code`, with every image the template serves:
/// both themes at three scales, still, and animated too where `animated`
/// (a fragment's or a list's `format` has `animated`).
pub fn emote(id: &str, code: impl Into<String>, animated: bool) -> Option<Emote> {
    emote_with(id, code, animated, &[Theme::Dark, Theme::Light], &SCALES)
}

/// As [`emote`], with only the `themes` and `scales` given.
pub fn emote_with(
    id: &str,
    code: impl Into<String>,
    animated: bool,
    themes: &[Theme],
    scales: &[u8],
) -> Option<Emote> {
    let mut images = Vec::new();
    for moving in [true, false] {
        if moving && !animated {
            continue;
        }
        for theme in themes {
            for scale in scales {
                images.extend(image(id, moving, *theme, *scale));
            }
        }
    }
    if images.is_empty() {
        return None;
    }
    Some(Emote::new(
        EmoteId::new(EmoteProvider::Twitch, id),
        code,
        images,
    ))
}

/// A Twitch emote from the lists Helix and EventSub describe it with: its
/// `format` (`static`, `animated`), `scale` (`1.0`, `2.0`, `3.0`) and
/// `theme_mode` (`light`, `dark`). A list that is empty or has nothing known
/// in it stands for all of its values (for `format`: still alone).
pub fn emote_from_lists<S: AsRef<str>>(
    id: &str,
    code: impl Into<String>,
    format: &[S],
    scale: &[S],
    theme_mode: &[S],
) -> Option<Emote> {
    let animated = format.iter().any(|format| format.as_ref() == "animated");
    let themes: Vec<Theme> = theme_mode
        .iter()
        .filter_map(|theme| match theme.as_ref() {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            _ => None,
        })
        .collect();
    let scales: Vec<u8> = scale
        .iter()
        .filter_map(|scale| {
            let scale = scale.as_ref();
            scale.strip_suffix(".0").unwrap_or(scale).parse().ok()
        })
        .filter(|scale| SCALES.contains(scale))
        .collect();
    emote_with(
        id,
        code,
        animated,
        if themes.is_empty() {
            &[Theme::Dark, Theme::Light]
        } else {
            &themes
        },
        if scales.is_empty() { &SCALES } else { &scales },
    )
}

#[derive(Deserialize)]
struct WireList {
    data: Vec<Value>,
}

#[derive(Deserialize)]
struct WireEmote {
    id: String,
    name: String,
    #[serde(default)]
    format: Vec<String>,
    #[serde(default)]
    scale: Vec<String>,
    #[serde(default)]
    theme_mode: Vec<String>,
}

fn emote_from_wire(value: Value) -> Option<Emote> {
    let wire: WireEmote = serde_json::from_value(value).ok()?;
    if wire.name.is_empty() {
        return None;
    }
    emote_from_lists(
        &wire.id,
        wire.name,
        &wire.format,
        &wire.scale,
        &wire.theme_mode,
    )
}

/// Reads the body of Helix's Get Global Emotes, Get Channel Emotes or Get
/// Emote Sets (`{"data": [{"id", "name", "format", "scale", "theme_mode",
/// …}], "template"}`). The images are built from each emote's id, as the
/// `template` Twitch documents does, never taken from the answer.
pub fn parse_emotes(body: &str) -> Result<EmoteSet, ParseError> {
    let list: WireList = serde_json::from_str(body)?;
    let mut set = EmoteSet::default();
    for value in list.data {
        match emote_from_wire(value) {
            Some(emote) => set.emotes.push(emote),
            None => set.skipped += 1,
        }
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ImageRequest;

    #[test]
    fn images_follow_the_template() {
        assert_eq!(
            image_url("25", false, Theme::Dark, 2).as_deref(),
            Some("https://static-cdn.jtvnw.net/emoticons/v2/25/static/dark/2.0")
        );
        assert_eq!(
            image_url("emotesv2_1", true, Theme::Light, 3).as_deref(),
            Some("https://static-cdn.jtvnw.net/emoticons/v2/emotesv2_1/animated/light/3.0")
        );
        assert_eq!(image_url("../../me", false, Theme::Dark, 2), None);
        assert_eq!(image_url("25", false, Theme::Dark, 4), None);
    }

    #[test]
    fn an_emote_has_every_image_of_the_template() {
        let still = emote("25", "Kappa", false).expect("an emote");
        assert!(!still.animated);
        assert_eq!(still.images.len(), 6);
        let moving = emote("emotesv2_1", "Dance", true).expect("an emote");
        assert!(moving.animated);
        assert_eq!(moving.images.len(), 12);
        let request = ImageRequest::default();
        assert!(
            moving
                .image(&request)
                .unwrap()
                .url
                .contains("/animated/dark/2.0")
        );
        let frozen = ImageRequest {
            animated: false,
            ..request
        };
        assert!(
            moving
                .image(&frozen)
                .unwrap()
                .url
                .contains("/static/dark/2.0")
        );
        assert!(emote("a b", "Bad", false).is_none());
    }

    #[test]
    fn helix_lists_are_read() {
        let set = parse_emotes(include_str!("../tests/fixtures/twitch-global.json"))
            .expect("the fixture is a list");
        let codes: Vec<&str> = set.emotes.iter().map(|e| e.code.as_str()).collect();
        assert_eq!(
            codes,
            [
                "TwitchUnity",
                "DemoDance",
                "DemoWave",
                "DemoGrin",
                "DemoNod",
                "Demo Space"
            ]
        );
        assert!(!set.emotes[0].animated && set.emotes[1].animated);
        let request = ImageRequest::default();
        assert_eq!(
            set.emotes[1].image(&request).unwrap().url,
            "https://static-cdn.jtvnw.net/emoticons/v2/emotesv2_a11/animated/dark/2.0"
        );
        assert_eq!(
            set.emotes[0].image(&request).unwrap().url,
            "https://static-cdn.jtvnw.net/emoticons/v2/196892/static/dark/2.0"
        );
    }

    #[test]
    fn a_bad_entry_is_skipped_and_a_bad_answer_is_an_error() {
        let body = r#"{"data":[
            {"id":"../x","name":"Bad","format":["static"]},
            {"id":"25","name":"Kappa","format":["static"]},
            {"id":"26","name":""},
            {"name":"NoId"}
        ]}"#;
        let set = parse_emotes(body).expect("a list");
        assert_eq!(set.emotes.len(), 1);
        assert_eq!(set.emotes[0].code, "Kappa");
        assert_eq!(set.skipped, 3);
        assert!(parse_emotes("{}").is_err());
        assert!(parse_emotes("not json").is_err());
    }
}
