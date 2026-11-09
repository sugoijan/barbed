//! Shared core types for the `barbed` workspace.

pub mod emotes;

/// `User-Agent` sent by the workspace's HTTP clients; tracks the crate version.
pub const USER_AGENT: &str = concat!("barbed/", env!("CARGO_PKG_VERSION"));
