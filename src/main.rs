use colored::*;

mod config;
mod get;
mod api;

use get::get_price;
use config::read_portfolio;

#[tokio::main]
async fn main() {
    let portfolio = read_portfolio("config/portfolio.toml")
        .expect("無法讀取資產組合檔案");

    let mut total_value = 0.0;

    for category in ["crypto", "us-stock", "us-etf", "tw-stock", "tw-etf"] {
        if let Some(items) = portfolio.get(category) {
            for (symbol, amount) in items {
                match get_price(symbol, category).await {
                    Ok(price) => {
                        println!("{}: {} 股 x ${:.2}", symbol, amount, price);
                        total_value += amount * price;
                    }
                    Err(e) => {
                        eprintln!("{}: 查詢失敗 - {}", symbol, e.red());
                    }
                }
            }
        }
    }

    println!(
        "\n{}",
        format!("總資產 (USD)：${:.2}", total_value)
            .bold()
            .green()
    );
}
