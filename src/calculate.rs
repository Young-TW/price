use colored::*;
use futures::stream::{FuturesUnordered, StreamExt};

use crate::config::read_portfolio;
use crate::get::get_price;

pub async fn calculate_total() -> f64 {
    let portfolio = read_portfolio("config/portfolio.toml").expect("無法讀取資產組合檔案");

    let mut total_value = 0.0;
    let mut tasks = FuturesUnordered::new();

    for category in ["crypto", "us-stock", "us-etf", "tw-stock", "tw-etf"] {
        if let Some(items) = portfolio.get(category) {
            for (symbol, amount) in items {
                let symbol = symbol.clone();
                let category = category.to_string();
                let amount = *amount;

                tasks.push(async move {
                    match get_price(&symbol, &category).await {
                        Ok(price) => {
                            println!("{}: {} 股 x ${:.2}", symbol, amount, price);
                            Some(amount * price)
                        }
                        Err(e) => {
                            eprintln!("{}: 查詢失敗 - {}", symbol, e.red());
                            None
                        }
                    }
                });
            }
        }
    }

    while let Some(result) = tasks.next().await {
        if let Some(value) = result {
            total_value += value;
        }
    }

    return total_value;
}
