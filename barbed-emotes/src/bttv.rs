//! BetterTTV's emotes, from its v3 cached API.
//!
//! Both endpoints are public and need no token. BetterTTV's image host sends
//! no CORS headers, so a browser that reads pixels (a canvas, WebGL) has to
//! ask for these images through a proxy ([`crate::proxy`]).

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::model::{
    Emote, EmoteId, EmoteImage, EmoteModifier, EmoteModifierFlags, EmoteProvider, EmoteSet,
    ImageFormat, ModifierTarget, ParseError,
};
use crate::safe::is_plain_id;
use crate::wire::{expect_object, read_set};

/// The host BetterTTV's emote images are on.
pub const IMAGE_HOST: &str = "cdn.betterttv.net";

/// The host of BetterTTV's API.
pub(crate) const API_HOST: &str = "api.betterttv.net";

/// BetterTTV's global emotes.
pub const GLOBAL_URL: &str = "https://api.betterttv.net/3/cached/emotes/global";

/// The address of the channel and shared emotes of Twitch user
/// `twitch_user_id`. It answers `404` for a channel without BetterTTV.
/// `None` for an id of another shape than Twitch's.
pub fn channel_url(twitch_user_id: &str) -> Option<String> {
    is_plain_id(twitch_user_id)
        .then(|| format!("https://{API_HOST}/3/cached/users/twitch/{twitch_user_id}"))
}

/// The files of an emote as it was uploaded, by scale.
const UPLOADED: [&str; 3] = ["1x", "2x", "3x"];
/// Its WebP files, and under `static/` an animated emote's still ones.
const WEBP: [&str; 3] = ["1x.webp", "2x.webp", "3x.webp"];

/// Whether the segments of a path on [`IMAGE_HOST`] are an emote's image:
/// `emote/<id>/<n>x[.webp]` or `emote/<id>/static/<n>x.webp`.
pub(crate) fn is_image_path(path: &[&str]) -> bool {
    match path {
        ["emote", id, file] => is_plain_id(id) && (UPLOADED.contains(file) || WEBP.contains(file)),
        ["emote", id, "static", file] => is_plain_id(id) && WEBP.contains(file),
        _ => false,
    }
}

/// Whether the segments of a path on [`API_HOST`] are a channel's or the
/// global emotes' address ([`channel_url`], [`GLOBAL_URL`]).
pub(crate) fn is_api_path(path: &[&str]) -> bool {
    match path {
        ["3", "cached", "users", "twitch", id] => is_plain_id(id),
        ["3", "cached", "emotes", "global"] => true,
        _ => false,
    }
}

/// What a modifier's code does to the emote after it, as BetterTTV's own
/// client draws them.
fn modifier_effect(code: &str) -> EmoteModifierFlags {
    match code {
        "w!" => EmoteModifierFlags::GROW_X,
        "h!" => EmoteModifierFlags::FLIP_X,
        "v!" => EmoteModifierFlags::FLIP_Y,
        "z!" => EmoteModifierFlags::NO_SPACE,
        "c!" => EmoteModifierFlags::CURSED,
        "l!" => EmoteModifierFlags::ROTATE_270,
        "r!" => EmoteModifierFlags::ROTATE_90,
        "p!" => EmoteModifierFlags::RAINBOW,
        "s!" => EmoteModifierFlags::HYPER_SHAKE,
        _ => EmoteModifierFlags::empty(),
    }
}

#[derive(Deserialize)]
struct WireEmote {
    id: String,
    code: String,
    #[serde(rename = "imageType", default)]
    image_type: String,
    #[serde(default)]
    animated: bool,
    #[serde(default)]
    modifier: bool,
}

fn emote_from_wire(wire: WireEmote) -> Option<Emote> {
    if wire.code.is_empty() || !is_plain_id(&wire.id) {
        return None;
    }
    let animated = wire.animated || wire.image_type.eq_ignore_ascii_case("gif");
    let uploaded = ImageFormat::from_extension(&wire.image_type);
    let at = |path: &str| format!("https://{IMAGE_HOST}/emote/{}/{path}", wire.id);
    let mut images = Vec::with_capacity(9);
    for (scale, (original, webp)) in (1u8..).zip(UPLOADED.into_iter().zip(WEBP)) {
        // The emote as it was uploaded (a GIF where it is animated).
        let mut image = EmoteImage::new(at(original), scale).animated(animated);
        image.format = uploaded;
        images.push(image);
        images.push(
            EmoteImage::new(at(webp), scale)
                .animated(animated)
                .with_format(ImageFormat::WebP),
        );
        if animated {
            images.push(
                EmoteImage::new(at(&format!("static/{webp}")), scale)
                    .with_format(ImageFormat::WebP),
            );
        }
    }
    let mut emote = Emote::new(
        EmoteId::new(EmoteProvider::BetterTtv, wire.id),
        wire.code,
        images,
    );
    emote.animated = animated;
    if wire.modifier {
        emote.modifier = Some(EmoteModifier::effect(
            ModifierTarget::Following,
            modifier_effect(&emote.code),
        ));
    }
    Some(emote)
}

fn set_from_wire(name: &str, entries: Vec<&RawValue>) -> EmoteSet {
    read_set(None, Some(name.to_string()), entries, emote_from_wire)
}

