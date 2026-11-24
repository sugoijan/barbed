//! A message cut into text and emotes.

use std::ops::Range;
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
    /// Where [`source`](Self::source) is in the message, in bytes
    /// ([`Piece::range`]).
    pub range: Range<usize>,
}

impl EmoteUse {
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
    Text {
        text: String,
        /// Where the text is in the message, in bytes ([`Piece::range`]).
        range: Range<usize>,
    },
    Emote(EmoteUse),
}

impl Piece {
    /// The text this piece stands for. Every piece's, joined, is the message
    /// as it was given.
    pub fn source(&self) -> &str {
        match self {
            Self::Text { text, .. } => text,
            Self::Emote(emote) => &emote.source,
        }
    }

    /// Where the piece is in the message: the bytes of its
    /// [`source`](Self::source), counted over everything the tokenizer was
    /// given, in the order it was given, from
    /// [`Tokenizer::starting_at`] (zero unless set). The pieces' ranges
    /// follow one another without a gap.
    pub fn range(&self) -> Range<usize> {
        match self {
            Self::Text { range, .. } => range.clone(),
            Self::Emote(emote) => emote.range.clone(),
        }
    }

    /// The text of a text piece.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text, .. } => Some(text),
            Self::Emote(_) => None,
        }
    }

    pub fn as_emote(&self) -> Option<&EmoteUse> {
        match self {
            Self::Emote(emote) => Some(emote),
            Self::Text { .. } => None,
        }
    }

    /// Whether it is text of spaces alone.
    pub fn is_blank(&self) -> bool {
        matches!(self, Self::Text { text, .. } if text.trim().is_empty())
    }
}

/// What a run of the message's text is.
enum Kind {
    Space,
    Text,
    Emote(Arc<Emote>),
}

/// A run of the message's text: a word, the spaces between two, or a part
/// given whole. `range` is its place in [`Tokenizer::source`].
struct Token {
    kind: Kind,
    range: Range<usize>,
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
/// assert_eq!(pieces[0].as_text(), Some("so "));
/// let cat = pieces[1].as_emote().unwrap();
/// assert_eq!(cat.emote.code, "catJAM");
/// assert_eq!(cat.modifiers[0].code, "RainTime");
/// assert_eq!(cat.source, "catJAM RainTime");
/// assert_eq!(cat.range, 3..18);
/// assert_eq!(pieces[2].as_text(), Some(" today"));
/// ```
pub struct Tokenizer<'a> {
    index: &'a EmoteIndex,
    /// Everything given so far, in order.
    source: String,
    tokens: Vec<Token>,
    start: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(index: &'a EmoteIndex) -> Self {
        Self {
            index,
            source: String::new(),
            tokens: Vec::new(),
            start: 0,
        }
    }

    /// Counts the pieces' ranges ([`Piece::range`]) from byte `start`: for a
    /// tokenizer given only a part of a longer text, the part's place in it.
    pub fn starting_at(mut self, start: usize) -> Self {
        self.start = start;
        self
    }

    fn push(&mut self, kind: Kind, text: &str) {
        let start = self.source.len();
        self.source.push_str(text);
        self.tokens.push(Token {
            kind,
            range: start..self.source.len(),
        });
    }

    /// Text whose words are looked up in the index.
    pub fn text(&mut self, text: &str) -> &mut Self {
        for (run, space) in split_keeping_spaces(text) {
            let kind = match self.index.get(run) {
                _ if space => Kind::Space,
                Some(emote) => Kind::Emote(Arc::clone(emote)),
                None => Kind::Text,
            };
            self.push(kind, run);
        }
        self
    }

    /// Text that is kept as it is, never looked up.
    pub fn verbatim(&mut self, text: &str) -> &mut Self {
        if !text.is_empty() {
            let space = text.chars().all(char::is_whitespace);
            self.push(if space { Kind::Space } else { Kind::Text }, text);
        }
        self
    }

    /// An emote the platform named, written `source` in the message.
    pub fn emote(&mut self, emote: impl Into<Arc<Emote>>, source: &str) -> &mut Self {
        self.push(Kind::Emote(emote.into()), source);
        self
    }

