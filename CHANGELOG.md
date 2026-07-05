# Changelog

This file is append-only and records `barbed` changesO. Each entry lists the public API involved, the root feature flags needed, and any known gaps that still remain.

## Unreleased

### Typed EventSub notification payloads generated from the documented schemas

- Public API added/changed:
  - `tools/twitch_surface.py` now also scrapes the EventSub reference page
    (a third HTML input, `--eventsub-reference-html`); `twitch_catalog/eventsub.json`
    carries per-subscription `event` field shapes plus a `shared_objects` map
  - all 83 subscriptions decode to typed structs via
    `EventSubWebSocketEnvelope::known_payload` /
    `EventSubWebhookEnvelope::known_payload`: `KnownEventSubPayload` variants
    now carry generated `<Variant>Event` structs (e.g. `ChannelFollow1Event`),
    with shared doc objects emitted once as `Shared*` structs (`SharedReward`,
    `SharedChoice`, ...); `channel.chat.message`/`_delete` keep the hand-written
    structs; subscriptions that fail to scrape fall back to
    `GenericEventSubPayload` (currently zero)
  - explicit JSON `null` in event payloads now decodes as the field default
    instead of failing the typed decode
  - breaking: the `<Variant>Event` aliases to `GenericEventSubPayload` are now
    real structs (or gone, for the chat pair); `barbed::eventsub` re-exports all
    generated payload types
  - fixed: `known_payload()` now stamps `source_timestamp` on typed chat events
    (previously only generic payloads were stamped)
  - fixed: two subscriptions (`channel.unban_request.create/resolve`) were
    silently dropped from the catalog by a scraper regex bug and corrupted the
    `channel.moderate` v1 entry; the catalog grows from 81 to 83 subscriptions
- Known remaining gaps:
  - typed payloads mirror the documented schemas; undocumented fields are
    dropped (use the raw envelope for lossless capture) and `source_timestamp`
    is not serialized
  - timestamps remain `String` (RFC3339 in prose only)

### Typed Helix responses generated from the documented API surface

- Public API added/changed:
  - `tools/twitch_surface.py` now scrapes each endpoint's Response Body table
    and success status; `twitch_catalog/helix.json` carries `response` shapes
    and `expected_status` per endpoint
  - generated endpoint modules (`barbed::helix::users`, `::streams`,
    `::eventsub_endpoints`, ...) now define typed `<Name>Response` structs with
    `EXPECTED_STATUS` and `parse(RawResponse)`; endpoints whose docs cannot be
    scraped keep the untyped `HelixJsonResponse` alias (currently 2 of 149)
  - `barbed::helix::{parse_typed_response, parse_empty_response, HelixPagination}`
  - breaking: `user_lookup_request`, `user_lookup_by_login_request`,
    `list_eventsub_subscriptions_request`, and
    `delete_eventsub_subscription_request` now return
    `Result<PreparedRequest, HelixError>` and route through the generated
    registry; `list_eventsub_subscriptions_request` takes a
    `subscription_type: Option<&str>` filter instead of hardcoding
    `channel.chat.message`
  - breaking: `eventsub::CreateEventSubSubscriptionResponse` was removed; the
    create/list parsers and the `cloudflare-worker` helpers return the
    generated `helix::eventsub_endpoints::{CreateEventsubSubscriptionResponse,
    GetEventsubSubscriptionsResponse}` types
  - breaking: `twitch_catalog` count helpers now recompute from the generated
    registries; the catalog JSON structs/accessors were removed and the JSON is
    no longer embedded in the library
  - catalog refresh: the checked-in surface grew to 149 Helix endpoints and 81
    EventSub subscriptions
- Root feature flags:
  - none for the typed responses and registry; existing `cloudflare-worker`,
    `reqwest-client`, and `tokio-eventsub` helpers updated in place
- Known remaining gaps:
  - timestamps in typed responses remain `String` (RFC3339 in prose only)
  - `get-hype-train-status` and `get-channel-icalendar` remain untyped
    (malformed/absent docs tables)
  - EventSub notification payloads other than chat message/delete still decode
    to `GenericEventSubPayload`

### EventSub chat stream connects faster

- Public API added/changed:
  - none; `connect_chat_stream` now issues its two subscription requests
    concurrently (message subscription failures still abort, message-delete
    stays best-effort)
- Root feature flags:
  - `tokio-eventsub`
- Known remaining gaps:
  - none

## 0.0.3 - 2026-07-05

### Dependency updates

- Public API added/changed:
  - `worker` updated from 0.7 to 0.8; `worker::Error` is embedded in the `cloudflare-worker` error type, so downstream users of that feature must use `worker` 0.8
- Root feature flags:
  - `cloudflare-worker` for the `worker` upgrade
- Known remaining gaps:
  - none

### Root emote and auth/session abstractions

- Public API added/changed:
  - `barbed::emotes::*`
  - `barbed::session::{TwitchAuthStore, InMemoryTwitchAuthStore}`
  - `barbed::session::{ensure_valid_stored_auth, refresh_stored_auth}` behind `reqwest-client`
- Root feature flags:
  - none for the shared types and in-memory store
  - `reqwest-client` for the token validation/refresh helpers

### Twitch IRC support

- Public API added/changed:
  - `barbed::irc::{TwitchIrcConfig, TwitchIrcEvent, TwitchIrcPrivmsg}`
  - `barbed::irc::{TwitchIrcApi, InMemoryTwitchIrcClient}`
  - `barbed::irc::TwitchIrcClient` behind `tokio-irc`
  - parsing helpers such as `normalize_token`, `parse_privmsg`, `parse_notice`, `connect_commands`, and `disconnect_commands`
- Root feature flags:
  - none for parsing/types/in-memory client
  - `tokio-irc` for the native async IRC transport
- Known remaining gaps:
  - no higher-level formatting helpers are provided; only transport/parsing coverage is included

### 7TV provider coverage

- Public API added/changed:
  - `barbed::seventv::*` behind `seventv`
  - `barbed-7tv::{SevenTvApi, InMemorySevenTvApi}`
  - `barbed-7tv::SevenTvClient` behind `reqwest-client` or root `seventv-reqwest`
- Root feature flags:
  - `seventv`
  - `seventv-reqwest`

### BetterTTV provider coverage

- Public API added/changed:
  - `barbed::bttv::*` behind `bttv`
  - `barbed-bttv::{BttvApi, InMemoryBttvApi, BttvEmoteSet, BttvChannelEmoteSets}`
  - `barbed-bttv::BttvClient` behind `reqwest-client` or root `bttv-reqwest`
- Root feature flags:
  - `bttv`
  - `bttv-reqwest`

### FrankerFaceZ provider coverage

- Public API added/changed:
  - `barbed::ffz::*` behind `ffz`
  - `barbed-ffz::{FfzApi, InMemoryFfzApi, FfzGlobalEmoteSets, FfzRoomEmoteSets, FfzEmoteSet}`
  - `barbed-ffz::FfzClient` behind `reqwest-client` or root `ffz-reqwest`
- Root feature flags:
  - `ffz`
  - `ffz-reqwest`
