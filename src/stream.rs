//! The running application: spawns the live price streams and background tasks,
//! drives the TUI display loop, and polls Taiwan-market prices and config
//! hot-reloads.

use chrono::prelude::*;
use chrono_tz::Asia::Taipei;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
};
use futures::stream::{FuturesUnordered, StreamExt};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};

use crate::api::pyth::{get_pyth_feed_id, spawn_price_stream, stream_into_map};
use crate::api::twse::get_close_price_from_twse;
use crate::config;
use crate::get::{get_history, get_price};
use crate::history;
use crate::paths;
use crate::tui::{self, ViewMode};
use crate::types::{Portfolio, PortfolioSnapshot};

type SharedPriceMap = Arc<tokio::sync::Mutex<HashMap<String, f64>>>;
type SharedHistory = Arc<tokio::sync::Mutex<Vec<PortfolioSnapshot>>>;
/// Portfolio and display currency are wrapped in `RwLock` so the config
/// hot-reload watcher can swap in fresh values while reader tasks keep running.
type SharedPortfolio = Arc<RwLock<Portfolio>>;
type SharedTargetForex = Arc<RwLock<String>>;
/// Keys (forex pairs and `category:symbol`) for which a price stream/seed has
/// already been started, so reloads only subscribe to genuinely new holdings.
type SubscribedSet = Arc<Mutex<HashSet<String>>>;

/// How often a live snapshot of the portfolio is recorded (seconds).
const SNAPSHOT_INTERVAL_SECS: u64 = 300;
/// How far back the historical back-fill reaches (seconds).
const BACKFILL_WINDOW_SECS: i64 = 365 * 86_400;
/// How often the config files are checked for changes (seconds).
const CONFIG_POLL_SECS: u64 = 2;

/// Returns `true` if `dt` (interpreted in its own timezone) falls within TWSE
/// trading hours: Monday–Friday, 09:00–13:29 inclusive.
///
/// Callers are responsible for supplying a `DateTime` already converted to
/// Asia/Taipei. In production `is_twse_market_open` does this; in tests a
/// fixed Taipei `DateTime` is injected directly, making tests deterministic
/// without mocking system time.
fn is_twse_market_open_at<Tz: chrono::TimeZone>(dt: chrono::DateTime<Tz>) -> bool {
    if !matches!(
        dt.weekday(),
        chrono::Weekday::Mon
            | chrono::Weekday::Tue
            | chrono::Weekday::Wed
            | chrono::Weekday::Thu
            | chrono::Weekday::Fri
    ) {
        return false;
    }

    let current_time = dt.hour() * 60 + dt.minute();
    let open_time = 9 * 60; // 09:00
    let close_time = 13 * 60 + 30; // 13:30

    current_time >= open_time && current_time < close_time
}

/// Check if Taiwan Stock Exchange (TWSE) market is currently open.
/// TWSE trading hours: 09:00–13:30 (Monday–Friday) in Asia/Taipei.
fn is_twse_market_open() -> bool {
    is_twse_market_open_at(Local::now().with_timezone(&Taipei))
}

/// Run the application: start the background price/snapshot/reload tasks, set up
/// the terminal, run the display loop until the user quits, then restore the
/// terminal.
///
/// `cycle` is the Taiwan-market polling interval in seconds, `portfolio` the
/// initial holdings and `target_forex` the initial display currency. Returns
/// when the user exits the display loop.
pub async fn stream(cycle: u64, portfolio: Portfolio, target_forex: String) {
    let prices: SharedPriceMap = Arc::new(Mutex::new(HashMap::new()));
    let history: SharedHistory =
        Arc::new(Mutex::new(history::load_history(&paths::history_file())));
    let portfolio: SharedPortfolio = Arc::new(RwLock::new(portfolio));
    let target_forex: SharedTargetForex = Arc::new(RwLock::new(target_forex));
    let subscribed: SubscribedSet = Arc::new(Mutex::new(HashSet::new()));
    // Start background tasks
    start_background_tasks(
        &prices,
        &history,
        &portfolio,
        &target_forex,
        &subscribed,
        cycle,
    )
    .await;
    // Setup terminal
    let mut terminal = setup_terminal();
    // Main display loop
    run_display_loop(&mut terminal, &prices, &history, &portfolio, &target_forex).await;
    // Cleanup
    restore_terminal(&mut terminal);
}

