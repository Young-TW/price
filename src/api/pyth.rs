use reqwest::Client;
use serde::Deserialize;
use eventsource_client::{ClientBuilder, SSE};
use eventsource_client::Client as EventSourceClient; // 避免與 reqwest::Client 衝突
use futures::StreamExt;
use serde_json::Value;
use std::error::Error;
use std::fs;

const BASE_URL: &str = "https://hermes.pyth.network";

#[derive(Debug, Deserialize)]
struct PythFeed {
    id: String,
    product: PythProduct,
}

#[derive(Debug, Deserialize)]
struct PythProduct {
    base: String,
    #[serde(rename = "asset_type")]
    asset_type: String,
}

#[derive(Debug, Deserialize)]
struct PythPriceEntry {
    price: PythPrice,
}

#[derive(Debug, Deserialize)]
struct PythPrice {
    price: i64,
    expo: i32,
}

/// 查詢最新價格（會自動找 feed id）
pub async fn get_price_from_pyth(id: &str) -> Result<f64, String> {
    let url = format!("{}/api/latest_price_feeds?ids[]={}", BASE_URL, id);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(&url).send().await.map_err(|e| {
        format!("[Pyth] 查詢 {} 價格失敗：{}", id, e)
    })?;

    let data: Vec<PythPriceEntry> = response.json().await.map_err(|e| {
        format!("[Pyth] JSON 格式錯誤：{}", e)
    })?;

    let entry = data.get(0).ok_or("[Pyth] 無價格資料")?;
    let value = entry.price.price as f64 * 10f64.powi(entry.price.expo);
    Ok(value)
}

/// 訂閱 Pyth 即時價格串流，並將價格回傳給 callback 函數。
///
/// # 參數
/// - `id`: Pyth price feed 的 ID（hex 字串）
/// - `on_price`: 回呼函數，接收實際價格（`f64`）
///
/// # 範例
/// pyth_stream::subscribe_price_stream("0xe62d...", |price| println!("價格: {}", price)).await;
pub async fn get_price_stream_from_pyth<F>(id: &str, mut on_price: F) -> Result<(), Box<dyn Error>>
where
    F: FnMut(f64) + Send + 'static,
{
    let url = format!(
        "{}/v2/updates/price/stream?ids[]={}",
        BASE_URL,
        id
    );

    let mut stream = ClientBuilder::for_url(&url)?.build().stream();

    while let Some(event) = stream.next().await {
        match event {
            Ok(SSE::Event(ev)) => {
                if let Ok(json) = serde_json::from_str::<Value>(&ev.data) {
                    if let Some(parsed_array) = json.get("parsed").and_then(|v| v.as_array()) {
                        for entry in parsed_array {
                            if let Some(price_obj) = entry.get("price") {
                                if let (Some(price_str), Some(expo)) = (
                                    price_obj.get("price").and_then(|p| p.as_str()),
                                    price_obj.get("expo").and_then(|e| e.as_i64())
                                ) {
                                    if let Ok(price_int) = price_str.parse::<f64>() {
                                        let actual_price = price_int * 10f64.powi(expo as i32);
                                        on_price(actual_price);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(_) => {} // 略過 Ping/Comment
            Err(e) => {
                eprintln!("SSE 錯誤: {}", e);
            }
        }
    }

    Ok(())
}

pub async fn get_pyth_feed_id(symbol: &str, category: &str) -> String {
    let data = fs::read_to_string("src/api/data/pyth.toml")
        .expect("無法讀取 Pyth 配置檔案");
    let pairs: toml::Value = toml::from_str(&data)
        .expect("無法解析 Pyth 配置檔案");
    let feeds = pairs.get("id").expect("無法找到 feeds");
    let feed_id = feeds.get(symbol).expect("無法找到 feed_id");
    return feed_id.to_string();
}
