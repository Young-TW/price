use crate::api::pyth::get_price_from_pyth;
use crate::api::alpha_vantage::get_price_from_alpha_vantage;
use crate::api::binance::get_price_from_binance;
use crate::api::redstone::get_price_from_redstone;
use crate::api::yahoo::get_price_from_yahoo;
use crate::api::exchangerate::get_rate;

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

            return // fail
                Err(format!(
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

            return // fail
                Err(format!(
                    "無法取得美股價格 (可能為 API 限制或錯誤 symbol: {})",
                    symbol
                ));
        }

        "tw-stock" | "tw-etf" => {
            let tw_price = get_price_from_yahoo(symbol).await?;
            let rate = get_rate("TWD", "USD").await?;
            Ok(tw_price * rate)
        }

        _ => Err(format!("未知的資產類別：{}", symbol)),
    }
}