async fn start_background_tasks(
    prices: &SharedPriceMap,
    history: &SharedHistory,
    portfolio: &SharedPortfolio,
    target_forex: &SharedTargetForex,
    subscribed: &SubscribedSet,
    cycle: u64,
) {
    // Subscribe to every price/forex stream the initial portfolio needs. Take a
    // snapshot of the shared config first so we don't hold the lock across the
    // network calls inside `ensure_subscriptions`.
    let initial_portfolio = portfolio.read().await.clone();
    let initial_target = target_forex.read().await.clone();
    ensure_subscriptions(&initial_portfolio, &initial_target, prices, subscribed).await;

    // Start polling stream
    let polling_prices = prices.clone();
    let polling_portfolio = portfolio.clone();
    tokio::spawn(async move {
        polling_stream(polling_prices, cycle, polling_portfolio).await;
    });

    // Back-fill historical daily data once at startup, using the holdings known
    // at launch. Symbols added later via hot-reload are not back-filled (they
    // accumulate live snapshots instead).
    let backfill_history = history.clone();
    let backfill_portfolio = initial_portfolio;
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

    // Watch the config files and hot-reload portfolio / target currency.
    let watch_portfolio = portfolio.clone();
    let watch_target = target_forex.clone();
    let watch_prices = prices.clone();
    let watch_subscribed = subscribed.clone();
    tokio::spawn(async move {
        watch_config(
            watch_portfolio,
            watch_target,
            watch_prices,
            watch_subscribed,
        )
        .await;
    });
}

