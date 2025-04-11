use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct TwseResponse {
    msgArray: Vec<TwseStock>,
}

#[derive(Deserialize, Debug)]
struct TwseStock {
    z: String, // 最新成交價
}

pub async fn get_price_from_twse(symbol: &str) -> Result<f64, String> {
    // ex: 2330 -> tse_2330.tw
    let pair = format!("tse_{}.tw", symbol);
    let url = format!(
        "https://mis.twse.com.tw/stock/api/getStockInfo.jsp?ex_ch={}",
        pair
    );

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("[TWSE] 查詢 {} 價格失敗：{}", symbol, e))?;

    if response.status().is_success() {
        let data: TwseResponse = response
            .json()
            .await
            .map_err(|e| format!("[TWSE] 回傳 JSON 格式錯誤：{}", e))?;
        let price_str = data
            .msgArray
            .get(0)
            .ok_or("[TWSE] 找不到股票資料")?
            .z
            .clone();

        price_str
            .parse::<f64>()
            .map_err(|_| "[TWSE] 無法解析價格為浮點數".to_string())
    } else {
        Err(format!("[TWSE] HTTP 錯誤碼：{}", response.status()))
    }
}
