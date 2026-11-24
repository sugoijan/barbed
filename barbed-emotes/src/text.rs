//! A message cut into text and emotes.

use std::sync::Arc;

use crate::index::EmoteIndex;
use crate::model::{Emote, EmoteModifierFlags, ModifierTarget};

/// One emote of a message, with the modifiers attached to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmoteUse {
    pub emote: Arc<Emote>,
    /// The modifiers attached to it, in the order they were written, each
    /// one once: overlays drawn over it (7TV's zero-width emotes) and effects
    /// on it (FrankerFaceZ's and BetterTTV's modifiers).
    pub modifiers: Vec<Arc<Emote>>,
    /// The text this stands for: the emote's word, its modifiers' and the
    /// spaces between them.
    pub source: String,
}

impl EmoteUse {
    fn alone(emote: Arc<Emote>, source: &str) -> Self {
        Self {
            emote,
            modifiers: Vec::new(),
            source: source.to_string(),
        }
    }

    /// The effects of its modifiers together, without
    /// [`HIDDEN`](EmoteModifierFlags::HIDDEN).
    ///
    /// [`NO_SPACE`](EmoteModifierFlags::NO_SPACE) asks that the text before
    /// the emote be dropped where it is only a space. The tokenizer keeps
    /// that text: dropping it is the display's choice.
    pub fn flags(&self) -> EmoteModifierFlags {
        self.modifiers
            .iter()
            .filter_map(|modifier| modifier.modifier.as_ref())
            .fold(EmoteModifierFlags::empty(), |all, modifier| {
                all | modifier.flags
            })
            .difference(EmoteModifierFlags::HIDDEN)
    }

    /// The modifiers with a picture of their own, drawn over the emote in
    /// this order.
    pub fn overlays(&self) -> impl Iterator<Item = &Arc<Emote>> {
        self.modifiers
            .iter()
            .filter(|modifier| modifier.is_visible())
    }
}

/// A piece of a message, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Text between emotes, spaces and all.
    Text(String),
    Emote(EmoteUse),
}

impl Piece {
    /// The text this piece stands for. Every piece's, joined, is the message
    /// as it was given.
    pub fn source(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Emote(emote) => &emote.source,
        }
    }

    pub fn as_emote(&self) -> Option<&EmoteUse> {
        match self {
            Self::Emote(emote) => Some(emote),
            Self::Text(_) => None,
        }
    }

    /// Whether it is text of spaces alone.
    pub fn is_blank(&self) -> bool {
        matches!(self, Self::Text(text) if text.trim().is_empty())
    }
}

enum Raw {
    Space(String),
    Text(String),
    Emote(Arc<Emote>, String),
}

/// Cuts one message into [`Piece`]s.
///
/// A message is given in parts, in order: text to look for emote codes in
/// ([`text`](Self::text)), an emote the platform already named
/// ([`emote`](Self::emote): a Twitch emote of an EventSub fragment or of an
/// IRC `emotes` range), and text that is never an emote
/// ([`verbatim`](Self::verbatim): a mention, a cheermote, a link).
///
/// A code is a whole word between spaces, matched exactly
/// ([`EmoteIndex::get`]). A modifier is attached to its neighbor
/// ([`EmoteUse::modifiers`]) when nothing but spaces is between them:
///
/// - one that targets the emote before it (a 7TV zero-width emote, a
///   FrankerFaceZ modifier) joins that emote, whether the index or the
///   platform named it. With no emote before it, it is a piece of its own,
///   as its provider's client draws it;
/// - one that targets the emote after it (a BetterTTV `w!`) joins the next
///   emote. With none after it, it stays text.
///
/// ```
/// use barbed_emotes::{Emote, EmoteId, EmoteIndex, EmoteModifier, EmoteProvider, EmoteScope, Piece};
///
/// let seven = |id: &str| EmoteId::new(EmoteProvider::SevenTv, id);
/// let mut index = EmoteIndex::new();
/// index.insert(EmoteScope::Channel, Emote::new(seven("1"), "catJAM", Vec::new()));
/// index.insert(
///     EmoteScope::Global,
///     Emote::new(seven("2"), "RainTime", Vec::new()).with_modifier(EmoteModifier::overlay()),
/// );
///
/// let pieces = index.tokenize("so catJAM RainTime today");
/// assert_eq!(pieces.len(), 3);
/// assert_eq!(pieces[0], Piece::Text("so ".into()));
/// let cat = pieces[1].as_emote().unwrap();
/// assert_eq!(cat.emote.code, "catJAM");
/// assert_eq!(cat.modifiers[0].code, "RainTime");
/// assert_eq!(cat.source, "catJAM RainTime");
/// assert_eq!(pieces[2], Piece::Text(" today".into()));
/// ```
pub struct Tokenizer<'a> {
    index: &'a EmoteIndex,
    raw: Vec<Raw>,
}

