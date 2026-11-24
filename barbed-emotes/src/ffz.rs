//! FrankerFaceZ's emote sets, from its v1 API.
//!
//! Both endpoints are public and need no token.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::model::{
    Emote, EmoteId, EmoteImage, EmoteModifier, EmoteModifierFlags, EmoteProvider, EmoteSet,
    ImageFormat, ModifierTarget, ParseError,
};
use crate::safe::{is_plain_id, path_on};
use crate::wire::{read_set, segments};

/// The host FrankerFaceZ's emote images are on.
pub const IMAGE_HOST: &str = "cdn.frankerfacez.com";

/// The host of FrankerFaceZ's API.
pub(crate) const API_HOST: &str = "api.frankerfacez.com";

/// FrankerFaceZ's global emote sets.
pub const GLOBAL_URL: &str = "https://api.frankerfacez.com/v1/set/global";

/// The address of the room of Twitch user `twitch_user_id`, with its emote
/// set. It answers `404` for a channel without FrankerFaceZ. `None` for an
/// id of another shape than Twitch's.
pub fn room_url(twitch_user_id: &str) -> Option<String> {
    is_plain_id(twitch_user_id).then(|| format!("https://{API_HOST}/v1/room/id/{twitch_user_id}"))
}

/// The size steps FrankerFaceZ serves.
const SCALES: [&str; 3] = ["1", "2", "4"];

/// Whether the segments of a path on [`IMAGE_HOST`] are an emote's image:
/// `emote/<id>/<scale>`, or `emote/<id>/animated/<scale>`.
pub(crate) fn is_image_path(path: &[&str]) -> bool {
    match path {
        ["emote", id, scale] | ["emote", id, "animated", scale] => {
            is_plain_id(id) && SCALES.contains(scale)
        }
        _ => false,
    }
}

/// Whether the segments of a path on [`API_HOST`] are a room's or the
/// global sets' address ([`room_url`], [`GLOBAL_URL`]).
pub(crate) fn is_api_path(path: &[&str]) -> bool {
    match path {
        ["v1", "room", "id", id] => is_plain_id(id),
        ["v1", "set", "global"] => true,
        _ => false,
    }
}

/// A number or a string, as FrankerFaceZ writes ids.
#[derive(Deserialize)]
#[serde(untagged)]
enum WireId {
    Number(u64),
    Text(String),
}

impl WireId {
    fn into_string(self) -> String {
        match self {
            Self::Number(number) => number.to_string(),
            Self::Text(text) => text,
        }
    }
}

type WireUrls = BTreeMap<String, Option<String>>;

#[derive(Deserialize)]
struct WireEmote {
    id: WireId,
    name: String,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    modifier: bool,
    #[serde(default)]
    modifier_flags: u32,
    #[serde(default)]
    urls: WireUrls,
    #[serde(default)]
    animated: Option<WireUrls>,
    #[serde(default)]
    mask: Option<WireUrls>,
    #[serde(default)]
    mask_animated: Option<WireUrls>,
}

#[derive(Deserialize)]
struct WireSet<'a> {
    #[serde(default)]
    id: Option<WireId>,
    #[serde(default)]
    title: Option<String>,
    #[serde(borrow, default)]
    emoticons: Vec<&'a RawValue>,
}

/// Which addresses a `{"1": url, "2": url, "4": url}` map may hold.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// An emote's own image: only an address of the shape they all have
    /// ([`is_image_path`]), so that the proxy carries every one of them.
    Emote,
    /// A modifier's mask: any plain path on the image host.
    Mask,
}

/// The images of a `{"1": url, "2": url, "4": url}` map that are on
/// FrankerFaceZ's image host, smallest first.
fn images(
    urls: &WireUrls,
    kind: Kind,
    animated: bool,
    size: Option<(f64, f64)>,
) -> Vec<EmoteImage> {
    let mut images: Vec<EmoteImage> = urls
        .iter()
        .filter_map(|(scale, url)| {
            let scale: u8 = scale.parse().ok().filter(|scale| (1..=4).contains(scale))?;
            let path = path_on(IMAGE_HOST, url.as_deref()?)?;
            if kind == Kind::Emote && !is_image_path(&segments(path)) {
                return None;
            }
            let mut image = EmoteImage::new(format!("https://{IMAGE_HOST}{path}"), scale)
                .animated(animated)
                .with_format(if animated {
                    ImageFormat::WebP
                } else {
                    ImageFormat::Png
                });
            // The answer's size is the 1× image's.
            if let Some((width, height)) = size
                && width >= 1.0
                && height >= 1.0
            {
                let scaled = |side: f64| (side * f64::from(scale)).round() as u32;
                image = image.with_size(scaled(width), scaled(height));
            }
            Some(image)
        })
        .collect();
    images.sort_by_key(|image| image.scale);
    images
}

