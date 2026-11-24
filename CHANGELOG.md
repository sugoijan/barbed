# Changelog

This file is append-only and records `barbed` changesO. Each entry lists the public API involved, the root feature flags needed, and any known gaps that still remain.

## 0.0.7 - 2026-11-24

### Emotes: what porting LUPO to `barbed-emotes` showed was missing

- Public API added/changed:
  - breaking: every piece of a tokenized message says where it is.
    `Piece::Text` is now `Piece::Text { text, range }`, `EmoteUse` gains
    `range`, and `Piece::range()` returns either: the bytes of the piece's
    source, counted over everything the `Tokenizer` was given. The ranges
    follow one another without a gap. `Piece::as_text()` reads a text piece
  - `Tokenizer::starting_at(byte)` counts those ranges from the place of a
    part in a longer text, for a caller that tokenizes a message in parts
  - `EmoteClient::new()` gives each request `DEFAULT_TIMEOUT` (15 seconds);
    before, a provider that never answered kept `EmoteClient::load` waiting
    forever. `EmoteClient::with_http` still takes the client as it is
  - the opt-in `recorded_sets` test accepts a channel whose set is empty or
    whose 7TV account has no active set; only a global set must have emotes
- Root feature flags:
  - none; `reqwest-client` for the `EmoteClient` change
- Known remaining gaps:
  - an emote's modifiers have no ranges of their own: the emote's range
    covers its word, theirs and the spaces between
  - the channel endpoints' gap of 0.0.6 is closed: on 2026-11-24 the parsers
    read the real answers of two large channels (969 7TV, 274 BetterTTV and
    4 FrankerFaceZ emotes) with nothing left out

## 0.0.6 - 2026-11-24

### Twitch surface refreshed against the relaunched docs (2026-11-10)

- Public API added/changed:
  - Twitch relaunched its documentation on 2026-11-10: the single-page Helix
    reference and the two EventSub pages became one page per resource, which
    the scraper could not read. `tools/twitch_surface.py` gains a `fetch`
    subcommand that downloads both reference indexes and the markdown copy of
    every resource page, and `build --docs-dir` replaces the three `--*-html`
    options. Names, groups, and beta flags come from the index HTML, the
    rest from the markdown field lists, whose nesting is explicit (dotted
    paths), so the indentation-repair heuristics are gone
  - catalog refresh: 147 Helix endpoints (was 149) and 83 EventSub
    subscriptions. Entries gain `page`; EventSub entries swap `event_section`
    for `slug`; fields gain `nullable` where the docs flag it;
    `eventsub.json` drops `event_source`. Registry order (`ALL_ENDPOINTS`,
    `ALL_EVENTSUB_SUBSCRIPTIONS`, `KnownEventSubPayload` variants) follows the
    new index
  - breaking: `helix::tags` (Get All Stream Tags, Get Stream Tags) is gone;
    Twitch dropped the deprecated endpoints from the reference
  - breaking: generated Helix responses follow the corrections Twitch made
    while migrating:
    - Get Ad Schedule / Snooze Next Ad timestamps are `i64` Unix seconds
      (were `String`)
    - Create/Get/End Guest Star Session nest guests under
      `guests: Vec<...Guests>` with `audio`/`video` (were flattened next to
      the session with `audio_settings`/`video_settings`);
      `GetChannelGuestStarSettingsResponse` wraps its settings in `data`;
      Update Channel Guest Star Settings is `PATCH` (was `PUT`)
    - the three poll responses lose `bits_voting_enabled`, `bits_per_vote`,
      and `choices.bits_votes`
    - `GetStreamMarkersItem` nests `videos: Vec<GetStreamMarkersVideos>`
      holding `video_id` and `markers`; a marker's `url` is read from `URL`
    - `GetChannelStreamScheduleResponse::pagination` moved to the top level
    - `GetCheermotesTiers::images` is a struct tree (`dark`/`light`,
      `animated`/`static`, `n1` ... `n4`) instead of a map
    - `types` on the suspicious-status responses is `Vec<String>`
  - Helix additions: `GetHypeTrainStatusResponse` is typed (was the untyped
    alias); `is_batching_enabled` and `transport.conduit_id` on the EventSub
    subscription responses; `views.live_config` and the component sizing
    fields on the extension responses
  - breaking: generated EventSub payloads:
    - `channel.unban_request.resolve`: `moderator_user_{id,login,name}`
      (were `moderator_{id,login,name}`)
    - `conduit.shard.disabled`: the transport fields live under
      `transport: ConduitShardDisabled1EventTransport` (were flattened)
    - `drop.entitlement.grant`: `data` is a typed
      `DropEntitlementGrant1EventData` (was `Vec<serde_json::Value>`)
    - `channel.shared_chat.{begin,update}`: `participants` is a `Vec`
    - `channel.chat.notification`: `source_badges` is a `Vec`,
      `message.text` is a `String`, and `chatter_user_login` is new
    - `channel.suspicious_user.message`: cheermote `bits`/`tier` are `i64`
    - `channel.chat.user_message_{hold,update}`: fragments gain `type`
    - `channel.channel_points_automatic_reward_redemption.add` v1: `message`
      is `SharedMessage` (was a structurally identical per-event struct)
  - shared EventSub objects: the docs now repeat them inline in every
    payload instead of linking a section, so `SHARED_SECTIONS` in the tool
    lists each `Shared*` struct's field names and a payload field joins it
    when its name, array-ness, and child names match; the two unused
    hype-train entries were dropped
  - `HelixEndpoint::supports_pagination` is derived from an `after`/`before`
    query parameter or a top-level `pagination` response field; the old
    text search had flagged 25 endpoints that do not paginate
  - auth metadata: the Guest Star endpoints report `HelixAuthKind::User`
    (were `None`), Get Shared Chat Session `User` (was `Either`), Check User
    Subscription `Either` (was `User`), Get Channel iCalendar `None` (was
    `Custom`); five endpoints list an additional accepted scope
  - stability: the docs no longer mark anything "new", so the ten Helix
    endpoints and seven subscriptions that were `New` are `Ga`; the beta
    counts are unchanged (12 Helix, 4 EventSub)
  - fixed: generated Helix response fields decode `null` as the default
    value, as the EventSub payloads already did. Before, any `null` failed
    the whole typed parse, including Twitch's own Get Predictions example
    (`winning_outcome_id`, `ended_at`, `locked_at`)
  - `.github/workflows/twitch-surface.yml` uses `fetch` and `build
    --docs-dir`
