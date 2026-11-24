//! Emote requests through one's own server.
//!
//! A browser cannot always ask a provider directly: BetterTTV's images come
//! without CORS headers, Helix needs a token, and a network may block a
//! host. A proxy fixes that, but an open one would fetch anything. This
//! module is the closed list both sides share:
//!
//! - the client turns a provider's address into a route ([`route`]), such as
//!   `7tv/emote/<id>/2x.webp`, and asks its server for it under whatever
//!   base the server mounts the proxy at;
//! - the server turns the route back into the one address it stands for
//!   ([`upstream`]), or refuses it.
//!
//! Only the shapes this crate itself produces pass: the four providers'
//! image files and their emote set endpoints, every id of one plain shape
//! ([`is_plain_id`]). A route cannot name a host, a
//! query, or a path outside those shapes.
//!
//! | Route | Upstream |
//! | --- | --- |
//! | `twitch/emoticons/v2/<id>/<static\|animated>/<dark\|light>/<1.0\|2.0\|3.0>` | `static-cdn.jtvnw.net` |
//! | `twitch-api/chat/emotes/global` | `api.twitch.tv/helix`, with the server's app token |
//! | `7tv/emote/<id>/<1-4>x[_static].<webp\|avif\|gif\|png>` | `cdn.7tv.app` |
//! | `7tv-api/v3/users/twitch/<id>`, `7tv-api/v3/emote-sets/<id>` | `7tv.io` |
//! | `bttv/emote/<id>/<1-3>x[.webp]`, `bttv/emote/<id>/static/<1-3>x.webp` | `cdn.betterttv.net` |
//! | `bttv-api/3/cached/users/twitch/<id>`, `bttv-api/3/cached/emotes/global` | `api.betterttv.net` |
//! | `ffz/emote/<id>/<1\|2\|4>`, `ffz/emote/<id>/animated/<1\|2\|4>` | `cdn.frankerfacez.com` |
//! | `ffz-api/v1/room/id/<id>`, `ffz-api/v1/set/global` | `api.frankerfacez.com` |

use crate::model::EmoteProvider;
use crate::safe::is_plain_id;

/// What a route's answer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UpstreamKind {
    /// An emote's image file.
    Image,
    /// An emote set, as JSON.
    Json,
}

/// What the server adds to its request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UpstreamAuth {
    /// Nothing: the address is public.
    None,
    /// The server's own Twitch app access token and client id (Helix).
    TwitchApp,
}

/// The one address a route stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upstream {
    pub url: String,
    pub provider: EmoteProvider,
    pub kind: UpstreamKind,
    pub auth: UpstreamAuth,
}

impl Upstream {
    /// Whether an answer of `content_type` is what this route may pass on:
    /// an image for an image route, JSON for a set. Anything else (an HTML
    /// error page, a script) is refused, so the proxy never serves a
    /// provider's active content from its own origin.
    pub fn accepts(&self, content_type: &str) -> bool {
        let media = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match self.kind {
            UpstreamKind::Json => media == "application/json",
            UpstreamKind::Image => matches!(
                media.as_str(),
                "image/png" | "image/webp" | "image/gif" | "image/avif" | "image/jpeg"
            ),
        }
    }
}

fn one_of(value: &str, list: &[&str]) -> bool {
    list.contains(&value)
}

/// Whether `file` is a 7TV image file: `<1-4>x[_static].<format>`.
fn seventv_file(file: &str) -> bool {
    let Some((stem, extension)) = file.split_once('.') else {
        return false;
    };
    let stem = stem.strip_suffix("_static").unwrap_or(stem);
    one_of(stem, &["1x", "2x", "3x", "4x"]) && one_of(extension, &["webp", "avif", "gif", "png"])
}

