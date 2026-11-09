# Changelog

This file is append-only and records `barbed` changesO. Each entry lists the public API involved, the root feature flags needed, and any known gaps that still remain.

## 0.0.5 - 2026-11-09

### Twitch surface refreshed against the 2026-11-09 docs

- Public API added/changed:
  - catalog refresh: still 149 Helix endpoints and 83 EventSub subscriptions;
    Helix is unchanged, EventSub picks up every change Twitch logged between
    2026-07-16 and 2026-10-01
  - `channel.chat.message` v1: the hand-written structs now cover the whole
    documented schema: `EventSubMessageFragmentType::Gif` and
    `EventSubMessageFragment::gif` (`EventSubMessageGif { id, url }`; `gif_id`
    is accepted as an alias because the change log and the reference table
    disagree on the name), `EventSubCheermote::tier`, and on
    `EventSubChatMessage` the fields `message_type` (`EventSubChatMessageType`),
    `reply` (`EventSubChatReply`), `channel_points_custom_reward_id`,
    `source_broadcaster_user_{id,login,name}`, `source_message_id`,
    `source_badges`, and `is_source_only`
  - `channel.chat.message_delete` v1: `EventSubChatMessageDeleted` gains
    `broadcaster_user_{login,name}` and `target_user_{login,name}`
  - fixed: `EventSubMessageFragmentType::Unknown` (and the new
    `EventSubChatMessageType::Unknown`) are `#[serde(other)]` catch-alls; since
    Twitch shipped `gif` fragments on 2026-07-16 every chat message containing
    a GIF failed to decode through `EventSubChatMessage`, and a future fragment
    or message type can no longer fail the whole decode
  - breaking: the new enum variants and struct fields above break exhaustive
    matches and struct literals downstream
  - `channel.chat.notification` v1: generated `gifted_drops_summary`
    (`recipient_count`) and `shared_chat_gifted_drops_summary` fields plus the
    two matching `notice_type` values
  - `automod.message.hold`/`automod.message.update` v2: `blocked_link` is
    documented as a `reason` value (doc comments only; the fields are strings)
  - fixed: `tools/twitch_surface.py` slugifies named doc types, so
    `channel.custom_power_up_redemption.add` keeps `SharedCustomPowerUp` after
    Twitch changed the type cell from `custom_power_up` to a "Custom Power Up"
    link; without the fix the field regressed to `serde_json::Value`
- Root feature flags:
  - none
- Known remaining gaps:
  - `shared_chat_gifted_drops_summary` stays `serde_json::Value` like the other
    `shared_chat_*` notice objects; the docs publish no field table for them
  - the IRC `gifs` PRIVMSG tag (2026-07-17) is not modeled; `parse_privmsg`
    still extracts only `display-name` and `badges`

### Code cleanup pass

- Public API added/changed:
  - `barbed_core::USER_AGENT` (`barbed/<version>`) replaces the three
    hard-coded `barbed/0.0.2` user agents in the 7TV, BetterTTV, and FFZ
    clients, which had drifted from the workspace version
  - `barbed_core::emotes::EmoteImage::{new, with_theme}`; providers no longer
    spell out four-field image literals
  - `helix::TwitchTokenExchange::into_token_state` is the one place a token
    response becomes `TwitchTokenState` (used by `build_auth_outcome` and
    `native::refresh_access_token`); breaking: `oauth::refreshed_twitch_token_state`
    (seven positional parameters, no callers) was removed in its favour
  - breaking: the hidden `barbed::client` alias of `barbed::cloudflare_worker`
    was removed
  - `helix::HelixEndpoint::prepare` treats an empty access token like a
    missing one (`MissingAccessToken`), so the `cloudflare-worker` helpers no
    longer pre-check credentials themselves
  - `eventsub::InMemoryEventSubReplayStore` now forgets message ids after a
    retention window (`DEFAULT_RETENTION` of ten minutes, `with_retention`)
    instead of growing forever; the `seen_at` argument was previously ignored
  - typed EventSub decoding no longer clones the event JSON per
    `known_payload()` call; the generated and hand-written decode paths share
    one helper, and `GenericEventSubPayload` implements the same timestamp hook
  - `helix::HelixEndpoint::parse_json_response` parses the body once instead of
    twice (pagination cursor and result), and `native::send_prepared_request`
    no longer collects response headers it then drops
  - `tokio-irc`: the client reuses one line buffer, writes each line in a
    single syscall, and keeps only the fields it needs after connecting (the
    OAuth token is no longer retained)
  - `tools/twitch_surface.py`: event sections and named types are resolved
    from the links the docs already carry (the types page links each
    subscription's `event` row to its reference section; type cells link to
    their shared object), so the nine-entry section override table and the
    name-guessing fallback are gone; the array/struct-name tables merged into
    one `SHARED_SECTIONS` map; `generated_at` is stamped once per build
- Root feature flags:
  - none
- Known remaining gaps:
  - several public helpers have no in-tree callers or tests
    (`oauth::OAuthTokenKind`, `device_code_request`, `openid_jwks_request`,
    `build_oidc_authorize_url`, `EventSubTransport::{webhook, conduit}`,
    `GenericCreateEventSubSubscriptionRequest`, `native::fetch_user_by_login`,
    ...); kept for downstream use, prune or test as needed
  - `EventSubMessageType` still derives `Serialize`/`Deserialize` with
    PascalCase spellings that differ from the wire form `parse_message_type`
    accepts; nothing in tree uses the derives

### Dependency updates

- Public API added/changed:
  - `base64` 0.22 → 0.23, `hmac` 0.12 → 0.13, `sha2` 0.10 → 0.11, and
    `tokio-tungstenite` 0.28 → 0.30; none of their types are part of the
    public API, so nothing changes downstream
  - breaking: MSRV raised from 1.88 to 1.97. `worker` 0.8.7 and
    `wasm-streams` 0.7 need 1.91; the rest is deliberate headroom (stable
    minus two at the time) so the MSRV-aware resolver stops silently holding
    dependency updates back
  - `Cargo.lock` refreshed to the newest releases (`reqwest` 0.13.5, `tokio`
    1.54, `rustls` 0.23.46, `worker` 0.8.7, ...)
- Root feature flags:
  - none
- Known remaining gaps:
  - none

## 0.0.4 - 2026-07-06

### MSRV

- breaking: MSRV raised from 1.85 to 1.88 after a dependency
  update stopped building on the older toolchain

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
