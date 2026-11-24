//! The provider-neutral emote model.

use std::fmt;

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// Where an emote comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EmoteProvider {
    Twitch,
    SevenTv,
    BetterTtv,
    FrankerFaceZ,
    /// Anything else: an application's own emotes, emoji, a test's pictures.
    Other(String),
}

impl EmoteProvider {
    /// A short lowercase name: `twitch`, `7tv`, `bttv`, `ffz`, or the
    /// [`Other`](Self::Other) provider's own.
    pub fn name(&self) -> &str {
        match self {
            Self::Twitch => "twitch",
            Self::SevenTv => "7tv",
            Self::BetterTtv => "bttv",
            Self::FrankerFaceZ => "ffz",
            Self::Other(name) => name,
        }
    }

    /// The host this provider serves its emote images from.
    pub fn image_host(&self) -> Option<&'static str> {
        match self {
            Self::Twitch => Some(crate::twitch::IMAGE_HOST),
            Self::SevenTv => Some(crate::seventv::IMAGE_HOST),
            Self::BetterTtv => Some(crate::bttv::IMAGE_HOST),
            Self::FrankerFaceZ => Some(crate::ffz::IMAGE_HOST),
            Self::Other(_) => None,
        }
    }
}

impl fmt::Display for EmoteProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// An emote's identity: its provider and the provider's id for it.
///
/// This is the key to keep images by. Two sets may list the same emote under
/// different codes (7TV's aliases), and they share one id.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EmoteId {
    pub provider: EmoteProvider,
    pub value: String,
}

impl EmoteId {
    pub fn new(provider: EmoteProvider, value: impl Into<String>) -> Self {
        Self {
            provider,
            value: value.into(),
        }
    }
}

impl fmt::Display for EmoteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.provider, self.value)
    }
}

/// An image file's container, where the provider says which it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageFormat {
    Png,
    Gif,
    WebP,
    Avif,
}

impl ImageFormat {
    /// Its media type.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
            Self::Avif => "image/avif",
        }
    }

    /// The format of a file extension (`webp`, `AVIF`, …).
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "gif" => Some(Self::Gif),
            "webp" => Some(Self::WebP),
            "avif" => Some(Self::Avif),
            _ => None,
        }
    }
}

/// The background an image is drawn for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Theme {
    Light,
    #[default]
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// One image file of an emote.
///
/// Images the parsers build are always `https` addresses on their provider's
/// image host ([`EmoteProvider::image_host`]).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EmoteImage {
    pub url: String,
    /// The provider's size step: 1 to 4. A 1× emote is about 28 to 32 pixels
    /// tall.
    pub scale: u8,
    /// Whether this file moves. An animated emote usually has still files
    /// too.
    pub animated: bool,
    /// `None` where the provider serves whatever the emote was uploaded as.
    pub format: Option<ImageFormat>,
    /// `None` where one image serves every theme (everyone but Twitch).
    pub theme: Option<Theme>,
    /// Pixels, where the provider says.
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl EmoteImage {
    /// A still image of unknown format and size.
    pub fn new(url: impl Into<String>, scale: u8) -> Self {
        Self {
            url: url.into(),
            scale,
            animated: false,
            format: None,
            theme: None,
            width: None,
            height: None,
        }
    }

    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }

    pub fn with_format(mut self, format: ImageFormat) -> Self {
        self.format = Some(format);
        self
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = Some(theme);
        self
    }

    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }
}

/// Which of an emote's images to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageRequest<'a> {
    /// The size step wanted. The nearest larger one is taken where it is
    /// missing, else the largest there is.
    pub scale: u8,
    /// Whether an animated emote's moving image is wanted. `false` asks for
    /// its still image, which only some providers have: check
    /// [`EmoteImage::animated`] on what comes back.
    pub animated: bool,
    pub theme: Theme,
    /// The formats the caller can decode, most wanted first. Empty: any. An
    /// image whose format is not known is always a candidate.
    pub formats: &'a [ImageFormat],
}

impl Default for ImageRequest<'_> {
    /// 2×, animated, dark theme, any format.
    fn default() -> Self {
        Self {
            scale: 2,
            animated: true,
            theme: Theme::Dark,
            formats: &[],
        }
    }
}

/// The image of `images` that fits `request` best, if any can be decoded.
pub fn pick_image<'a>(
    images: &'a [EmoteImage],
    request: &ImageRequest<'_>,
) -> Option<&'a EmoteImage> {
    let format_rank = |image: &EmoteImage| match image.format {
        _ if request.formats.is_empty() => Some(0),
        None => Some(request.formats.len()),
        Some(format) => request.formats.iter().position(|f| *f == format),
    };
    let moving = request.animated && images.iter().any(|image| image.animated);
    images
        .iter()
        .enumerate()
        .filter_map(|(at, image)| {
            let format = format_rank(image)?;
            let scale = match image.scale.cmp(&request.scale) {
                std::cmp::Ordering::Equal => (0, 0),
                std::cmp::Ordering::Greater => (1, image.scale - request.scale),
                std::cmp::Ordering::Less => (2, request.scale - image.scale),
            };
            let key = (
                image.animated != moving,
                image.theme.is_some_and(|theme| theme != request.theme),
                scale,
                format,
                at,
            );
            Some((key, image))
        })
        .min_by_key(|(key, _)| *key)
        .map(|(_, image)| image)
}

