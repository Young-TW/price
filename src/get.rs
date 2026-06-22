//! Category-aware price and history lookup that selects and falls back across
//! the provider APIs.

// use crate::api::alpha_vantage::get_price_from_alpha_vantage;
// use crate::api::binance::get_price_from_binance;
use crate::api::pyth::{get_history_from_pyth, pyth_tv_symbol};
use crate::api::redstone::get_price_from_redstone;
use crate::api::twse::get_price_from_twse;
use crate::api::yahoo::{get_history_from_yahoo_range, get_price_from_yahoo};

/// Try `primary`; on failure try `secondary`; if both fail return
/// `Err(err_msg(symbol))`.
///
/// Accepting `AsyncFn` bounds lets callers (and tests) inject any async
/// callable — a real API function, an async closure stub, or a spy — without
/// touching the fallback logic.
async fn get_price_with_fetchers(
    symbol: &str,
    primary: impl AsyncFn(&str) -> Result<f64, String>,
    secondary: impl AsyncFn(&str) -> Result<f64, String>,
    err_msg: impl Fn(&str) -> String,
) -> Result<f64, String> {
    if let Ok(price) = primary(symbol).await {
        return Ok(price);
    }
    if let Ok(price) = secondary(symbol).await {
        return Ok(price);
    }
    Err(err_msg(symbol))
}

/// Fetch the current price of `symbol` for the given asset `category`.
///
/// `US-Stock`/`US-ETF` try RedStone then Yahoo; `TW-Stock`/`TW-ETF` try TWSE
/// then Yahoo. Returns an `Err` string for an unknown category, for `Crypto`
/// (currently disabled), or when every source for the category fails.
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
            get_price_with_fetchers(
                symbol,
                get_price_from_redstone,
                get_price_from_yahoo,
                |s| format!(
                    "Failed to get US stock price (possibly due to API limit or invalid symbol: {})",
                    s
                ),
            )
            .await
        }

        "TW-Stock" | "TW-ETF" => {
            get_price_with_fetchers(
                symbol,
                get_price_from_twse,
                get_price_from_yahoo,
                |s| format!(
                    "Failed to get Taiwan stock price (possibly due to API limit or invalid symbol: {})",
                    s
                ),
            )
            .await
        }

        _ => Err(format!("Unknown asset category: {}", category)),
    }
}

/// Fetch historical daily close prices for a holding between `from` and `to`
/// (unix epoch seconds). Pyth Benchmarks is the primary source for crypto, US
/// equities/ETFs and forex; Taiwan equities fall back to Yahoo since Pyth does
/// not cover them. Returns `(timestamp, close)` pairs.
pub async fn get_history(
    symbol: &str,
    category: &str,
    from: i64,
    to: i64,
) -> Result<Vec<(i64, f64)>, String> {
    match category {
        "Crypto" | "US-Stock" | "US-ETF" | "Forex" => {
            let tv_symbol = pyth_tv_symbol(symbol, category)
                .ok_or_else(|| format!("No Pyth symbol mapping for {} ({})", symbol, category))?;
            get_history_from_pyth(&tv_symbol, from, to).await
        }

        "TW-Stock" | "TW-ETF" => {
            // Pyth has no Taiwan equities; Yahoo needs the .TW suffix.
            let yahoo_symbol = format!("{}.TW", symbol);
            get_history_from_yahoo_range(&yahoo_symbol, from, to, "1d").await
        }

        _ => Err(format!("Unknown asset category: {}", category)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live_price_tests_enabled() -> bool {
        matches!(std::env::var("RUN_LIVE_PRICE_TESTS").as_deref(), Ok("1"))
    }

    #[tokio::test]
    async fn test_get_price() {
        if live_price_tests_enabled() {
            let price = get_price("AAPL", "US-Stock").await.unwrap();
            assert!(price > 0.0);
            let price = get_price("QQQ", "US-ETF").await.unwrap();
            assert!(price > 0.0);
            let price = get_price("2330", "TW-Stock").await.unwrap();
            assert!(price > 0.0);
            let price = get_price("0050", "TW-ETF").await.unwrap();
            assert!(price > 0.0);
        }

        let price = get_price("eth", "Crypto").await;
        assert!(price.is_err()); // Crypto fetching is currently disabled
        let price = get_price("AAPL", "Unknown").await;
        assert!(price.is_err());
    }

    #[tokio::test]
    async fn test_get_history_tw_respects_range() {
        if !live_price_tests_enabled() {
            return;
        }
        let to = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let from = to - 30 * 86_400;

        for (symbol, category) in [("2330", "TW-Stock"), ("0050", "TW-ETF")] {
            let series = get_history(symbol, category, from, to).await.unwrap();
            assert!(
                !series.is_empty(),
                "{} ({}) returned no history",
                symbol,
                category
            );
            for (ts, _) in &series {
                assert!(*ts >= from, "{}: timestamp {} < from {}", symbol, ts, from);
                assert!(*ts <= to, "{}: timestamp {} > to {}", symbol, ts, to);
            }
        }
    }

    // --- Fallback-chain unit tests (deterministic, no network) ---

    // US-Stock / US-ETF: Redstone → Yahoo

    #[tokio::test]
    async fn test_us_primary_succeeds_secondary_not_called() {
        let result = get_price_with_fetchers(
            "AAPL",
            async |_s| Ok::<f64, String>(150.0),
            async |_s| -> Result<f64, String> {
                panic!("secondary must not be called when primary succeeds")
            },
            |s| format!("error: {}", s),
        )
        .await;
        assert_eq!(result, Ok(150.0));
    }

    #[tokio::test]
    async fn test_us_primary_fails_secondary_called_and_succeeds() {
        let result = get_price_with_fetchers(
            "AAPL",
            async |_s| -> Result<f64, String> { Err("redstone unavailable".into()) },
            async |_s| Ok::<f64, String>(200.0),
            |s| format!("error: {}", s),
        )
        .await;
        assert_eq!(result, Ok(200.0));
    }

    #[tokio::test]
    async fn test_us_both_sources_fail_returns_err() {
        let result = get_price_with_fetchers(
            "AAPL",
            async |_s| -> Result<f64, String> { Err("redstone unavailable".into()) },
            async |_s| -> Result<f64, String> { Err("yahoo unavailable".into()) },
            |s| format!("Failed to get US stock price for {}", s),
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AAPL"));
    }

    // TW-Stock / TW-ETF: TWSE → Yahoo

    #[tokio::test]
    async fn test_tw_primary_succeeds_secondary_not_called() {
        let result = get_price_with_fetchers(
            "2330",
            async |_s| Ok::<f64, String>(600.0),
            async |_s| -> Result<f64, String> {
                panic!("secondary must not be called when primary succeeds")
            },
            |s| format!("error: {}", s),
        )
        .await;
        assert_eq!(result, Ok(600.0));
    }

    #[tokio::test]
    async fn test_tw_primary_fails_secondary_called_and_succeeds() {
        let result = get_price_with_fetchers(
            "2330",
            async |_s| -> Result<f64, String> { Err("twse unavailable".into()) },
            async |_s| Ok::<f64, String>(610.0),
            |s| format!("error: {}", s),
        )
        .await;
        assert_eq!(result, Ok(610.0));
    }

    #[tokio::test]
    async fn test_tw_both_sources_fail_returns_err() {
        let result = get_price_with_fetchers(
            "2330",
            async |_s| -> Result<f64, String> { Err("twse unavailable".into()) },
            async |_s| -> Result<f64, String> { Err("yahoo unavailable".into()) },
            |s| format!("Failed to get Taiwan stock price for {}", s),
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("2330"));
    }
}
