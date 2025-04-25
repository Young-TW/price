use crate::api::alpha_vantage::get_price_from_alpha_vantage;
use crate::api::binance::get_price_from_binance;
use crate::api::pyth::{
    get_price_from_pyth, get_pyth_feed_id, get_price_stream_from_pyth,
};
use crate::api::redstone::get_price_from_redstone;
use crate::api::twse::get_price_from_twse;
use crate::api::yahoo::get_price_from_yahoo;

pub async fn get_price(symbol: &str, category: &str) -> Result<f64, String> {
    match category {
        "crypto" => {
            if let Ok(price) = get_price_from_pyth(symbol).await {
                return Ok(price);
            }
            if let Ok(price) = get_price_from_redstone(symbol).await {
                return Ok(price);
            }
            if let Ok(price) = get_price_from_binance(symbol).await {
                return Ok(price);
            }
            if let Ok(price) = get_price_from_alpha_vantage(symbol).await {
                return Ok(price);
            }

            return Err(format!(
                "無法取得加密貨幣價格 (可能為 API 限制或錯誤 symbol: {})",
                symbol
            ));
        }

        "us-stock" | "us-etf" => {
            if let Ok(price) = get_price_from_pyth(symbol).await {
                return Ok(price);
            }
            if let Ok(price) = get_price_from_redstone(symbol).await {
                return Ok(price);
            }
            if let Ok(price) = get_price_from_yahoo(symbol).await {
                return Ok(price);
            }

            return Err(format!(
                "無法取得美股價格 (可能為 API 限制或錯誤 symbol: {})",
                symbol
            ));
        }

        "tw-stock" | "tw-etf" => {
            if let Ok(price) = get_price_from_twse(symbol).await {
                return Ok(price);
            }

            if let Ok(price) = get_price_from_yahoo(symbol).await {
                return Ok(price);
            }

            return Err(format!(
                "無法取得台灣股票價格 (可能為 API 限制或錯誤 symbol: {})",
                symbol
            ));
        }

        _ => Err(format!("未知的資產類別：{}", symbol)),
    }
}
