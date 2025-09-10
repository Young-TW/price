use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct RedstonePrice {
    value: f64,
}

pub async fn get_price_from_redstone(symbol: &str) -> Result<f64, String> {
    let symbol = symbol.to_uppercase();
    let url = format!(
        "https://api.redstone.finance/prices/?symbol={}&provider=redstone&limit=1",
        symbol
    );

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(&url).send().await.map_err(|e| {
        format!("[RedStone] Failed to query price for {}: {}", symbol, e)
    })?;

    if response.status().is_success() {
        let data: Vec<RedstonePrice> = response.json().await.map_err(|e| {
            format!("[RedStone] Returned JSON format error: {}", e)
        })?;

        if let Some(price_data) = data.get(0) {
            Ok(price_data.value)
        } else {
            Err(format!("[RedStone] No price data found for {}", symbol))
        }
    } else {
        Err(format!("[RedStone] HTTP error code: {}", response.status()))
    }
}
