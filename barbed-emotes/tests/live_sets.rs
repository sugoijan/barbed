//! The providers themselves, asked over the network. Opt-in:
//!
//! ```sh
//! cargo test -p barbed-emotes --features reqwest-client --test live_sets -- --ignored --nocapture
//! ```
#![cfg(feature = "reqwest-client")]

use barbed_emotes::{EmoteClient, SetLoader, SetStatus};

#[tokio::test(flavor = "current_thread")]
#[ignore = "asks 7TV, BetterTTV and FrankerFaceZ over the network"]
async fn the_global_sets_are_read_from_the_providers() {
    let client = EmoteClient::new().expect("an HTTP client");
    let mut loader = SetLoader::for_globals();
    client.load(&mut loader).await;
    assert!(loader.is_done());
    for set in loader.report() {
        eprintln!("{set}");
        assert_eq!(set.status, SetStatus::Read, "{set}");
        assert_eq!(set.skipped, 0, "{set}");
    }
    assert!(!loader.index().is_empty());
}
