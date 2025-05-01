// server.rs ─ WebSocket 版：上傳 TOML → 即時推播價格

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::{broadcast, Mutex};
use warp::ws::{Message, WebSocket};
use warp::{http::StatusCode, reply, Filter, Reply};
use futures_util::{SinkExt, StreamExt};          // ← split() 需要
use bytes::Bytes;

use crate::config::read_portfolio_from_string;
use crate::stream::stream;                       // fn stream(pf, cycle, tx)

type SharedPriceMap = Arc<Mutex<std::collections::HashMap<String, f64>>>;

static STREAM_RUNNING: AtomicBool = AtomicBool::new(false);

/// 每個 WebSocket 連線：訂閱 broadcast → 轉發給客戶端
async fn handle_connection(ws: WebSocket, mut rx: broadcast::Receiver<String>) {
    let (mut tx, _) = ws.split();
    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if tx.send(Message::text(msg)).await.is_err() {
                break; // 客戶端斷線
            }
        }
    });
}

/// POST /upload：接收 TOML，若格式正確就啟動 stream 推播
async fn handle_post_toml(
    body: String,
    _prices: SharedPriceMap,                 // 現階段用不到，可保留擴充
    tx: broadcast::Sender<String>,
) -> Result<impl Reply, warp::Rejection> {
    match read_portfolio_from_string(&body) {
        Ok(parsed) => {
            // 僅第一次啟動 background stream
            if !STREAM_RUNNING.swap(true, Ordering::SeqCst) {
                let pf = Arc::new(parsed.clone());
                tokio::spawn(stream(pf, 10, tx.clone())); // 10 秒輪詢一次
            }
            let ok_msg = String::from("TOML 檔案已成功上傳並開始串流價格");
            Ok(reply::with_status(ok_msg, StatusCode::OK))
        }
        Err(e) => {
            let err_msg = format!("TOML 解析失敗: {}", e);
            Ok(reply::with_status(err_msg, StatusCode::BAD_REQUEST))
        }
    }
}

pub async fn start_server() {
    // 共用狀態（目前僅預留，stream() 自己管理）
    let prices: SharedPriceMap = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let (tx, _) = broadcast::channel::<String>(32);

    // ────────── Filters ──────────
    let prices_f = warp::any().map(move || prices.clone());

    // WebSocket 路由
    let tx_ws = tx.clone();
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .map(move |ws: warp::ws::Ws| {
            let rx = tx_ws.subscribe();
            ws.on_upgrade(move |sock| handle_connection(sock, rx))
        });

    // Upload 路由
    let tx_up = tx.clone();
    let post_route = warp::path("upload")
        .and(warp::post())
        .and(warp::body::bytes())
        .and(prices_f)
        .and_then(move |body: Bytes, prices| {
            let tx_inner = tx_up.clone();
            async move {
                let body_str = String::from_utf8(body.to_vec()).map_err(|_| warp::reject())?;
                handle_post_toml(body_str, prices, tx_inner).await
            }
        });

    // 伺服器啟動
    warp::serve(ws_route.or(post_route))
        .run(([127, 0, 0, 1], 3030))
        .await;
}