bitflags! {
    /// How a modifier changes the emote it is attached to.
    ///
    /// The low bits are FrankerFaceZ's `modifier_flags` as its API sends
    /// them, so bits it adds later are kept (`from_bits_retain`); the other
    /// providers' modifiers are mapped onto the same names. Bits from 24 up
    /// are this crate's own.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct EmoteModifierFlags: u32 {
        /// The modifier has no picture of its own to draw.
        const HIDDEN = 1 << 0;
        const FLIP_X = 1 << 1;
        const FLIP_Y = 1 << 2;
        /// Stretched wide.
        const GROW_X = 1 << 3;
        const SLIDE = 1 << 4;
        const APPEAR = 1 << 5;
        const LEAVE = 1 << 6;
        /// Spinning.
        const ROTATE = 1 << 7;
        /// Turned a quarter clockwise.
        const ROTATE_90 = 1 << 8;
        const GREYSCALE = 1 << 9;
        const SEPIA = 1 << 10;
        const RAINBOW = 1 << 11;
        const HYPER_RED = 1 << 12;
        const HYPER_SHAKE = 1 << 13;
        const CURSED = 1 << 14;
        const JAM = 1 << 15;
        const BOUNCE = 1 << 16;
        /// Drawn without the space before it, against the emote before.
        const NO_SPACE = 1 << 17;
        /// Turned a quarter counterclockwise (BetterTTV's `l!`).
        const ROTATE_270 = 1 << 24;
    }
}

/// Which emote a modifier is attached to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModifierTarget {
    /// The emote before it (7TV's zero-width emotes, FrankerFaceZ's
    /// modifiers).
    Preceding,
    /// The emote after it (BetterTTV's `w!`, `h!`, …).
    Following,
}

/// What makes an emote a modifier: it is not drawn by itself, but on or
/// around a neighbor.
///
/// A modifier without [`HIDDEN`](EmoteModifierFlags::HIDDEN) is an overlay:
/// its own image is drawn over its target, centered (a 7TV zero-width emote).
/// One with it only changes how the target is drawn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmoteModifier {
    pub target: ModifierTarget,
    pub flags: EmoteModifierFlags,
    /// Masks the effect is drawn through (FrankerFaceZ).
    pub mask_images: Vec<EmoteImage>,
}

impl EmoteModifier {
    /// An overlay on the emote before it, with no other effect.
    pub fn overlay() -> Self {
        Self {
            target: ModifierTarget::Preceding,
            flags: EmoteModifierFlags::empty(),
            mask_images: Vec::new(),
        }
    }

    /// An effect on `target` with no picture of its own.
    pub fn effect(target: ModifierTarget, flags: EmoteModifierFlags) -> Self {
        Self {
            target,
            flags: flags | EmoteModifierFlags::HIDDEN,
            mask_images: Vec::new(),
        }
    }

    pub fn is_hidden(&self) -> bool {
        self.flags.contains(EmoteModifierFlags::HIDDEN)
    }
}

/// One emote of a set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emote {
    pub id: EmoteId,
    /// The word that stands for it in a message. In a 7TV set this is the
    /// set's name for the emote (its alias), not the emote's own.
    pub code: String,
    /// Whether its provider animates it.
    pub animated: bool,
    /// The provider keeps it out of its emote menus and does not show its
    /// name (FrankerFaceZ's `hidden`). FrankerFaceZ's own client still draws
    /// it in a message that has its code; a display may choose not to.
    pub hidden: bool,
    pub images: Vec<EmoteImage>,
    /// Set where the emote is drawn on or changes a neighbor instead of
    /// standing by itself.
    pub modifier: Option<EmoteModifier>,
}

impl Emote {
    /// An emote that stands by itself, animated if any of `images` is.
    pub fn new(id: EmoteId, code: impl Into<String>, images: Vec<EmoteImage>) -> Self {
        Self {
            id,
            code: code.into(),
            animated: images.iter().any(|image| image.animated),
            hidden: false,
            images,
            modifier: None,
        }
    }

    pub fn with_modifier(mut self, modifier: EmoteModifier) -> Self {
        self.modifier = Some(modifier);
        self
    }

    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    /// Whether it stands by itself in a message: it is no modifier.
    pub fn is_standalone(&self) -> bool {
        self.modifier.is_none()
    }

    /// Whether it has a picture to draw: every emote but a modifier that is
    /// only an effect ([`EmoteModifier::is_hidden`]).
    pub fn is_visible(&self) -> bool {
        self.modifier.as_ref().is_none_or(|m| !m.is_hidden())
    }

