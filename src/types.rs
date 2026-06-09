use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Portfolio(pub Vec<PortfolioItem>);

impl<'de> Deserialize<'de> for Portfolio {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw: HashMap<String, HashMap<String, f64>> = HashMap::deserialize(deserializer)?;
        let mut items = Vec::new();

        for (category, symbols) in raw {
            for (symbol, quantity) in symbols {
                items.push(PortfolioItem {
                    symbol,
                    category: category.clone(),
                    quantity,
                });
            }
        }

        Ok(Portfolio(items))
    }
}

impl Portfolio {
    pub fn iter(&self) -> std::slice::Iter<'_, PortfolioItem> {
        self.0.iter()
    }

    pub fn group_by_category(&self) -> HashMap<String, Vec<PortfolioItem>> {
        let mut map: HashMap<String, Vec<PortfolioItem>> = HashMap::new();
        for item in &self.0 {
            map.entry(item.category.clone()).or_default().push(item.clone());
        }
        map
    }

    pub fn get(&self, category: &str) -> Option<Vec<PortfolioItem>> {
        self.group_by_category().get(category).cloned()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct PortfolioItem {
    pub symbol: String,
    pub category: String,
    pub quantity: f64,
}

/// A point-in-time snapshot of the portfolio, used to build historical
/// price and allocation series. Persisted as one JSON line per snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    /// Unix epoch seconds.
    pub timestamp: i64,
    /// Total portfolio value expressed in USD.
    pub total_value_usd: f64,
    /// Category -> USD value. Allocation ratio = value / total_value_usd.
    pub category_values: HashMap<String, f64>,
    /// Per-symbol price at this point in time (the historical price).
    pub prices: HashMap<String, f64>,
}

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