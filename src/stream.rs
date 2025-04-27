use colored::*;
use crossterm::{cursor, execute, terminal};
use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::HashMap;
use std::io::stdout;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::api::pyth::{get_price_stream_from_pyth, get_pyth_feed_id};
use crate::config::read_portfolio;
use crate::get::get_price;

type SharedPriceMap = Arc<Mutex<HashMap<String, f64>>>;

pub async fn stream(cycle: u64) {
    let prices: SharedPriceMap = Arc::new(Mutex::new(HashMap::new()));

    let lazy_prices = prices.clone();
    let polling_prices = prices.clone();

    tokio::spawn(async move {
        lazy_stream(lazy_prices).await;
    });

    tokio::spawn(async move {
        polling_stream(polling_prices, cycle).await;
    });

    let mut stdout = stdout();

    loop {
        execute!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .unwrap();

        let map = prices.lock().unwrap();
        let mut total_value = 0.0;

        for (symbol, value) in map.iter() {
            println!("{symbol}: ${:.2}", value);
            total_value += value;
        }

        println!(
            "\n{}",
            format!("總資產 (USD)：${:.2}", total_value).bold().green()
        );

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn lazy_stream(prices: SharedPriceMap) {
    let portfolio = read_portfolio("config/portfolio.toml").expect("無法讀取資產組合檔案");

    if let Some(items) = portfolio.get("crypto") {
        for (symbol, amount) in items {
            let prices = prices.clone();
            let symbol_owned = symbol.clone();
            let amount = *amount;
            let id = get_pyth_feed_id(&symbol_owned, "crypto").await;

            tokio::spawn(async move {
                let _ = get_price_stream_from_pyth(&id, {
                    let symbol_in_cb = symbol_owned.clone();
                    move |price| {
                        let mut map = prices.lock().unwrap();
                        let total = price * amount;
                        map.insert(symbol_in_cb.clone(), total);
                    }
                }).await;
            });
        }
    } else {
        println!("[警告] portfolio.toml 中找不到 [crypto] 欄位");
    }
}
pub async fn polling_stream(prices: SharedPriceMap, cycle: u64) {
    let portfolio = read_portfolio("config/portfolio.toml").expect("無法讀取資產組合檔案");

    loop {
        let mut tasks = FuturesUnordered::new();

        for category in ["us-stock", "us-etf", "tw-stock", "tw-etf"] {
            if let Some(items) = portfolio.get(category) {
                for (symbol, amount) in items {
                    let symbol = symbol.clone();
                    let category = category.to_string();
                    let amount = *amount;

                    tasks.push(async move {
                        match get_price(&symbol, &category).await {
                            Ok(price) => Some((symbol, amount, price)),
                            Err(_) => {
                                println!("無法獲取 {} 的價格", symbol);
                                None
                            }
                        }
                    });
                }
            }
        }

        while let Some(result) = tasks.next().await {
            if let Some((symbol, amount, price)) = result {
                let mut map = prices.lock().unwrap();
                map.insert(symbol, price * amount);
            }
        }

        tokio::time::sleep(Duration::from_secs(cycle)).await;
    }
}
