use std::collections::HashMap;
use std::fs;
use toml;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Portfolio(HashMap<String, HashMap<String, f64>>);

pub fn read_portfolio(path: &str) -> Result<HashMap<String, HashMap<String, f64>>, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("讀取檔案失敗: {}", e))?;
    let portfolio: Portfolio = toml::from_str(&content).map_err(|e| format!("TOML 解析失敗: {}", e))?;
    Ok(portfolio.0)
}

#[derive(Debug, Deserialize)]
pub struct ApiKeys(HashMap<String, String>);

pub fn read_api_keys(path: &str) -> Result<HashMap<String, String>, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("讀取檔案失敗: {}", e))?;
    let keys: ApiKeys = toml::from_str(&content).map_err(|e| format!("TOML 解析失敗: {}", e))?;
    Ok(keys.0)
}

pub fn read_target_forex(path: &str) -> Result<String, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("讀取檔案失敗: {}", e))?;
    let value: toml::Value = toml::from_str(&content).map_err(|e| format!("TOML 解析失敗: {}", e))?;
    value.get("target")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "未找到 target 欄位".to_string())
}