/// Reads the answer of [`GLOBAL_URL`] (an array of emotes).
///
/// Each emote has, at three scales, the file as it was uploaded (PNG, or a
/// GIF where animated), its WebP, and an animated emote's still WebP. A
/// modifier (`w!`, `h!`, …) is an [effect](EmoteModifier::effect) on the
/// emote after it.
pub fn parse_global(body: &str) -> Result<EmoteSet, ParseError> {
    let entries: Vec<&RawValue> = serde_json::from_str(body)?;
    Ok(set_from_wire("BetterTTV Global", entries))
}

/// A channel's BetterTTV emotes: its own and the ones it shares from other
/// channels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BttvChannel {
    pub channel: EmoteSet,
    pub shared: EmoteSet,
}

impl BttvChannel {
    pub fn is_empty(&self) -> bool {
        self.channel.is_empty() && self.shared.is_empty()
    }

    /// The channel's emotes followed by its shared ones.
    pub fn into_emotes(self) -> Vec<Emote> {
        let mut emotes = self.channel.emotes;
        emotes.extend(self.shared.emotes);
        emotes
    }

    /// Entries of the answer left out ([`EmoteSet::skipped`]).
    pub fn skipped(&self) -> usize {
        self.channel.skipped + self.shared.skipped
    }
}

#[derive(Deserialize)]
struct WireChannel<'a> {
    #[serde(rename = "channelEmotes", borrow, default)]
    channel_emotes: Vec<&'a RawValue>,
    #[serde(rename = "sharedEmotes", borrow, default)]
    shared_emotes: Vec<&'a RawValue>,
}

/// Reads the answer of [`channel_url`] (`{"channelEmotes": [...],
/// "sharedEmotes": [...]}`).
pub fn parse_channel(body: &str) -> Result<BttvChannel, ParseError> {
    expect_object(body, "a BetterTTV channel is an object")?;
    let wire: WireChannel<'_> = serde_json::from_str(body)?;
    Ok(BttvChannel {
        channel: set_from_wire("Channel Emotes", wire.channel_emotes),
        shared: set_from_wire("Shared Emotes", wire.shared_emotes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ImageRequest;

    fn codes(emotes: &[Emote]) -> Vec<&str> {
        emotes.iter().map(|emote| emote.code.as_str()).collect()
    }

    #[test]
    fn a_channels_and_its_shared_emotes_are_read() {
        let channel =
            parse_channel(include_str!("../tests/fixtures/bttv-user.json")).expect("a channel");
        assert_eq!(codes(&channel.channel.emotes), ["PogBTTV", "catJAM"]);
        assert_eq!(codes(&channel.shared.emotes), ["OhMyGoodness"]);
        assert_eq!(channel.skipped(), 0);
        let emotes = channel.into_emotes();
        assert!(!emotes[0].animated && emotes[1].animated);
        // As uploaded: a GIF where animated.
        let uploaded = [ImageFormat::Gif, ImageFormat::Png];
        let request = ImageRequest {
            formats: &uploaded,
            ..ImageRequest::default()
        };
        let cat = emotes[1].image(&request).unwrap();
        assert_eq!(
            cat.url,
            "https://cdn.betterttv.net/emote/5f1b0186cf6d2144653d2971/2x"
        );
        assert!(cat.animated);
        assert_eq!(
            emotes[0].image(&request).unwrap().url,
            "https://cdn.betterttv.net/emote/5f1b0186cf6d2144653d2970/2x"
        );
        // Its WebP, and an animated emote's still one.
        let webp = [ImageFormat::WebP];
        let request = ImageRequest {
            formats: &webp,
            ..request
        };
        assert!(emotes[1].image(&request).unwrap().url.ends_with("/2x.webp"));
        let still = ImageRequest {
            animated: false,
            ..request
        };
        assert!(
            emotes[1]
                .image(&still)
                .unwrap()
                .url
                .ends_with("/static/2x.webp")
        );
    }

    #[test]
    fn modifiers_change_the_emote_after_them() {
        let set = parse_global(include_str!("../tests/fixtures/bttv-global.json")).expect("a set");
        assert_eq!(codes(&set.emotes), [":tf:", "DatSauce", "w!"]);
        // The fixture's note is an entry that is no emote.
        assert_eq!(set.skipped, 1);
        let wide = set.emotes[2].modifier.as_ref().expect("a modifier");
        assert_eq!(wide.target, ModifierTarget::Following);
        assert!(wide.is_hidden());
        assert!(wide.flags.contains(EmoteModifierFlags::GROW_X));
        assert!(set.emotes[0].is_standalone() && !set.emotes[2].is_visible());
    }

    #[test]
    fn ids_of_another_shape_are_left_out() {
        let body = r#"{"channelEmotes":[
            {"id":"../../../me","code":"Bad"},
            {"id":"5f1b0186cf6d2144653d2970","code":"Good"}
        ],"sharedEmotes":[]}"#;
        let channel = parse_channel(body).expect("a channel");
        assert_eq!(codes(&channel.channel.emotes), ["Good"]);
        assert_eq!(channel.skipped(), 1);
        assert!(parse_channel("[]").is_err());
        assert!(parse_global("{}").is_err());
        assert_eq!(channel_url("../../me"), None);
        assert_eq!(
            channel_url("1971641").as_deref(),
            Some("https://api.betterttv.net/3/cached/users/twitch/1971641")
        );
    }
}
