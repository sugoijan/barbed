//! The providers' real answers, read by the parsers. Opt-in, because the
//! answers are third-party data that is never committed: record them into a
//! folder of your own and point the test at it.
//!
//! ```sh
//! mkdir sets && cd sets
//! curl -o 7tv-global.json https://7tv.io/v3/emote-sets/global
//! curl -o bttv-global.json https://api.betterttv.net/3/cached/emotes/global
//! curl -o ffz-global.json https://api.frankerfacez.com/v1/set/global
//! # and, for a channel of Twitch user <id>:
//! curl -o 7tv-user.json https://7tv.io/v3/users/twitch/<id>
//! curl -o bttv-user.json https://api.betterttv.net/3/cached/users/twitch/<id>
//! curl -o ffz-room.json https://api.frankerfacez.com/v1/room/id/<id>
//! cd .. && BARBED_EMOTE_SETS=sets cargo test -p barbed-emotes --test recorded_sets -- --ignored --nocapture
//! ```
//!
//! Whichever of the files are there are read. Nothing may be skipped (an
//! entry left out means a provider's answer has a shape the parser does not
//! know), every image must be one the proxy carries, and every emote must
//! have an image a display can ask for. A global set must have emotes; a
//! channel's may be empty.

use barbed_emotes::{Emote, ImageFormat, ImageRequest, bttv, ffz, proxy, seventv, twitch};

fn recorded(name: &str) -> Option<String> {
    let folder = std::env::var_os("BARBED_EMOTE_SETS")?;
    std::fs::read_to_string(std::path::Path::new(&folder).join(name)).ok()
}

fn check(what: &str, emotes: &[Emote], skipped: usize) {
    let global = what.ends_with("global");
    let animated = emotes.iter().filter(|emote| emote.animated).count();
    let modifiers = emotes
        .iter()
        .filter(|emote| emote.modifier.is_some())
        .count();
    let hidden = emotes.iter().filter(|emote| emote.hidden).count();
    eprintln!(
        "{what}: {} emotes, {animated} animated, {modifiers} modifiers, {hidden} hidden, {skipped} skipped",
        emotes.len()
    );
    assert_eq!(skipped, 0, "{what}: entries the parser does not know");
    assert!(!global || !emotes.is_empty(), "{what}: no emote");
    let decodable = [ImageFormat::WebP, ImageFormat::Gif, ImageFormat::Png];
    let request = ImageRequest {
        formats: &decodable,
        ..ImageRequest::default()
    };
    for emote in emotes {
        let host = emote.id.provider.image_host().expect("a provider's host");
        for image in &emote.images {
            assert!(
                image.url.starts_with(&format!("https://{host}/")),
                "{what} {}: {}",
                emote.code,
                image.url
            );
            assert!(
                proxy::route(&image.url).is_some(),
                "{what} {}: no proxy route for {}",
                emote.code,
                image.url
            );
        }
        let image = emote
            .image(&request)
            .unwrap_or_else(|| panic!("{what} {}: no image to ask for", emote.code));
        assert_eq!(image.scale, 2, "{what} {}: {}", emote.code, image.url);
        assert_eq!(image.animated, emote.animated, "{what} {}", emote.code);
    }
}

#[test]
#[ignore = "needs BARBED_EMOTE_SETS, a folder of recorded answers"]
fn the_recorded_sets_are_read() {
    let mut read = 0;
    if let Some(body) = recorded("7tv-global.json") {
        let set = seventv::parse_emote_set(&body).expect("7TV's global set");
        check("7tv global", &set.emotes, set.skipped);
        read += 1;
    }
    if let Some(body) = recorded("7tv-user.json") {
        let user = seventv::parse_user(&body).expect("a 7TV user");
        // A 7TV account may have no active set.
        let set = user.emote_set.unwrap_or_default();
        check("7tv channel", &set.emotes, set.skipped);
        read += 1;
    }
    if let Some(body) = recorded("bttv-global.json") {
        let set = bttv::parse_global(&body).expect("BetterTTV's globals");
        check("bttv global", &set.emotes, set.skipped);
        read += 1;
    }
    if let Some(body) = recorded("bttv-user.json") {
        let channel = bttv::parse_channel(&body).expect("a BetterTTV channel");
        let skipped = channel.skipped();
        check("bttv channel", &channel.into_emotes(), skipped);
        read += 1;
    }
    if let Some(body) = recorded("ffz-global.json") {
        let global = ffz::parse_global(&body).expect("FrankerFaceZ's global sets");
        check("ffz global", &global.default_emotes(), global.skipped());
        read += 1;
    }
    if let Some(body) = recorded("ffz-room.json") {
        let room = ffz::parse_room(&body).expect("a FrankerFaceZ room");
        let skipped = room.skipped();
        check("ffz channel", &room.into_emotes(), skipped);
        read += 1;
    }
    if let Some(body) = recorded("twitch-global.json") {
        let set = twitch::parse_emotes(&body).expect("Helix's global emotes");
        check("twitch global", &set.emotes, set.skipped);
        read += 1;
    }
    if read == 0 {
        eprintln!("BARBED_EMOTE_SETS names no folder with recorded answers: nothing read");
    }
}
