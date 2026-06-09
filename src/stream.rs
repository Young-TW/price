use futures::stream::{FuturesUnordered, StreamExt};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, cursor, terminal as crossterm_terminal, event::{self, Event, KeyCode}};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;
use chrono::prelude::*;
use chrono_tz::Asia::Taipei;

use crate::api::pyth::{get_price_stream_from_pyth, get_pyth_feed_id, spawn_price_stream};
use crate::api::twse::get_close_price_from_twse;
use crate::get::{get_history, get_price};
use crate::history;
use crate::tui::{self, ViewMode};
use crate::types::{Portfolio, PortfolioSnapshot};

type SharedPriceMap = Arc<tokio::sync::Mutex<HashMap<String, f64>>>;
type SharedHistory = Arc<tokio::sync::Mutex<Vec<PortfolioSnapshot>>>;

/// How often a live snapshot of the portfolio is recorded (seconds).
const SNAPSHOT_INTERVAL_SECS: u64 = 300;
/// How far back the historical back-fill reaches (seconds).
const BACKFILL_WINDOW_SECS: i64 = 365 * 86_400;

/// Check if Taiwan Stock Exchange (TWSE) market is currently open
/// TWSE trading hours: 09:00 - 13:30 (Monday to Friday)
fn is_twse_market_open() -> bool {
    let now = Local::now().with_timezone(&Taipei);
    let weekday = now.weekday();

    // Only open on weekdays (Monday to Friday)
    if !matches!(weekday, chrono::Weekday::Mon | chrono::Weekday::Tue | chrono::Weekday::Wed | chrono::Weekday::Thu | chrono::Weekday::Fri) {
        return false;
    }

    let hour = now.hour();
    let minute = now.minute();
    let current_time = hour * 60 + minute; // Convert to minutes since midnight
    let open_time = 9 * 60; // 09:00
    let close_time = 13 * 60 + 30; // 13:30

    current_time >= open_time && current_time < close_time
}

pub async fn stream(cycle: u64, portfolio: Portfolio, target_forex: &str) {
    let prices = Arc::new(Mutex::new(HashMap::new()));
    let history: SharedHistory = Arc::new(Mutex::new(history::load_history(history::HISTORY_PATH)));
    // Start background tasks
    start_background_tasks(&prices, &history, &portfolio, cycle, target_forex).await;
    // Setup terminal
    let mut terminal = setup_terminal();
    // Main display loop
    run_display_loop(&mut terminal, &prices, &history, &portfolio, target_forex).await;
    // Cleanup
    disable_raw_mode().unwrap();
}

async fn start_background_tasks(
    prices: &SharedPriceMap,
    history: &SharedHistory,
    portfolio: &Portfolio,
    cycle: u64,
    target_forex: &str,
) {
    seed_twse_cache(prices.clone(), portfolio).await;

    // Subscribe to every forex rate the portfolio actually depends on to be
    // valued in USD (forex cash holdings, USD/TWD for Taiwan equities) plus the
    // chosen display currency. USD is the base currency, so USD/USD is skipped.
    for forex_symbol in required_forex_pairs(portfolio, target_forex) {
        println!("Subscribing to forex rate: {}", forex_symbol);
        start_forex_stream(prices.clone(), &forex_symbol).await;
    }

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

    // Back-fill historical daily data once at startup.
    let backfill_history = history.clone();
    let backfill_portfolio = portfolio.clone();
    tokio::spawn(async move {
        backfill_history_task(backfill_history, backfill_portfolio).await;
    });

    // Record periodic live snapshots into the history.
    let snapshot_history = history.clone();
    let snapshot_prices = prices.clone();
    let snapshot_portfolio = portfolio.clone();
    tokio::spawn(async move {
        snapshot_recorder(snapshot_history, snapshot_prices, snapshot_portfolio).await;
    });
}

