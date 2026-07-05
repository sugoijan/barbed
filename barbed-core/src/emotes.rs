use bitflags::bitflags;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmoteProvider {
    Twitch,
    BetterTtv,
    SevenTv,
    FrankerFaceZ,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmoteImageFormat {
    Static,
    Animated,
    Other(String),
}

impl EmoteImageFormat {
    pub fn from_animated(animated: bool) -> Self {
        if animated {
            Self::Animated
        } else {
            Self::Static
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "static" => Self::Static,
            "animated" => Self::Animated,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Static => "static",
            Self::Animated => "animated",
            Self::Other(value) => value.as_str(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmoteThemeMode {
    Light,
    Dark,
    Other(String),
}

impl EmoteThemeMode {
    pub fn parse(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Other(value) => value.as_str(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmoteImageScale {
    One,
    Two,
    Three,
    Other(String),
}

impl EmoteImageScale {
    pub fn parse(value: &str) -> Self {
        match value {
            "1" | "1.0" => Self::One,
            "2" | "2.0" => Self::Two,
            "3" | "3.0" => Self::Three,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::One => "1.0",
            Self::Two => "2.0",
            Self::Three => "3.0",
            Self::Other(value) => value.as_str(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EmoteImage {
    pub format: EmoteImageFormat,
    pub theme_mode: EmoteThemeMode,
    pub scale: EmoteImageScale,
    pub url: String,
}

bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct EmoteModifierFlags: u32 {
        const HIDDEN = 1 << 0;
        const FLIP_X = 1 << 1;
        const FLIP_Y = 1 << 2;
        const GROW_X = 1 << 3;
        const RAINBOW = 1 << 11;
        const HYPER_RED = 1 << 12;
        const HYPER_SHAKE = 1 << 13;
        const CURSED = 1 << 14;
        const JAM = 1 << 15;
        const BOUNCE = 1 << 16;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmoteModifier {
    pub flags: EmoteModifierFlags,
    pub mask_images: Vec<EmoteImage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emote {
    pub id: EmoteId,
    pub code: String,
    pub images: Vec<EmoteImage>,
    pub modifier: Option<EmoteModifier>,
}

impl Emote {
    pub fn new(id: EmoteId, code: impl Into<String>, images: Vec<EmoteImage>) -> Self {
        Self {
            id,
            code: code.into(),
            images,
            modifier: None,
        }
    }

    pub fn with_modifier(mut self, modifier: EmoteModifier) -> Self {
        self.modifier = Some(modifier);
        self
    }

    pub fn is_animated(&self) -> bool {
        self.images
            .iter()
            .any(|image| matches!(image.format, EmoteImageFormat::Animated))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_format_parse_round_trips_known_values() {
        assert_eq!(EmoteImageFormat::parse("static").as_str(), "static");
        assert_eq!(EmoteImageFormat::parse("animated").as_str(), "animated");
    }

    #[test]
    fn image_scale_parses_provider_variants() {
        assert_eq!(EmoteImageScale::parse("1"), EmoteImageScale::One);
        assert_eq!(EmoteImageScale::parse("2.0"), EmoteImageScale::Two);
        assert_eq!(EmoteImageScale::parse("3"), EmoteImageScale::Three);
    }

    #[test]
    fn emote_modifier_preserves_flags() {
        let emote = Emote::new(
            EmoteId::new(EmoteProvider::SevenTv, "123"),
            "Wave",
            Vec::new(),
        )
        .with_modifier(EmoteModifier {
            flags: EmoteModifierFlags::RAINBOW,
            mask_images: Vec::new(),
        });

        assert!(!emote.is_animated());
        assert!(
            emote
                .modifier
                .as_ref()
                .expect("modifier should exist")
                .flags
                .contains(EmoteModifierFlags::RAINBOW)
        );
    }
}
