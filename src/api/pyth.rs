use reqwest::Client;
use serde::Deserialize;

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
pub async fn get_price_from_pyth(symbol: &str) -> Result<f64, String> {
    let feed_id = get_pyth_feed_id(symbol, "crypto").await?
        .ok_or_else(|| format!("[Pyth] 找不到資產：{}", symbol))?;

    let url = format!("{}/api/latest_price_feeds?ids[]={}", BASE_URL, feed_id);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(&url).send().await.map_err(|e| {
        format!("[Pyth] 查詢 {} 價格失敗：{}", symbol, e)
    })?;

    let data: Vec<PythPriceEntry> = response.json().await.map_err(|e| {
        format!("[Pyth] JSON 格式錯誤：{}", e)
    })?;

    let entry = data.get(0).ok_or("[Pyth] 無價格資料")?;
    let value = entry.price.price as f64 * 10f64.powi(entry.price.expo);
    Ok(value)
}

/// 查詢對應 symbol 的 feed_id
pub async fn get_pyth_feed_id(symbol: &str, asset_type: &str) -> Result<Option<String>, String> {
    let url = format!("{}/api/price_feeds", BASE_URL);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(&url).send().await.map_err(|e| {
        format!("[Pyth] 查詢 feed 失敗：{}", e)
    })?;

    let feeds: Vec<PythFeed> = response.json().await.map_err(|e| {
        format!("[Pyth] Feed JSON 格式錯誤：{}", e)
    })?;

    for feed in feeds {
        if feed.product.base.eq_ignore_ascii_case(symbol)
            && feed.product.asset_type.eq_ignore_ascii_case(asset_type)
        {
            return Ok(Some(feed.id));
        }
    }

    Ok(None)
}