impl<'a> Tokenizer<'a> {
    pub fn new(index: &'a EmoteIndex) -> Self {
        Self {
            index,
            raw: Vec::new(),
        }
    }

    /// Text whose words are looked up in the index.
    pub fn text(&mut self, text: &str) -> &mut Self {
        for (token, space) in split_keeping_spaces(text) {
            self.raw.push(if space {
                Raw::Space(token.to_string())
            } else {
                match self.index.get(token) {
                    Some(emote) => Raw::Emote(Arc::clone(emote), token.to_string()),
                    None => Raw::Text(token.to_string()),
                }
            });
        }
        self
    }

    /// Text that is kept as it is, never looked up.
    pub fn verbatim(&mut self, text: &str) -> &mut Self {
        if text.is_empty() {
            return self;
        }
        self.raw.push(if text.chars().all(char::is_whitespace) {
            Raw::Space(text.to_string())
        } else {
            Raw::Text(text.to_string())
        });
        self
    }

    /// An emote the platform named, written `source` in the message.
    pub fn emote(&mut self, emote: impl Into<Arc<Emote>>, source: &str) -> &mut Self {
        self.raw.push(Raw::Emote(emote.into(), source.to_string()));
        self
    }

    /// The message's pieces: neighboring text joined, modifiers attached.
    pub fn finish(self) -> Vec<Piece> {
        let raw = self.raw;
        let mut out: Vec<Piece> = Vec::new();
        let mut at = 0;
        while let Some(item) = raw.get(at) {
            let (emote, source) = match item {
                Raw::Space(text) | Raw::Text(text) => {
                    push_text(&mut out, text);
                    at += 1;
                    continue;
                }
                Raw::Emote(emote, source) => (emote, source),
            };
            if !targets_following(emote) {
                // An emote, or a modifier of the emote before it that has
                // none: a piece of its own, with what follows it attached.
                let mut piece = EmoteUse::alone(Arc::clone(emote), source);
                at = attach_followers(&raw, at + 1, &mut piece);
                out.push(Piece::Emote(piece));
                continue;
            }
            // Modifiers of the emote after them: all of them up to that
            // emote, or text where there is none.
            let mut end = at;
            let mut prefixes: Vec<&Arc<Emote>> = Vec::new();
            loop {
                match raw.get(end) {
                    Some(Raw::Emote(next, _)) if targets_following(next) => prefixes.push(next),
                    Some(Raw::Space(_)) => {}
                    _ => break,
                }
                end += 1;
            }
            match raw.get(end) {
                Some(Raw::Emote(base, base_source)) => {
                    let mut piece = EmoteUse::alone(Arc::clone(base), base_source);
                    piece.source = raw[at..=end].iter().map(source_of).collect();
                    for prefix in prefixes {
                        attach(&mut piece, prefix);
                    }
                    at = attach_followers(&raw, end + 1, &mut piece);
                    out.push(Piece::Emote(piece));
                }
                _ => {
                    push_text(&mut out, source);
                    at += 1;
                }
            }
        }
        out
    }
}

fn targets_following(emote: &Emote) -> bool {
    emote
        .modifier
        .as_ref()
        .is_some_and(|modifier| modifier.target == ModifierTarget::Following)
}

fn targets_preceding(emote: &Emote) -> bool {
    emote
        .modifier
        .as_ref()
        .is_some_and(|modifier| modifier.target == ModifierTarget::Preceding)
}

