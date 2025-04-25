use colored::*;
use crossterm::{cursor, execute, terminal};
use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::HashMap;
use std::io::stdout;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::config::read_portfolio;
use crate::get::get_price;
use crate::api::pyth::{get_price_stream_from_pyth, get_pyth_feed_id};

pub async fn lazy_stream() {
    type SharedPriceMap = Arc<Mutex<HashMap<String, f64>>>;
    let prices: SharedPriceMap = Arc::new(Mutex::new(HashMap::new()));

    let portfolio = read_portfolio("config/portfolio.toml").expect("無法讀取資產組合檔案");

    if let Some(items) = portfolio.get("crypto") {
        for (symbol, amount) in items {
            let prices = prices.clone();
            let symbol = symbol.clone();
            let amount = *amount;
            let id = get_pyth_feed_id(&symbol, "crypto")
                .await
                .expect("查詢 feed id 失敗")
                .expect("找不到資產");

            tokio::spawn(async move {
                let _ = get_price_stream_from_pyth(&id, move |price| {
                    let mut map = prices.lock().unwrap();
                    map.insert(symbol.clone(), price * amount);
                })
                .await;
            });
        }
    }

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
            println!("{}: ${:.2}", symbol, value);
            total_value += value;
        }

        println!(
            "\n{}",
            format!("總資產 (USD)：${:.2}", total_value).bold().green()
        );
    }
}

pub async fn stream(cycle: u64) {
    let mut stdout = stdout();

    // 初始化終端機
    execute!(stdout, terminal::Clear(terminal::ClearType::All)).unwrap();

    let portfolio = read_portfolio("config/portfolio.toml").expect("無法讀取資產組合檔案");

    loop {
        let mut total_value = 0.0;
        let mut tasks = FuturesUnordered::new();

        for category in ["us-stock", "us-etf", "tw-stock", "tw-etf"] {
            if let Some(items) = portfolio.get(category) {
                for (symbol, amount) in items {
                    let symbol = symbol.clone();
                    let category = category.to_string();
                    let amount = *amount;

                    tasks.push(async move {
                        match get_price(&symbol, &category).await {
                            Ok(price) => Some((symbol, amount, price, category)),
                            Err(_) => {
                                print!("無法獲取 {} 的價格", symbol);
                                None
                            }
                        }
                    });
                }
            }
        }

        // 清除畫面並將游標移到左上角
        execute!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .unwrap();

        while let Some(result) = tasks.next().await {
            if let Some((symbol, amount, price, _category)) = result {
                println!("{}: {} 股 x ${:.2}", symbol, amount, price);
                total_value += amount * price;
            } else {
                println!("{}", "查詢失敗".red());
            }
        }

        println!(
            "\n{}",
            format!("總資產 (USD)：${:.2}", total_value).bold().green()
        );

        tokio::time::sleep(Duration::from_secs(cycle)).await;
    }
}