/// Start any price/forex streams required by `portfolio` (valued in
/// `target_forex`) that have not been started yet. Idempotent: a key is only
/// acted on the first time it is seen, so this can be called on every reload to
/// pick up newly added holdings without duplicating existing subscriptions.
async fn ensure_subscriptions(
    portfolio: &Portfolio,
    target_forex: &str,
    prices: &SharedPriceMap,
    subscribed: &SubscribedSet,
) {
    // Forex rates needed to value the portfolio in USD plus the display
    // currency. USD is the base currency, so USD/USD is skipped.
    for forex_symbol in required_forex_pairs(portfolio, target_forex) {
        if mark_new(subscribed, &forex_symbol).await {
            crate::log_line!("Subscribing to forex rate: {}", forex_symbol);
            start_forex_stream(prices.clone(), &forex_symbol).await;
        }
    }

    // Live crypto / US equity streams.
    for category in ["Crypto", "US-Stock", "US-ETF"] {
        if let Some(items) = portfolio.get(category) {
            for item in items {
                let key = format!("{}:{}", category, item.symbol);
                if mark_new(subscribed, &key).await {
                    spawn_price_stream(&item.symbol, category, prices.clone());
                }
            }
        }
    }

    // Seed cached close prices for Taiwan holdings (refreshed by polling_stream).
    for category in ["TW-Stock", "TW-ETF"] {
        if let Some(items) = portfolio.get(category) {
            for item in items {
                let key = format!("{}:{}", category, item.symbol);
                if mark_new(subscribed, &key).await {
                    match get_close_price_from_twse(&item.symbol).await {
                        Ok(price) => {
                            prices.lock().await.insert(item.symbol.clone(), price);
                        }
                        Err(e) => {
                            crate::log_line!(
                                "Failed to seed close price for {}: {}",
                                item.symbol,
                                e
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Record `key` as subscribed, returning `true` only if it was not already
/// present (i.e. this is the first time we've seen it).
async fn mark_new(subscribed: &SubscribedSet, key: &str) -> bool {
    subscribed.lock().await.insert(key.to_string())
}

/// Most-recent modification time of `path`, or `None` if it can't be read.
fn file_mtime(path: &str) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Poll the config files and, when one changes on disk, swap the new values
/// into the shared state and subscribe to any newly required streams. Reader
/// tasks (display loop, polling, snapshots) observe the change automatically.
async fn watch_config(
    portfolio: SharedPortfolio,
    target_forex: SharedTargetForex,
    prices: SharedPriceMap,
    subscribed: SubscribedSet,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(CONFIG_POLL_SECS));
    interval.tick().await; // Skip the immediate first tick.

    // Resolve the paths once; the env vars they derive from don't change at runtime.
    let portfolio_path = paths::portfolio_file();
    let target_path = paths::target_forex_file();

    let mut portfolio_mtime = file_mtime(&portfolio_path);
    let mut target_mtime = file_mtime(&target_path);

    loop {
        interval.tick().await;
        let mut changed = false;

        let new_portfolio_mtime = file_mtime(&portfolio_path);
        if new_portfolio_mtime != portfolio_mtime {
            portfolio_mtime = new_portfolio_mtime;
            match config::try_read_portfolio(&portfolio_path) {
                Ok(new_portfolio) => {
                    crate::log_line!("[config] portfolio.toml reloaded");
                    *portfolio.write().await = new_portfolio;
                    changed = true;
                }
                Err(e) => crate::log_line!("[config] failed to reload portfolio.toml: {}", e),
            }
        }

        let new_target_mtime = file_mtime(&target_path);
        if new_target_mtime != target_mtime {
            target_mtime = new_target_mtime;
            let new_target = config::read_target_forex_or_default(&target_path);
            crate::log_line!("[config] target_forex.toml reloaded -> {}", new_target);
            *target_forex.write().await = new_target;
            changed = true;
        }

        if changed {
            // Snapshot the latest config (releasing the locks) before the
            // network calls in `ensure_subscriptions`.
            let current_portfolio = portfolio.read().await.clone();
            let current_target = target_forex.read().await.clone();
            ensure_subscriptions(&current_portfolio, &current_target, &prices, &subscribed).await;
        }
    }
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
                requests.push((
                    item.symbol.clone(),
                    item.symbol.clone(),
                    item.category.clone(),
                ));
            }
            _ => {
                requests.push((
                    item.symbol.clone(),
                    item.symbol.clone(),
                    item.category.clone(),
                ));
            }
        }
    }

    // TW holdings need a USD/TWD rate even if TWD isn't held as cash.
    if has_tw && !has_twd_forex {
        requests.push((
            "USD/TWD".to_string(),
            "TWD".to_string(),
            "Forex".to_string(),
        ));
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
            Err(e) => crate::log_line!("[backfill] {} ({}) failed: {}", symbol, category, e),
        }
    }

    // Rebuild a snapshot for each day using current quantities.
    let mut backfilled = Vec::new();
    for (day, map) in day_maps {
        // Days that lack a close for some holdings (e.g. weekends, when TWSE is
        // shut but crypto still trades) would value the missing assets at zero,
        // so skip them rather than recording an artificially low total.
        if !history::is_complete(&portfolio, &map) {
            continue;
        }
        let (category_values, total) = history::compute_category_values(&portfolio, &map);
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
    if let Err(e) = history::save_all(&paths::history_file(), &merged) {
        crate::log_line!("[backfill] failed to save history: {}", e);
    }
    *guard = merged;
}

/// Append a live snapshot of the portfolio at a fixed interval.
async fn snapshot_recorder(
    history: SharedHistory,
    prices: SharedPriceMap,
    portfolio: SharedPortfolio,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(SNAPSHOT_INTERVAL_SECS));
    interval.tick().await; // Skip the immediate first tick.

    loop {
        interval.tick().await;

        let map = { prices.lock().await.clone() };
        let portfolio = portfolio.read().await.clone();
        // Only record once every holding has a price; a partial map would
        // understate the total and produce spurious dips in the history.
        if !history::is_complete(&portfolio, &map) {
            continue;
        }
        let snapshot = history::take_snapshot(&portfolio, &map);

        // Add the snapshot, then downsample so both the in-memory Vec and the
        // on-disk file stay bounded (recent high-res + one-per-day for older
        // data) instead of growing forever. The file is rewritten rather than
        // appended; after downsampling it is small, so this is cheap.
        let mut guard = history.lock().await;
        guard.push(snapshot);
        let bounded = history::downsample(std::mem::take(&mut *guard), Utc::now().timestamp());
        if let Err(e) = history::save_all(&paths::history_file(), &bounded) {
            crate::log_line!("[snapshot] failed to persist: {}", e);
        }
        *guard = bounded;
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
    let id = match get_pyth_feed_id(forex_symbol, "Forex") {
        Ok(id) => id,
        Err(e) => {
            crate::log_line!("[forex] cannot subscribe to {}: {}", forex_symbol, e);
            return;
        }
    };
    let forex_symbol = forex_symbol.to_string();

    tokio::spawn(async move {
        // Reconnects automatically; forex rates must not silently go stale.
        stream_into_map(id, forex_symbol, prices).await;
    });
}

/// Switch into a dedicated full-screen buffer so the TUI never draws over (or
/// leaves residue in) the user's normal terminal scrollback.
fn setup_terminal() -> Terminal<CrosstermBackend<std::io::Stdout>> {
    install_panic_hook();
    enable_raw_mode().unwrap();
    execute!(std::io::stdout(), EnterAlternateScreen, cursor::Hide).unwrap();
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout())).unwrap();
    terminal.clear().unwrap();
    terminal
}

/// Undo [`setup_terminal`]: leave the alternate screen, disable raw mode and
/// restore the cursor. Best-effort — failures here must not mask the real exit.
fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) {
    let _ = disable_raw_mode();
    let _ = execute!(std::io::stdout(), LeaveAlternateScreen, cursor::Show);
    let _ = terminal.show_cursor();
}

/// Restore the terminal on panic before the default hook prints, so a crash
/// can't leave the user stuck in raw mode / the alternate screen.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen, cursor::Show);
        default_hook(info);
    }));
}

