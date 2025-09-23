use std::collections::HashMap;
use std::fs;
use toml;

use crate::types::{ApiKeys, Portfolio};

pub fn read_portfolio(path: &str) -> Result<HashMap<String, HashMap<String, f64>>, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let portfolio: Portfolio = toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))?;
    Ok(portfolio.0)
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
