use config::read_portfolio;

mod api;
mod config;
mod get;

mod calculate;
mod stream;

#[tokio::main]
async fn main() {
    let path = "config/portfolio.toml";
    let portfolio = read_portfolio(path).unwrap();
    stream::stream(5, portfolio).await;
}
