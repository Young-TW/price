use config::read_portfolio;

mod api;
mod config;
mod get;
mod history;
mod tui;
mod types;
mod stream;

#[tokio::main]
async fn main() {
    let portfolio_path = "config/portfolio.toml";
    let portfolio = read_portfolio(portfolio_path);

    let target_forex_path = "config/target_forex.toml";
    let target_forex: String = match config::read_target_forex(target_forex_path) {
        Ok(forex) => forex,
        Err(e) => {
            eprintln!(
                "[config] {} not usable ({}); defaulting target forex to USD",
                target_forex_path, e
            );
            "USD".to_string()
        }
    };
    stream::stream(5, portfolio, &target_forex).await;
}
