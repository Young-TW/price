use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct BinancePrice {
    price: String,
}

pub async fn _get_price_from_binance(symbol: &str) -> Result<f64, String> {
    let pair = format!("{}USDT", symbol.to_uppercase());
    let url = format!("https://api.binance.com/api/v3/ticker/price?symbol={}", pair);

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(&url).send().await.map_err(|e| {
        format!("[Binance] Failed to query price for {}: {}", symbol, e)
    })?;

    if response.status().is_success() {
        let data: BinancePrice = response.json().await.map_err(|e| {
            format!("[Binance] Returned JSON format error: {}", e)
        })?;
        data.price.parse::<f64>().map_err(|_| "Failed to parse price as float".to_string())
    } else {
        Err(format!("[Binance] HTTP error code: {}", response.status()))
    }
}
