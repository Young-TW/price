//! Core data types: the portfolio holdings model, historical snapshots and the
//! API response/key structures used across the crate.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// A flat list of portfolio holdings.
///
/// Deserialized from a nested TOML table of `category -> { symbol -> quantity }`
/// and flattened into one [`PortfolioItem`] per `(category, symbol)` pair.
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
    /// Borrowing iterator over the holdings in storage order.
    pub fn iter(&self) -> std::slice::Iter<'_, PortfolioItem> {
        self.0.iter()
    }

    /// Group the holdings by their `category` field, cloning each item into the
    /// bucket for its category.
    ///
    /// ```
    /// use price::types::{Portfolio, PortfolioItem};
    /// let p = Portfolio(vec![
    ///     PortfolioItem { symbol: "AAPL".into(), category: "US-Stock".into(), quantity: 1.0 },
    ///     PortfolioItem { symbol: "VOO".into(),  category: "US-Stock".into(), quantity: 2.0 },
    ///     PortfolioItem { symbol: "BTC".into(),  category: "Crypto".into(),   quantity: 3.0 },
    /// ]);
    /// let groups = p.group_by_category();
    /// assert_eq!(groups["US-Stock"].len(), 2);
    /// assert_eq!(groups["Crypto"].len(), 1);
    /// ```
    pub fn group_by_category(&self) -> HashMap<String, Vec<PortfolioItem>> {
        let mut map: HashMap<String, Vec<PortfolioItem>> = HashMap::new();
        for item in &self.0 {
            map.entry(item.category.clone()).or_default().push(item.clone());
        }
        map
    }

    /// All holdings in `category`, or `None` if no holding has that category.
    ///
    /// ```
    /// use price::types::{Portfolio, PortfolioItem};
    /// let p = Portfolio(vec![
    ///     PortfolioItem { symbol: "AAPL".into(), category: "US-Stock".into(), quantity: 1.0 },
    /// ]);
    /// assert!(p.get("US-Stock").is_some());
    /// assert!(p.get("Crypto").is_none());
    /// ```
    pub fn get(&self, category: &str) -> Option<Vec<PortfolioItem>> {
        self.group_by_category().get(category).cloned()
    }
}

/// A single portfolio holding.
#[derive(Debug, Deserialize, Clone)]
pub struct PortfolioItem {
    /// Asset symbol (e.g. `AAPL`, `2330`, `BTC`, `TWD`).
    pub symbol: String,
    /// Asset category (e.g. `US-Stock`, `TW-Stock`, `Crypto`, `Forex`).
    pub category: String,
    /// Number of units held.
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

/// API credentials, deserialized from a flat TOML table of `name -> key`.
#[derive(Debug, Deserialize)]
pub struct ApiKeys(pub HashMap<String, String>);

/// A single price quote with its provenance and an optional error.
#[derive(Debug, Deserialize)]
pub struct PriceResponse {
    /// Quoted price.
    pub price: f64,
    /// Name of the data source that produced the price.
    pub source: String,
    /// Asset symbol the price is for.
    pub symbol: String,
    /// Asset category the price is for.
    pub category: String,
    /// Timestamp of the quote, as a string.
    pub timestamp: String,
    /// Error message if the quote could not be obtained, otherwise `None`.
    pub error: Option<String>,
}