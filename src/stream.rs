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
pub async fn stream(
    portfolio: Arc<Mutex<Option<HashMap<String, HashMap<String, f64>>>>>,
    cycle: u64,
    tx: broadcast::Sender<String>
) {
    // 這裡不要再巢狀 loop
    loop {
        // 只 lock 一下，馬上 clone 出來
        let pf_opt = {
            let pf_lock = portfolio.lock().await;
            pf_lock.clone()
        };

        if let Some(ref pf) = pf_opt {
            let prices: SharedPriceMap = Arc::new(Mutex::new(HashMap::new()));
            let pf_arc = Arc::new(pf.clone());

            // 這裡直接 await，等這一輪 lazy/polling 結束再進下一輪
            let lazy = tokio::spawn({
                let p = pf_arc.clone();
                let pr = prices.clone();
                async move { lazy_stream(p, pr).await; }
            });

            let polling = tokio::spawn({
                let p = pf_arc.clone();
                let pr = prices.clone();
                async move { polling_stream(p, pr, cycle).await; }
            });

            // 主 loop：每秒整合並廣播一次總表，這裡只跑一輪
            let mut stdout = stdout();
            for _ in 0..cycle {
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

            // 結束這一輪後，abort 掉 lazy/polling
            lazy.abort();
            polling.abort();
        } else {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
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
