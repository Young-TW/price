use colored::*;
use crossterm::{cursor, execute, terminal};
use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::HashMap;
use std::io::stdout;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;

use crate::api::pyth::spawn_price_stream;
use crate::get::get_price;
type SharedPriceMap = Arc<tokio::sync::Mutex<HashMap<String, f64>>>;
type Portfolio = HashMap<String, HashMap<String, f64>>;

pub async fn stream(cycle: u64, portfolio: Portfolio) {
    let prices: SharedPriceMap = Arc::new(Mutex::new(HashMap::new()));

    let lazy_prices = prices.clone();
    let polling_prices = prices.clone();
    let portfolio_clone_for_lazy = portfolio.clone();
    let portfolio_clone_for_polling = portfolio.clone();

    // 啟動 lazy_stream
    tokio::spawn(async move {
        lazy_stream(lazy_prices, portfolio_clone_for_lazy).await;
    });

    // 啟動 polling_stream
    tokio::spawn(async move {
        polling_stream(polling_prices, cycle, portfolio_clone_for_polling).await;
    });

    // 主 loop
    let mut stdout = stdout();

    loop {
        execute!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .unwrap();

        let map = prices.lock().await;
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

pub async fn lazy_stream(prices: SharedPriceMap, portfolio: Portfolio) {
    // 定義需要處理的分類
    let categories = ["Crypto", "US-Stock", "US-ETF"];

    for category in categories {
        if let Some(items) = portfolio.get(category) {
            for (symbol, _amount) in items {
                let prices = prices.clone();
                let symbol_owned = symbol.clone();
                let category_owned = category.to_string();
                // 如果需要乘上持倉量 amount，可以在這裡處理
                spawn_price_stream(&symbol_owned, &category_owned, prices.clone());
            }
        } else {
            println!("[警告] portfolio.toml 中找不到 [{category}] 欄位");
        }
    }
}

pub async fn polling_stream(prices: SharedPriceMap, cycle: u64, portfolio: Portfolio) {
    loop {
        let mut tasks = FuturesUnordered::new();

        for category in ["TW-Stock", "TW-ETF"] {
            if let Some(items) = portfolio.get(category) {
                for (symbol, _amount) in items {
                    let symbol = symbol.clone();
                    let category = category.to_string();

                    tasks.push(async move {
                        match get_price(&symbol, &category).await {
                            Ok(price) => Some((symbol, _amount, price)),
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
            if let Some((symbol, _amount, price)) = result {
                let mut map = prices.lock().await;
                map.insert(symbol, price);
            }
        }

        tokio::time::sleep(Duration::from_secs(cycle)).await;
    }
}
