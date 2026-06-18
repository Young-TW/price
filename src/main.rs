//! Binary entry point for the `price` portfolio tracker.
//!
//! Installs the rustls crypto provider, initialises file logging, loads the
//! portfolio and display currency from the config files, and hands off to the
//! streaming TUI.

use config::read_portfolio;

mod api;
mod config;
mod get;
mod history;
mod logging;
mod paths;
mod stream;
mod tui;
mod types;

#[tokio::main]
async fn main() {
    // rustls 0.23 cannot auto-select a CryptoProvider when both aws-lc-rs and
    // ring are present in the dependency tree, so pick one explicitly up front.
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install rustls CryptoProvider");
    // Route diagnostics to a log file before the TUI takes over the terminal.
    logging::init();
    let portfolio = read_portfolio(&paths::portfolio_file());
    let target_forex = config::read_target_forex_or_default(&paths::target_forex_file());
    stream::stream(5, portfolio, target_forex).await;
}
