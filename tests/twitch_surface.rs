//! Triangulates the checked-in catalog JSON, the generated registries, and the
//! generated summary counts against each other.

use barbed::{eventsub, helix, twitch_catalog};
use serde::Deserialize;

#[derive(Deserialize)]
struct Summary {
    counts: SummaryCounts,
}

#[derive(Deserialize)]
struct SummaryCounts {
    helix_total: usize,
    helix_ga_or_new: usize,
    helix_beta: usize,
    eventsub_total: usize,
    eventsub_ga_or_new: usize,
    eventsub_beta: usize,
    eventsub_typed_events: usize,
    eventsub_untyped_events: usize,
    auth_flows: usize,
    auth_endpoints: usize,
}

#[derive(Deserialize)]
struct HelixCatalog {
    endpoints: Vec<Stability>,
}

#[derive(Deserialize)]
struct EventSubCatalog {
    subscriptions: Vec<EventSubCatalogEntry>,
}

#[derive(Deserialize)]
struct EventSubCatalogEntry {
    stability: String,
    event: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Stability {
    stability: String,
}

#[derive(Deserialize)]
struct AuthCatalog {
    flows: Vec<serde_json::Value>,
    endpoints: Vec<serde_json::Value>,
}

fn summary() -> Summary {
    serde_json::from_str(include_str!("../twitch_catalog/summary.json"))
        .expect("summary.json should decode")
}

fn helix_catalog() -> HelixCatalog {
    serde_json::from_str(include_str!("../twitch_catalog/helix.json"))
        .expect("helix.json should decode")
}

fn eventsub_catalog() -> EventSubCatalog {
    serde_json::from_str(include_str!("../twitch_catalog/eventsub.json"))
        .expect("eventsub.json should decode")
}

fn auth_catalog() -> AuthCatalog {
    serde_json::from_str(include_str!("../twitch_catalog/auth.json"))
        .expect("auth.json should decode")
}

fn beta_count(entries: &[Stability]) -> usize {
    entries
        .iter()
        .filter(|entry| entry.stability == "beta")
        .count()
}

#[test]
fn generated_helix_surface_matches_checked_in_catalog() {
    let summary = summary();
    let catalog = helix_catalog();

    assert_eq!(summary.counts.helix_total, helix::ALL_ENDPOINTS.len());
    assert_eq!(summary.counts.helix_total, catalog.endpoints.len());
    assert_eq!(
        summary.counts.helix_ga_or_new,
        twitch_catalog::ga_or_new_helix_endpoint_count()
    );
    assert_eq!(
        summary.counts.helix_beta,
        twitch_catalog::beta_helix_endpoint_count()
    );
    assert_eq!(summary.counts.helix_beta, beta_count(&catalog.endpoints));
}

#[test]
fn generated_eventsub_surface_matches_checked_in_catalog() {
    let summary = summary();
    let catalog = eventsub_catalog();

    assert_eq!(
        summary.counts.eventsub_total,
        eventsub::ALL_EVENTSUB_SUBSCRIPTIONS.len()
    );
    assert_eq!(summary.counts.eventsub_total, catalog.subscriptions.len());
    assert_eq!(
        summary.counts.eventsub_ga_or_new,
        twitch_catalog::ga_or_new_eventsub_count()
    );
    assert_eq!(
        summary.counts.eventsub_beta,
        twitch_catalog::beta_eventsub_count()
    );
    assert_eq!(
        summary.counts.eventsub_beta,
        catalog
            .subscriptions
            .iter()
            .filter(|entry| entry.stability == "beta")
            .count()
    );

    let typed = catalog
        .subscriptions
        .iter()
        .filter(|entry| entry.event.is_some())
        .count();
    assert_eq!(summary.counts.eventsub_typed_events, typed);
    assert_eq!(
        summary.counts.eventsub_untyped_events,
        catalog.subscriptions.len() - typed
    );
    assert_eq!(
        summary.counts.eventsub_typed_events + summary.counts.eventsub_untyped_events,
        summary.counts.eventsub_total
    );
}

#[test]
fn summary_matches_auth_catalog() {
    let summary = summary();
    let catalog = auth_catalog();

    assert_eq!(summary.counts.auth_flows, catalog.flows.len());
    assert_eq!(summary.counts.auth_endpoints, catalog.endpoints.len());
}

#[test]
fn beta_surfaces_remain_explicitly_counted() {
    assert_eq!(twitch_catalog::beta_helix_endpoint_count(), 12);
    assert_eq!(twitch_catalog::beta_eventsub_count(), 4);
}
