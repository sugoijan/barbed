# barbed

![Photo of barbed wires.](./barbed.jpg)

`barbed` is a small Rust crate for [Twitch integrations](https://dev.twitch.tv/docs/api/).

It currently includes:

- OAuth authorize URL construction and signed state verification
- Helix request builders and response parsers
- EventSub websocket payload decoding and chat subscription helpers
- HMAC signing helpers for short-lived tokens and state payloads
- An optional `cloudflare-worker` feature for sending prepared requests via the Cloudflare Workers `Fetch` API

The default crate stays independent of Cloudflare-specific APIs so the Twitch integration can be reused from other runtimes.

It is pretty much extracted from my other projects for my own use, but I do want to fully cover the Twitch API surface and make it generally useful, but for now it is pretty much experimental and new releases WILL BREAK, VERY OFTEN.

Disclaimer: this crate is heavily vibecoded, if you don't like it, don't use it.
