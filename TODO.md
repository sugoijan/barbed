# TODO

Deferred design work, mostly fallout from the 2026-07 code review.

## Unify the generated Helix endpoint registry with the hand-written builders

The generated registry (`src/helix_generated.rs`: `ALL_ENDPOINTS`, typed request
structs, `HelixEndpoint::prepare`) is exercised only by catalog-count tests; all
real flows use the hand-written builders in `src/helix.rs` because the generator
stubs every response as untyped `HelixJsonResponse`. Pick one altitude:

- teach `tools/twitch_surface.py` to emit typed response structs so the
  hand-written per-endpoint builders/parsers can be deleted, or
- demote the registry to pure metadata and stop generating request machinery
  nothing calls.

Related: `list_eventsub_subscriptions_request` hardcodes
`?type=channel.chat.message`; the subscription type should become a parameter
when this surface is reworked.

## Stop re-decoding catalog JSON at runtime

`src/twitch_catalog.rs` decodes ~127 KB of embedded JSON via `OnceLock` to
recompute stability counts that `summary.json` (and the generated
`ALL_ENDPOINTS`/`ALL_SUBSCRIPTIONS` consts) already carry. Compute the counts
from the generated consts or have the generator emit them; keep the JSON only
if a consumer needs metadata the Rust consts lack. Same smell: the auth catalog
(`twitch_catalog/auth.json`) duplicates the URL constants in `src/oauth.rs`
with nothing linking them.

## Parallelize the EventSub subscription POSTs at stream connect

`create_chat_subscription` (`src/native.rs`) awaits the `channel.chat.message`
and `channel.chat.message_delete` subscription requests sequentially; they
could run under `tokio::join!` to save a round-trip, but the "first must
succeed, second is best-effort" semantics need to be preserved.
