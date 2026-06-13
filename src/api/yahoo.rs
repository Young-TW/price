use reqwest::Client;
use serde::Deserialize;

/// Yahoo Finance rejects requests without a browser-like User-Agent (HTTP 429
/// "Edge: Too Many Requests"), so every call must send one.
const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

#[derive(Deserialize, Debug)]
struct YahooChartResponse {
    chart: Chart,
}

#[derive(Deserialize, Debug)]
struct Chart {
    result: Option<Vec<ChartResult>>,
}

#[derive(Deserialize, Debug)]
struct ChartResult {
    #[serde(default)]
    timestamp: Vec<i64>,
    indicators: Indicators,
}

#[derive(Deserialize, Debug)]
struct Indicators {
    quote: Vec<Quote>,
}

#[derive(Deserialize, Debug)]
struct Quote {
    close: Vec<Option<f64>>, // Some time points may be null
}

pub async fn get_price_from_yahoo(symbol: &str) -> Result<f64, String> {
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=1d",
        symbol
    );

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(&url).send().await.map_err(|e| {
        format!("[Yahoo] Failed to query {}: {}", symbol, e)
    })?;

    if !response.status().is_success() {
        return Err(format!("[Yahoo] HTTP error: {}", response.status()));
    }

    let data: YahooChartResponse = response.json().await.map_err(|e| {
        format!("[Yahoo] JSON format error: {}", e)
    })?;

    let close = data
        .chart
        .result
        .as_ref()
        .and_then(|r| r.get(0))
        .and_then(|r| r.indicators.quote.get(0))
        .and_then(|q| q.close.last().copied().flatten());

    match close {
        Some(price) => Ok(price),
        None => Err(format!("[Yahoo] Failed to get closing price for {}", symbol)),
    }
}

/// Fetch historical daily close prices from Yahoo Finance.
///
/// `range` is a Yahoo range string (e.g. `3mo`, `1y`) and `interval` the bar
/// size (e.g. `1d`). Returns `(timestamp, close)` pairs, skipping null closes.
/// Used as the historical fallback for Taiwan equities, which Pyth does not cover.
pub async fn get_history_from_yahoo(
    symbol: &str,
    range: &str,
    interval: &str,
) -> Result<Vec<(i64, f64)>, String> {
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval={}&range={}",
        symbol, interval, range
    );

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("[Yahoo] Failed to query history {}: {}", symbol, e))?;

    if !response.status().is_success() {
        return Err(format!("[Yahoo] HTTP error: {}", response.status()));
    }

    let data: YahooChartResponse = response
        .json()
        .await
        .map_err(|e| format!("[Yahoo] JSON format error: {}", e))?;

    let result = data
        .chart
        .result
        .as_ref()
        .and_then(|r| r.get(0))
        .ok_or_else(|| format!("[Yahoo] No history result for {}", symbol))?;

    let quote = result
        .indicators
        .quote
        .get(0)
        .ok_or_else(|| format!("[Yahoo] No quote data for {}", symbol))?;

    let series = result
        .timestamp
        .iter()
        .zip(quote.close.iter())
        .filter_map(|(t, c)| c.map(|close| (*t, close)))
        .collect();

    Ok(series)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_price_from_yahoo() {
        let symbol = "VOO"; // VOO
        match get_price_from_yahoo(symbol).await {
            Ok(price) => println!("Price of {}: {}", symbol, price),
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}