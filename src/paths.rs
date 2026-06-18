//! Runtime file locations, resolved from environment variables with sensible
//! defaults so the program can be deployed outside its source tree.
//!
//! | Variable           | Default  | Controls                                  |
//! |--------------------|----------|-------------------------------------------|
//! | `PRICE_CONFIG_DIR` | `config` | portfolio / target-forex / api-key files  |
//! | `PRICE_DATA_DIR`   | `data`   | history, CSV export and (default) log     |
//! | `PRICE_LOG`        | —        | overrides the log file path outright      |
//!
//! The Pyth feed table is compiled into the binary (see `api::pyth`) and needs
//! no path.

use std::env;

const DEFAULT_CONFIG_DIR: &str = "config";
const DEFAULT_DATA_DIR: &str = "data";

/// Directory holding the user's configuration files.
pub fn config_dir() -> String {
    env::var("PRICE_CONFIG_DIR").unwrap_or_else(|_| DEFAULT_CONFIG_DIR.to_string())
}

/// Directory for program-generated data (history, exports, log).
pub fn data_dir() -> String {
    env::var("PRICE_DATA_DIR").unwrap_or_else(|_| DEFAULT_DATA_DIR.to_string())
}

/// Path to the portfolio file: `<config dir>/portfolio.toml`.
pub fn portfolio_file() -> String {
    format!("{}/portfolio.toml", config_dir())
}

/// Path to the display-currency file: `<config dir>/target_forex.toml`.
pub fn target_forex_file() -> String {
    format!("{}/target_forex.toml", config_dir())
}

// Only referenced by the AlphaVantage/ExchangeRate sources, which are currently
// disabled; kept so they resolve the path consistently once re-enabled.
/// Path to the API key file: `<config dir>/api_key.toml`.
#[allow(dead_code)]
pub fn api_key_file() -> String {
    format!("{}/api_key.toml", config_dir())
}

/// Path to the snapshot history file: `<data dir>/history.jsonl`.
pub fn history_file() -> String {
    format!("{}/history.jsonl", data_dir())
}

/// Path to the CSV export of the history: `<data dir>/history.csv`.
pub fn history_csv_file() -> String {
    format!("{}/history.csv", data_dir())
}

/// Log file path: `PRICE_LOG` if set, otherwise `<data dir>/price.log`.
pub fn log_file() -> String {
    env::var("PRICE_LOG").unwrap_or_else(|_| format!("{}/price.log", data_dir()))
}