- Root feature flags:
  - none
- Known remaining gaps:
  - the docs flag only some of the fields they describe as null, so
    `nullable` in the catalog is incomplete and nothing is typed `Option`
  - request parameters and bodies sit in the same markdown lists but are
    still not captured in the catalog
  - Get Channel iCalendar stays untyped (the response is not JSON)

### Dependency updates

- Public API added/changed:
  - breaking: `worker` 0.8 → 0.9 for the `cloudflare-worker` feature.
    `worker::Error` is part of `CloudflareWorkerError::Transport` and of the
    `cloudflare_worker::send_prepared_request*` return types, so downstream
    crates have to move to `worker` 0.9 in the same step; no barbed code
    needed to change
  - breaking: MSRV raised from 1.97 to 1.98 (stable minus two); no dependency
    requires it yet
  - `Cargo.lock` refreshed; `url` 2.5.9 fixes RUSTSEC-2026-0592 (stack
    overflow in `Url::origin()` on nested `blob:` URLs, which barbed never
    calls)
  - `.cargo/audit.toml` ignores RUSTSEC-2026-0543 (`chrono` is unmaintained);
    barbed itself uses `time`, and `chrono` only arrives through `worker`
- Root feature flags:
  - `cloudflare-worker` for the `worker` bump; none otherwise