fn emote_from_wire(wire: WireEmote) -> Option<Emote> {
    let id = wire.id.into_string();
    if wire.name.is_empty() || !is_plain_id(&id) {
        return None;
    }
    let size = wire.width.zip(wire.height);
    // The animated WebP files first where it has them, then its still ones.
    let mut all = wire
        .animated
        .as_ref()
        .map(|urls| images(urls, Kind::Emote, true, size))
        .unwrap_or_default();
    all.extend(images(&wire.urls, Kind::Emote, false, size));
    if all.is_empty() {
        return None;
    }
    let mut emote = Emote::new(
        EmoteId::new(EmoteProvider::FrankerFaceZ, id),
        wire.name,
        all,
    )
    .with_hidden(wire.hidden);
    if wire.modifier {
        let mut mask_images = wire
            .mask_animated
            .as_ref()
            .map(|urls| images(urls, Kind::Mask, true, size))
            .unwrap_or_default();
        if let Some(urls) = &wire.mask {
            mask_images.extend(images(urls, Kind::Mask, false, size));
        }
        emote.modifier = Some(EmoteModifier {
            target: ModifierTarget::Preceding,
            flags: EmoteModifierFlags::from_bits_retain(wire.modifier_flags),
            mask_images,
        });
    }
    Some(emote)
}

/// The sets of a `{"<id>": set}` map in the order of their ids, and how many
/// of them were of another shape.
fn sets_from_wire(sets: BTreeMap<String, &RawValue>) -> (Vec<EmoteSet>, usize) {
    let mut out: Vec<EmoteSet> = Vec::new();
    let mut skipped = 0;
    for (key, value) in sets {
        match serde_json::from_str::<WireSet<'_>>(value.get()) {
            Ok(wire) => out.push(read_set(
                Some(wire.id.map(WireId::into_string).unwrap_or(key)),
                wire.title,
                wire.emoticons,
                emote_from_wire,
            )),
            Err(_) => skipped += 1,
        }
    }
    let number = |set: &EmoteSet| set.id.as_deref().and_then(|id| id.parse::<u64>().ok());
    out.sort_by(|a, b| number(a).cmp(&number(b)).then_with(|| a.id.cmp(&b.id)));
    (out, skipped)
}

/// What [`GLOBAL_URL`] answered: every global set, which of them everyone
/// has, and which belong to certain users.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FfzGlobal {
    /// Every set of the answer, in the order of their ids.
    pub sets: Vec<EmoteSet>,
    /// The ids of the sets every chatter has.
    pub default_set_ids: Vec<String>,
    /// For a set that is not a default one, the logins of the users who have
    /// it.
    pub user_sets: BTreeMap<String, Vec<String>>,
    /// Sets of the answer that could not be read.
    pub skipped_sets: usize,
}

impl FfzGlobal {
    /// The sets every chatter has, in the answer's `default_sets` order.
    pub fn default_sets(&self) -> impl Iterator<Item = &EmoteSet> {
        self.default_set_ids
            .iter()
            .filter_map(|id| self.sets.iter().find(|set| set.id.as_ref() == Some(id)))
    }

    /// The emotes of the default sets: what is global for everyone.
    pub fn default_emotes(&self) -> Vec<Emote> {
        self.default_sets()
            .flat_map(|set| set.emotes.iter().cloned())
            .collect()
    }

    /// The sets user `login` has beyond the default ones.
    pub fn sets_for_user<'a>(&'a self, login: &'a str) -> impl Iterator<Item = &'a EmoteSet> {
        self.sets.iter().filter(move |set| {
            set.id
                .as_ref()
                .and_then(|id| self.user_sets.get(id))
                .is_some_and(|users| users.iter().any(|user| user == login))
        })
    }

    /// Entries left out of the default sets ([`EmoteSet::skipped`]).
    pub fn skipped(&self) -> usize {
        self.default_sets().map(|set| set.skipped).sum::<usize>() + self.skipped_sets
    }
}

