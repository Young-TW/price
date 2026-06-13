use config::read_portfolio;

mod api;
mod config;
mod get;
mod history;
mod logging;
mod paths;
mod tui;
mod types;
mod stream;

#[tokio::main]
async fn main() {
    // Route diagnostics to a log file before the TUI takes over the terminal.
    logging::init();
    let portfolio = read_portfolio(&paths::portfolio_file());
    let target_forex = config::read_target_forex_or_default(&paths::target_forex_file());
    stream::stream(5, portfolio, target_forex).await;
}