fn source_of(raw: &Raw) -> &str {
    match raw {
        Raw::Space(text) | Raw::Text(text) | Raw::Emote(_, text) => text,
    }
}

fn attach(piece: &mut EmoteUse, modifier: &Arc<Emote>) {
    if !piece.modifiers.iter().any(|had| had.id == modifier.id) {
        piece.modifiers.push(Arc::clone(modifier));
    }
}

/// Attaches to `piece` the modifiers of the emote before them that follow
/// it from `at`, spaces alone between them. The place after the last one.
fn attach_followers(raw: &[Raw], mut at: usize, piece: &mut EmoteUse) -> usize {
    loop {
        let mut next = at;
        while let Some(Raw::Space(_)) = raw.get(next) {
            next += 1;
        }
        match raw.get(next) {
            Some(Raw::Emote(modifier, _)) if targets_preceding(modifier) => {
                for item in &raw[at..=next] {
                    piece.source.push_str(source_of(item));
                }
                attach(piece, modifier);
                at = next + 1;
            }
            _ => return at,
        }
    }
}

fn push_text(out: &mut Vec<Piece>, text: &str) {
    match out.last_mut() {
        Some(Piece::Text(had)) => had.push_str(text),
        _ => out.push(Piece::Text(text.to_string())),
    }
}

