//! Portfolio snapshot history: computing category values, recording snapshots
//! to the JSONL store, loading them back and exporting to CSV.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chrono::Utc;

use crate::types::{Portfolio, PortfolioSnapshot};

/// Compute the USD value of each portfolio category given a price map.
///
/// This is the single source of truth for the "value an item in USD" logic and
/// is reused by the live allocation view, the snapshot recorder and the
/// historical back-fill. Forex holdings are merged into a synthetic `Cash`
/// category to match the live allocation display.
///
/// Returns `(category -> usd value, total usd value)`. Missing prices count as 0.
pub fn compute_category_values(
    portfolio: &Portfolio,
    map: &HashMap<String, f64>,
) -> (HashMap<String, f64>, f64) {
    let mut categories: HashMap<String, f64> = HashMap::new();
    let mut total = 0.0;

    for (category, items) in portfolio.group_by_category() {
        let mut category_value = 0.0;

        for item in &items {
            let usd_value = if category == "TW-Stock" || category == "TW-ETF" {
                match (map.get(&item.symbol), map.get("USD/TWD")) {
                    (Some(price), Some(rate)) if *rate != 0.0 => price * item.quantity / rate,
                    _ => 0.0,
                }
            } else if category == "Forex" {
                if item.symbol == "USD" {
                    item.quantity
                } else {
                    match map.get(&format!("USD/{}", item.symbol)) {
                        Some(rate) if *rate != 0.0 => item.quantity / rate,
                        _ => 0.0,
                    }
                }
            } else {
                // Crypto, US-Stock, US-ETF are already priced in USD.
                map.get(&item.symbol)
                    .map(|p| p * item.quantity)
                    .unwrap_or(0.0)
            };

            category_value += usd_value;
        }

        if category_value > 0.0 {
            // Merge cash-like holdings under a single "Cash" bucket.
            let key = if category == "Forex" {
                "Cash".to_string()
            } else {
                category
            };
            *categories.entry(key).or_insert(0.0) += category_value;
            total += category_value;
        }
    }

    (categories, total)
}

/// The price-map keys required to fully value `portfolio`.
///
/// `compute_category_values` silently treats a missing price as zero, so a
/// snapshot built before every one of these keys is present would understate
/// the total (and skew allocation ratios). Callers use [`is_complete`] to skip
/// such partial snapshots instead of persisting the distortion.
pub fn required_price_keys(portfolio: &Portfolio) -> Vec<String> {
    let mut keys = Vec::new();
    let mut needs_twd = false;

    for item in portfolio.iter() {
        match item.category.as_str() {
            "TW-Stock" | "TW-ETF" => {
                keys.push(item.symbol.clone());
                needs_twd = true; // Taiwan equities are priced in TWD.
            }
            "Forex" => {
                if item.symbol != "USD" {
                    keys.push(format!("USD/{}", item.symbol));
                }
            }
            // Crypto, US-Stock, US-ETF are priced directly in USD.
            _ => keys.push(item.symbol.clone()),
        }
    }

    if needs_twd {
        keys.push("USD/TWD".to_string());
    }

    keys.sort();
    keys.dedup();
    keys
}

/// Whether `map` holds a non-zero price for every key needed to fully value
/// `portfolio`. Only complete price maps yield meaningful snapshots.
pub fn is_complete(portfolio: &Portfolio, map: &HashMap<String, f64>) -> bool {
    required_price_keys(portfolio)
        .iter()
        .all(|key| map.get(key).is_some_and(|v| *v != 0.0))
}

/// Build a snapshot of the portfolio from the current price map.
pub fn take_snapshot(portfolio: &Portfolio, map: &HashMap<String, f64>) -> PortfolioSnapshot {
    let (category_values, total_value_usd) = compute_category_values(portfolio, map);
    PortfolioSnapshot {
        timestamp: Utc::now().timestamp(),
        total_value_usd,
        category_values,
        prices: map.clone(),
    }
}

/// Snapshots newer than this (relative to "now") are kept at full resolution;
/// older ones are thinned to the last snapshot of each UTC day. Live snapshots
/// are recorded every few minutes, so without this the file would grow without
/// bound during a long-running deployment.
pub const HIGH_RES_WINDOW_SECS: i64 = 7 * 86_400;

