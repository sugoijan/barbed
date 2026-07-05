//! Counts over the generated Twitch surface registries.
//!
//! The full catalogs live as JSON under `twitch_catalog/` in the repository and
//! feed `tools/twitch_surface.py`, which generates the endpoint and
//! subscription registries compiled into this crate. These helpers recompute
//! the summary counts from those generated constants; the JSON itself is not
//! embedded in the library.

use crate::helix::EndpointStability;

pub fn ga_or_new_helix_endpoint_count() -> usize {
    crate::helix::ALL_ENDPOINTS
        .iter()
        .filter(|endpoint| endpoint.stability != EndpointStability::Beta)
        .count()
}

pub fn beta_helix_endpoint_count() -> usize {
    crate::helix::ALL_ENDPOINTS.len() - ga_or_new_helix_endpoint_count()
}

pub fn ga_or_new_eventsub_count() -> usize {
    crate::eventsub::ALL_EVENTSUB_SUBSCRIPTIONS
        .iter()
        .filter(|subscription| subscription.stability != EndpointStability::Beta)
        .count()
}

pub fn beta_eventsub_count() -> usize {
    crate::eventsub::ALL_EVENTSUB_SUBSCRIPTIONS.len() - ga_or_new_eventsub_count()
}
