use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct TwitchCatalogAuthMetadata {
    pub kind: String,
    #[serde(default)]
    pub raw: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct HelixCatalogEndpoint {
    pub id: String,
    pub group: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub stability: String,
    pub method: String,
    pub path: String,
    pub url: String,
    pub auth: TwitchCatalogAuthMetadata,
    pub supports_pagination: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct HelixCatalog {
    pub generated_at: String,
    pub source: String,
    pub group_counts: std::collections::BTreeMap<String, usize>,
    pub endpoints: Vec<HelixCatalogEndpoint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct EventSubCatalogSubscription {
    pub id: String,
    pub name: String,
    pub subscription_type: String,
    pub version: String,
    pub description: String,
    pub stability: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct EventSubCatalog {
    pub generated_at: String,
    pub source: String,
    pub subscriptions: Vec<EventSubCatalogSubscription>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AuthCatalogFlow {
    pub id: String,
    pub name: String,
    pub token_kind: String,
    #[serde(default)]
    pub authorize_endpoint: Option<String>,
    #[serde(default)]
    pub token_endpoint: Option<String>,
    #[serde(default)]
    pub device_endpoint: Option<String>,
    #[serde(default)]
    pub response_type: Option<String>,
    #[serde(default)]
    pub grant_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AuthCatalogEndpoint {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AuthCatalogRules {
    pub validate_interval_seconds: u64,
    pub validate_required_for_oauth_sessions: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AuthCatalog {
    pub generated_at: String,
    pub generated_from: String,
    pub source_urls: std::collections::BTreeMap<String, String>,
    pub flows: Vec<AuthCatalogFlow>,
    pub endpoints: Vec<AuthCatalogEndpoint>,
    pub rules: AuthCatalogRules,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CatalogCounts {
    pub auth_endpoints: usize,
    pub auth_flows: usize,
    pub eventsub_beta: usize,
    pub eventsub_ga_or_new: usize,
    pub eventsub_total: usize,
    pub helix_beta: usize,
    pub helix_ga_or_new: usize,
    pub helix_groups: usize,
    pub helix_total: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CatalogSummary {
    pub generated_at: String,
    pub sources: std::collections::BTreeMap<String, String>,
    pub counts: CatalogCounts,
}

macro_rules! json_once {
    ($name:ident, $ty:ty, $raw:expr) => {
        pub fn $name() -> &'static $ty {
            static VALUE: OnceLock<$ty> = OnceLock::new();
            VALUE.get_or_init(|| serde_json::from_str($raw).expect("catalog JSON should decode"))
        }
    };
}

json_once!(
    helix_catalog,
    HelixCatalog,
    include_str!("../twitch_catalog/helix.json")
);
json_once!(
    eventsub_catalog,
    EventSubCatalog,
    include_str!("../twitch_catalog/eventsub.json")
);
json_once!(
    auth_catalog,
    AuthCatalog,
    include_str!("../twitch_catalog/auth.json")
);
json_once!(
    catalog_summary,
    CatalogSummary,
    include_str!("../twitch_catalog/summary.json")
);

pub fn ga_or_new_helix_endpoint_count() -> usize {
    helix_catalog()
        .endpoints
        .iter()
        .filter(|endpoint| endpoint.stability != "beta")
        .count()
}

pub fn beta_helix_endpoint_count() -> usize {
    helix_catalog()
        .endpoints
        .iter()
        .filter(|endpoint| endpoint.stability == "beta")
        .count()
}

pub fn ga_or_new_eventsub_count() -> usize {
    eventsub_catalog()
        .subscriptions
        .iter()
        .filter(|subscription| subscription.stability != "beta")
        .count()
}

pub fn beta_eventsub_count() -> usize {
    eventsub_catalog()
        .subscriptions
        .iter()
        .filter(|subscription| subscription.stability == "beta")
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_matches_embedded_catalogs() {
        let summary = catalog_summary();
        assert_eq!(summary.counts.helix_total, helix_catalog().endpoints.len());
        assert_eq!(
            summary.counts.eventsub_total,
            eventsub_catalog().subscriptions.len()
        );
        assert_eq!(summary.counts.auth_flows, auth_catalog().flows.len());
        assert_eq!(
            summary.counts.auth_endpoints,
            auth_catalog().endpoints.len()
        );
    }
}
