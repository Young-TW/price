mod api;
mod config;
mod get;

mod calculate;
mod stream;

#[tokio::main]
async fn main() {
    stream::stream(5).await;
    // stream::lazy_stream().await;
}
