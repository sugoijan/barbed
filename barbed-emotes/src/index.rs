//! The emotes a chat knows by word.

use std::collections::HashMap;
use std::sync::Arc;

use crate::model::{Emote, EmoteProvider};
use crate::text::{Piece, Tokenizer};

/// Whose emotes a set holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EmoteScope {
    /// Every channel has them.
    Global,
    /// One channel's own. They win over global ones of the same code.
    Channel,
}

/// The providers in the order their emotes win a code two of them have, as
/// the chat clients that show all three resolve it: 7TV's over BetterTTV's
/// over FrankerFaceZ's. Any other provider comes after them, Twitch last.
pub const DEFAULT_PROVIDER_ORDER: [EmoteProvider; 3] = [
    EmoteProvider::SevenTv,
    EmoteProvider::BetterTtv,
    EmoteProvider::FrankerFaceZ,
];

#[derive(Clone, Debug)]
struct Entry {
    emote: Arc<Emote>,
    rank: (EmoteScope, usize),
}

/// The emotes one chat matches in the text of its messages, by code.
///
/// Sets are added in any order. Where two emotes have the same code the
/// channel's wins over a global one, then the provider earlier in the order
/// ([`DEFAULT_PROVIDER_ORDER`]), then the one added first.
///
/// Twitch's own emotes need no index: a message says which of its words they
/// are ([`Tokenizer::emote`]).
#[derive(Clone, Debug)]
pub struct EmoteIndex {
    by_code: HashMap<String, Entry>,
    order: Vec<EmoteProvider>,
}

impl Default for EmoteIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl EmoteIndex {
    /// An empty index with the [`DEFAULT_PROVIDER_ORDER`].
    pub fn new() -> Self {
        Self::with_provider_order(DEFAULT_PROVIDER_ORDER)
    }

    /// An empty index where the providers win in `order`, the first one over
    /// the rest. A provider left out comes after all of them.
    pub fn with_provider_order(order: impl IntoIterator<Item = EmoteProvider>) -> Self {
        Self {
            by_code: HashMap::new(),
            order: order.into_iter().collect(),
        }
    }

    fn rank(&self, scope: EmoteScope, provider: &EmoteProvider) -> (EmoteScope, usize) {
        let listed = self.order.iter().position(|p| p == provider);
        let weight = match (listed, provider) {
            (Some(at), _) => self.order.len() - at + 1,
            (None, EmoteProvider::Twitch) => 0,
            (None, _) => 1,
        };
        (scope, weight)
    }

    /// Adds one emote of `scope`. Whether it now stands for its code.
    pub fn insert(&mut self, scope: EmoteScope, emote: impl Into<Arc<Emote>>) -> bool {
        let emote = emote.into();
        let rank = self.rank(scope, &emote.id.provider);
        if emote.code.is_empty()
            || self
                .by_code
                .get(&emote.code)
                .is_some_and(|had| had.rank >= rank)
        {
            return false;
        }
        self.by_code
            .insert(emote.code.clone(), Entry { emote, rank });
        true
    }

    /// Adds the emotes of a set of `scope`.
    ///
    /// To leave out what a display does not draw, filter first: for instance
    /// `set.emotes.into_iter().filter(Emote::is_standalone)` drops modifiers,
    /// whose words then stay text.
    pub fn extend(&mut self, scope: EmoteScope, emotes: impl IntoIterator<Item = Emote>) {
        for emote in emotes {
            self.insert(scope, emote);
        }
    }

    /// The emote `code` stands for. Codes are matched exactly, case and all.
    pub fn get(&self, code: &str) -> Option<&Arc<Emote>> {
        self.by_code.get(code).map(|entry| &entry.emote)
    }

    /// Codes known.
    pub fn len(&self) -> usize {
        self.by_code.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_code.is_empty()
    }