#[derive(Deserialize)]
struct WireGlobal<'a> {
    #[serde(default)]
    default_sets: Vec<WireId>,
    #[serde(borrow)]
    sets: BTreeMap<String, &'a RawValue>,
    #[serde(default)]
    users: BTreeMap<String, Vec<String>>,
}

/// Reads the answer of [`GLOBAL_URL`] (`{"default_sets": [...], "sets":
/// {...}, "users": {...}}`).
///
/// Each emote has its still PNG files at the scales the answer gives (1, 2
/// and 4), after its animated WebP files where it has them. A modifier is an
/// effect on the emote before it ([`EmoteModifierFlags`] are FrankerFaceZ's
/// own bits), drawn over it too unless its flags say it is hidden. An emote
/// FrankerFaceZ does not list is marked ([`Emote::hidden`]), not left out.
pub fn parse_global(body: &str) -> Result<FfzGlobal, ParseError> {
    let wire: WireGlobal<'_> = serde_json::from_str(body)?;
    let (sets, skipped_sets) = sets_from_wire(wire.sets);
    Ok(FfzGlobal {
        sets,
        default_set_ids: wire
            .default_sets
            .into_iter()
            .map(WireId::into_string)
            .collect(),
        user_sets: wire
            .users
            .into_iter()
            .filter(|(_, users)| !users.is_empty())
            .collect(),
        skipped_sets,
    })
}

/// What [`room_url`] answered: a channel's room and its emote sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FfzRoom {
    /// The channel's Twitch user id.
    pub twitch_id: Option<String>,
    /// The channel's login.
    pub login: Option<String>,
    /// The id of the room's own set.
    pub set_id: Option<String>,
    /// Every set of the answer, in the order of their ids.
    pub sets: Vec<EmoteSet>,
    /// Sets of the answer that could not be read.
    pub skipped_sets: usize,
}

impl FfzRoom {
    /// The room's own set.
    pub fn emote_set(&self) -> Option<&EmoteSet> {
        let id = self.set_id.as_ref()?;
        self.sets.iter().find(|set| set.id.as_ref() == Some(id))
    }

    /// The emotes of the room's own set, or of every set of the answer where
    /// the room names none.
    pub fn into_emotes(self) -> Vec<Emote> {
        let own = self.set_id.clone();
        self.sets
            .into_iter()
            .filter(|set| own.is_none() || set.id == own)
            .flat_map(|set| set.emotes)
            .collect()
    }

    /// Entries left out ([`EmoteSet::skipped`]).
    pub fn skipped(&self) -> usize {
        self.sets.iter().map(|set| set.skipped).sum::<usize>() + self.skipped_sets
    }
}

#[derive(Deserialize)]
struct WireRoomAnswer<'a> {
    room: WireRoom,
    #[serde(borrow, default)]
    sets: BTreeMap<String, &'a RawValue>,
}

#[derive(Deserialize)]
struct WireRoom {
    #[serde(default)]
    twitch_id: Option<WireId>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    set: Option<WireId>,
}

