use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct TwseResponse {
    #[serde(rename = "msgArray")]
    msg_array: Vec<TwseStock>,
}

#[derive(Deserialize, Debug)]
struct TwseStock {
    z: String, // 最新成交價
    a: String, // 賣一價（多個以_分隔，取第一個）
    b: String, // 買一價（多個以_分隔，取第一個）
    y: String, // 昨收
}

pub async fn get_price_from_twse(symbol: &str) -> Result<f64, String> {
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
        let text = response.text().await.unwrap_or_default();
        let data: TwseResponse = serde_json::from_str(&text)
            .map_err(|e| format!("[TWSE] 回傳 JSON 格式錯誤：{}\n{}", e, text))?;
        let stock = data
            .msg_array
            .get(0)
            .ok_or("[TWSE] 找不到股票資料")?;

        // 如果成交價不是 "-"，直接用
        if stock.z != "-" {
            stock.z
                .parse::<f64>()
                .map_err(|_| "[TWSE] 無法解析價格為浮點數".to_string())
        } else {
            // 買一、賣一可能是多個價格，用 "_" 分隔，取第一個
            let a1 = stock.a.split('_').next().unwrap_or("-");
            let b1 = stock.b.split('_').next().unwrap_or("-");
            let a1f = a1.parse::<f64>();
            let b1f = b1.parse::<f64>();
            match (a1f, b1f) {
                (Ok(a), Ok(b)) => {
                    let geo_mean = (a * b).sqrt();
                    Ok(geo_mean)
                }
                _ => {
                    // 如果買一賣一也無法解析，改用昨收
                    stock.y
                        .parse::<f64>()
                        .map_err(|_| "[TWSE] 無法解析昨收價格為浮點數".to_string())
                }
            }
        }
    } else {
        Err(format!("[TWSE] HTTP 錯誤碼：{}", response.status()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_price_from_twse() {
        let symbol = "2330"; // 台積電
        match get_price_from_twse(symbol).await {
            Ok(price) => println!("{} 的價格是：{}", symbol, price),
            Err(e) => eprintln!("錯誤：{}", e),
        }
    }
}
