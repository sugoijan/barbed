//! Chat emotes: the [`barbed_emotes`] crate, with conversions from this
//! crate's Twitch types.
//!
//! [`barbed_emotes`] has no I/O and no Twitch API types of its own, so that a
//! chat display can use it alone. Here it meets the rest of `barbed`:
//!
//! - an EventSub chat message is cut into text and emotes with
//!   [`EventSubChatMessageText::tokenize`](crate::eventsub::EventSubChatMessageText::tokenize),
//!   and one of its emote fragments becomes an [`Emote`] with
//!   [`EventSubMessageEmote::to_emote`](crate::eventsub::EventSubMessageEmote::to_emote);
//! - Helix's emote lists become [`Emote`]s with [`HelixEmotes::emotes`].
//!
//! With the `reqwest-client` feature, `EmoteClient` reads a channel's 7TV,
//! BetterTTV and FrankerFaceZ sets.

pub use barbed_emotes::*;

use crate::helix::chat::{GetChannelEmotesResponse, GetEmoteSetsResponse, GetGlobalEmotesResponse};

/// Helix's emote lists (Get Global Emotes, Get Channel Emotes, Get Emote
/// Sets) as [`Emote`]s.
pub trait HelixEmotes {
    /// The response's emotes, each with the images Twitch's template serves
    /// for its `format`, `scale` and `theme_mode` lists. An emote whose id is
    /// not of Twitch's shape ([`is_plain_id`]) is left out.
    fn emotes(&self) -> Vec<Emote>;
}

macro_rules! impl_helix_emotes {
    ($($response:ty),+) => {$(
        impl HelixEmotes for $response {
            fn emotes(&self) -> Vec<Emote> {
                self.data
                    .iter()
                    .filter(|item| !item.name.is_empty())
                    .filter_map(|item| {
                        twitch::emote_from_lists(
                            &item.id,
                            item.name.as_str(),
                            &item.format,
                            &item.scale,
                            &item.theme_mode,
                        )
                    })
                    .collect()
            }
        }
    )+};
}

impl_helix_emotes!(
    GetGlobalEmotesResponse,
    GetChannelEmotesResponse,
    GetEmoteSetsResponse
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helix_emote_lists_become_emotes() {
        let body = r#"{
            "data": [
                {"id": "25", "name": "Kappa", "format": ["static"],
                 "scale": ["1.0", "2.0", "3.0"], "theme_mode": ["light", "dark"]},
                {"id": "emotesv2_a11", "name": "Dance", "format": ["static", "animated"],
                 "scale": ["1.0", "2.0", "3.0"], "theme_mode": ["light", "dark"]},
                {"id": "../x", "name": "Odd", "format": ["static"]}
            ],
            "template": "https://static-cdn.jtvnw.net/emoticons/v2/{{id}}/{{format}}/{{theme_mode}}/{{scale}}"
        }"#;
        let global: GetGlobalEmotesResponse = serde_json::from_str(body).expect("a response");
        let emotes = global.emotes();
        assert_eq!(emotes.len(), 2);
        assert_eq!(emotes[0].id, EmoteId::new(EmoteProvider::Twitch, "25"));
        assert!(!emotes[0].animated && emotes[1].animated);
        assert_eq!(
            emotes[1].image(&ImageRequest::default()).unwrap().url,
            "https://static-cdn.jtvnw.net/emoticons/v2/emotesv2_a11/animated/dark/2.0"
        );
        // The same reading as the body's own parser.
        assert_eq!(twitch::parse_emotes(body).expect("a list").emotes, emotes);

        let channel: GetChannelEmotesResponse = serde_json::from_str(body).expect("a response");
        assert_eq!(channel.emotes(), emotes);
        let sets: GetEmoteSetsResponse = serde_json::from_str(body).expect("a response");
        assert_eq!(sets.emotes(), emotes);
    }
}
