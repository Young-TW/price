//! Library crate for the `price` portfolio tracker.
//!
//! It fetches asset prices and historical series from several providers
//! ([`api`]), reads the user's holdings and settings from TOML files
//! ([`config`], [`paths`], [`types`]), records periodic snapshots
//! ([`history`]), and renders a live terminal UI ([`tui`], [`stream`]).

pub mod api;
pub mod config;
pub mod get;
pub mod history;
pub mod logging;
pub mod paths;
pub mod stream;
pub mod tui;
pub mod types;
