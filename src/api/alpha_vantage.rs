use crate::config::read_api_keys;
use crate::types::Price_Response;

/// Alpha Vantage free account: 5 requests per minute, 500 requests per day
pub async fn get_price_from_alpha_vantage(symbol: &str) -> Result<f64, String> {
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

    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let mut response: Price_Response = Price_Response {
        price: 0.0,
        source: "Alpha Vantage".to_string(),
        symbol: symbol.to_string(),
        category: "N/A".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        error: None,
    };

    let body = resp.text().await.map_err(|e| e.to_string())?;
    response.price = serde_json::from_str::<serde_json::Value>(&body)
        .map_err(|e| e.to_string())?
        .get("Global Quote")
        .and_then(|v| v.get("05. price"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| {
            format!(
                "[AlphaVantage] Failed to get price (possibly due to API limit or invalid symbol: {})",
                symbol
            )
        })?;
    Ok(response.price)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_price_from_twse() {
        let symbol = "VOO"; // VOO
        match get_price_from_alpha_vantage(symbol).await {
            Ok(price) => println!("Price of {}: {}", symbol, price),
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}
