use barbed::{eventsub, helix, twitch_catalog};

#[test]
fn generated_helix_surface_matches_checked_in_catalog() {
    let summary = twitch_catalog::catalog_summary();
    assert_eq!(summary.counts.helix_total, helix::ALL_ENDPOINTS.len());
    assert_eq!(summary.counts.helix_ga_or_new, twitch_catalog::ga_or_new_helix_endpoint_count());
    assert_eq!(summary.counts.helix_beta, twitch_catalog::beta_helix_endpoint_count());
}

#[test]
fn generated_eventsub_surface_matches_checked_in_catalog() {
    let summary = twitch_catalog::catalog_summary();
    assert_eq!(
        summary.counts.eventsub_total,
        eventsub::ALL_EVENTSUB_SUBSCRIPTIONS.len()
    );
    assert_eq!(
        summary.counts.eventsub_ga_or_new,
        twitch_catalog::ga_or_new_eventsub_count()
    );
    assert_eq!(summary.counts.eventsub_beta, twitch_catalog::beta_eventsub_count());
}

#[test]
fn beta_surfaces_remain_explicitly_counted() {
    assert_eq!(twitch_catalog::beta_helix_endpoint_count(), 12);
    assert_eq!(twitch_catalog::beta_eventsub_count(), 4);
}
