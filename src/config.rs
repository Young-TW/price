//! Reading and parsing of the TOML configuration files: portfolio holdings,
//! API keys and the display (target) currency.

use std::collections::HashMap;
use std::fs;
use toml;

use crate::types::{ApiKeys, Portfolio};

/// Read and parse the portfolio TOML file at `path`.
///
/// # Panics
///
/// Panics if the file cannot be read or its contents are not valid portfolio
/// TOML. Use [`try_read_portfolio`] for a non-panicking variant.
pub fn read_portfolio(path: &str) -> Portfolio {
    let content = fs::read_to_string(path).expect("Failed to read portfolio file");
    let portfolio: Portfolio = toml::from_str(&content).expect("Failed to parse portfolio TOML");
    portfolio
}

/// Like [`read_portfolio`] but returns an error instead of panicking, so a
/// transient bad edit picked up by the hot-reload watcher does not crash the
/// running program.
pub fn try_read_portfolio(path: &str) -> Result<Portfolio, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
    toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))
}

/// Read the API key TOML file at `path` into a `name -> key` map.
///
/// A missing file is not an error: it yields an empty map. Returns an `Err`
/// string if the file exists but cannot be read or parsed.
pub fn read_api_keys(path: &str) -> Result<HashMap<String, String>, String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        // A missing api key file is not fatal: callers that need a specific key
        // will surface a clear "key not found" error instead.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(format!("Failed to read file: {}", e)),
    };
    let keys: ApiKeys =
        toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))?;
    Ok(keys.0)
}

/// Read the display currency from the `target` field of the TOML file at `path`.
///
/// Returns an `Err` string if the file cannot be read or parsed, or if the
/// `target` field is missing or not a string.
pub fn read_target_forex(path: &str) -> Result<String, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let value: toml::Value =
        toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))?;
    value
        .get("target")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "Target field not found".to_string())
}

/// Read the display currency, falling back to USD (with a diagnostic) when the
/// file is missing or unparseable. Used for both the initial load and reloads.
pub fn read_target_forex_or_default(path: &str) -> String {
    match read_target_forex(path) {
        Ok(forex) => forex,
        Err(e) => {
            crate::log_line!(
                "[config] {} not usable ({}); defaulting target forex to USD",
                path,
                e
            );
            "USD".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_read_portfolio() {
        let portfolio = read_portfolio("test/portfolio.toml");
        assert!(portfolio.get("US-Stock").is_some());
        assert!(portfolio.get("US-ETF").is_some());
        assert!(portfolio.get("TW-Stock").is_some());
        assert!(portfolio.get("TW-ETF").is_some());
        assert!(portfolio.get("Crypto").is_some());
        assert!(portfolio.get("Forex").is_some());
    }

    #[test]
    fn test_read_api_keys() {
        let api_keys = read_api_keys("test/api_key.toml").unwrap();
        assert!(api_keys.contains_key("alpha_vantage_api_key"));
    }

    #[test]
    fn test_read_target_forex() {
        let target = read_target_forex("test/target_forex.toml").unwrap();
        assert_eq!(target, "TWD");
    }
}
