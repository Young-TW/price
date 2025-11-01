use std::collections::HashMap;
use std::fs;
use toml;

use crate::types::{ApiKeys, Portfolio};

pub fn read_portfolio(path: &str) -> Portfolio {
    let content = fs::read_to_string(path).expect("Failed to read portfolio file");
    let portfolio: Portfolio = toml::from_str(&content).expect("Failed to parse portfolio TOML");
    portfolio
}

pub fn read_api_keys(path: &str) -> Result<HashMap<String, String>, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let keys: ApiKeys = toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))?;
    Ok(keys.0)
}

pub fn read_target_forex(path: &str) -> Result<String, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let value: toml::Value = toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))?;
    value.get("target")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "Target field not found".to_string())
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
