//! ExchangeRate-API forex rate source.

use reqwest::Client;
use serde::Deserialize;

use crate::config::read_api_keys;

#[derive(Deserialize, Debug)]
struct ExchangeRateResponse {
    result: String,
    #[serde(rename = "conversion_rates")]
    rates: std::collections::HashMap<String, f64>,
}

/// Fetch the conversion rate from currency `from` to currency `to`.
///
/// Both codes are upper-cased. Requires `exchangerate_api_key` in the API key
/// file. Returns the rate, or an `Err` string if the key is missing, the request
/// or HTTP call fails, the response status is not `"success"`, or `to` is absent
/// from the returned rate table.
pub async fn get_rate(from: &str, to: &str) -> Result<f64, String> {
    let api_keys = read_api_keys(&crate::paths::api_key_file())
        .map_err(|e| format!("[ExchangeRate] Failed to read API key: {}", e))?;
    let api_key = api_keys
        .get("exchangerate_api_key")
        .ok_or_else(|| "[ExchangeRate] Exchangerate API key not found".to_string())?;
    let url = format!(
        "https://v6.exchangerate-api.com/v6/{}/latest/{}",
        api_key,
        from.to_uppercase()
    );

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("[ExchangeRate] Query failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("[ExchangeRate] HTTP error: {}", response.status()));
    }

    let data: ExchangeRateResponse = response
        .json()
        .await
        .map_err(|e| format!("[ExchangeRate] JSON format error: {}", e))?;

    if data.result != "success" {
        return Err("[ExchangeRate] Response failed".to_string());
    }

    data.rates
        .get(&to.to_uppercase())
        .copied()
        .ok_or_else(|| format!("[ExchangeRate] Cannot find rate for {}", to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_rate() {
        let from = "USD";
        let to = "EUR";
        match get_rate(from, to).await {
            Ok(rate) => println!("Exchange rate from {} to {}: {}", from, to, rate),
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}