async fn run_display_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    prices: &SharedPriceMap,
    history: &SharedHistory,
    portfolio: &SharedPortfolio,
    target_forex: &SharedTargetForex,
) {
    let mut view_mode = ViewMode::Live;

    loop {
        // Handle key presses: 'q' quits, Tab toggles between the main (live)
        // page and the history page; 'h'/'l' remain as explicit shortcuts.
        //
        // `event::poll`/`event::read` can fail with an I/O error (stdin closed,
        // terminal disconnected, or a non-interactive environment). Treat that as
        // "no input this tick" and continue rather than unwrapping and crashing.
        match event::poll(Duration::from_millis(10)) {
            Ok(true) => match event::read() {
                Ok(Event::Key(key_event)) => match key_event.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Tab => view_mode = view_mode.toggle(),
                    KeyCode::Char('h') => view_mode = ViewMode::History,
                    KeyCode::Char('l') => view_mode = ViewMode::Live,
                    KeyCode::Char('e') => {
                        let snapshot = { history.lock().await.clone() };
                        if let Err(e) = history::export_csv(&snapshot, &paths::history_csv_file()) {
                            crate::log_line!("[export] {}", e);
                        }
                    }
                    _ => {}
                },
                Ok(_) => {}
                Err(e) => crate::log_line!("[input] read failed: {}", e),
            },
            Ok(false) => {}
            Err(e) => crate::log_line!("[input] poll failed: {}", e),
        }

        let portfolio = portfolio.read().await.clone();
        let target_forex = target_forex.read().await.clone();
        let map = prices.lock().await;
        let (lines, total_value) = build_portfolio_display(&map, &portfolio);

        // Borrow the history under its lock for the synchronous draw instead of
        // cloning the whole (unbounded) Vec every frame. The draw holds no
        // .await, and the only other writers touch it every 5 minutes, so
        // contention is negligible.
        let history_guard = history.lock().await;

        // Render display
        tui::render_portfolio(
            terminal,
            &lines,
            total_value,
            &map,
            &target_forex,
            &portfolio,
            &history_guard,
            view_mode,
        );

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn build_portfolio_display(
    map: &HashMap<String, f64>,
    portfolio: &Portfolio,
) -> (Vec<String>, f64) {
    // Delegate USD valuation to the single source of truth so the total here
    // always matches the allocation bar chart (which also calls this function).
    let (_, total_value) = history::compute_category_values(portfolio, map);

    let mut lines = vec![];

    // Get grouped categories and sort for stable ordering
    let mut categories: Vec<_> = portfolio.group_by_category().into_iter().collect();
    categories.sort_by(|a, b| a.0.cmp(&b.0));

    // Handle non-forex assets
    for (category, mut items) in categories {
        if category == "Forex" {
            continue;
        }

        // Sort items by symbol for stable ordering
        items.sort_by(|a, b| a.symbol.cmp(&b.symbol));

        for item in items {
            let symbol = &item.symbol;
            let amount = item.quantity;
            if let Some(price) = map.get(symbol) {
                let asset_value = price * amount;

                if category == "TW-Stock" || category == "TW-ETF" {
                    lines.push(format!(
                        "{}: NT${:.2} x {:.4} = NT${:.2}",
                        symbol, price, amount, asset_value
                    ));

                    match map.get("USD/TWD") {
                        Some(rate) if *rate != 0.0 => {
                            let usd_value = asset_value / rate;
                            lines.push(format!(
                                "  (Converted to USD): ${:.2} / {:.4} = ${:.2}",
                                asset_value, rate, usd_value
                            ));
                        }
                        _ => lines.push("  [Warning] USD/TWD rate not available".to_string()),
                    }
                } else {
                    lines.push(format!(
                        "{}: ${:.2} x {:.4} = ${:.2}",
                        symbol, price, amount, asset_value
                    ));
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
            lines.push(format!(
                "{}: ${:.2} x {:.4} = ${:.2}",
                symbol, 1.0, quantity, quantity
            ));

            if symbol != "USD" {
                let forex_key = format!("USD/{}", symbol);
                match map.get(&forex_key) {
                    Some(forex_price) if *forex_price != 0.0 => {
                        let converted_value = quantity / forex_price;
                        lines.push(format!(
                            "  (Converted to USD): ${:.2} / {:.4} = ${:.2}",
                            quantity, forex_price, converted_value
                        ));
                    }
                    _ => lines.push(format!("  Cannot get forex rate for {}", symbol)),
                }
            }
        }
    }

    (lines, total_value)
}

/// Poll Taiwan-market (`TW-Stock`, `TW-ETF`) prices every `cycle` seconds and
/// write them into `prices`.
///
/// Loops forever. The first immediate tick is skipped, and cycles where the TWSE
/// market is closed are skipped (cached prices remain). Holdings are re-read from
/// `portfolio` each cycle so hot-reloaded changes are picked up.
pub async fn polling_stream(prices: SharedPriceMap, cycle: u64, portfolio: SharedPortfolio) {
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

        // Read the latest holdings each cycle so hot-reloaded changes are picked
        // up without restarting the stream.
        let portfolio = portfolio.read().await.clone();

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
                                        crate::log_line!(
                                            "Failed to get price for {}: {}",
                                            symbol,
                                            e
                                        );
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
        assert_eq!(
            required_forex_pairs(&p, "EUR"),
            vec!["USD/EUR".to_string(), "USD/TWD".to_string(),]
        );
    }

    #[test]
    fn usd_display_currency_alone_needs_no_pairs() {
        let p = portfolio(&[("US-Stock", "AAPL"), ("Forex", "USD")]);
        assert!(required_forex_pairs(&p, "USD").is_empty());
    }

    #[tokio::test]
    async fn tw_stock_zero_rate_yields_finite_total() {
        let p = portfolio(&[("TW-Stock", "2330")]);
        // Price present but rate is 0.0 — must not produce Infinity.
        let map: HashMap<String, f64> = [("2330".to_string(), 100.0), ("USD/TWD".to_string(), 0.0)]
            .into_iter()
            .collect();
        let (lines, total) = build_portfolio_display(&map, &p);
        assert!(total.is_finite(), "total_value must be finite, got {total}");
        assert!(
            lines.iter().any(|l| l.contains("[Warning]")),
            "warning line expected when rate is 0.0",
        );
    }

    #[tokio::test]
    async fn tw_stock_absent_rate_yields_finite_total() {
        let p = portfolio(&[("TW-Stock", "2330")]);
        // Rate key entirely absent — must not produce Infinity.
        let map: HashMap<String, f64> = [("2330".to_string(), 100.0)].into_iter().collect();
        let (lines, total) = build_portfolio_display(&map, &p);
        assert!(total.is_finite(), "total_value must be finite, got {total}");
        assert!(
            lines.iter().any(|l| l.contains("[Warning]")),
            "warning line expected when rate is absent",
        );
    }

    #[tokio::test]
    async fn forex_zero_rate_yields_finite_total() {
        let p = portfolio(&[("Forex", "TWD")]);
        // Forex rate is 0.0 — must not produce Infinity.
        let map: HashMap<String, f64> = [("USD/TWD".to_string(), 0.0)].into_iter().collect();
        let (_, total) = build_portfolio_display(&map, &p);
        assert!(total.is_finite(), "total_value must be finite, got {total}");
    }

    #[tokio::test]
    async fn forex_absent_rate_yields_finite_total() {
        let p = portfolio(&[("Forex", "TWD")]);
        // Forex rate key absent — must not produce Infinity.
        let map: HashMap<String, f64> = HashMap::new();
        let (_, total) = build_portfolio_display(&map, &p);
        assert!(total.is_finite(), "total_value must be finite, got {total}");
    }

    /// Regression test for issue #13: the display loop must not panic when
    /// `event::poll`/`event::read` return an I/O error (stdin closed, terminal
    /// disconnected, or a non-interactive CI runner). Expected behaviour: handle
    /// the error gracefully — skip the tick and keep going — rather than
    /// unwrapping and crashing. The buggy code unwrapped and panicked.
    ///
    /// Uses a headless `TestBackend` so constructing the terminal never touches a
    /// real TTY (a `CrosstermBackend` on stdout fails with `WouldBlock` in CI).
    /// `event::poll`/`event::read` still hit the absent real stdin and error,
    /// which is exactly the path this fix must survive.
    /// Regression test for issue #9 (criterion 2): `build_portfolio_display` must
    /// be a plain `fn`, not `async fn`.  The function contains no `.await`
    /// expressions, so declaring it `async fn` is a footgun: callers must use
    /// `.await`, which keeps any `MutexGuard` borrow alive across that await point
    /// and blocks every concurrent price writer for the entire render frame.
    ///
    /// This test is intentionally placed in a non-async (`#[test]`) context.
    /// When `build_portfolio_display` is still `async fn`, calling it without
    /// `.await` yields `impl Future<Output = (Vec<String>, f64)>`, not
    /// `(Vec<String>, f64)`, so the explicit type annotation below is a
    /// compile-time type-mismatch error — i.e. the test fails on the buggy code
    /// as intended.
    #[test]
    fn build_portfolio_display_is_not_async_fn() {
        let map: HashMap<String, f64> = HashMap::new();
        let p = portfolio(&[]);
        // Compiles only when build_portfolio_display is a plain `fn`.
        // If it is still `async fn`, this is a type-mismatch compile error:
        //   expected `(Vec<String>, f64)`, found opaque type (Future).
        let _: (Vec<String>, f64) = build_portfolio_display(&map, &p);
    }

    // ── is_twse_market_open_at ─────────────────────────────────────────────────
    //
    // All dates use Asia/Taipei via Taipei.with_ymd_and_hms so tests are
    // deterministic and require no network access.
    //
    // Calendar reference (verified):
    //   2024-01-22 Mon · 2024-01-25 Thu · 2024-01-27 Sat · 2024-01-28 Sun

    #[test]
    fn twse_open_at_exactly_09_00() {
        let dt = Taipei.with_ymd_and_hms(2024, 1, 22, 9, 0, 0).unwrap();
        assert!(
            is_twse_market_open_at(dt),
            "09:00 Taipei on a Monday must be open"
        );
    }

    #[test]
    fn twse_closed_at_exactly_13_30() {
        let dt = Taipei.with_ymd_and_hms(2024, 1, 22, 13, 30, 0).unwrap();
        assert!(
            !is_twse_market_open_at(dt),
            "13:30 Taipei (close boundary) must be closed"
        );
    }

    #[test]
    fn twse_open_at_13_29() {
        let dt = Taipei.with_ymd_and_hms(2024, 1, 22, 13, 29, 0).unwrap();
        assert!(
            is_twse_market_open_at(dt),
            "13:29 Taipei (last open minute) must be open"
        );
    }

    #[test]
    fn twse_closed_on_saturday() {
        let dt = Taipei.with_ymd_and_hms(2024, 1, 27, 11, 0, 0).unwrap();
        assert!(
            !is_twse_market_open_at(dt),
            "Saturday must be closed regardless of time"
        );
    }

    #[test]
    fn twse_closed_on_sunday() {
        let dt = Taipei.with_ymd_and_hms(2024, 1, 28, 11, 0, 0).unwrap();
        assert!(
            !is_twse_market_open_at(dt),
            "Sunday must be closed regardless of time"
        );
    }

    #[test]
    fn twse_closed_thursday_at_08_59() {
        let dt = Taipei.with_ymd_and_hms(2024, 1, 25, 8, 59, 0).unwrap();
        assert!(
            !is_twse_market_open_at(dt),
            "08:59 Taipei on Thursday must be closed (before open)"
        );
    }

    /// Verifies that Asia/Taipei is used for both weekday detection and hour
    /// comparison, not the server's UTC time.
    ///
    /// UTC Sunday 2024-01-21 16:00 = Taipei Monday 2024-01-22 00:00.
    /// UTC Sunday 2024-01-22 01:00 = Taipei Monday 2024-01-22 09:00 (market open).
    ///
    /// A buggy implementation that checked trading hours in UTC (01:00) would
    /// return `false`; the correct implementation uses Taipei time (09:00) and
    /// returns `true`.  The intermediate UTC-Sunday timestamp shows that the
    /// weekday check also uses Taipei dates: the same wall-clock moment is a
    /// Sunday in UTC but a Monday in Taipei.
    #[test]
    fn twse_uses_taipei_tz_not_utc() {
        use chrono::Utc;

        // UTC Sunday 16:00 = Taipei Monday 00:00 — weekday in Taipei, weekend in UTC.
        // Market is closed (before 09:00) but weekday detection must use Taipei date.
        let utc_sunday = Utc.with_ymd_and_hms(2024, 1, 21, 16, 0, 0).unwrap();
        let taipei_monday_midnight = utc_sunday.with_timezone(&Taipei);
        assert_eq!(taipei_monday_midnight.weekday(), chrono::Weekday::Mon);
        assert!(
            !is_twse_market_open_at(taipei_monday_midnight),
            "Taipei Monday 00:00 must be closed (before market open)"
        );

        // UTC Monday 01:00 = Taipei Monday 09:00 — exactly at open in Taipei.
        // A UTC-hours check (01:00) would return false; Taipei (09:00) returns true.
        let utc_monday_early = Utc.with_ymd_and_hms(2024, 1, 22, 1, 0, 0).unwrap();
        let taipei_monday_09 = utc_monday_early.with_timezone(&Taipei);
        assert!(
            is_twse_market_open_at(taipei_monday_09),
            "Taipei Monday 09:00 must be open even though UTC is 01:00 (outside UTC market hours)"
        );
    }

    fn item_with_qty(symbol: &str, category: &str, quantity: f64) -> PortfolioItem {
        PortfolioItem {
            symbol: symbol.to_string(),
            category: category.to_string(),
            quantity,
        }
    }

    fn no_nan_or_inf(lines: &[String]) {
        for line in lines {
            assert!(!line.contains("NaN"), "NaN in line: {line}");
            assert!(!line.contains("inf"), "inf in line: {line}");
            assert!(!line.contains("Inf"), "Inf in line: {line}");
        }
    }

    #[test]
    fn build_display_all_zero_price_map() {
        // Assets that require a price-map lookup: US-Stock, Crypto, TW-Stock (needs
        // USD/TWD), non-USD Forex. None of these prices are in the map, so only the
        // USD cash (which needs no lookup) should contribute to the total.
        let p = Portfolio(vec![
            item_with_qty("AAPL", "US-Stock", 10.0),
            item_with_qty("BTC", "Crypto", 1.0),
            item_with_qty("2330", "TW-Stock", 100.0),
            item_with_qty("TWD", "Forex", 3000.0),
        ]);
        let (lines, total) = build_portfolio_display(&HashMap::new(), &p);
        assert_eq!(total, 0.0);
        no_nan_or_inf(&lines);
    }

    #[test]
    fn build_display_tw_stock_missing_twd_rate() {
        let p = Portfolio(vec![item_with_qty("2330", "TW-Stock", 10.0)]);
        let mut map = HashMap::new();
        map.insert("2330".to_string(), 600.0);
        // USD/TWD absent — total must be 0 and a warning line must appear
        let (lines, total) = build_portfolio_display(&map, &p);
        assert_eq!(total, 0.0);
        assert!(
            lines.iter().any(|l| l.contains("[Warning]")),
            "expected a warning line, got: {lines:?}"
        );
        no_nan_or_inf(&lines);
    }

    #[test]
    fn build_display_non_usd_forex_holding() {
        let p = Portfolio(vec![item_with_qty("TWD", "Forex", 30000.0)]);
        let mut map = HashMap::new();
        map.insert("USD/TWD".to_string(), 30.0);
        let (lines, total) = build_portfolio_display(&map, &p);
        // 30 000 TWD / 30 = 1 000 USD
        assert!(
            (total - 1000.0).abs() < 1e-6,
            "expected 1000.0, got {total}"
        );
        assert!(lines.iter().any(|l| l.contains("TWD")));
        no_nan_or_inf(&lines);
    }

    #[test]
    fn build_display_mixed_portfolio_all_prices() {
        let p = Portfolio(vec![
            item_with_qty("AAPL", "US-Stock", 10.0),
            item_with_qty("2330", "TW-Stock", 100.0),
            item_with_qty("USD", "Forex", 500.0),
            item_with_qty("TWD", "Forex", 3000.0),
        ]);
        let mut map = HashMap::new();
        map.insert("AAPL".to_string(), 200.0); // 10 × 200 = 2000 USD
        map.insert("2330".to_string(), 600.0); // 100 × 600 TWD / 30 = 2000 USD
        map.insert("USD/TWD".to_string(), 30.0);
        // Cash: 500 USD + 3000 TWD / 30 = 600 USD  →  total 4600 USD
        let (lines, total) = build_portfolio_display(&map, &p);
        assert!(
            (total - 4600.0).abs() < 1e-6,
            "expected 4600.0, got {total}"
        );
        assert!(!lines.is_empty());
        no_nan_or_inf(&lines);
    }

    #[test]
    fn build_display_total_matches_compute_category_values() {
        // The total returned by build_portfolio_display must equal the total
        // from compute_category_values for the same inputs (single source of truth).
        let p = Portfolio(vec![
            item_with_qty("AAPL", "US-Stock", 5.0),
            item_with_qty("2330", "TW-Stock", 50.0),
            item_with_qty("EUR", "Forex", 1000.0),
        ]);
        let mut map = HashMap::new();
        map.insert("AAPL".to_string(), 150.0);
        map.insert("2330".to_string(), 500.0);
        map.insert("USD/TWD".to_string(), 32.0);
        map.insert("USD/EUR".to_string(), 1.1);

        let (_, display_total) = build_portfolio_display(&map, &p);
        let (_, canon_total) = history::compute_category_values(&p, &map);
        assert!(
            (display_total - canon_total).abs() < 1e-9,
            "build_portfolio_display total ({display_total}) != compute_category_values total ({canon_total})"
        );
    }

    #[tokio::test]
    async fn display_loop_survives_event_poll_io_error() {
        use ratatui::backend::TestBackend;

        let prices: SharedPriceMap = Arc::new(Mutex::new(HashMap::new()));
        let history: SharedHistory = Arc::new(Mutex::new(Vec::new()));
        let portfolio: SharedPortfolio = Arc::new(RwLock::new(Portfolio(vec![])));
        let target_forex: SharedTargetForex = Arc::new(RwLock::new("USD".to_string()));
        let mut terminal =
            Terminal::new(TestBackend::new(80, 24)).expect("construct headless test terminal");

        let handle = tokio::spawn(async move {
            run_display_loop(&mut terminal, &prices, &history, &portfolio, &target_forex).await;
        });

        // Give the loop time to hit the failing event::poll path several times.
        tokio::time::sleep(Duration::from_millis(200)).await;

        if handle.is_finished() {
            // Returned on its own (e.g. a graceful break): fine as long as no panic.
            let result = handle.await;
            assert!(
                result.is_ok(),
                "display loop panicked on an event::poll/read I/O error instead of \
                 handling it gracefully: {:?}",
                result.err()
            );
        } else {
            // Still running after repeated I/O errors: it survived without crashing.
            handle.abort();
        }
    }
}
