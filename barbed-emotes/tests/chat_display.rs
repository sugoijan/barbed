//! A chat display from end to end, through the public API alone: a channel's
//! sets read with requests the caller makes, a message cut into text and
//! emotes, the image to draw picked for each, and the same image asked
//! through a proxy whose server checks the route.

use barbed_emotes::proxy::{self, UpstreamAuth, UpstreamKind};
use barbed_emotes::{
    Emote, EmoteProvider, EmoteScope, ImageFormat, ImageRequest, LoadError, Piece, SetLoader,
    SetRequest, Theme, Via, twitch,
};

const PROXY: &str = "https://game.example/api/emotes";

/// What a display that decodes WebP, GIF and PNG asks for: the 2× image on a
/// dark background, moving where the emote does.
const WANTED: ImageRequest<'static> = ImageRequest {
    scale: 2,
    animated: true,
    theme: Theme::Dark,
    formats: &[ImageFormat::WebP, ImageFormat::Gif, ImageFormat::Png],
};

fn fixture(request: &SetRequest) -> &'static str {
    match (&request.provider, request.scope) {
        (EmoteProvider::SevenTv, EmoteScope::Channel) => include_str!("fixtures/7tv-user.json"),
        (EmoteProvider::SevenTv, EmoteScope::Global) => include_str!("fixtures/7tv-global.json"),
        (EmoteProvider::BetterTtv, EmoteScope::Channel) => include_str!("fixtures/bttv-user.json"),
        (EmoteProvider::BetterTtv, EmoteScope::Global) => {
            include_str!("fixtures/bttv-global.json")
        }
        (EmoteProvider::FrankerFaceZ, EmoteScope::Channel) => {
            include_str!("fixtures/ffz-room.json")
        }
        (EmoteProvider::FrankerFaceZ, EmoteScope::Global) => {
            include_str!("fixtures/ffz-global.json")
        }
        other => panic!("no fixture for {other:?}"),
    }
}

/// The channel's sets, read as a browser would: BetterTTV's API unreachable
/// directly, so asked again through the proxy.
fn loaded() -> SetLoader {
    let mut loader = SetLoader::for_channel("1971641").with_proxy(PROXY);
    let mut rounds = 0;
    while !loader.is_done() {
        rounds += 1;
        for request in loader.requests() {
            // Every request is one the server's proxy would carry too.
            let direct = match request.via {
                Via::Direct => request.url.clone(),
                Via::Proxy => {
                    let route = request
                        .url
                        .strip_prefix(&format!("{PROXY}/"))
                        .expect("under the proxy's base");
                    let upstream = proxy::upstream(route).expect("a route the server knows");
                    assert_eq!(upstream.kind, UpstreamKind::Json);
                    assert_eq!(upstream.auth, UpstreamAuth::None);
                    upstream.url
                }
            };
            assert!(proxy::route(&direct).is_some(), "{direct}");
            let blocked =
                request.provider == EmoteProvider::BetterTtv && request.via == Via::Direct;
            let answer = if blocked {
                Err(LoadError::Failed("TypeError: Failed to fetch".to_string()))
            } else {
                Ok(fixture(&request))
            };
            loader.answered(request.key, request.via, answer);
        }
    }
    assert_eq!(rounds, 2);
    loader
}

/// The address the display asks for `emote`'s image.
fn wanted(emote: &Emote) -> &str {
    &emote.image(&WANTED).expect("an image").url
}