/// Bound history growth by keeping full resolution only within
/// [`HIGH_RES_WINDOW_SECS`] of `now` and collapsing older snapshots to one per
/// UTC day (the latest of that day). Input need not be sorted; output is sorted
/// ascending by timestamp.
///
/// This preserves the year of daily back-fill while capping the high-frequency
/// live snapshots to a recent window, so both the file and the in-memory Vec
/// stay bounded no matter how long the program runs.
pub fn downsample(mut snapshots: Vec<PortfolioSnapshot>, now: i64) -> Vec<PortfolioSnapshot> {
    snapshots.sort_by_key(|s| s.timestamp);
    let cutoff = now - HIGH_RES_WINDOW_SECS;

    let mut out: Vec<PortfolioSnapshot> = Vec::with_capacity(snapshots.len());
    let mut last_old_day: Option<i64> = None;

    for snap in snapshots {
        if snap.timestamp >= cutoff {
            out.push(snap); // Recent: keep every snapshot.
            continue;
        }
        // Old: keep only the last snapshot of each day. Since the input is
        // sorted ascending and all old entries precede the recent ones, the
        // running tail is always the current day's latest so far.
        let day = snap.timestamp.div_euclid(86_400);
        if last_old_day == Some(day) {
            *out.last_mut().expect("old-day entry exists") = snap;
        } else {
            last_old_day = Some(day);
            out.push(snap);
        }
    }

    out
}

/// Load all snapshots from the JSON-Lines history file. Returns an empty vector
/// if the file does not exist. Malformed lines are skipped.
pub fn load_history(path: &str) -> Vec<PortfolioSnapshot> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut snapshots: Vec<PortfolioSnapshot> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    snapshots.sort_by_key(|s| s.timestamp);
    snapshots
}

/// Overwrite the history file with the full set of snapshots (one JSON line
/// each). Used after a back-fill merge to keep the file de-duplicated.
pub fn save_all(path: &str, history: &[PortfolioSnapshot]) -> Result<(), String> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create data dir: {}", e))?;
    }

    let mut out = String::new();
    for snap in history {
        let line = serde_json::to_string(snap)
            .map_err(|e| format!("Failed to serialize snapshot: {}", e))?;
        out.push_str(&line);
        out.push('\n');
    }

    fs::write(path, out).map_err(|e| format!("Failed to write history file: {}", e))
}

/// Merge new snapshots into an existing (timestamp-sorted) history, dropping
/// duplicates that fall on the same UTC day, then return the sorted result.
/// Existing entries win over new ones for the same day.
pub fn merge_snapshots(
    existing: Vec<PortfolioSnapshot>,
    incoming: Vec<PortfolioSnapshot>,
) -> Vec<PortfolioSnapshot> {
    let mut by_day: HashMap<i64, PortfolioSnapshot> = HashMap::new();

    // Incoming first so existing entries overwrite them on collision.
    for snap in incoming.into_iter().chain(existing.into_iter()) {
        let day = snap.timestamp.div_euclid(86_400);
        by_day.insert(day, snap);
    }

    let mut merged: Vec<PortfolioSnapshot> = by_day.into_values().collect();
    merged.sort_by_key(|s| s.timestamp);
    merged
}

