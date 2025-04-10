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
        format!("[RedStone] 查詢 {} 價格失敗：{}", symbol, e)
    })?;

    if response.status().is_success() {
        let data: Vec<RedstonePrice> = response.json().await.map_err(|e| {
            format!("[RedStone] 回傳 JSON 格式錯誤：{}", e)
        })?;

        if let Some(price_data) = data.get(0) {
            Ok(price_data.value)
        } else {
            Err(format!("[RedStone] 沒有取得 {} 的價格資料", symbol))
        }
    } else {
        Err(format!("[RedStone] HTTP 錯誤碼：{}", response.status()))
    }
}
