//! Twitch chat emotes from Twitch, 7TV, BetterTTV and FrankerFaceZ, without
//! I/O.
//!
//! Everything a chat display needs between "a message arrived" and "these
//! are the images to draw", for any HTTP client and any target (it builds
//! for `wasm32-unknown-unknown`):
//!
//! - **One model** for all four providers ([`Emote`], [`EmoteImage`]):
//!   every image file with its scale, format, theme and size, and what kind
//!   of emote it is: one that stands by itself, an overlay drawn over its
//!   neighbor (7TV's zero-width emotes), or an effect on it (FrankerFaceZ's
//!   and BetterTTV's modifiers) ([`EmoteModifier`]).
//! - **Parsers for each provider's answers** ([`twitch`], [`seventv`],
//!   [`bttv`], [`ffz`]), given the body as text, with the addresses to ask.
//!   A provider's answer is someone else's data: ids have one plain shape,
//!   every image is an `https` address on the provider's own image host, and
//!   an entry that fails is left out and counted instead of failing the set.
//! - **Matching** ([`EmoteIndex`], [`Tokenizer`]): which words of a message
//!   are emotes, a channel's over a global one and 7TV's over BetterTTV's
//!   over FrankerFaceZ's, with modifiers attached to the emote they change.
//! - **Loading** ([`SetLoader`]): which sets a channel's chat needs, as
//!   requests the caller makes, retried through a proxy where the direct one
//!   failed, with a report of what each provider answered.
//! - **Proxy routes** ([`proxy`]): the closed list of addresses a server may
//!   fetch on a client's behalf, shared by both sides.
//!
//! ```
//! use barbed_emotes::{ImageFormat, ImageRequest, SetLoader};
//!
//! // Ask for the channel's sets with any HTTP client.
//! let mut loader = SetLoader::for_channel("1971641");
//! for request in loader.requests() {
//!     let body: &str = // GET request.url
//! #       match request.url.as_str() {
//! #           "https://api.betterttv.net/3/cached/emotes/global" => {
//! #               r#"[{"id": "54fa8f1401e468494b85b537", "code": ":tf:", "imageType": "png"}]"#
//! #           }
//! #           _ => "",
//! #       };
//!     loader.answered(request.key, request.via, Ok(body));
//! }
//!
//! // Cut a message into text and emotes.
//! let index = loader.index();
//! let pieces = index.tokenize("nice :tf:");
//! assert_eq!(pieces[0].as_text(), Some("nice "));
//!
//! // Pick the image to draw.
//! let emote = &pieces[1].as_emote().unwrap().emote;
//! let request = ImageRequest {
//!     scale: 2,
//!     formats: &[ImageFormat::WebP, ImageFormat::Png],
//!     ..ImageRequest::default()
//! };
//! assert_eq!(
//!     emote.image(&request).unwrap().url,
//!     "https://cdn.betterttv.net/emote/54fa8f1401e468494b85b537/2x.webp"
//! );
//! ```
//!
//! With the `reqwest-client` feature, `EmoteClient` makes the loader's
//! requests for programs that have `reqwest` and an async runtime.
#![deny(rustdoc::broken_intra_doc_links)]

pub mod bttv;
#[cfg(feature = "reqwest-client")]
mod client;
pub mod ffz;
mod index;
mod load;
mod model;
pub mod proxy;
mod safe;
pub mod seventv;
mod text;
pub mod twitch;
mod wire;

#[cfg(feature = "reqwest-client")]
pub use client::{DEFAULT_TIMEOUT, EmoteClient, USER_AGENT};
pub use index::{DEFAULT_PROVIDER_ORDER, EmoteIndex, EmoteScope};
pub use load::{LoadError, SetLoader, SetReport, SetRequest, SetStatus, Via};
pub use model::{
    Emote, EmoteId, EmoteImage, EmoteModifier, EmoteModifierFlags, EmoteProvider, EmoteSet,
    ImageFormat, ImageRequest, ModifierTarget, ParseError, Theme, pick_image,
};
pub use safe::is_plain_id;
pub use text::{EmoteUse, Piece, Tokenizer};