/// Reconstruct daily historical snapshots from API back-fill using the current
/// holdings, then merge with any existing on-disk history and persist.
async fn backfill_history_task(history: SharedHistory, portfolio: Portfolio) {
    let to = Utc::now().timestamp();
    let from = to - BACKFILL_WINDOW_SECS;

    // (price-map key, fetch symbol, category)
    let mut requests: Vec<(String, String, String)> = Vec::new();
    let mut has_tw = false;
    let mut has_twd_forex = false;

    for item in portfolio.iter() {
        match item.category.as_str() {
            "Forex" => {
                if item.symbol == "USD" {
                    continue; // USD is the base currency; no rate needed.
                }
                if item.symbol == "TWD" {
                    has_twd_forex = true;
                }
                requests.push((
                    format!("USD/{}", item.symbol),
                    item.symbol.clone(),
                    "Forex".to_string(),
                ));
            }
            "TW-Stock" | "TW-ETF" => {
                has_tw = true;
                requests.push((item.symbol.clone(), item.symbol.clone(), item.category.clone()));
            }
            _ => {
                requests.push((item.symbol.clone(), item.symbol.clone(), item.category.clone()));
            }
        }
    }

    // TW holdings need a USD/TWD rate even if TWD isn't held as cash.
    if has_tw && !has_twd_forex {
        requests.push(("USD/TWD".to_string(), "TWD".to_string(), "Forex".to_string()));
    }

    // Bucket every fetched close into a per-UTC-day price map.
    let mut day_maps: BTreeMap<i64, HashMap<String, f64>> = BTreeMap::new();
    for (key, symbol, category) in requests {
        match get_history(&symbol, &category, from, to).await {
            Ok(series) => {
                for (ts, price) in series {
                    let day = ts.div_euclid(86_400);
                    day_maps.entry(day).or_default().insert(key.clone(), price);
                }
            }
            Err(e) => eprintln!("[backfill] {} ({}) failed: {}", symbol, category, e),
        }
    }

    // Rebuild a snapshot for each day using current quantities.
    let mut backfilled = Vec::new();
    for (day, map) in day_maps {
        let (category_values, total) = history::compute_category_values(&portfolio, &map);
        if total <= 0.0 {
            continue;
        }
        backfilled.push(PortfolioSnapshot {
            timestamp: day * 86_400,
            total_value_usd: total,
            category_values,
            prices: map,
        });
    }

    // Merge with existing history (existing wins per day) and persist.
    let mut guard = history.lock().await;
    let existing = std::mem::take(&mut *guard);
    let merged = history::merge_snapshots(existing, backfilled);
    if let Err(e) = history::save_all(history::HISTORY_PATH, &merged) {
        eprintln!("[backfill] failed to save history: {}", e);
    }
    *guard = merged;
}

/// Append a live snapshot of the portfolio at a fixed interval.
async fn snapshot_recorder(history: SharedHistory, prices: SharedPriceMap, portfolio: Portfolio) {
    let mut interval = tokio::time::interval(Duration::from_secs(SNAPSHOT_INTERVAL_SECS));
    interval.tick().await; // Skip the immediate first tick.

    loop {
        interval.tick().await;

        let map = { prices.lock().await.clone() };
        let snapshot = history::take_snapshot(&portfolio, &map);
        if snapshot.total_value_usd <= 0.0 {
            continue; // Skip until prices are populated.
        }

        if let Err(e) = history::append_snapshot(history::HISTORY_PATH, &snapshot) {
            eprintln!("[snapshot] failed to persist: {}", e);
        }
        history.lock().await.push(snapshot);
    }
}

/// Determine every forex pair (as `USD/{ccy}`) whose live rate is required to
/// value the portfolio in USD and to render the display currency.
///
/// Dependencies:
/// - Each non-USD Forex cash holding needs its own `USD/{ccy}` rate.
/// - Taiwan equities (TW-Stock/TW-ETF) are priced in TWD, so they depend on
///   `USD/TWD` even when no TWD cash is held.
/// - The display currency needs `USD/{target}` for the converted total line.
///
/// USD is the base currency (`USD/USD` is trivially 1.0 and has no Pyth feed),
/// so it is never included.
fn required_forex_pairs(portfolio: &Portfolio, target_forex: &str) -> Vec<String> {
    let mut currencies: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for item in portfolio.iter() {
        match item.category.as_str() {
            "Forex" => {
                currencies.insert(item.symbol.to_uppercase());
            }
            "TW-Stock" | "TW-ETF" => {
                currencies.insert("TWD".to_string());
            }
            _ => {}
        }
    }

    currencies.insert(target_forex.to_uppercase());

    currencies
        .into_iter()
        .filter(|ccy| ccy != "USD")
        .map(|ccy| format!("USD/{}", ccy))
        .collect()
}

