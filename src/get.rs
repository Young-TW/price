// use crate::api::alpha_vantage::get_price_from_alpha_vantage;
// use crate::api::binance::get_price_from_binance;
use crate::api::redstone::get_price_from_redstone;
use crate::api::twse::get_price_from_twse;
use crate::api::yahoo::get_price_from_yahoo;

pub async fn get_price(symbol: &str, category: &str) -> Result<f64, String> {
    match category {
        /*
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
                "Failed to get crypto price (possibly due to API limit or invalid symbol: {})",
                symbol
            ));
        } */

        "US-Stock" | "US-ETF" => {
            /*
            if let Ok(price) = get_price_from_pyth(symbol).await {
                return Ok(price);
            }
            */
            if let Ok(price) = get_price_from_redstone(symbol).await {
                return Ok(price);
            }
            if let Ok(price) = get_price_from_yahoo(symbol).await {
                return Ok(price);
            }

            return Err(format!(
                "Failed to get US stock price (possibly due to API limit or invalid symbol: {})",
                symbol
            ));
        }

        "TW-Stock" | "TW-ETF" => {
            if let Ok(price) = get_price_from_twse(symbol).await {
                return Ok(price);
            }

            if let Ok(price) = get_price_from_yahoo(symbol).await {
                return Ok(price);
            }

            return Err(format!(
                "Failed to get Taiwan stock price (possibly due to API limit or invalid symbol: {})",
                symbol
            ));
        }

        _ => Err(format!("Unknown asset category: {}", symbol)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_get_price() {
        let price = get_price("AAPL", "US-Stock").await.unwrap();
        assert!(price > 0.0);
        let price = get_price("QQQ", "US-ETF").await.unwrap();
        assert!(price > 0.0);
        let price = get_price("2330", "TW-Stock").await.unwrap();
        assert!(price > 0.0);
        let price = get_price("0050", "TW-ETF").await.unwrap();
        assert!(price > 0.0);
        let price = get_price("eth", "Crypto").await;
        assert!(price.is_err()); // Crypto fetching is currently disabled
    }
}
