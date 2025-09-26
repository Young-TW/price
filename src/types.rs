use std::collections::HashMap;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Portfolio(pub HashMap<String, HashMap<String, f64>>);

#[derive(Debug, Deserialize)]
pub struct ApiKeys(pub HashMap<String, String>);

#[derive(Debug, Deserialize)]
pub struct PriceResponse {
    pub price: f64,
    pub source: String,
    pub symbol: String,
    pub category: String,
    pub timestamp: String,
    pub error: Option<String>,
}