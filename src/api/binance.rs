use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct BinancePrice {
    price: String,
}

pub async fn get_price_from_binance(symbol: &str) -> Result<f64, String> {
    let pair = format!("{}USDT", symbol.to_uppercase());
    let url = format!("https://api.binance.com/api/v3/ticker/price?symbol={}", pair);

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(&url).send().await.map_err(|e| {
        format!("[Binance] 查詢 {} 價格失敗：{}", symbol, e)
    })?;

    if response.status().is_success() {
        let data: BinancePrice = response.json().await.map_err(|e| {
            format!("[Binance] 回傳 JSON 格式錯誤：{}", e)
        })?;
        data.price.parse::<f64>().map_err(|_| "無法解析價格為浮點數".to_string())
    } else {
        Err(format!("[Binance] HTTP 錯誤碼：{}", response.status()))
    }
}
