use config::read_portfolio;

mod api;
mod config;
mod get;
mod history;
mod logging;
mod tui;
mod types;
mod stream;

#[tokio::main]
async fn main() {
    // Route diagnostics to a log file before the TUI takes over the terminal.
    logging::init();
    let portfolio = read_portfolio(config::PORTFOLIO_PATH);
    let target_forex = config::read_target_forex_or_default(config::TARGET_FOREX_PATH);
    stream::stream(5, portfolio, target_forex).await;
}
