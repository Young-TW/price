use crossterm::{cursor, execute, terminal};
use futures::stream::{FuturesUnordered, StreamExt};
use ratatui::{
    prelude::*,
    widgets::{Block, Paragraph, Borders},
};
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

    // Start forex price stream
    let forex_symbol = "USD/".to_owned() + &target_forex.to_string();
    println!("Subscribing to forex rate: {}", forex_symbol);
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
            eprintln!("Failed to subscribe to forex rate {}: {}", forex_symbol_for_error, e);
        }
    });

    // Start lazy_stream
    tokio::spawn(async move {
        lazy_stream(lazy_prices, portfolio_clone_for_lazy).await;
    });

    // Start polling_stream
    tokio::spawn(async move {
        polling_stream(polling_prices, cycle, portfolio_clone_for_polling).await;
    });

    // Main loop
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout())).unwrap();

    // Clear terminal once before entering the loop
    execute!(
        stdout(),
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0)
    ).unwrap();

    loop {
        let map = prices.lock().await;
        let mut total_value = 0.0;
        let mut lines = vec![];

        // Handle non-forex assets
        for (category, items) in &portfolio {
            if category == "Forex" { continue; }
            if category == "TW-Stock" || category == "TW-ETF" {
                for (symbol, amount) in items {
                    if let Some(price) = map.get(symbol) {
                        let asset_value = price * amount;
                        lines.push(format!("{symbol}: NT${:.2} x {:.4} = NT${:.2}", price, amount, asset_value));
                        if let Some(rate) = map.get("USD/TWD") {
                            lines.push(format!("(Converted to USD): ${:.2} / {:.4} = ${:.2}", asset_value, rate, asset_value / rate));
                            total_value += asset_value / rate;
                        } else {
                            lines.push("[Warning] USD/TWD rate not available, cannot convert TWD assets to USD.".to_string());
                        }
                    }
                }
            } else {
                for (symbol, amount) in items {
                    if let Some(price) = map.get(symbol) {
                        let asset_value = price * amount;
                        lines.push(format!("{symbol}: ${:.2} x {:.4} = ${:.2}", price, amount, asset_value));
                        total_value += asset_value;
                    }
                }
            }
        }

        if let Some(forex_items) = portfolio.get("Forex") {
            for (currency, amount) in forex_items {
                lines.push(format!("{currency}: ${:.2} x {:.4} = ${:.2}", 1.0, amount, amount));
                if currency == "USD" {
                    total_value += amount;
                } else {
                    if let Some(forex_price) = map.get(&("USD/".to_owned() + currency)) {
                        let converted_value = amount / forex_price;
                        lines.push(format!("(Converted to USD): ${:.2} / {:.4} = ${:.2}", amount, forex_price, converted_value));
                        total_value += converted_value;
                    } else {
                        lines.push(format!("Cannot get forex rate for {}", currency));
                    }
                }
            }
        }

        lines.push(format!("Total assets (USD): ${:.2}", total_value));
        if let Some(forex_price) = map.get(&("USD/".to_owned() + target_forex)) {
            let converted_value = total_value * forex_price;
            lines.push(format!("Total assets ({}): ${:.2}", target_forex, converted_value));
        }

        // 使用 ratatui 輸出
        terminal.draw(|f| {
            let area = f.area();
            let block = Block::default().title("Portfolio").borders(Borders::ALL);
            let paragraph = Paragraph::new(lines.join("\n")).block(block);
            f.render_widget(paragraph, area);
        }).unwrap();

        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

pub async fn lazy_stream(prices: SharedPriceMap, portfolio: Portfolio) {
    // Define categories to handle
    let categories = ["Crypto", "US-Stock", "US-ETF"];

    for category in categories {
        if let Some(items) = portfolio.get(category) {
            for (symbol, _amount) in items {
                let prices = prices.clone();
                let symbol_owned = symbol.clone();
                let category_owned = category.to_string();
                // If you need to multiply by amount, handle it here
                spawn_price_stream(&symbol_owned, &category_owned, prices.clone());
            }
        } else {
            println!("[Warning] [{category}] section not found in portfolio.toml");
        }
    }
}

pub async fn polling_stream(prices: SharedPriceMap, cycle: u64, portfolio: Portfolio) {
    let mut interval = tokio::time::interval(Duration::from_secs(cycle));
    // Skip the first immediate tick
    interval.tick().await;

    loop {
        interval.tick().await;
        let mut tasks = FuturesUnordered::new();

        for category in ["TW-Stock", "TW-ETF"] {
            if let Some(items) = portfolio.get(category) {
                for (symbol, _amount) in items {
                    let symbol = symbol.clone();
                    let category = category.to_string();

                    tasks.push(async move {
                        let mut attempts = 0;
                        let max_attempts = 3;
                        let mut delay = Duration::from_secs(1);

                        loop {
                            match get_price(&symbol, &category).await {
                                Ok(price) => break Some((symbol.clone(), _amount, price)),
                                Err(e) => {
                                    if attempts < max_attempts {
                                        attempts += 1;
                                        tokio::time::sleep(delay).await;
                                        delay *= 2;
                                    } else {
                                        println!("Failed to get price for {}: {}", symbol, e);
                                        break None;
                                    }
                                }
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
    }
}
