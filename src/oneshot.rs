//! One-shot (non-TUI) mode: fetch every price the portfolio needs exactly
//! once, print the valuation as text or JSON on stdout, and exit. This makes
//! the binary usable from scripts, cron jobs and status bars (waybar/polybar)
//! without taking over the terminal.

use std::collections::HashMap;

use chrono::Utc;
use futures::future::join_all;
use serde_json::json;

use crate::api::pyth::{get_latest_price_from_pyth, get_pyth_feed_id};
use crate::api::redstone::get_price_from_redstone;
use crate::get::get_price;
use crate::history;
use crate::stream::{build_portfolio_display, required_forex_pairs};
use crate::types::Portfolio;

/// Fetch the current price for one price-map key without opening a stream.
///
/// Crypto and Forex have no polling path in [`get_price`] (their live prices
/// normally arrive via the Pyth SSE stream), so they are served by the Hermes
/// "latest price" REST endpoint instead, with RedStone as the crypto fallback.
/// Everything else goes through the regular fallback chain.
///
/// For Forex, `symbol` is the whole pair (e.g. `USD/TWD`) — the same form the
/// bundled feed table is keyed by.
async fn fetch_one(symbol: &str, category: &str) -> Result<f64, String> {
    match category {
        "Forex" | "Crypto" => {
            let via_pyth = match get_pyth_feed_id(symbol, category) {
                Ok(id) => get_latest_price_from_pyth(&id).await,
                Err(e) => Err(e),
            };
            match via_pyth {
                Ok(price) => Ok(price),
                Err(_) if category == "Crypto" => get_price_from_redstone(symbol).await,
                Err(e) => Err(e),
            }
        }
        _ => get_price(symbol, category).await,
    }
}

/// Every `(price-map key, fetch symbol, category)` request needed to fully
/// value `portfolio` and render the `target_forex` conversion line.
///
/// Forex cash holdings themselves need no quote — their USD value comes from
/// the `USD/{ccy}` rates, which [`required_forex_pairs`] already includes
/// (along with `USD/TWD` for Taiwan equities and the display currency).
fn price_requests(portfolio: &Portfolio, target_forex: &str) -> Vec<(String, String, String)> {
    let mut requests = Vec::new();

    for pair in required_forex_pairs(portfolio, target_forex) {
        requests.push((pair.clone(), pair, "Forex".to_string()));
    }

    for item in portfolio.iter() {
        if item.category == "Forex" {
            continue;
        }
        requests.push((
            item.symbol.clone(),
            item.symbol.clone(),
            item.category.clone(),
        ));
    }

    requests
}

/// Fetch all required prices concurrently. Returns the price map plus a
/// human-readable error line for every quote that could not be obtained.
async fn fetch_prices_once(
    portfolio: &Portfolio,
    target_forex: &str,
) -> (HashMap<String, f64>, Vec<String>) {
    let requests = price_requests(portfolio, target_forex);

    let results = join_all(
        requests
            .into_iter()
            .map(|(key, symbol, category)| async move {
                let result = fetch_one(&symbol, &category).await;
                (key, symbol, category, result)
            }),
    )
    .await;

    let mut map = HashMap::new();
    let mut errors = Vec::new();
    for (key, symbol, category, result) in results {
        match result {
            Ok(price) => {
                map.insert(key, price);
            }
            Err(e) => errors.push(format!("{} ({}): {}", symbol, category, e)),
        }
    }
    (map, errors)
}

/// Render the same per-holding breakdown and totals the TUI shows, as plain
/// text.
fn render_text(portfolio: &Portfolio, map: &HashMap<String, f64>, target_forex: &str) -> String {
    let (mut lines, total_value) = build_portfolio_display(map, portfolio);
    lines.push(format!("Total assets (USD): ${:.2}", total_value));
    if let Some(rate) = map.get(&format!("USD/{}", target_forex)) {
        lines.push(format!(
            "Total assets ({}): ${:.2}",
            target_forex,
            total_value * rate
        ));
    }
    lines.join("\n")
}

/// Build the machine-readable valuation for `--json` mode.
///
/// `total_value_target` is `null` when the `USD/{target}` rate could not be
/// fetched; `complete` is `false` when any holding is missing a price (the
/// USD total then understates the portfolio).
fn render_json(
    portfolio: &Portfolio,
    map: &HashMap<String, f64>,
    target_forex: &str,
    errors: &[String],
    timestamp: i64,
) -> serde_json::Value {
    let (category_values, total_value) = history::compute_category_values(portfolio, map);
    let total_value_target = if target_forex.eq_ignore_ascii_case("USD") {
        Some(total_value)
    } else {
        map.get(&format!("USD/{}", target_forex))
            .map(|rate| total_value * rate)
    };

    json!({
        "timestamp": timestamp,
        "total_value_usd": total_value,
        "target_currency": target_forex,
        "total_value_target": total_value_target,
        "category_values_usd": category_values,
        "prices": map,
        "complete": history::is_complete(portfolio, map),
        "errors": errors,
    })
}