async fn start_forex_stream(prices: SharedPriceMap, forex_symbol: &str) {
    let id = match get_pyth_feed_id(forex_symbol, "Forex").await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("[forex] cannot subscribe to {}: {}", forex_symbol, e);
            return;
        }
    };
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

async fn seed_twse_cache(prices: SharedPriceMap, portfolio: &Portfolio) {
    for category in ["TW-Stock", "TW-ETF"] {
        if let Some(items) = portfolio.get(category) {
            for item in items {
                let symbol = item.symbol.clone();
                match get_close_price_from_twse(&symbol).await {
                    Ok(price) => {
                        let mut map = prices.lock().await;
                        map.insert(symbol, price);
                    }
                    Err(e) => {
                        println!("Failed to seed close price for {}: {}", symbol, e);
                    }
                }
            }
        }
    }
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
    history: &SharedHistory,
    portfolio: &Portfolio,
    target_forex: &str,
) {
    let mut view_mode = ViewMode::Live;

    loop {
        // Handle key presses: 'q' quits, 'h'/'l' switch history/live views.
        if event::poll(Duration::from_millis(10)).unwrap() {
            if let Event::Key(key_event) = event::read().unwrap() {
                match key_event.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('h') => view_mode = ViewMode::History,
                    KeyCode::Char('l') => view_mode = ViewMode::Live,
                    KeyCode::Char('e') => {
                        let snapshot = { history.lock().await.clone() };
                        if let Err(e) = history::export_csv(&snapshot, "data/history.csv") {
                            eprintln!("[export] {}", e);
                        }
                    }
                    _ => {}
                }
            }
        }

        let map = prices.lock().await;
        let (lines, total_value) = build_portfolio_display(&map, portfolio).await;
        let history_snapshot = { history.lock().await.clone() };

        // Render display
        tui::render_portfolio(
            terminal,
            &lines,
            total_value,
            &map,
            target_forex,
            portfolio,
            &history_snapshot,
            view_mode,
        );

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Clear terminal on exit
    terminal.clear().unwrap();
}

async fn build_portfolio_display(
    map: &HashMap<String, f64>,
    portfolio: &Portfolio,
) -> (Vec<String>, f64) {
    let mut lines = vec![];
    let mut total_value = 0.0;

    // Get grouped categories and sort for stable ordering
    let mut categories: Vec<_> = portfolio.group_by_category().into_iter().collect();
    categories.sort_by(|a, b| a.0.cmp(&b.0));

    // Handle non-forex assets
    for (category, mut items) in categories {
        if category == "Forex" { continue; }

        // Sort items by symbol for stable ordering
        items.sort_by(|a, b| a.symbol.cmp(&b.symbol));

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
    if let Some(mut forex_items) = portfolio.get("Forex") {
        // Sort forex items by symbol for stable ordering
        forex_items.sort_by(|a, b| a.symbol.cmp(&b.symbol));

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

        // Only fetch new prices if TWSE market is open
        // If market is closed, cached prices are used
        if !is_twse_market_open() {
            continue;
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PortfolioItem;

    fn portfolio(items: &[(&str, &str)]) -> Portfolio {
        Portfolio(
            items
                .iter()
                .map(|(category, symbol)| PortfolioItem {
                    symbol: symbol.to_string(),
                    category: category.to_string(),
                    quantity: 1.0,
                })
                .collect(),
        )
    }

    #[test]
    fn tw_equities_require_usd_twd_even_without_twd_cash() {
        let p = portfolio(&[("TW-Stock", "2330"), ("US-Stock", "AAPL")]);
        assert_eq!(required_forex_pairs(&p, "USD"), vec!["USD/TWD".to_string()]);
    }

    #[test]
    fn forex_holdings_and_display_currency_are_collected_without_usd() {
        let p = portfolio(&[("Forex", "USD"), ("Forex", "TWD"), ("US-Stock", "AAPL")]);
        // USD is the base currency and must never appear; TWD held as cash does.
        assert_eq!(required_forex_pairs(&p, "EUR"), vec![
            "USD/EUR".to_string(),
            "USD/TWD".to_string(),
        ]);
    }

    #[test]
    fn usd_display_currency_alone_needs_no_pairs() {
        let p = portfolio(&[("US-Stock", "AAPL"), ("Forex", "USD")]);
        assert!(required_forex_pairs(&p, "USD").is_empty());
    }
}
