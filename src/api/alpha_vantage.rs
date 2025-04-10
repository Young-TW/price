use reqwest::Error;
use serde::Deserialize;

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

/// Alpha Vantage 免費帳號：每分鐘 5 次，每天 500 次
pub async fn get_price_from_alpha_vantage(symbol: &str, api_key: &str) -> Result<f64, String> {
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
            .map_err(|_| "無法解析價格".to_string())
    } else {
        Err(format!(
            "無法取得價格 (可能為 API 限制或錯誤 symbol: {})",
            symbol
        ))
    }
}