    /// The image that fits `request` best ([`pick_image`]).
    pub fn image(&self, request: &ImageRequest<'_>) -> Option<&EmoteImage> {
        pick_image(&self.images, request)
    }
}

/// A list of emotes as one provider's answer gave it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmoteSet {
    pub id: Option<String>,
    pub name: Option<String>,
    pub emotes: Vec<Emote>,
    /// Entries of the answer left out: not an emote of the shape expected, or
    /// one whose id or images failed the checks of this crate. A whole set
    /// skipped usually means the provider changed its answer.
    pub skipped: usize,
}

impl EmoteSet {
    pub fn is_empty(&self) -> bool {
        self.emotes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.emotes.len()
    }
}

/// Why a provider's answer could not be read at all. A single bad entry is
/// never an error: it is left out and counted ([`EmoteSet::skipped`]).
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("not JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// JSON of another shape than the endpoint's.
    #[error("unexpected answer: {0}")]
    Shape(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Emote {
        let image = |name: &str, scale, animated, format| {
            EmoteImage::new(format!("https://cdn.example/{name}"), scale)
                .animated(animated)
                .with_format(format)
        };
        Emote::new(
            EmoteId::new(EmoteProvider::SevenTv, "a"),
            "catJAM",
            vec![
                image("1x.avif", 1, true, ImageFormat::Avif),
                image("1x.webp", 1, true, ImageFormat::WebP),
                image("1x_static.webp", 1, false, ImageFormat::WebP),
                image("2x.avif", 2, true, ImageFormat::Avif),
                image("2x.webp", 2, true, ImageFormat::WebP),
                image("2x_static.webp", 2, false, ImageFormat::WebP),
                image("4x.webp", 4, true, ImageFormat::WebP),
            ],
        )
    }

    fn picked<'a>(emote: &'a Emote, request: ImageRequest<'_>) -> &'a str {
        let url = &emote.image(&request).expect("an image").url;
        url.rsplit('/').next().expect("a file name")
    }

    #[test]
    fn the_image_asked_for_is_picked() {
        let emote = sample();
        assert!(emote.animated);
        let webp = [ImageFormat::WebP, ImageFormat::Gif, ImageFormat::Png];
        let request = ImageRequest {
            formats: &webp,
            ..ImageRequest::default()
        };
        assert_eq!(picked(&emote, request), "2x.webp");
        // Any format: the first listed at the scale.
        assert_eq!(picked(&emote, ImageRequest::default()), "2x.avif");
        // The still image of an animated emote.
        let still = ImageRequest {
            animated: false,
            ..request
        };
        assert_eq!(picked(&emote, still), "2x_static.webp");
        // A missing scale: the nearest larger one, else the largest.
        assert_eq!(
            picked(
                &emote,
                ImageRequest {
                    scale: 3,
                    ..request
                }
            ),
            "4x.webp"
        );
        let small = Emote::new(emote.id.clone(), "small", emote.images[..3].to_vec());
        assert_eq!(
            picked(
                &small,
                ImageRequest {
                    scale: 3,
                    ..request
                }
            ),
            "1x.webp"
        );
        // Nothing the caller can decode.
        let png = [ImageFormat::Png];
        assert!(
            emote
                .image(&ImageRequest {
                    formats: &png,
                    ..request
                })
                .is_none()
        );
    }

    #[test]
    fn a_theme_is_matched_and_an_unknown_format_is_a_candidate() {
        let images = vec![
            EmoteImage::new("light", 2).with_theme(Theme::Light),
            EmoteImage::new("dark", 2).with_theme(Theme::Dark),
        ];
        let request = ImageRequest {
            formats: &[ImageFormat::Png],
            ..ImageRequest::default()
        };
        assert_eq!(pick_image(&images, &request).unwrap().url, "dark");
        let light = ImageRequest {
            theme: Theme::Light,
            ..request
        };
        assert_eq!(pick_image(&images, &light).unwrap().url, "light");
        assert!(pick_image(&[], &request).is_none());
    }

    #[test]
    fn modifiers_are_not_standalone() {
        let id = EmoteId::new(EmoteProvider::FrankerFaceZ, "1");
        let plain = Emote::new(id.clone(), "Plain", Vec::new());
        assert!(plain.is_standalone() && plain.is_visible());
        let overlay = plain.clone().with_modifier(EmoteModifier::overlay());
        assert!(!overlay.is_standalone() && overlay.is_visible());
        let effect = plain.clone().with_modifier(EmoteModifier::effect(
            ModifierTarget::Preceding,
            EmoteModifierFlags::RAINBOW,
        ));
        assert!(!effect.is_standalone() && !effect.is_visible());
        // An emote its provider does not list is still an emote.
        let hidden = plain.with_hidden(true);
        assert!(hidden.hidden && hidden.is_standalone() && hidden.is_visible());
        assert_eq!(id.to_string(), "ffz:1");
    }
}
