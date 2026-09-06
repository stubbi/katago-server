//! REST API server for the KataGo analysis engine.
//!
//! The crate is split into a library (so integration tests can build the
//! router in-process against a fake KataGo) and a thin binary in `main.rs`.

pub mod api;
pub mod config;
pub mod coords;
pub mod engine;
pub mod error;
pub mod metrics;

/// Git commit the binary was built from, if the build provided it.
pub const GIT_SHA: Option<&str> = option_env!("GIT_SHA");

/// Crate version from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