/// Reads the answer of [`room_url`] (`{"room": {"set": <id>, …}, "sets":
/// {...}}`).
pub fn parse_room(body: &str) -> Result<FfzRoom, ParseError> {
    let wire: WireRoomAnswer<'_> = serde_json::from_str(body)?;
    let (sets, skipped_sets) = sets_from_wire(wire.sets);
    Ok(FfzRoom {
        twitch_id: wire.room.twitch_id.map(WireId::into_string),
        login: wire.room.id,
        set_id: wire.room.set.map(WireId::into_string),
        sets,
        skipped_sets,
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
    fn a_rooms_set_is_read_with_its_animated_files() {
        let room = parse_room(include_str!("../tests/fixtures/ffz-room.json")).expect("a room");
        assert_eq!(room.twitch_id.as_deref(), Some("1971641"));
        assert_eq!(room.login.as_deref(), Some("streamer"));
        assert_eq!(
            room.emote_set().and_then(|set| set.name.as_deref()),
            Some("Channel: Streamer")
        );
        let emotes = room.into_emotes();
        assert_eq!(codes(&emotes), ["LilZ", "LilDance"]);
        let request = ImageRequest::default();
        assert!(!emotes[0].animated && emotes[1].animated);
        let still = emotes[0].image(&request).unwrap();
        assert_eq!(still.url, "https://cdn.frankerfacez.com/emote/28136/2");
        assert_eq!((still.width, still.height), (Some(64), Some(64)));
        assert_eq!(
            emotes[1].image(&request).unwrap().url,
            "https://cdn.frankerfacez.com/emote/720507/animated/2"
        );
        let frozen = ImageRequest {
            animated: false,
            ..request
        };
        assert_eq!(
            emotes[1].image(&frozen).unwrap().url,
            "https://cdn.frankerfacez.com/emote/720507/2"
        );
        // A scale FrankerFaceZ does not have: the next larger one.
        let three = ImageRequest {
            scale: 3,
            ..request
        };
        assert!(emotes[0].image(&three).unwrap().url.ends_with("/4"));
    }

    #[test]
    fn the_default_global_sets_are_told_from_the_others() {
        let global =
            parse_global(include_str!("../tests/fixtures/ffz-global.json")).expect("the sets");
        assert_eq!(global.sets.len(), 3);
        assert_eq!(global.default_set_ids, ["3", "7002"]);
        let emotes = global.default_emotes();
        assert_eq!(
            codes(&emotes),
            ["ZreknarF", "HiddenOne", "FzSecond", "ffzHyper"]
        );
        // What a display that draws neither modifiers nor unlisted emotes
        // keeps.
        let alone: Vec<&str> = emotes
            .iter()
            .filter(|emote| emote.is_standalone() && !emote.hidden)
            .map(|emote| emote.code.as_str())
            .collect();
        assert_eq!(alone, ["ZreknarF", "FzSecond"]);
        let hyper = emotes[3].modifier.as_ref().expect("a modifier");
        assert_eq!(hyper.target, ModifierTarget::Preceding);
        assert!(hyper.is_hidden());
        assert!(global.user_sets.is_empty());
        assert_eq!(global.skipped(), 0);
    }

    #[test]
    fn sets_of_certain_users_are_kept_apart() {
        let global =
            parse_global(include_str!("../tests/fixtures/ffz-user-sets.json")).expect("the sets");
        assert_eq!(codes(&global.default_emotes()), ["FFZWave"]);
        let scoped: Vec<_> = global.sets_for_user("42").collect();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].name.as_deref(), Some("Scoped FFZ"));
        assert_eq!(global.sets_for_user("nobody").count(), 0);
    }

    #[test]
    fn a_modifier_keeps_its_flags_and_masks() {
        let room =
            parse_room(include_str!("../tests/fixtures/ffz-room-modifier.json")).expect("a room");
        // A room that names no set: every set of the answer.
        assert_eq!(room.set_id, None);
        let emotes = room.into_emotes();
        let modifier = emotes[0].modifier.as_ref().expect("a modifier");
        assert!(modifier.flags.contains(EmoteModifierFlags::HIDDEN));
        assert!(modifier.flags.contains(EmoteModifierFlags::RAINBOW));
        assert_eq!(
            modifier.mask_images[0].url,
            "https://cdn.frankerfacez.com/emote/700/mask/1"
        );
        assert!(emotes[0].hidden && !emotes[0].is_visible());
    }

    #[test]
    fn images_stay_on_frankerfacezs_host() {
        let body = r#"{"room":{"set":1},"sets":{"1":{"emoticons":[
            {"id":1,"name":"Up","urls":{"2":"https://cdn.frankerfacez.com/emote/1/../../me"}},
            {"id":2,"name":"Query","urls":{"2":"https://cdn.frankerfacez.com/emote/2/2?x=/../"}},
            {"id":3,"name":"Away","urls":{"2":"https://evil.example/emote/3/2"}},
            {"id":4,"name":"Fine","urls":{"2":"//cdn.frankerfacez.com/emote/4/2","4":null}},
            {"id":"../5","name":"BadId","urls":{"2":"https://cdn.frankerfacez.com/emote/5/2"}}
        ]}}}"#;
        let room = parse_room(body).expect("a room");
        assert_eq!(room.skipped(), 4);
        let emotes = room.into_emotes();
        assert_eq!(codes(&emotes), ["Fine"]);
        assert_eq!(
            emotes[0].images[0].url,
            "https://cdn.frankerfacez.com/emote/4/2"
        );
        assert!(parse_room("{}").is_err());
        assert!(parse_global("[]").is_err());
        assert_eq!(room_url("a b"), None);
        assert_eq!(
            room_url("9").as_deref(),
            Some("https://api.frankerfacez.com/v1/room/id/9")
        );
    }
}