/// `text` cut where spaces begin and end: each run and whether it is spaces.
fn split_keeping_spaces(text: &str) -> impl Iterator<Item = (&str, bool)> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        let space = first.is_whitespace();
        let end = rest
            .find(|c: char| c.is_whitespace() != space)
            .unwrap_or(rest.len());
        let (run, after) = rest.split_at(end);
        rest = after;
        Some((run, space))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::EmoteScope;
    use crate::model::{EmoteId, EmoteModifier, EmoteProvider};

    fn emote(provider: EmoteProvider, code: &str) -> Emote {
        Emote::new(EmoteId::new(provider, code), code, Vec::new())
    }

    fn index() -> EmoteIndex {
        let mut index = EmoteIndex::new();
        index.extend(
            EmoteScope::Global,
            [
                emote(EmoteProvider::SevenTv, "catJAM"),
                emote(EmoteProvider::BetterTtv, ":tf:"),
                emote(EmoteProvider::SevenTv, "RainTime").with_modifier(EmoteModifier::overlay()),
                emote(EmoteProvider::SevenTv, "Hat").with_modifier(EmoteModifier::overlay()),
                emote(EmoteProvider::FrankerFaceZ, "ffzRainbow").with_modifier(
                    EmoteModifier::effect(ModifierTarget::Preceding, EmoteModifierFlags::RAINBOW),
                ),
                emote(EmoteProvider::BetterTtv, "w!").with_modifier(EmoteModifier::effect(
                    ModifierTarget::Following,
                    EmoteModifierFlags::GROW_X,
                )),
                emote(EmoteProvider::BetterTtv, "h!").with_modifier(EmoteModifier::effect(
                    ModifierTarget::Following,
                    EmoteModifierFlags::FLIP_X,
                )),
            ],
        );
        index
    }

    /// The pieces in short: text in quotes, an emote by its code with its
    /// modifiers after a `+`.
    fn short(pieces: &[Piece]) -> Vec<String> {
        pieces
            .iter()
            .map(|piece| match piece {
                Piece::Text(text) => format!("{text:?}"),
                Piece::Emote(emote) => std::iter::once(emote.emote.code.as_str())
                    .chain(emote.modifiers.iter().map(|m| m.code.as_str()))
                    .collect::<Vec<_>>()
                    .join("+"),
            })
            .collect()
    }

    fn rejoined(pieces: &[Piece]) -> String {
        pieces.iter().map(Piece::source).collect()
    }

    #[test]
    fn words_are_matched_whole_and_text_is_kept_as_it_came() {
        let index = index();
        for (message, expected) in [
            ("", vec![]),
            ("hello  there", vec!["\"hello  there\""]),
            ("catJAM", vec!["catJAM"]),
            (
                "a catJAM  :tf: b",
                vec!["\"a \"", "catJAM", "\"  \"", ":tf:", "\" b\""],
            ),
            ("catJAM catJAM", vec!["catJAM", "\" \"", "catJAM"]),
            // Only a whole word between spaces.
            ("catJAM! xcatJAM catjam", vec!["\"catJAM! xcatJAM catjam\""]),
            (" catJAM\n", vec!["\" \"", "catJAM", "\"\\n\""]),
        ] {
            let pieces = index.tokenize(message);
            assert_eq!(short(&pieces), expected, "{message:?}");
            assert_eq!(rejoined(&pieces), message);
        }
    }

    #[test]
    fn modifiers_join_the_emote_before_them() {
        let index = index();
        for (message, expected) in [
            ("catJAM RainTime", vec!["catJAM+RainTime"]),
            (
                "catJAM  RainTime Hat ffzRainbow x",
                vec!["catJAM+RainTime+Hat+ffzRainbow", "\" x\""],
            ),
            // Each modifier once.
            ("catJAM RainTime RainTime", vec!["catJAM+RainTime"]),
            // Text between them: the modifier is a piece of its own, and
            // takes the modifiers after it.
            (
                "catJAM x RainTime Hat",
                vec!["catJAM", "\" x \"", "RainTime+Hat"],
            ),
            ("RainTime", vec!["RainTime"]),
            (
                "catJAM RainTime catJAM",
                vec!["catJAM+RainTime", "\" \"", "catJAM"],
            ),
        ] {
            let pieces = index.tokenize(message);
            assert_eq!(short(&pieces), expected, "{message:?}");
            assert_eq!(rejoined(&pieces), message);
        }
        let pieces = index.tokenize("catJAM RainTime ffzRainbow");
        let cat = pieces[0].as_emote().expect("an emote");
        assert_eq!(cat.source, "catJAM RainTime ffzRainbow");
        assert_eq!(cat.flags(), EmoteModifierFlags::RAINBOW);
        let overlays: Vec<&str> = cat.overlays().map(|e| e.code.as_str()).collect();
        assert_eq!(overlays, ["RainTime"]);
    }

    #[test]
    fn modifiers_join_the_emote_after_them() {
        let index = index();
        for (message, expected) in [
            ("w! catJAM", vec!["catJAM+w!"]),
            ("w! h!  catJAM RainTime", vec!["catJAM+w!+h!+RainTime"]),
            // No emote after it: text.
            ("w! hello", vec!["\"w! hello\""]),
            ("w!", vec!["\"w!\""]),
            ("w! h!", vec!["\"w! h!\""]),
            ("catJAM w!", vec!["catJAM", "\" w!\""]),
        ] {
            let pieces = index.tokenize(message);
            assert_eq!(short(&pieces), expected, "{message:?}");
            assert_eq!(rejoined(&pieces), message);
        }
        let pieces = index.tokenize("w! h! catJAM");
        let cat = pieces[0].as_emote().expect("an emote");
        assert_eq!(
            cat.flags(),
            EmoteModifierFlags::GROW_X | EmoteModifierFlags::FLIP_X
        );
        assert_eq!(cat.overlays().count(), 0);
    }

    #[test]
    fn a_platforms_emotes_and_verbatim_text_are_parts_of_a_message() {
        let index = index();
        let kappa = Arc::new(emote(EmoteProvider::Twitch, "Kappa"));
        let mut tokenizer = index.tokenizer();
        tokenizer
            .text("hi ")
            .emote(Arc::clone(&kappa), "Kappa")
            .text(" RainTime and ")
            .verbatim("@catJAM")
            .verbatim(" ")
            .text("catJAM");
        let pieces = tokenizer.finish();
        assert_eq!(
            short(&pieces),
            ["\"hi \"", "Kappa+RainTime", "\" and @catJAM \"", "catJAM"]
        );
        assert_eq!(rejoined(&pieces), "hi Kappa RainTime and @catJAM catJAM");
        assert!(!pieces[0].is_blank());
        // A code given verbatim is never an emote.
        let mut tokenizer = index.tokenizer();
        tokenizer.verbatim("catJAM").verbatim("");
        assert_eq!(short(&tokenizer.finish()), ["\"catJAM\""]);
    }
}
