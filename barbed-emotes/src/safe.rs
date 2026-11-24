//! Checks on what a provider's answer names.
//!
//! An emote set is someone else's data: an id or an image address in it is
//! used to build URLs a client then requests, sometimes through its own
//! server's proxy and with its own cookies. So ids have one plain shape and
//! every image stays on its provider's image host, under a path that cannot
//! climb out of it.

/// Whether `id` has the shape every provider gives its ids: 1 to 64 of
/// `[0-9A-Za-z_-]` (Twitch's `emotesv2_…` ids are about 40 characters; 7TV's
/// and BetterTTV's 24 to 26; FrankerFaceZ's are numbers).
pub fn is_plain_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Whether `path` (from its first `/`) is made only of segments of
/// `[0-9A-Za-z_.-]`, none of them `.` or `..`: no escape, query, fragment or
/// backslash, 256 bytes at most.
pub(crate) fn is_plain_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix('/') else {
        return false;
    };
    path.len() <= 256
        && rest.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
        })
}

/// The path of `url` (`https://host/path`, `http://…` or `//host/path`) when
/// it is on `host` and plain ([`is_plain_path`]).
pub(crate) fn path_on<'a>(host: &str, url: &'a str) -> Option<&'a str> {
    let rest = url
        .strip_prefix("https:")
        .or_else(|| url.strip_prefix("http:"))
        .unwrap_or(url);
    let rest = rest.strip_prefix("//")?;
    let (found, path) = rest.split_at(rest.find('/')?);
    (found == host && is_plain_path(path)).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_have_one_plain_shape() {
        assert!(is_plain_id("emotesv2_1a2b"));
        assert!(is_plain_id("60ae7316f7c927fad14e6ca2"));
        assert!(is_plain_id("25"));
        for bad in ["", "../x", "a b", "a/b", "a%2e", "a?b", &"x".repeat(65)] {
            assert!(!is_plain_id(bad), "{bad}");
        }
    }

    #[test]
    fn paths_cannot_leave_their_host() {
        assert!(is_plain_path("/emote/1/2x.webp"));
        for bad in [
            "emote/1",
            "/",
            "/emote//1",
            "/emote/../me",
            "/emote/./1",
            "/emote/%2e%2e",
            "/emote/1?x=1",
            "/emote/1#x",
            "/emote\\1",
        ] {
            assert!(!is_plain_path(bad), "{bad}");
        }
    }

    #[test]
    fn a_path_is_read_only_on_its_host() {
        let host = "cdn.7tv.app";
        assert_eq!(path_on(host, "//cdn.7tv.app/emote/a"), Some("/emote/a"));
        assert_eq!(
            path_on(host, "https://cdn.7tv.app/emote/a"),
            Some("/emote/a")
        );
        assert_eq!(path_on(host, "https://evil.example/emote/a"), None);
        assert_eq!(path_on(host, "https://cdn.7tv.app.evil.example/a"), None);
        assert_eq!(path_on(host, "//cdn.7tv.app/../x"), None);
        assert_eq!(path_on(host, "cdn.7tv.app/emote/a"), None);
    }
}
