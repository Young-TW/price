use colored::*;
use crossterm::{cursor, execute, terminal};
use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::HashMap;
use std::io::stdout;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;

use crate::api::pyth::{get_price_stream_from_pyth, get_pyth_feed_id, spawn_price_stream};
use crate::get::get_price;
type SharedPriceMap = Arc<tokio::sync::Mutex<HashMap<String, f64>>>;
type Portfolio = HashMap<String, HashMap<String, f64>>;

pub async fn stream(cycle: u64, portfolio: Portfolio, target_forex: &str) {
    let prices: SharedPriceMap = Arc::new(Mutex::new(HashMap::new()));

    let lazy_prices = prices.clone();
    let polling_prices = prices.clone();
    let portfolio_clone_for_lazy = portfolio.clone();
    let portfolio_clone_for_polling = portfolio.clone();

    // 啟動匯率價格流
    let forex_symbol = "USD/".to_owned() + &target_forex.to_string(); // 擁有所有權
    println!("訂閱匯率: {}", forex_symbol);
    let id = get_pyth_feed_id(&forex_symbol, "Forex").await;
    let prices_clone = prices.clone();
    let forex_symbol_for_error = forex_symbol.clone();
    let forex_symbol_for_spawn = forex_symbol.clone();

    tokio::spawn(async move {
        if let Err(e) = get_price_stream_from_pyth(&id, move |price| {
            let prices = prices_clone.clone();
            let forex_symbol = forex_symbol_for_spawn.clone();

            tokio::spawn(async move {
                let mut map = prices.lock().await;
                map.insert(forex_symbol.clone(), price);
            });
        }).await {
            eprintln!("無法訂閱匯率 {} 的價格: {}", forex_symbol_for_error, e);
        }
    });


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

        // 處理非外匯資產
        for (category, items) in &portfolio {
            if category == "Forex" { continue; }
            for (symbol, amount) in items {
                if let Some(price) = map.get(symbol) {
                    let asset_value = price * amount;
                    println!("{symbol}: ${:.2} x {:.4} = ${:.2}", price, amount, asset_value);
                    total_value += asset_value;
                }
            }
        }

        if let Some(forex_items) = portfolio.get("Forex") {
            for (currency, amount) in forex_items {
                println!("{currency}: ${:.2} x {:.4} = ${:.2}", 1.0, amount, amount);
                // 只有 USD 要直接加到 total_value
                if currency == "USD" {
                    total_value += amount;
                } else { // 其他貨幣先換算成 USD
                    if let Some(forex_price) = map.get(&("USD/".to_owned() + currency)) {
                        let converted_value = amount / forex_price;
                        println!(
                            "{}",
                            format!("(換算成 USD): ${:.2} x {:.4} = ${:.2}", forex_price, amount, converted_value)
                                .dimmed()
                        );
                        total_value += converted_value;
                    } else {
                        println!(
                            "{}",
                            format!("無法取得 {} 的匯率價格", currency).red()
                        );
                    }
                }
            }
        }

        println!(
            "{}",
            format!("總資產 (USD): ${:.2}", total_value).bold().green()
        );

        // 換算成目標幣別
        if let Some(forex_price) = map.get(&forex_symbol) {
            let converted_value = total_value * forex_price;
            println!(
                "{}",
                format!("總資產 ({}): ${:.2}", target_forex, converted_value).bold().green()
            );
        }

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
