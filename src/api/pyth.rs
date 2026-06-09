use eventsource_client::Client as EventSourceClient; // 避免與 reqwest::Client 衝突
use eventsource_client::{ClientBuilder, SSE};

use futures::StreamExt;
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::sync::Arc;

const BASE_URL: &str = "https://hermes.pyth.network";
const BENCHMARKS_URL: &str = "https://benchmarks.pyth.network";

/// Map a portfolio `(symbol, category)` to a Pyth Benchmarks TradingView symbol.
/// Returns `None` for categories Pyth does not cover (e.g. Taiwan equities).
pub fn pyth_tv_symbol(symbol: &str, category: &str) -> Option<String> {
    let sym = symbol.to_uppercase();
    match category {
        "Crypto" => Some(format!("Crypto.{}/USD", sym)),
        "US-Stock" | "US-ETF" => Some(format!("Equity.US.{}/USD", sym)),
        // For forex we always price against USD, e.g. USD/TWD.
        "Forex" => Some(format!("FX.USD/{}", sym)),
        _ => None,
    }
}

/// Fetch historical daily close prices from the Pyth Benchmarks TradingView shim.
///
/// `tv_symbol` is a Pyth TradingView symbol (e.g. `Equity.US.AAPL/USD`).
/// `from`/`to` are unix epoch seconds. Returns `(timestamp, close)` pairs.
pub async fn get_history_from_pyth(
    tv_symbol: &str,
    from: i64,
    to: i64,
) -> Result<Vec<(i64, f64)>, String> {
    let url = format!(
        "{}/v1/shims/tradingview/history?symbol={}&resolution=D&from={}&to={}",
        BENCHMARKS_URL, tv_symbol, from, to
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("[Pyth] Failed to query {}: {}", tv_symbol, e))?;

    if !response.status().is_success() {
        return Err(format!("[Pyth] HTTP error: {}", response.status()));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("[Pyth] JSON format error: {}", e))?;

    match json.get("s").and_then(|s| s.as_str()) {
        Some("ok") => {}
        other => {
            return Err(format!(
                "[Pyth] No history for {} (status: {:?})",
                tv_symbol, other
            ));
        }
    }

    let times = json.get("t").and_then(|v| v.as_array());
    let closes = json.get("c").and_then(|v| v.as_array());

    match (times, closes) {
        (Some(times), Some(closes)) => {
            let series = times
                .iter()
                .zip(closes.iter())
                .filter_map(|(t, c)| Some((t.as_i64()?, c.as_f64()?)))
                .collect();
            Ok(series)
        }
        _ => Err(format!("[Pyth] Malformed history response for {}", tv_symbol)),
    }
}

pub trait PriceContainer {
    fn update(&mut self, symbol: String, price: f64);
}

impl PriceContainer for Vec<(String, f64)> {
    fn update(&mut self, symbol: String, price: f64) {
        if let Some(entry) = self.iter_mut().find(|(s, _)| *s == symbol) {
            entry.1 = price;
        } else {
            self.push((symbol, price));
        }
    }
}

impl PriceContainer for HashMap<String, f64> {
    fn update(&mut self, symbol: String, price: f64) {
        self.insert(symbol, price);
    }
}

/// 訂閱 Pyth 即時價格串流，並將價格回傳給 callback 函數。
///
/// # 參數
/// - `id`: Pyth price feed 的 ID（hex 字串）
/// - `on_price`: 回呼函數，接收實際價格（`f64`）
///
/// # 範例
/// pyth_stream::subscribe_price_stream("0xe62d...", |price| println!("價格: {}", price)).await;
pub async fn get_price_stream_from_pyth<F>(id: &str, mut on_price: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnMut(f64) + Send + 'static,
{
    let url = format!("{}/v2/updates/price/stream?ids[]={}", BASE_URL, id);

    let mut stream = ClientBuilder::for_url(&url)?.build().stream();

    while let Some(event) = stream.next().await {
        match event {
            Ok(SSE::Event(ev)) => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&ev.data) {
                    if let Some(parsed_array) = json.get("parsed").and_then(|v| v.as_array()) {
                        for entry in parsed_array {
                            if let Some(price_obj) = entry.get("price") {
                                if let (Some(price_str), Some(expo)) = (
                                    price_obj.get("price").and_then(|p| p.as_str()),
                                    price_obj.get("expo").and_then(|e| e.as_i64()),
                                ) {
                                    if let Ok(price_int) = price_str.parse::<f64>() {
                                        let actual_price = price_int * 10f64.powi(expo as i32);
                                        on_price(actual_price);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(_) => {} // Skip Ping/Comment
            Err(e) => {
                eprintln!("SSE error: {}", e);
            }
        }
    }

    Ok(())
}

pub async fn get_pyth_feed_id(symbol: &str, category: &str) -> String {
    let target = symbol.to_uppercase();
    let data = std::fs::read_to_string("src/api/data/pyth.toml").expect("Failed to read Pyth config file");
    let pairs: toml::Value = toml::from_str(&data).expect("Failed to parse Pyth config file");
    let feeds = pairs.get(category).expect("Cannot find feeds");
    let feed_id = feeds
        .get(&target)
        .unwrap_or_else(|| panic!("Cannot find feed_id, symbol = {}", symbol));
    let raw = feed_id.as_str().expect("feed_id should be a string");
    return raw.to_string();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pyth_tv_symbol() {
        assert_eq!(pyth_tv_symbol("eth", "Crypto").unwrap(), "Crypto.ETH/USD");
        assert_eq!(pyth_tv_symbol("aapl", "US-Stock").unwrap(), "Equity.US.AAPL/USD");
        assert_eq!(pyth_tv_symbol("QQQ", "US-ETF").unwrap(), "Equity.US.QQQ/USD");
        assert_eq!(pyth_tv_symbol("TWD", "Forex").unwrap(), "FX.USD/TWD");
        assert!(pyth_tv_symbol("2330", "TW-Stock").is_none());
    }

    #[tokio::test]
    async fn test_get_history_from_pyth() {
        if !matches!(std::env::var("RUN_LIVE_PRICE_TESTS").as_deref(), Ok("1")) {
            return;
        }
        let to = chrono::Utc::now().timestamp();
        let from = to - 30 * 86_400;
        let series = get_history_from_pyth("Equity.US.AAPL/USD", from, to)
            .await
            .unwrap();
        assert!(!series.is_empty());
        assert!(series.iter().all(|(_, c)| *c > 0.0));
        // Timestamps should be strictly increasing.
        assert!(series.windows(2).all(|w| w[0].0 < w[1].0));
    }
}

pub fn spawn_price_stream<C>(
    symbol: &str,
    category: &str,
    prices: Arc<Mutex<C>>,
) where
    C: PriceContainer + Send + 'static,
{
    let symbol = symbol.to_string();
    let category = category.to_string();
    tokio::spawn(async move {
        let id = get_pyth_feed_id(&symbol, &category).await;
        let symbol_clone = symbol.clone();
        if let Err(e) = get_price_stream_from_pyth(id.as_str(), move |price| {
            let prices = Arc::clone(&prices);
            let symbol_clone = symbol_clone.clone();
            tokio::spawn(async move {
                let mut prices = prices.lock().await;
                prices.update(symbol_clone.clone(), price);
            });
        }).await {
            eprintln!("Error occurred for {}: {}", symbol, e);
        }
    });
}
