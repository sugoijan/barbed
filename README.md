# barbed

[![github](https://img.shields.io/badge/github-sugoijan/barbed-8da0cb?logo=github)](https://github.com/sugoijan/barbed)
[![crates.io](https://img.shields.io/crates/v/barbed.svg?logo=rust)](https://crates.io/crates/barbed)
[![docs.rs](https://img.shields.io/badge/docs.rs-barbed-66c2a5?logo=docs.rs)](https://docs.rs/barbed)
[![CI](https://img.shields.io/github/actions/workflow/status/sugoijan/barbed/ci.yml?branch=main)](https://github.com/sugoijan/barbed/actions?query=branch%3Amain)

> [!CAUTION]
> This project is pretty much experimental and new releases WILL BREAK, VERY OFTEN.

[![photo of barbed wires](./barbed.jpg)](#)

`barbed` is a small Rust workspace for Twitch and Twitch-adjacent integrations.

The root `barbed` crate currently includes:

- OAuth (authorize URL, device code flow, token refresh, signed state verification)
- Helix request builders and response parsers
- EventSub WebSocket decoding, chat subscription helpers, and a native async stream client
- Checked-in Twitch surface catalogs plus generated low-level Helix/EventSub registries
- Twitch IRC parsing plus an optional native async IRC client
- Provider-neutral emote/domain types
- HMAC signing helpers
- Trait-based auth/session abstractions with in-memory reference implementations

Optional feature flags add integrations for specific runtimes and providers:

- `cloudflare-worker` — send prepared requests via the Cloudflare Workers Fetch API
- `reqwest-client` — native HTTP helpers built on `reqwest`
- `tokio-eventsub` — native EventSub WebSocket client using `tokio` and `tungstenite`
- `tokio-irc` — native Twitch IRC client using `tokio`
- `seventv`, `bttv`, `ffz` — re-export the corresponding provider crates from the root crate
- `seventv-reqwest`, `bttv-reqwest`, `ffz-reqwest` — enable reqwest-backed HTTP clients in those provider crates

Workspace crates:

- `barbed` — core Twitch/OAuth/EventSub/IRC APIs
- `barbed-7tv` — 7TV API coverage used by LUPO
- `barbed-bttv` — BetterTTV API coverage used by LUPO
- `barbed-ffz` — FrankerFaceZ API coverage used by LUPO
- `barbed-core` — internal shared domain types used by the workspace

Generated Twitch surface data lives in [`twitch_catalog`](/Users/jan/sugoijan/barbed/twitch_catalog/summary.json) and is refreshed by [`tools/twitch_surface.py`](/Users/jan/sugoijan/barbed/tools/twitch_surface.py).

> [!NOTE]
> Disclaimer: this crate is heavily vibecoded, if you don't like it, don't use it.
