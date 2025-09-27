use futures::stream::{FuturesUnordered, StreamExt};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, cursor, terminal as crossterm_terminal, event::{self, Event, KeyCode}};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;

use crate::api::pyth::{get_price_stream_from_pyth, get_pyth_feed_id, spawn_price_stream};
use crate::get::get_price;
use crate::tui;
use crate::types::Portfolio;

type SharedPriceMap = Arc<tokio::sync::Mutex<HashMap<String, f64>>>;

pub async fn stream(cycle: u64, portfolio: Portfolio, target_forex: &str) {
    let prices = Arc::new(Mutex::new(HashMap::new()));
    // Start background tasks
    start_background_tasks(&prices, &portfolio, cycle, target_forex).await;
    // Setup terminal
    let mut terminal = setup_terminal();
    // Main display loop
    run_display_loop(&mut terminal, &prices, &portfolio, target_forex).await;
    // Cleanup
    disable_raw_mode().unwrap();
}

async fn start_background_tasks(
    prices: &SharedPriceMap,
    portfolio: &Portfolio,
    cycle: u64,
    target_forex: &str,
) {
    let forex_symbol = format!("USD/{}", target_forex);
    println!("Subscribing to forex rate: {}", forex_symbol);

    // Start forex stream
    start_forex_stream(prices.clone(), &forex_symbol).await;

    // Start lazy stream
    let lazy_prices = prices.clone();
    let lazy_portfolio = portfolio.clone();
    tokio::spawn(async move {
        lazy_stream(lazy_prices, lazy_portfolio).await;
    });

    // Start polling stream
    let polling_prices = prices.clone();
    let polling_portfolio = portfolio.clone();
    tokio::spawn(async move {
        polling_stream(polling_prices, cycle, polling_portfolio).await;
    });
}

async fn start_forex_stream(prices: SharedPriceMap, forex_symbol: &str) {
    let id = get_pyth_feed_id(forex_symbol, "Forex").await;
    let forex_symbol_clone = forex_symbol.to_string();
    let forex_symbol_for_error = forex_symbol.to_string();

    tokio::spawn(async move {
        if let Err(e) = get_price_stream_from_pyth(&id, move |price| {
            let prices = prices.clone();
            let symbol = forex_symbol_clone.clone();

            tokio::spawn(async move {
                let mut map = prices.lock().await;
                map.insert(symbol, price);
            });
        }).await {
            eprintln!("Failed to subscribe to forex rate {}: {}", forex_symbol_for_error, e);
        }
    });
}

fn setup_terminal() -> Terminal<CrosstermBackend<std::io::Stdout>> {
    enable_raw_mode().unwrap();
    let terminal = Terminal::new(CrosstermBackend::new(std::io::stdout())).unwrap();

    execute!(
        std::io::stdout(),
        crossterm_terminal::Clear(crossterm_terminal::ClearType::All),
        cursor::MoveTo(0, 0)
    ).unwrap();

    terminal
}

async fn run_display_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    prices: &SharedPriceMap,
    portfolio: &Portfolio,
    target_forex: &str,
) {
    loop {
        // Check for 'q' key press to exit
        if event::poll(Duration::from_millis(10)).unwrap() {
            if let Event::Key(key_event) = event::read().unwrap() {
                if key_event.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        let map = prices.lock().await;
        let (lines, total_value) = build_portfolio_display(&map, portfolio).await;

        // Render display
        tui::render_portfolio(terminal, &lines, total_value, &map, target_forex, portfolio);

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn build_portfolio_display(
    map: &HashMap<String, f64>,
    portfolio: &Portfolio,
) -> (Vec<String>, f64) {
    let mut lines = vec![];
    let mut total_value = 0.0;

    // Handle non-forex assets
    for (category, items) in portfolio.group_by_category().iter() {
        if category == "Forex" { continue; }

        for item in items {
            let symbol = &item.symbol;
            let amount = item.quantity;
            if let Some(price) = map.get(symbol) {
                let asset_value = price * amount;

                if category == "TW-Stock" || category == "TW-ETF" {
                    lines.push(format!("{}: NT${:.2} x {:.4} = NT${:.2}", symbol, price, amount, asset_value));

                    if let Some(rate) = map.get("USD/TWD") {
                        let usd_value = asset_value / rate;
                        lines.push(format!("  (Converted to USD): ${:.2} / {:.4} = ${:.2}", asset_value, rate, usd_value));
                        total_value += usd_value;
                    } else {
                        lines.push("  [Warning] USD/TWD rate not available".to_string());
                    }
                } else if category == "Crypto" || category == "US-Stock" || category == "US-ETF" {
                    lines.push(format!("{}: ${:.2} x {:.4} = ${:.2}", symbol, price, amount, asset_value));
                    total_value += asset_value;
                } else {
                    lines.push(format!("{}: ${:.2} x {:.4} = ${:.2}", symbol, price, amount, asset_value));
                    total_value += asset_value;
                }
            }
        }
    }

    // Handle forex assets
    if let Some(forex_items) = portfolio.get("Forex") {
        for item in forex_items {
            let symbol = &item.symbol;
            let quantity = item.quantity;
            lines.push(format!("{}: ${:.2} x {:.4} = ${:.2}", symbol, 1.0, quantity, quantity));

            if symbol == "USD" {
                total_value += quantity;
            } else {
                let forex_key = format!("USD/{}", symbol);
                if let Some(forex_price) = map.get(&forex_key) {
                    let converted_value = quantity / forex_price;
                    lines.push(format!("  (Converted to USD): ${:.2} / {:.4} = ${:.2}", quantity, forex_price, converted_value));
                    total_value += converted_value;
                } else {
                    lines.push(format!("  Cannot get forex rate for {}", symbol));
                }
            }
        }
    }

    (lines, total_value)
}

pub async fn lazy_stream(prices: SharedPriceMap, portfolio: Portfolio) {
    // Define categories to handle
    let categories = ["Crypto", "US-Stock", "US-ETF"];

    for category in categories {
        if let Some(items) = portfolio.get(category) {
            for item in items {
                let prices = prices.clone();
                let symbol_owned = item.symbol.clone();
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
                for item in items {
                    let symbol = item.symbol.clone();
                    let amount = item.quantity;
                    let category = category.to_string();

                    tasks.push(async move {
                        let mut attempts = 0;
                        let max_attempts = 3;
                        let mut delay = Duration::from_secs(1);

                        loop {
                            match get_price(&symbol, &category).await {
                                Ok(price) => break Some((symbol.clone(), amount, price)),
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
