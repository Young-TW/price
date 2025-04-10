use reqwest::Client;
use serde::Deserialize;

use crate::config::read_api_keys;

#[derive(Deserialize, Debug)]
struct ExchangeRateResponse {
    result: String,
    #[serde(rename = "conversion_rates")]
    rates: std::collections::HashMap<String, f64>,
}

pub async fn get_rate(from: &str, to: &str) -> Result<f64, String> {
    let api_keys = read_api_keys("config/api_key.toml")
        .map_err(|e| format!("[ExchangeRate] 讀取 API 金鑰失敗：{}", e))?;
    let api_key = api_keys
        .get("exchangerate_api_key")
        .ok_or_else(|| "[ExchangeRate] 找不到 exchangerate API 金鑰".to_string())?;
    let url = format!(
        "https://v6.exchangerate-api.com/v6/{}/latest/{}",
        api_key,
        from.to_uppercase()
    );

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(&url).send().await.map_err(|e| {
        format!("[ExchangeRate] 查詢失敗：{}", e)
    })?;

    if !response.status().is_success() {
        return Err(format!("[ExchangeRate] HTTP 錯誤：{}", response.status()));
    }

    let data: ExchangeRateResponse = response.json().await.map_err(|e| {
        format!("[ExchangeRate] JSON 格式錯誤：{}", e)
    })?;

    if data.result != "success" {
        return Err("[ExchangeRate] 回應失敗".to_string());
    }

    data.rates.get(&to.to_uppercase())
        .copied()
        .ok_or_else(|| format!("[ExchangeRate] 找不到 {} 匯率", to))
}
