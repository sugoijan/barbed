//! Reading a provider's answer entry by entry.
//!
//! An answer's entries are kept as the text they were ([`RawValue`]) until
//! each is read into its own small struct, so that one entry of another
//! shape costs that entry alone, and the fields nothing here uses (owners,
//! timestamps, counts) are never built. A large channel's 7TV set is a few
//! megabytes of them.

use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

use crate::model::{Emote, EmoteSet, ParseError};

/// The emotes of `entries`: each read as a `W` and made an emote by
/// `to_emote`. An entry that fails either step is left out and counted.
pub(crate) fn read_entries<'a, W: Deserialize<'a>>(
    entries: impl IntoIterator<Item = &'a RawValue>,
    to_emote: impl Fn(W) -> Option<Emote>,
) -> (Vec<Emote>, usize) {
    let mut emotes = Vec::new();
    let mut skipped = 0;
    for entry in entries {
        match serde_json::from_str(entry.get()).ok().and_then(&to_emote) {
            Some(emote) => emotes.push(emote),
            None => skipped += 1,
        }
    }
    (emotes, skipped)
}

/// A set named `id` and `name` with the emotes of `entries`
/// ([`read_entries`]).
pub(crate) fn read_set<'a, W: Deserialize<'a>>(
    id: Option<String>,
    name: Option<String>,
    entries: impl IntoIterator<Item = &'a RawValue>,
    to_emote: impl Fn(W) -> Option<Emote>,
) -> EmoteSet {
    let (emotes, skipped) = read_entries(entries, to_emote);
    EmoteSet {
        id,
        name,
        emotes,
        skipped,
    }
}

/// Checks that `body` is a JSON object, before it is read into a struct
/// whose fields are all optional (which would take an array too). `what`
/// says what was expected.
pub(crate) fn expect_object(body: &str, what: &'static str) -> Result<(), ParseError> {
    if body.trim_start().starts_with('{') {
        return Ok(());
    }
    // Not JSON at all is the more useful thing to say.
    serde_json::from_str::<IgnoredAny>(body)?;
    Err(ParseError::Shape(what))
}

/// An optional field, where `null` says the same as leaving it out: its
/// default (`#[serde(default, deserialize_with = "or_default")]`). Providers
/// write `null` for what they have not got, and a `null` flag is no reason
/// to lose the emote, or a `null` list the whole answer. A value of another
/// type still fails what it is in.
pub(crate) fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// The segments of `path`, which starts with its `/`.
pub(crate) fn segments(path: &str) -> Vec<&str> {
    path.strip_prefix('/').unwrap_or(path).split('/').collect()
}
