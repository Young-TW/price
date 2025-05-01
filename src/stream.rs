// stream.rs — WebSocket 版本：以已解析的 portfolio Map 為輸入，推播即時價格

use colored::*;
use crossterm::{cursor, execute, terminal};
use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::HashMap;
use std::io::stdout;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};

use crate::api::pyth::spawn_price_stream;
use crate::get::get_price;

type SharedPriceMap = Arc<Mutex<HashMap<String, f64>>>;
type Portfolio = Arc<HashMap<String, HashMap<String, f64>>>;

/// 啟動完整串流：
/// * `portfolio`   — 已解析的資產組合 (Arc 共用)
/// * `cycle`       — 輪詢股/ETF 價格的秒數
/// * `tx`          — broadcast sender，推播給所有 WebSocket 客戶端
pub async fn stream(portfolio: Portfolio, cycle: u64, tx: broadcast::Sender<String>) {
    let prices: SharedPriceMap = Arc::new(Mutex::new(HashMap::new()));

    // 啟動 lazy_stream（加密貨幣即時串流）
    tokio::spawn({
        let p = portfolio.clone();
        let pr = prices.clone();
        async move { lazy_stream(p, pr).await; }
    });

    // 啟動 polling_stream（股票/ETF 輪詢）
    tokio::spawn({
        let p = portfolio.clone();
        let pr = prices.clone();
        async move { polling_stream(p, pr, cycle).await; }
    });

    // 主 loop：每秒整合並廣播一次總表
    let mut stdout = stdout();
    loop {
        execute!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .unwrap();

        let map = prices.lock().await;
        let mut total_value = 0.0;
        let mut buf = String::new();

        for (symbol, value) in map.iter() {
            buf.push_str(&format!("{symbol}: ${:.2}\n", value));
            total_value += value;
        }
        buf.push_str(&format!("\n總資產 (USD)：${:.2}", total_value));

        // 印在 CLI
        println!("{}", buf.replace('\n', "\n"));

        // 推播給所有 WS 客戶端
        let _ = tx.send(buf);

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// 加密貨幣：使用外部即時流直接更新 `prices`
async fn lazy_stream(portfolio: Portfolio, prices: SharedPriceMap) {
    if let Some(items) = portfolio.get("crypto") {
        for (symbol, _amount) in items { // 持倉量如要加權，可在外層處理
            let prices = prices.clone();
            let sym_owned = symbol.clone();
            spawn_price_stream(&sym_owned, "crypto", prices);
        }
    }
}

/// 輪詢股票/ETF 價格，每 `cycle` 秒更新一次 `prices`
async fn polling_stream(portfolio: Portfolio, prices: SharedPriceMap, cycle: u64) {
    loop {
        let mut tasks = FuturesUnordered::new();

        for category in ["us-stock", "us-etf", "tw-stock", "tw-etf"] {
            if let Some(items) = portfolio.get(category) {
                for (symbol, amount) in items {
                    let symbol = symbol.clone();
                    let category = category.to_string();
                    let amount = *amount;
                    tasks.push(async move {
                        match get_price(&symbol, &category).await {
                            Ok(price) => Some((symbol, amount, price)),
                            Err(_) => {
                                println!("無法獲取 {} 的價格", symbol);
                                None
                            }
                        }
                    });
                }
            }
        }

        while let Some(Some((sym, amount, price))) = tasks.next().await {
            let mut map = prices.lock().await;
            map.insert(sym, price * amount);
        }

        tokio::time::sleep(Duration::from_secs(cycle)).await;
    }
}