    /// The message's pieces: neighboring text joined, modifiers attached.
    pub fn finish(self) -> Vec<Piece> {
        let Self {
            source,
            tokens,
            start,
            ..
        } = self;
        let kind = |at: usize| tokens.get(at).map(|token| &token.kind);
        let mut pieces: Vec<Piece> = Vec::new();
        let mut at = 0;
        while let Some(token) = tokens.get(at) {
            // The emote this piece is, and the first and last tokens of it
            // with its modifiers: a run of text where there is no emote.
            let mut modifiers: Vec<Arc<Emote>> = Vec::new();
            let mut emote_at = matches!(token.kind, Kind::Emote(_)).then_some(at);
            if let Kind::Emote(emote) = &token.kind
                && targets(emote, ModifierTarget::Following)
            {
                // Modifiers of the emote after them: all of them up to that
                // emote, or text where there is none.
                let mut next = at;
                loop {
                    match kind(next) {
                        Some(Kind::Emote(prefix)) if targets(prefix, ModifierTarget::Following) => {
                            attach(&mut modifiers, prefix)
                        }
                        Some(Kind::Space) => {}
                        _ => break,
                    }
                    next += 1;
                }
                emote_at = matches!(kind(next), Some(Kind::Emote(_))).then_some(next);
            }
            let Some(emote_at) = emote_at else {
                push_text(&mut pieces, &source, token.range.clone(), start);
                at += 1;
                continue;
            };
            let Some(Kind::Emote(emote)) = kind(emote_at) else {
                unreachable!("emote_at is the place of an emote");
            };
            // Modifiers of the emote before them, spaces alone between.
            let mut last = emote_at;
            loop {
                let mut next = last + 1;
                while let Some(Kind::Space) = kind(next) {
                    next += 1;
                }
                match kind(next) {
                    Some(Kind::Emote(suffix)) if targets(suffix, ModifierTarget::Preceding) => {
                        attach(&mut modifiers, suffix);
                        last = next;
                    }
                    _ => break,
                }
            }
            let range = token.range.start..tokens[last].range.end;
            pieces.push(Piece::Emote(EmoteUse {
                emote: Arc::clone(emote),
                modifiers,
                source: source[range.clone()].to_string(),
                range: start + range.start..start + range.end,
            }));
            at = last + 1;
        }
        pieces
    }
}

/// Whether `emote` is a modifier of the emote on that side of it.
fn targets(emote: &Emote, target: ModifierTarget) -> bool {
    emote
        .modifier
        .as_ref()
        .is_some_and(|modifier| modifier.target == target)
}

/// Adds `modifier`, once.
fn attach(modifiers: &mut Vec<Arc<Emote>>, modifier: &Arc<Emote>) {
    if !modifiers.iter().any(|had| had.id == modifier.id) {
        modifiers.push(Arc::clone(modifier));
    }
}

/// Adds the text at `range` of `source`, joined to the text piece before it
/// if there is one. Ranges are counted from `start`.
fn push_text(pieces: &mut Vec<Piece>, source: &str, range: Range<usize>, start: usize) {
    let end = start + range.end;
    match pieces.last_mut() {
        Some(Piece::Text { text, range: had }) => {
            text.push_str(&source[range]);
            had.end = end;
        }
        _ => pieces.push(Piece::Text {
            text: source[range.clone()].to_string(),
            range: start + range.start..end,
        }),
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
                Piece::Text { text, .. } => format!("{text:?}"),
                Piece::Emote(emote) => std::iter::once(emote.emote.code.as_str())
                    .chain(emote.modifiers.iter().map(|m| m.code.as_str()))
                    .collect::<Vec<_>>()
                    .join("+"),
            })
            .collect()
    }

    /// The pieces' sources joined, after checking that each piece's range
    /// is where its source is.
    fn rejoined(pieces: &[Piece]) -> String {
        let joined: String = pieces.iter().map(Piece::source).collect();
        let mut at = 0;
        for piece in pieces {
            assert_eq!(piece.range().start, at, "{piece:?}");
            assert_eq!(&joined[piece.range()], piece.source(), "{piece:?}");
            at = piece.range().end;
        }
        assert_eq!(at, joined.len());
        joined
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
        let ranges: Vec<Range<usize>> = pieces.iter().map(Piece::range).collect();
        assert_eq!(ranges, [0..3, 3..17, 17..30, 30..36]);
        // A code given verbatim is never an emote.
        let mut tokenizer = index.tokenizer();
        tokenizer.verbatim("catJAM").verbatim("");
        assert_eq!(short(&tokenizer.finish()), ["\"catJAM\""]);
    }

    #[test]
    fn ranges_count_bytes_from_where_the_text_starts() {
        let index = index();
        // Bytes, not characters: `世` is three.
        let pieces = index.tokenize("世 catJAM RainTime ok");
        assert_eq!(rejoined(&pieces), "世 catJAM RainTime ok");
        let ranges: Vec<Range<usize>> = pieces.iter().map(Piece::range).collect();
        assert_eq!(ranges, [0..4, 4..19, 19..22]);
        assert_eq!(pieces[1].as_emote().expect("an emote").range, 4..19);

        // A part of a longer text, from byte 100 of it.
        let mut tokenizer = index.tokenizer().starting_at(100);
        tokenizer.text("w! catJAM").text(" x");
        let ranges: Vec<Range<usize>> = tokenizer.finish().iter().map(Piece::range).collect();
        assert_eq!(ranges, [100..109, 109..111]);
        assert!(index.tokenizer().starting_at(7).finish().is_empty());
    }
}
