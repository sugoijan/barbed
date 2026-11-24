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
use serde_json::value::RawValue;

use crate::model::{
    Emote, EmoteId, EmoteImage, EmoteProvider, EmoteSet, ImageFormat, ParseError, Theme,
};
use crate::safe::is_plain_id;
use crate::wire::read_set;

/// The host Twitch's emote images are on.
pub const IMAGE_HOST: &str = "static-cdn.jtvnw.net";

/// Where Helix is, without the scheme.
pub(crate) const API_HOST: &str = "api.twitch.tv/helix";

/// Helix's Get Global Emotes. It needs an app or a user token.
pub const GLOBAL_EMOTES_URL: &str = "https://api.twitch.tv/helix/chat/emotes/global";

/// The size steps Twitch serves: `1.0`, `2.0` and `3.0`.
pub const SCALES: [u8; 3] = [1, 2, 3];

const FORMATS: [&str; 2] = ["static", "animated"];
const THEMES: [&str; 2] = ["dark", "light"];
const SCALE_NAMES: [&str; 3] = ["1.0", "2.0", "3.0"];

/// Whether the segments of a path on [`IMAGE_HOST`] are an emote image of
/// the template: `emoticons/v2/<id>/<format>/<theme>/<scale>`.
pub(crate) fn is_image_path(path: &[&str]) -> bool {
    matches!(path, ["emoticons", "v2", id, format, theme, scale]
        if is_plain_id(id)
            && FORMATS.contains(format)
            && THEMES.contains(theme)
            && SCALE_NAMES.contains(scale))
}

/// Whether the segments of a path under [`API_HOST`] are an emote list the
/// proxy asks Helix for: Get Global Emotes alone.
pub(crate) fn is_api_path(path: &[&str]) -> bool {
    path == ["chat", "emotes", "global"]
}

/// The address of the image of an emote whose id is known to be plain.
fn template(id: &str, animated: bool, theme: Theme, scale: u8) -> String {
    let format = FORMATS[usize::from(animated)];
    format!(
        "https://{IMAGE_HOST}/emoticons/v2/{id}/{format}/{}/{scale}.0",
        theme.as_str()
    )
}

fn template_image(id: &str, animated: bool, theme: Theme, scale: u8) -> EmoteImage {
    let format = if animated {
        ImageFormat::Gif
    } else {
        ImageFormat::Png
    };
    EmoteImage::new(template(id, animated, theme, scale), scale)
        .animated(animated)
        .with_format(format)
        .with_theme(theme)
}

/// The address of emote `id`'s image: its still PNG, or its animated GIF.
/// `None` for an id of another shape than Twitch's ([`is_plain_id`]) or a
/// scale Twitch does not serve.
pub fn image_url(id: &str, animated: bool, theme: Theme, scale: u8) -> Option<String> {
    (is_plain_id(id) && SCALES.contains(&scale)).then(|| template(id, animated, theme, scale))
}

/// Emote `id`'s image ([`image_url`]) with what is known of it.
pub fn image(id: &str, animated: bool, theme: Theme, scale: u8) -> Option<EmoteImage> {
    (is_plain_id(id) && SCALES.contains(&scale)).then(|| template_image(id, animated, theme, scale))
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
    if !is_plain_id(id) {
        return None;
    }
    let motions: &[bool] = if animated { &[true, false] } else { &[false] };
    let mut images = Vec::with_capacity(motions.len() * themes.len() * scales.len());
    for moving in motions {
        for theme in themes {
            for scale in scales.iter().filter(|scale| SCALES.contains(scale)) {
                images.push(template_image(id, *moving, *theme, *scale));
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
            let at = SCALE_NAMES
                .iter()
                .position(|name| *name == scale.as_ref())?;
            Some(SCALES[at])
        })
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
struct WireList<'a> {
    #[serde(borrow)]
    data: Vec<&'a RawValue>,
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

fn emote_from_wire(wire: WireEmote) -> Option<Emote> {
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
    let list: WireList<'_> = serde_json::from_str(body)?;
    Ok(read_set(None, None, list.data, emote_from_wire))
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
