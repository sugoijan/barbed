# Changelog

This file is append-only and records `barbed` changes that are intended to replace behavior currently implemented in LUPO. Each entry lists the public API involved, the root feature flags needed, the LUPO behavior it is meant to replace, and any known gaps that still remain.

## Unreleased

### Root emote and auth/session abstractions

- Public API added/changed:
  - `barbed::emotes::*`
  - `barbed::session::{TwitchAuthStore, InMemoryTwitchAuthStore}`
  - `barbed::session::{ensure_valid_stored_auth, refresh_stored_auth}` behind `reqwest-client`
- Root feature flags:
  - none for the shared types and in-memory store
  - `reqwest-client` for the token validation/refresh helpers
- Intended LUPO replacement:
  - shared emote domain models currently reconstructed in LUPO-side provider crates
  - duplicated token-store refresh/validation helpers in LUPO Twitch runtime code
- Known remaining gaps:
  - no LUPO adapter exists yet for `messages::EmoteRef`
  - no LUPO settings/secret-backed `TwitchAuthStore` implementation exists in this repo

### Twitch IRC support

- Public API added/changed:
  - `barbed::irc::{TwitchIrcConfig, TwitchIrcEvent, TwitchIrcPrivmsg}`
  - `barbed::irc::{TwitchIrcApi, InMemoryTwitchIrcClient}`
  - `barbed::irc::TwitchIrcClient` behind `tokio-irc`
  - parsing helpers such as `normalize_token`, `parse_privmsg`, `parse_notice`, `connect_commands`, and `disconnect_commands`
- Root feature flags:
  - none for parsing/types/in-memory client
  - `tokio-irc` for the native async IRC transport
- Intended LUPO replacement:
  - the current CLI Twitch IRC client and its parsing/handshake logic
- Known remaining gaps:
  - no LUPO command has been migrated yet
  - no higher-level formatting helpers are provided; only transport/parsing coverage is included

### 7TV provider coverage

- Public API added/changed:
  - `barbed::seventv::*` behind `seventv`
  - `barbed-7tv::{SevenTvApi, InMemorySevenTvApi}`
  - `barbed-7tv::SevenTvClient` behind `reqwest-client` or root `seventv-reqwest`
- Root feature flags:
  - `seventv`
  - `seventv-reqwest`
- Intended LUPO replacement:
  - 7TV global emote fetch
  - 7TV Twitch-user lookup by Twitch ID
  - 7TV channel emote-set resolution and emote-set fetch
- Known remaining gaps:
  - no LUPO resolver/catalog adapter is included here
  - coverage is intentionally limited to the API surface LUPO currently uses

### BetterTTV provider coverage

- Public API added/changed:
  - `barbed::bttv::*` behind `bttv`
  - `barbed-bttv::{BttvApi, InMemoryBttvApi, BttvEmoteSet, BttvChannelEmoteSets}`
  - `barbed-bttv::BttvClient` behind `reqwest-client` or root `bttv-reqwest`
- Root feature flags:
  - `bttv`
  - `bttv-reqwest`
- Intended LUPO replacement:
  - BetterTTV global emotes
  - BetterTTV channel emotes and shared emotes by Twitch ID
- Known remaining gaps:
  - no LUPO resolver/catalog adapter is included here
  - coverage is intentionally limited to the API surface LUPO currently uses

### FrankerFaceZ provider coverage

- Public API added/changed:
  - `barbed::ffz::*` behind `ffz`
  - `barbed-ffz::{FfzApi, InMemoryFfzApi, FfzGlobalEmoteSets, FfzRoomEmoteSets, FfzEmoteSet}`
  - `barbed-ffz::FfzClient` behind `reqwest-client` or root `ffz-reqwest`
- Root feature flags:
  - `ffz`
  - `ffz-reqwest`
- Intended LUPO replacement:
  - FFZ global sets
  - FFZ room lookup by Twitch ID
  - FFZ default global sets, user-scoped sets, user-scoped summary, modifier and mask metadata
- Known remaining gaps:
  - no LUPO resolver/catalog adapter is included here
  - coverage is intentionally limited to the API surface LUPO currently uses
