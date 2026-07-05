//! Small building blocks for Twitch integrations.
//!
//! `barbed` currently includes:
//!
//! - OAuth authorize URL construction and signed state verification
//! - Helix request builders and response parsers
//! - EventSub WebSocket payload decoding and chat subscription helpers
//! - IRC parsing plus an optional native async IRC client
//! - Checked-in Twitch API/EventSub/auth catalogs with generated low-level registries
//! - Provider-neutral emote models shared across Twitch/7TV/BTTV/FFZ helpers
//! - HMAC signing helpers for short-lived tokens and state payloads
//!
//! The default crate stays runtime-agnostic. Enable the `cloudflare-worker`
//! feature to send [`http::PreparedRequest`] values via the Cloudflare
//! Workers `Fetch` API. Enable the `reqwest-client`, `tokio-eventsub`,
//! and `tokio-irc` features for native Rust runtime helpers. Optional
//! provider crates are re-exported behind `seventv`, `bttv`, and `ffz`
//! feature flags.
//!
//! # Feature Flags
//!
//! - `cloudflare-worker`: adds transport helpers built on the Cloudflare
//!   Workers `Fetch` API.
//! - `reqwest-client`: adds native HTTP helpers built on `reqwest`.
//! - `tokio-eventsub`: adds the native EventSub websocket client/runtime.
//! - `tokio-irc`: adds the native Twitch IRC client/runtime.
//! - `seventv`, `bttv`, `ffz`: re-export the corresponding provider crates.
//! - `seventv-reqwest`, `bttv-reqwest`, `ffz-reqwest`: enable reqwest-based
//!   HTTP clients in the provider crates.
//!
//! # MSRV
//!
//! `barbed` currently targets Rust `1.85.0`.
//!
//! # Example
//!
//! ```rust
//! use barbed::http::percent_decode;
//! use barbed::oauth::{OAuthStatePayload, build_authorize_url, verify_oauth_state};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
//! struct AuthState {
//!     redirect_to: String,
//!     expires_at_ms: i64,
//! }
//!
//! impl OAuthStatePayload for AuthState {
//!     fn expires_at_ms(&self) -> i64 {
//!         self.expires_at_ms
//!     }
//! }
//!
//! let state = AuthState {
//!     redirect_to: "/dashboard".to_string(),
//!     expires_at_ms: 1_700_000_060_000,
//! };
//!
//! let authorize_url = build_authorize_url(
//!     "client-id",
//!     "https://example.com/twitch/callback",
//!     &["user:read:chat"],
//!     &state,
//!     "super-secret",
//! )?;
//!
//! let encoded_state = authorize_url
//!     .split("&state=")
//!     .nth(1)
//!     .expect("missing state parameter");
//! let signed_state = percent_decode(encoded_state)?;
//! let verified: AuthState =
//!     verify_oauth_state("super-secret", &signed_state, 1_700_000_000_000)?;
//!
//! assert_eq!(verified, state);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! This crate is intentionally small and still settling; expect API breakage
//! across `0.0.x` releases.
#![deny(rustdoc::broken_intra_doc_links)]

/// Re-exported provider-neutral emote types shared across workspace crates.
pub mod emotes {
    pub use barbed_core::emotes::*;
}
/// EventSub payload types and websocket subscription helpers.
pub mod eventsub;
/// Helix request builders and response parsers.
pub mod helix;
/// Percent-encoding and form-body helpers used by the request builders.
pub mod http;
/// Identity types for authenticated Twitch users.
pub mod identity;
/// IRC parsing plus optional native async Twitch IRC transport helpers.
pub mod irc;
/// Native async helpers for OAuth, Helix, and EventSub interactions.
#[cfg(feature = "reqwest-client")]
pub mod native;
/// OAuth URL construction, token lifecycle helpers, and signed state handling.
pub mod oauth;
/// Shared auth/session abstractions and in-memory implementations.
pub mod session;
/// Shared HMAC signing helpers for short-lived payloads.
pub mod signing;
/// Checked-in Twitch surface catalogs generated from the published docs.
pub mod twitch_catalog;

/// Cloudflare Workers transport helpers for prepared Twitch API requests.
#[cfg(feature = "cloudflare-worker")]
#[path = "client.rs"]
pub mod cloudflare_worker;

/// Backwards-compatible alias for the legacy `client` module path.
#[cfg(feature = "cloudflare-worker")]
#[doc(hidden)]
pub mod client {
    pub use super::cloudflare_worker::*;
}

/// Re-export of the core Twitch identity model.
pub use identity::TwitchIdentity;

/// Re-export of the optional 7TV provider crate.
#[cfg(feature = "seventv")]
pub mod seventv {
    pub use barbed_7tv::*;
}

/// Re-export of the optional BetterTTV provider crate.
#[cfg(feature = "bttv")]
pub mod bttv {
    pub use barbed_bttv::*;
}

/// Re-export of the optional FrankerFaceZ provider crate.
#[cfg(feature = "ffz")]
pub mod ffz {
    pub use barbed_ffz::*;
}
