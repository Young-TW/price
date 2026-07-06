//! Binary entry point for the `price` portfolio tracker.
//!
//! Installs the rustls crypto provider, initialises file logging, loads the
//! portfolio and display currency from the config files, and hands off to
//! either the streaming TUI (default) or the one-shot printer (`--once`,
//! `--json`).

use config::read_portfolio;

mod api;
mod config;
mod get;
mod history;
mod logging;
mod oneshot;
mod paths;
mod stream;
mod tui;
mod types;

const USAGE: &str = "\
Usage: price [--once | --json]

  (no flags)  run the interactive TUI
  --once      fetch prices once, print the portfolio to stdout, and exit
  --json      like --once, but print a machine-readable JSON object

Exit code in one-shot mode: 0 when every holding was priced, 1 otherwise.";

#[tokio::main]
async fn main() {
    let mut once = false;
    let mut json = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--once" => once = true,
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{}", USAGE);
                return;
            }
            other => {
                eprintln!("unknown argument: {}\n{}", other, USAGE);
                std::process::exit(2);
            }
        }
    }

    // rustls 0.23 cannot auto-select a CryptoProvider when both aws-lc-rs and
    // ring are present in the dependency tree, so pick one explicitly up front.
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install rustls CryptoProvider");
    // Route diagnostics to a log file so they never disturb the TUI (or, in
    // one-shot mode, the parseable stdout).
    logging::init();
    let portfolio = read_portfolio(&paths::portfolio_file());
    let target_forex = config::read_target_forex_or_default(&paths::target_forex_file());
    if once || json {
        std::process::exit(oneshot::run_once(portfolio, target_forex, json).await);
    }
    stream::stream(5, portfolio, target_forex).await;
}