/// Run one-shot mode end to end and return the process exit code: `0` when
/// every holding was priced, `1` when the valuation is incomplete.
pub async fn run_once(portfolio: Portfolio, target_forex: String, json: bool) -> i32 {
    let (map, errors) = fetch_prices_once(&portfolio, &target_forex).await;

    if json {
        // Errors are embedded in the payload so stdout stays parseable.
        let value = render_json(
            &portfolio,
            &map,
            &target_forex,
            &errors,
            Utc::now().timestamp(),
        );
        println!("{}", value);
    } else {
        for error in &errors {
            eprintln!("warning: failed to fetch {}", error);
        }
        println!("{}", render_text(&portfolio, &map, &target_forex));
    }

    if history::is_complete(&portfolio, &map) {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PortfolioItem;

    fn item(symbol: &str, category: &str, quantity: f64) -> PortfolioItem {
        PortfolioItem {
            symbol: symbol.to_string(),
            category: category.to_string(),
            quantity,
        }
    }

    fn sample_portfolio() -> Portfolio {
        Portfolio(vec![
            item("AAPL", "US-Stock", 10.0),
            item("2330", "TW-Stock", 100.0),
            item("eth", "Crypto", 2.0),
            item("TWD", "Forex", 30000.0),
            item("USD", "Forex", 500.0),
        ])
    }

    fn sample_prices() -> HashMap<String, f64> {
        [
            ("AAPL".to_string(), 200.0),   // 2000 USD
            ("2330".to_string(), 600.0),   // 60000 TWD = 2000 USD
            ("eth".to_string(), 2500.0),   // 5000 USD
            ("USD/TWD".to_string(), 30.0), // TWD cash: 1000 USD
        ]
        .into_iter()
        .collect()
        // Total: 2000 + 2000 + 5000 + 1000 + 500 = 10500 USD
    }

    #[test]
    fn price_requests_cover_every_required_key() {
        let requests = price_requests(&sample_portfolio(), "TWD");
        let keys: Vec<&str> = requests.iter().map(|(k, _, _)| k.as_str()).collect();

        // Non-forex holdings are requested under their own symbol.
        assert!(keys.contains(&"AAPL"));
        assert!(keys.contains(&"2330"));
        assert!(keys.contains(&"eth"));
        // TWD rate (cash + TW equities + display currency) as a full pair.
        assert!(keys.contains(&"USD/TWD"));
        // Forex cash holdings themselves must not be requested as quotes.
        assert!(!keys.contains(&"TWD"));
        assert!(!keys.contains(&"USD"));

        // Every key that is_complete checks for must be requested, so a fully
        // successful fetch always yields a complete valuation.
        for key in history::required_price_keys(&sample_portfolio()) {
            assert!(keys.contains(&key.as_str()), "missing request for {key}");
        }

        // Forex requests carry the whole pair as the fetch symbol (the feed
        // table is keyed by "USD/TWD", not "TWD").
        let (_, symbol, category) = requests
            .iter()
            .find(|(k, _, _)| k == "USD/TWD")
            .expect("USD/TWD request");
        assert_eq!(symbol, "USD/TWD");
        assert_eq!(category, "Forex");
    }

    #[test]
    fn price_requests_include_display_currency_pair() {
        let p = Portfolio(vec![item("AAPL", "US-Stock", 1.0)]);
        let requests = price_requests(&p, "EUR");
        assert!(requests.iter().any(|(k, _, _)| k == "USD/EUR"));
    }

    #[test]
    fn render_text_includes_totals_and_conversion() {
        let text = render_text(&sample_portfolio(), &sample_prices(), "TWD");
        assert!(text.contains("Total assets (USD): $10500.00"), "{text}");
        // 10500 USD × 30 = 315000 TWD
        assert!(text.contains("Total assets (TWD): $315000.00"), "{text}");
    }

    #[test]
    fn render_text_omits_conversion_without_rate() {
        let mut map = sample_prices();
        map.remove("USD/TWD");
        let text = render_text(&sample_portfolio(), &map, "TWD");
        assert!(!text.contains("Total assets (TWD)"), "{text}");
    }

    #[test]
    fn render_json_complete_portfolio() {
        let value = render_json(
            &sample_portfolio(),
            &sample_prices(),
            "TWD",
            &[],
            1_700_000_000,
        );
        assert_eq!(value["timestamp"], 1_700_000_000);
        assert_eq!(value["target_currency"], "TWD");
        assert!((value["total_value_usd"].as_f64().unwrap() - 10500.0).abs() < 1e-6);
        assert!((value["total_value_target"].as_f64().unwrap() - 315000.0).abs() < 1e-6);
        assert_eq!(value["complete"], true);
        assert_eq!(value["errors"].as_array().unwrap().len(), 0);
        // Forex holdings are merged into the synthetic Cash category.
        assert!(value["category_values_usd"]["Cash"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn render_json_usd_target_needs_no_rate() {
        let value = render_json(&sample_portfolio(), &sample_prices(), "USD", &[], 0);
        assert_eq!(
            value["total_value_target"].as_f64(),
            value["total_value_usd"].as_f64()
        );
    }

    #[test]
    fn render_json_missing_price_is_incomplete_with_null_target() {
        let mut map = sample_prices();
        map.remove("USD/TWD");
        let errors = vec!["USD/TWD (Forex): boom".to_string()];
        let value = render_json(&sample_portfolio(), &map, "TWD", &errors, 0);
        assert_eq!(value["complete"], false);
        assert!(value["total_value_target"].is_null());
        assert_eq!(value["errors"].as_array().unwrap().len(), 1);
    }
}