/// The address `route` (`<name>/<path>`, no leading `/`) stands for, or
/// `None` where it is not one of the shapes of this module's table. This is
/// the server's whole check of a request.
pub fn upstream(route: &str) -> Option<Upstream> {
    use EmoteProvider::{BetterTtv, FrankerFaceZ, SevenTv, Twitch};
    use UpstreamKind::{Image, Json};

    let (name, path) = route.split_once('/')?;
    let parts: Vec<&str> = path.split('/').collect();
    let (provider, kind, auth, host) = match name {
        "twitch" => (Twitch, Image, UpstreamAuth::None, "static-cdn.jtvnw.net"),
        "twitch-api" => (Twitch, Json, UpstreamAuth::TwitchApp, "api.twitch.tv/helix"),
        "7tv" => (SevenTv, Image, UpstreamAuth::None, "cdn.7tv.app"),
        "7tv-api" => (SevenTv, Json, UpstreamAuth::None, "7tv.io"),
        "bttv" => (BetterTtv, Image, UpstreamAuth::None, "cdn.betterttv.net"),
        "bttv-api" => (BetterTtv, Json, UpstreamAuth::None, "api.betterttv.net"),
        "ffz" => (
            FrankerFaceZ,
            Image,
            UpstreamAuth::None,
            "cdn.frankerfacez.com",
        ),
        "ffz-api" => (
            FrankerFaceZ,
            Json,
            UpstreamAuth::None,
            "api.frankerfacez.com",
        ),
        _ => return None,
    };
    let known = match (name, parts.as_slice()) {
        ("twitch", ["emoticons", "v2", id, format, theme, scale]) => {
            is_plain_id(id)
                && one_of(format, &["static", "animated"])
                && one_of(theme, &["dark", "light"])
                && one_of(scale, &["1.0", "2.0", "3.0"])
        }
        ("twitch-api", ["chat", "emotes", "global"]) => true,
        ("7tv", ["emote", id, file]) => is_plain_id(id) && seventv_file(file),
        ("7tv-api", ["v3", "users", "twitch", id]) => is_plain_id(id),
        ("7tv-api", ["v3", "emote-sets", id]) => is_plain_id(id),
        ("bttv", ["emote", id, size]) => {
            is_plain_id(id) && one_of(size, &["1x", "2x", "3x", "1x.webp", "2x.webp", "3x.webp"])
        }
        ("bttv", ["emote", id, "static", size]) => {
            is_plain_id(id) && one_of(size, &["1x.webp", "2x.webp", "3x.webp"])
        }
        ("bttv-api", ["3", "cached", "users", "twitch", id]) => is_plain_id(id),
        ("bttv-api", ["3", "cached", "emotes", "global"]) => true,
        ("ffz", ["emote", id, size]) => is_plain_id(id) && one_of(size, &["1", "2", "4"]),
        ("ffz", ["emote", id, "animated", size]) => {
            is_plain_id(id) && one_of(size, &["1", "2", "4"])
        }
        ("ffz-api", ["v1", "room", "id", id]) => is_plain_id(id),
        ("ffz-api", ["v1", "set", "global"]) => true,
        _ => false,
    };
    known.then(|| Upstream {
        url: format!("https://{host}/{path}"),
        provider,
        kind,
        auth,
    })
}

/// The route that stands for `url`, or `None` where the proxy does not
/// carry it: the client's side of [`upstream`]. For every route this
/// returns, `upstream(route)` gives `url` back.
pub fn route(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let (name, path) = match host {
        "static-cdn.jtvnw.net" => ("twitch", path),
        "api.twitch.tv" => ("twitch-api", path.strip_prefix("helix/")?),
        "cdn.7tv.app" => ("7tv", path),
        "7tv.io" => ("7tv-api", path),
        "cdn.betterttv.net" => ("bttv", path),
        "api.betterttv.net" => ("bttv-api", path),
        "cdn.frankerfacez.com" => ("ffz", path),
        "api.frankerfacez.com" => ("ffz-api", path),
        _ => return None,
    };
    let route = format!("{name}/{path}");
    (upstream(&route)?.url == url).then_some(route)
}

