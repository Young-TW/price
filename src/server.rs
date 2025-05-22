use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use warp::ws::{Message, WebSocket};
use warp::{http::StatusCode, reply, Filter, Reply};
use futures_util::{SinkExt, StreamExt};
use bytes::Bytes;

use crate::config::read_portfolio_from_string;
use crate::stream::stream;

type SharedPortfolio = Arc<Mutex<Option<HashMap<String, HashMap<String, f64>>>>>;
type SharedPriceMap = Arc<Mutex<std::collections::HashMap<String, f64>>>;

/// 每個 WebSocket 連線：訂閱 broadcast → 轉發給客戶端
async fn handle_connection(ws: WebSocket, mut rx: broadcast::Receiver<String>) {
    let (mut tx, _) = ws.split();
    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if tx.send(Message::text(msg)).await.is_err() {
                break;
            }
        }
    });
}

/// POST /upload：接收 TOML，若格式正確就啟動 stream 推播
async fn handle_post_toml(
    body: String,
    portfolio: SharedPortfolio,
    tx: broadcast::Sender<String>,
) -> Result<impl Reply, warp::Rejection> {
    match read_portfolio_from_string(&body) {
        Ok(parsed) => {
            // 覆蓋舊 portfolio
            {
                let mut pf_lock = portfolio.lock().await;
                *pf_lock = Some(parsed.clone());
            }

            Ok(reply::with_status("TOML 檔案已成功上傳並開始串流價格", StatusCode::OK))
        }
        Err(e) => {
            let err_msg = format!("TOML 解析失敗: {}", e);
            let leaked: &'static str = Box::leak(err_msg.into_boxed_str());
            Ok(reply::with_status(leaked, StatusCode::BAD_REQUEST))
        }
    }
}

pub async fn start_server() {
    let portfolio: SharedPortfolio = Arc::new(Mutex::new(None));
    let (tx, _) = broadcast::channel::<String>(32);

    // 只啟動一次 stream
    {
        let pf = portfolio.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            stream(pf, 10, tx).await;
        });
    }

    // WebSocket 路由
    let tx_ws = tx.clone();
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .map(move |ws: warp::ws::Ws| {
            let rx = tx_ws.subscribe();
            ws.on_upgrade(move |sock| handle_connection(sock, rx))
        });

    // Upload 路由
    let portfolio_f = warp::any().map(move || portfolio.clone());
    let post_route = warp::path("upload")
        .and(warp::post())
        .and(warp::body::bytes())
        .and(portfolio_f)
        .and_then(|body: Bytes, portfolio: SharedPortfolio| async move {
            let body_str = String::from_utf8(body.to_vec()).map_err(|_| warp::reject())?;
            match read_portfolio_from_string(&body_str) {
                Ok(parsed) => {
                    let mut pf_lock = portfolio.lock().await;
                    *pf_lock = Some(parsed);
                    Ok::<_, warp::Rejection>(reply::with_status("TOML 檔案已成功上傳並開始串流價格", StatusCode::OK))
                }
                Err(e) => {
                    let err_msg = format!("TOML 解析失敗: {}", e);
                    let leaked: &'static str = Box::leak(err_msg.into_boxed_str());
                    Ok::<_, warp::Rejection>(reply::with_status(leaked, StatusCode::BAD_REQUEST))
                }
            }
        });

    warp::serve(ws_route.or(post_route))
        .run(([0, 0, 0, 0], 3030))
        .await;
}