/// Export the history as CSV (timestamp, total_value_usd, then one column per
/// category) for analysis in external tools.
pub fn export_csv(history: &[PortfolioSnapshot], path: &str) -> Result<(), String> {
    // Collect the union of category names for a stable header.
    let mut categories: Vec<String> = history
        .iter()
        .flat_map(|s| s.category_values.keys().cloned())
        .collect();
    categories.sort();
    categories.dedup();

    let mut out = String::from("timestamp,total_value_usd");
    for cat in &categories {
        out.push(',');
        out.push_str(cat);
    }
    out.push('\n');

    for snap in history {
        out.push_str(&format!("{},{:.4}", snap.timestamp, snap.total_value_usd));
        for cat in &categories {
            let v = snap.category_values.get(cat).copied().unwrap_or(0.0);
            out.push_str(&format!(",{:.4}", v));
        }
        out.push('\n');
    }

    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {}", e))?;
    }
    fs::write(path, out).map_err(|e| format!("Failed to write CSV: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(symbol: &str, category: &str, quantity: f64) -> crate::types::PortfolioItem {
        crate::types::PortfolioItem {
            symbol: symbol.to_string(),
            category: category.to_string(),
            quantity,
        }
    }

    #[test]
    fn test_compute_category_values() {
        let portfolio = Portfolio(vec![
            item("AAPL", "US-Stock", 10.0),
            item("2330", "TW-Stock", 10.0),
            item("USD", "Forex", 100.0),
            item("TWD", "Forex", 3000.0),
        ]);

        let mut map = HashMap::new();
        map.insert("AAPL".to_string(), 200.0); // 2000 USD
        map.insert("2330".to_string(), 600.0); // 6000 TWD
        map.insert("USD/TWD".to_string(), 30.0); // -> 200 USD (stock), 100 USD (cash)

        let (cats, total) = compute_category_values(&portfolio, &map);
        assert!((cats["US-Stock"] - 2000.0).abs() < 1e-6);
        assert!((cats["TW-Stock"] - 200.0).abs() < 1e-6);
        // Cash = 100 USD + 3000 TWD / 30 = 100 + 100 = 200
        assert!((cats["Cash"] - 200.0).abs() < 1e-6);
        assert!((total - 2400.0).abs() < 1e-6);
    }

    #[test]
    fn test_is_complete_requires_every_holding() {
        let portfolio = Portfolio(vec![
            item("AAPL", "US-Stock", 10.0),
            item("2330", "TW-Stock", 10.0),
            item("USD", "Forex", 100.0),
            item("TWD", "Forex", 3000.0),
        ]);

        let mut map = HashMap::new();
        map.insert("AAPL".to_string(), 200.0);
        map.insert("2330".to_string(), 600.0);
        // USD/TWD still missing -> the TW stock and TWD cash can't be valued.
        assert!(!is_complete(&portfolio, &map));

        map.insert("USD/TWD".to_string(), 30.0);
        assert!(is_complete(&portfolio, &map));

        // USD cash needs no rate; a zero rate counts as missing.
        map.insert("USD/TWD".to_string(), 0.0);
        assert!(!is_complete(&portfolio, &map));
    }

    #[test]
    fn test_roundtrip_persistence() {
        let dir = std::env::temp_dir().join(format!("price_hist_{}", std::process::id()));
        let path = dir.join("history.jsonl");
        let path_str = path.to_str().unwrap();
        let _ = fs::remove_file(path_str);

        let snaps = vec![
            PortfolioSnapshot {
                timestamp: 1_700_000_000,
                total_value_usd: 1234.5,
                category_values: HashMap::from([("US-Stock".to_string(), 1234.5)]),
                prices: HashMap::from([("AAPL".to_string(), 123.45)]),
            },
            PortfolioSnapshot {
                timestamp: 1_700_086_400,
                total_value_usd: 1234.5,
                category_values: HashMap::from([("US-Stock".to_string(), 1234.5)]),
                prices: HashMap::from([("AAPL".to_string(), 123.45)]),
            },
        ];
        save_all(path_str, &snaps).unwrap();

        let loaded = load_history(path_str);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].timestamp, 1_700_000_000);
        assert!((loaded[1].total_value_usd - 1234.5).abs() < 1e-6);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_downsample_thins_old_keeps_recent() {
        let mk = |ts: i64| PortfolioSnapshot {
            timestamp: ts,
            total_value_usd: ts as f64, // unique marker per snapshot
            category_values: HashMap::new(),
            prices: HashMap::new(),
        };

        let now = 100 * 86_400;
        // Old day (day 1): three intra-day snapshots -> collapse to the last.
        // Old day (day 2): one snapshot -> kept.
        // Recent (within 7 days of now): two snapshots -> both kept.
        let recent_a = now - 2 * 86_400;
        let recent_b = now - 1 * 86_400 + 100;
        let input = vec![
            mk(1 * 86_400 + 10),
            mk(1 * 86_400 + 20),
            mk(1 * 86_400 + 30),
            mk(2 * 86_400 + 5),
            mk(recent_b),
            mk(recent_a),
        ];

        let out = downsample(input, now);
        let ts: Vec<i64> = out.iter().map(|s| s.timestamp).collect();
        assert_eq!(
            ts,
            vec![1 * 86_400 + 30, 2 * 86_400 + 5, recent_a, recent_b]
        );
    }

    #[test]
    fn test_merge_dedupes_by_day() {
        let mk = |ts: i64, v: f64| PortfolioSnapshot {
            timestamp: ts,
            total_value_usd: v,
            category_values: HashMap::new(),
            prices: HashMap::new(),
        };
        // Two snapshots on the same UTC day; existing should win.
        let existing = vec![mk(1_700_000_000, 100.0)];
        let incoming = vec![mk(1_700_003_600, 999.0), mk(1_700_200_000, 50.0)];
        let merged = merge_snapshots(existing, incoming);
        assert_eq!(merged.len(), 2);
        assert!((merged[0].total_value_usd - 100.0).abs() < 1e-6);
    }
}