/// `url` as asked through a proxy mounted at `base`
/// (`https://example.test/api/emotes`): `<base>/<route>`.
pub fn proxied(base: &str, url: &str) -> Option<String> {
    Some(format!("{}/{}", base.trim_end_matches('/'), route(url)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(route: &str) -> Option<String> {
        upstream(route).map(|upstream| upstream.url)
    }

    #[test]
    fn only_the_four_providers_shapes_pass() {
        assert_eq!(
            url("twitch/emoticons/v2/emotesv2_ab12/static/dark/2.0").as_deref(),
            Some("https://static-cdn.jtvnw.net/emoticons/v2/emotesv2_ab12/static/dark/2.0")
        );
        assert_eq!(
            url("twitch/emoticons/v2/emotesv2_ab12/animated/dark/2.0").as_deref(),
            Some("https://static-cdn.jtvnw.net/emoticons/v2/emotesv2_ab12/animated/dark/2.0")
        );
        assert_eq!(
            url("bttv/emote/5e76d338d6581c3724c0f0b2/2x").as_deref(),
            Some("https://cdn.betterttv.net/emote/5e76d338d6581c3724c0f0b2/2x")
        );
        assert_eq!(
            url("ffz/emote/720507/animated/2").as_deref(),
            Some("https://cdn.frankerfacez.com/emote/720507/animated/2")
        );
        for good in [
            "7tv/emote/01F6M/2x.webp",
            "7tv/emote/01F6M/4x_static.avif",
            "7tv-api/v3/users/twitch/1971641",
            "7tv-api/v3/emote-sets/global",
            "7tv-api/v3/emote-sets/01F6M",
            "bttv/emote/5e76/3x.webp",
            "bttv/emote/5e76/static/1x.webp",
            "bttv-api/3/cached/users/twitch/1971641",
            "bttv-api/3/cached/emotes/global",
            "ffz/emote/128054/2",
            "ffz-api/v1/room/id/1971641",
            "ffz-api/v1/set/global",
        ] {
            assert!(upstream(good).is_some(), "{good}");
        }
        for bad in [
            "",
            "twitch",
            "twitch/",
            "twitch/emoticons/v2/25/static/dark/4.0",
            "twitch/emoticons/v2/25/default/dark/2.0",
            "twitch/emoticons/v2/../static/dark/2.0",
            "twitch/emoticons/v1/25/2.0",
            "7tv/emote/01F6M/2x.exe",
            "7tv/emote/01F6M/5x.webp",
            "7tv/emote/01F6M/2x_static_static.webp",
            "7tv/emote/01F6M",
            "7tv-api/v3/users/youtube/1",
            "7tv-api/v3/users/twitch/1/presences",
            "bttv/emote/x/9x",
            "bttv/emote/a b/2x",
            "bttv/emote/x/static/2x",
            "bttv-api/3/cached/emotes/global?x=1",
            "ffz/emote/1/animated/3",
            "ffz/emote/1/still/2",
            "ffz/emote/1/2/3",
            "ffz-api/v1/room/streamer",
            "evil/emote/1/2",
            "/twitch/emoticons/v2/25/static/dark/2.0",
            "https://cdn.7tv.app/emote/01F6M/2x.webp",
        ] {
            assert_eq!(upstream(bad), None, "{bad}");
        }
    }

    #[test]
    fn helix_is_asked_with_the_servers_token_and_nothing_else_of_it() {
        let global = upstream("twitch-api/chat/emotes/global").expect("a route");
        assert_eq!(global.url, "https://api.twitch.tv/helix/chat/emotes/global");
        assert_eq!(global.url, crate::twitch::GLOBAL_EMOTES_URL);
        assert_eq!(global.auth, UpstreamAuth::TwitchApp);
        assert_eq!(global.kind, UpstreamKind::Json);
        for bad in [
            "twitch-api/chat/emotes",
            "twitch-api/chat/emotes/global/x",
            "twitch-api/chat/emotes?broadcaster_id=1",
            "twitch-api/users",
        ] {
            assert_eq!(upstream(bad), None, "{bad}");
        }
        assert_eq!(
            upstream("7tv/emote/a/2x.webp").unwrap().auth,
            UpstreamAuth::None
        );
    }

    #[test]
    fn an_address_and_its_route_stand_for_each_other() {
        for address in [
            "https://static-cdn.jtvnw.net/emoticons/v2/25/static/dark/2.0",
            "https://api.twitch.tv/helix/chat/emotes/global",
            "https://cdn.7tv.app/emote/60ae7316f7c927fad14e6ca2/2x.webp",
            "https://7tv.io/v3/users/twitch/1971641",
            "https://7tv.io/v3/emote-sets/global",
            "https://cdn.betterttv.net/emote/5f1b0186cf6d2144653d2971/2x",
            "https://cdn.betterttv.net/emote/5f1b0186cf6d2144653d2971/static/2x.webp",
            "https://api.betterttv.net/3/cached/users/twitch/1971641",
            "https://api.betterttv.net/3/cached/emotes/global",
            "https://cdn.frankerfacez.com/emote/720507/animated/2",
            "https://api.frankerfacez.com/v1/room/id/1971641",
            "https://api.frankerfacez.com/v1/set/global",
        ] {
            let route = route(address).unwrap_or_else(|| panic!("{address}"));
            assert_eq!(url(&route).as_deref(), Some(address));
        }
        assert_eq!(
            route("https://static-cdn.jtvnw.net/emoticons/v2/25/static/dark/2.0").as_deref(),
            Some("twitch/emoticons/v2/25/static/dark/2.0")
        );
        assert_eq!(
            route("https://api.twitch.tv/helix/chat/emotes/global").as_deref(),
            Some("twitch-api/chat/emotes/global")
        );
        for other in [
            "http://cdn.7tv.app/emote/a/2x.webp",
            "//cdn.7tv.app/emote/a/2x.webp",
            "https://cdn.7tv.app/emote/a/2x.webp?x=1",
            "https://cdn.7tv.app/user/a/avatar.webp",
            "https://api.twitch.tv/helix/users",
            "https://api.twitch.tv/chat/emotes/global",
            "https://example.test/emote/a/2x.webp",
            "shape:circle:f08030",
        ] {
            assert_eq!(route(other), None, "{other}");
        }
    }

    #[test]
    fn a_proxied_address_is_under_the_base() {
        assert_eq!(
            proxied(
                "https://example.test/api/emotes/",
                "https://cdn.betterttv.net/emote/5f1b/2x"
            )
            .as_deref(),
            Some("https://example.test/api/emotes/bttv/emote/5f1b/2x")
        );
        assert_eq!(
            proxied("https://example.test", "https://example.test/x"),
            None
        );
    }

    #[test]
    fn only_images_and_json_are_passed_on() {
        let image = upstream("7tv/emote/a/2x.webp").unwrap();
        assert!(image.accepts("image/webp"));
        assert!(image.accepts("IMAGE/GIF; charset=binary"));
        assert!(!image.accepts("application/json"));
        assert!(!image.accepts("text/html"));
        assert!(!image.accepts("image/svg+xml"));
        let set = upstream("7tv-api/v3/emote-sets/global").unwrap();
        assert!(set.accepts("application/json; charset=utf-8"));
        assert!(!set.accepts("image/png"));
        assert!(!set.accepts("text/html"));
    }
}