#[test]
fn a_message_becomes_text_and_images() {
    let loader = loaded();
    for set in loader.report() {
        let via = match set.provider {
            EmoteProvider::BetterTtv => Via::Proxy,
            _ => Via::Direct,
        };
        assert_eq!(set.via, Some(via), "{set}");
    }
    let index = loader.index();

    // The message as EventSub cuts it: text, one of Twitch's emotes, text.
    let kappa = twitch::emote("25", "Kappa", false).expect("a Twitch emote");
    let mut tokenizer = index.tokenizer();
    tokenizer
        .text("so catJAM ZeroCover and w! LilDance ")
        .emote(kappa, "Kappa")
        .text(" PogBTTV!");
    let pieces = tokenizer.finish();
    let source: String = pieces.iter().map(Piece::source).collect();
    assert_eq!(source, "so catJAM ZeroCover and w! LilDance Kappa PogBTTV!");

    let emotes: Vec<_> = pieces.iter().filter_map(Piece::as_emote).collect();
    assert_eq!(emotes.len(), 3);

    // 7TV's channel emote with the global zero-width one over it.
    let cat = emotes[0];
    assert_eq!(cat.source, "catJAM ZeroCover");
    assert_eq!(
        wanted(&cat.emote),
        "https://cdn.7tv.app/emote/60ae7316f7c927fad14e6ca2/2x.webp"
    );
    let overlays: Vec<&str> = cat.overlays().map(|overlay| wanted(overlay)).collect();
    assert_eq!(
        overlays,
        ["https://cdn.7tv.app/emote/01J00000000000000000000003/2x.webp"]
    );

    // FrankerFaceZ's animated emote, stretched by BetterTTV's modifier.
    let dance = emotes[1];
    assert_eq!(dance.source, "w! LilDance");
    assert_eq!(
        wanted(&dance.emote),
        "https://cdn.frankerfacez.com/emote/720507/animated/2"
    );
    assert!(
        dance
            .flags()
            .contains(barbed_emotes::EmoteModifierFlags::GROW_X)
    );
    assert_eq!(dance.overlays().count(), 0);

    // Twitch's own, from its id alone.
    assert_eq!(
        wanted(&emotes[2].emote),
        "https://static-cdn.jtvnw.net/emoticons/v2/25/static/dark/2.0"
    );

    // `PogBTTV!` is not the code `PogBTTV`.
    assert_eq!(pieces.last(), Some(&Piece::Text(" PogBTTV!".to_string())));

    // An image that failed directly is asked through the proxy, and the
    // server finds the same address behind the route.
    let image = wanted(index.get("PogBTTV").expect("an emote"));
    assert_eq!(
        image,
        "https://cdn.betterttv.net/emote/5f1b0186cf6d2144653d2970/2x.webp"
    );
    let proxied = proxy::proxied(PROXY, image).expect("a proxied address");
    assert_eq!(
        proxied,
        "https://game.example/api/emotes/bttv/emote/5f1b0186cf6d2144653d2970/2x.webp"
    );
    let upstream = proxy::upstream(&proxied[PROXY.len() + 1..]).expect("a route");
    assert_eq!(upstream.url, image);
    assert_eq!(upstream.kind, UpstreamKind::Image);
    assert!(upstream.accepts("image/webp") && !upstream.accepts("text/html"));
}

#[test]
fn a_display_that_draws_no_modifiers_keeps_their_words_as_text() {
    let loader = loaded();
    let index = loader.index_where(|emote| emote.is_standalone() && !emote.hidden);
    let pieces = index.tokenize("catJAM ZeroCover w! LilDance HiddenOne");
    let shown: Vec<String> = pieces
        .iter()
        .map(|piece| match piece {
            Piece::Text(text) => format!("{text:?}"),
            Piece::Emote(emote) => emote.emote.code.clone(),
        })
        .collect();
    assert_eq!(
        shown,
        ["catJAM", "\" ZeroCover w! \"", "LilDance", "\" HiddenOne\""]
    );
}

#[test]
fn every_image_of_every_set_is_one_the_proxy_carries() {
    let loader = loaded();
    let mut images = 0;
    for (_, provider, emotes) in loader.sets() {
        let host = provider.image_host().expect("a provider's host");
        for emote in emotes {
            assert!(!emote.images.is_empty(), "{}", emote.id);
            for image in &emote.images {
                assert!(image.url.starts_with(&format!("https://{host}/")));
                let route = proxy::route(&image.url)
                    .unwrap_or_else(|| panic!("no route for {}", image.url));
                let upstream = proxy::upstream(&route).expect("the route's address");
                assert_eq!(upstream.url, image.url);
                assert_eq!(upstream.kind, UpstreamKind::Image);
                assert_eq!(&upstream.provider, provider);
                images += 1;
            }
        }
    }
    assert!(images > 100, "{images}");
}