    /// Every emote that stands for a code, in no order.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<Emote>> {
        self.by_code.values().map(|entry| &entry.emote)
    }

    /// A tokenizer for one message.
    pub fn tokenizer(&self) -> Tokenizer<'_> {
        Tokenizer::new(self)
    }

    /// `text` cut into text and emotes: a whole message that carries no
    /// emotes of its own (see [`Tokenizer`] for one that does).
    pub fn tokenize(&self, text: &str) -> Vec<Piece> {
        let mut tokenizer = self.tokenizer();
        tokenizer.text(text);
        tokenizer.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EmoteId;

    fn emote(provider: EmoteProvider, id: &str, code: &str) -> Emote {
        Emote::new(EmoteId::new(provider, id), code, Vec::new())
    }

    fn id(index: &EmoteIndex, code: &str) -> Option<String> {
        index.get(code).map(|emote| emote.id.to_string())
    }

    #[test]
    fn a_channels_code_wins_then_the_provider_order() {
        let mut index = EmoteIndex::new();
        index.extend(
            EmoteScope::Global,
            [
                emote(EmoteProvider::SevenTv, "g7", "catJAM"),
                emote(EmoteProvider::SevenTv, "g7c", "Clap"),
            ],
        );
        index.extend(
            EmoteScope::Channel,
            [
                emote(EmoteProvider::FrankerFaceZ, "cf", "catJAM"),
                emote(EmoteProvider::BetterTtv, "cb", "catJAM"),
                emote(EmoteProvider::BetterTtv, "cb2", "PogBTTV"),
            ],
        );
        // The channel's over the global one, BetterTTV's over
        // FrankerFaceZ's, in whatever order they came.
        assert_eq!(id(&index, "catJAM").as_deref(), Some("bttv:cb"));
        assert!(index.insert(
            EmoteScope::Channel,
            emote(EmoteProvider::SevenTv, "c7", "catJAM")
        ));
        assert_eq!(id(&index, "catJAM").as_deref(), Some("7tv:c7"));
        assert!(!index.insert(
            EmoteScope::Global,
            emote(EmoteProvider::SevenTv, "late", "catJAM")
        ));
        assert_eq!(id(&index, "Clap").as_deref(), Some("7tv:g7c"));
        assert_eq!(id(&index, "PogBTTV").as_deref(), Some("bttv:cb2"));
        assert_eq!(id(&index, "Kappa"), None);
        assert_eq!(id(&index, "catjam"), None);
        assert_eq!(index.len(), 3);
        assert_eq!(index.iter().count(), 3);
    }

    #[test]
    fn the_first_added_of_equal_rank_stays() {
        let mut index = EmoteIndex::new();
        assert!(index.insert(EmoteScope::Global, emote(EmoteProvider::SevenTv, "a", "X")));
        assert!(!index.insert(EmoteScope::Global, emote(EmoteProvider::SevenTv, "b", "X")));
        assert!(!index.insert(EmoteScope::Global, emote(EmoteProvider::SevenTv, "c", "")));
        assert_eq!(id(&index, "X").as_deref(), Some("7tv:a"));
    }

    #[test]
    fn another_order_and_other_providers() {
        let other = EmoteProvider::Other("mine".into());
        let mut index =
            EmoteIndex::with_provider_order([EmoteProvider::FrankerFaceZ, EmoteProvider::SevenTv]);
        index.insert(EmoteScope::Global, emote(EmoteProvider::Twitch, "t", "X"));
        assert_eq!(id(&index, "X").as_deref(), Some("twitch:t"));
        index.insert(EmoteScope::Global, emote(other, "o", "X"));
        assert_eq!(id(&index, "X").as_deref(), Some("mine:o"));
        index.insert(EmoteScope::Global, emote(EmoteProvider::SevenTv, "s", "X"));
        assert_eq!(id(&index, "X").as_deref(), Some("7tv:s"));
        index.insert(
            EmoteScope::Global,
            emote(EmoteProvider::FrankerFaceZ, "f", "X"),
        );
        assert_eq!(id(&index, "X").as_deref(), Some("ffz:f"));
    }
}
