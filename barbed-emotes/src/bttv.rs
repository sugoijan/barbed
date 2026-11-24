//! BetterTTV's emotes, from its v3 cached API.
//!
//! Both endpoints are public and need no token. BetterTTV's image host sends
//! no CORS headers, so a browser that reads pixels (a canvas, WebGL) has to
//! ask for these images through a proxy ([`crate::proxy`]).

use serde::Deserialize;
use serde_json::Value;

use crate::model::{
    Emote, EmoteId, EmoteImage, EmoteModifier, EmoteModifierFlags, EmoteProvider, EmoteSet,
    ImageFormat, ModifierTarget, ParseError,
};
use crate::safe::{https, is_plain_id};

/// The host BetterTTV's emote images are on.
pub const IMAGE_HOST: &str = "cdn.betterttv.net";

/// BetterTTV's global emotes.
pub const GLOBAL_URL: &str = "https://api.betterttv.net/3/cached/emotes/global";

/// The address of the channel and shared emotes of Twitch user
/// `twitch_user_id`. It answers `404` for a channel without BetterTTV.
/// `None` for an id of another shape than Twitch's.
pub fn channel_url(twitch_user_id: &str) -> Option<String> {
    is_plain_id(twitch_user_id)
        .then(|| format!("https://api.betterttv.net/3/cached/users/twitch/{twitch_user_id}"))
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

fn emote_from_wire(value: Value) -> Option<Emote> {
    let wire: WireEmote = serde_json::from_value(value).ok()?;
    if wire.code.is_empty() || !is_plain_id(&wire.id) {
        return None;
    }
    let animated = wire.animated || wire.image_type.eq_ignore_ascii_case("gif");
    let uploaded = ImageFormat::from_extension(&wire.image_type);
    let at = |path: String| https(IMAGE_HOST, &format!("/emote/{}/{path}", wire.id));
    let mut images = Vec::new();
    for scale in 1..=3u8 {
        // The emote as it was uploaded (a GIF where it is animated).
        let mut original = EmoteImage::new(at(format!("{scale}x")), scale).animated(animated);
        original.format = uploaded;
        images.push(original);
        images.push(
            EmoteImage::new(at(format!("{scale}x.webp")), scale)
                .animated(animated)
                .with_format(ImageFormat::WebP),
        );
        if animated {
            images.push(
                EmoteImage::new(at(format!("static/{scale}x.webp")), scale)
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

fn set_from_wire(name: &str, entries: Vec<Value>) -> EmoteSet {
    let mut set = EmoteSet {
        name: Some(name.to_string()),
        ..EmoteSet::default()
    };
    for value in entries {
        match emote_from_wire(value) {
            Some(emote) => set.emotes.push(emote),
            None => set.skipped += 1,
        }
    }
    set
}

/// Reads the answer of [`GLOBAL_URL`] (an array of emotes).
///
/// Each emote has, at three scales, the file as it was uploaded (PNG, or a
/// GIF where animated), its WebP, and an animated emote's still WebP. A
/// modifier (`w!`, `h!`, …) is an [effect](EmoteModifier::effect) on the
/// emote after it.
pub fn parse_global(body: &str) -> Result<EmoteSet, ParseError> {
    let entries: Vec<Value> = serde_json::from_str(body)?;
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
struct WireChannel {
    #[serde(rename = "channelEmotes", default)]
    channel_emotes: Vec<Value>,
    #[serde(rename = "sharedEmotes", default)]
    shared_emotes: Vec<Value>,
}

/// Reads the answer of [`channel_url`] (`{"channelEmotes": [...],
/// "sharedEmotes": [...]}`).
pub fn parse_channel(body: &str) -> Result<BttvChannel, ParseError> {
    let value: Value = serde_json::from_str(body)?;
    if !value.is_object() {
        return Err(ParseError::Shape("a BetterTTV channel is an object"));
    }
    let wire: WireChannel = serde_json::from_value(value)?;
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
