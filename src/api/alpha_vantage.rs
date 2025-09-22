use serde::Deserialize;

use crate::config::read_api_keys;

#[derive(Deserialize, Debug)]
struct GlobalQuote {
    #[serde(rename = "05. price")]
    price: String,
}

#[derive(Deserialize, Debug)]
struct AlphaVantageResponse {
    #[serde(rename = "Global Quote")]
    global_quote: Option<GlobalQuote>,
}

/// Alpha Vantage free account: 5 requests per minute, 500 requests per day
pub async fn _get_price_from_alpha_vantage(symbol: &str) -> Result<f64, String> {
    let api_keys = read_api_keys("config/api_key.toml")
        .map_err(|e| format!("[AlphaVantage] Failed to read API key: {}", e))?;
    let api_key = api_keys
        .get("alpha_vantage_api_key")
        .ok_or_else(|| "[AlphaVantage] Alpha Vantage API key not found".to_string())?;
    let url = format!(
        "https://www.alphavantage.co/query?function=GLOBAL_QUOTE&symbol={}&apikey={}",
        symbol, api_key
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let data: AlphaVantageResponse = response.json().await.map_err(|e| e.to_string())?;

    if let Some(global_quote) = data.global_quote {
        global_quote
            .price
            .parse::<f64>()
            .map_err(|_| "Failed to parse price".to_string())
    } else {
        Err(format!(
            "Failed to get price (possibly due to API limit or invalid symbol: {})",
            symbol
        ))
    }
}
