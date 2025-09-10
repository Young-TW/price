use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct TwseResponse {
    #[serde(rename = "msgArray")]
    msg_array: Vec<TwseStock>,
}

#[derive(Deserialize, Debug)]
struct TwseStock {
    z: String, // Last traded price
    a: String, // Ask price (multiple prices separated by "_", take the first)
    b: String, // Bid price (multiple prices separated by "_", take the first)
    y: String, // Previous close
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
        .map_err(|e| format!("[TWSE] Failed to query price for {}: {}", symbol, e))?;

    if response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        let data: TwseResponse = serde_json::from_str(&text)
            .map_err(|e| format!("[TWSE] Returned JSON format error: {}\n{}", e, text))?;
        let stock = data
            .msg_array
            .get(0)
            .ok_or("[TWSE] Cannot find stock data")?;

        // Use last traded price if available
        if stock.z != "-" {
            stock.z
                .parse::<f64>()
                .map_err(|_| "[TWSE] Failed to parse price as float".to_string())
        } else {
            // Use geometric mean of ask and bid if last traded price is unavailable
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
                    // Use previous close if ask and bid cannot be parsed
                    stock.y
                        .parse::<f64>()
                        .map_err(|_| "[TWSE] Failed to parse previous close as float".to_string())
                }
            }
        }
    } else {
        Err(format!("[TWSE] HTTP error code: {}", response.status()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_price_from_twse() {
        let symbol = "2330"; // TSMC
        match get_price_from_twse(symbol).await {
            Ok(price) => println!("Price of {}: {}", symbol, price),
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}