- Known remaining gaps:
  - `chrono` stays in the dependency tree until `worker` ships its Jiff
    migration (cloudflare/workers-rs#1149); remove the audit ignore then

### Emotes: one `barbed-emotes` crate, without I/O

- Public API added/changed:
  - new crate `barbed-emotes`, re-exported as `barbed::emotes`: Twitch, 7TV,
    BetterTTV and FrankerFaceZ emotes for any HTTP client and any target
    (it depends on `serde`, `serde_json`, `bitflags` and `thiserror` only and
    builds for `wasm32-unknown-unknown`). It replaces `barbed-core`'s emote
    types and the `barbed-7tv`, `barbed-bttv` and `barbed-ffz` crates, which
    could only parse inside their own `reqwest` clients; DitoDitoInfinito
    and LUPO each carried a hand-written copy because of it
  - breaking: `barbed-core`, `barbed-7tv`, `barbed-bttv` and `barbed-ffz`
    leave the workspace (their 0.0.5 releases stay on crates.io), and with
    them `barbed::seventv`, `barbed::bttv`, `barbed::ffz`, the `*Api` traits,
    the `InMemory*Api` fakes, the three `reqwest` clients and
    `barbed_core::USER_AGENT`
  - breaking: the model. `EmoteImage` is `url`, `scale` (1 to 4), `animated`,
    `format` (`ImageFormat`: PNG, GIF, WebP, AVIF), `theme` and
    `width`/`height`; `EmoteImageFormat`, `EmoteImageScale` and
    `EmoteThemeMode` are gone. `Emote` gains `animated` and `hidden`
    (`is_animated()` is gone). `Emote::image(&ImageRequest)` picks the file
    to draw by scale, motion, theme and the formats the caller decodes
  - breaking: modifiers. `EmoteModifier` gains `target` (`Preceding` or
    `Following`); 7TV's zero-width emotes are overlays
    (`EmoteModifier::overlay()`), BetterTTV's `w!`, `h!`, … are effects on the
    emote after them, and `EmoteModifierFlags` names all of FrankerFaceZ's
    bits (`SLIDE`, `APPEAR`, `LEAVE`, `ROTATE`, `ROTATE_90`, `GREYSCALE`,
    `SEPIA`, `NO_SPACE`) plus `ROTATE_270`. FrankerFaceZ's top-level `hidden`
    is `Emote::hidden` and no longer sets `HIDDEN`, which is the modifier's
    own flag, as FrankerFaceZ's client reads them
  - parsers take the answer's body as text: `twitch::parse_emotes` (Helix's
    three emote lists), `seventv::{parse_user, parse_emote_set}` (the user
    answer carries the channel's set, one request instead of two),
    `bttv::{parse_global, parse_channel}`, `ffz::{parse_global, parse_room}`
    (the room's own set is told from the rest), next to the addresses to ask
    (`seventv::user_url`, `bttv::GLOBAL_URL`, …)
  - a provider's answer is treated as untrusted: ids have one plain shape
    (`is_plain_id`), every image is an `https` address on the provider's own
    image host under a path that cannot leave it, and an entry that fails is
    left out and counted (`EmoteSet::skipped`) instead of failing the set
  - 7TV images list every scale and format (with an animated emote's still
    frame) instead of guessing the scale from a substring of the file name;
    BetterTTV images include the WebP and still files; FrankerFaceZ images
    come in a fixed order
  - `twitch::{image_url, image, emote, emote_from_lists}` build Twitch's
    images from an id; `EventSubMessageEmote::to_emote` now returns `Option`
    and takes every scale when the fragment lists none
  - matching: `EmoteIndex` (a channel's code over a global one, then 7TV over
    BetterTTV over FrankerFaceZ, configurable) and `Tokenizer`, which cuts a
    message into `Piece::Text` and `Piece::Emote` with the modifiers attached
    to the emote they change; `EventSubChatMessageText::tokenize` does it for
    an EventSub chat message, `emotes::HelixEmotes` turns Helix's emote lists
    into `Emote`s
  - loading: `SetLoader` says which sets a channel's chat needs as requests
    the caller makes, treats a channel's `404` as "no set", retries a failed
    direct request through a proxy, and reports each set (`SetReport`)
  - `proxy::{route, upstream, proxied}`: the closed list of addresses a
    server may fetch for a client, shared by both sides, with
    `Upstream::accepts` for the answer's media type
- Root feature flags:
  - none for all of the above; `seventv`, `bttv`, `ffz` and their `-reqwest`
    variants are removed
  - `reqwest-client` adds `emotes::EmoteClient`, which makes a `SetLoader`'s
    requests (in `barbed-emotes`: its own `reqwest-client` feature)
- Known remaining gaps:
  - DitoDitoInfinito, its server's emote proxy and LUPO still use their own
    copies; this entry only makes the crate they can move to
  - reading an image's container, animation limits and image caches stay
    with the display
  - an IRC message's `emotes` tag is not turned into pieces yet (EventSub
    fragments are)
  - the parsers were run against the three providers' global sets as they
    answered on 2026-11-24; the channel endpoints are read from the
    providers' documentation and 7TV's source, with fixtures of that shape
  - FrankerFaceZ's mask images have no proxy route

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
