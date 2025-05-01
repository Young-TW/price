use server::start_server;

mod api;
mod get;
mod config;
mod stream;
mod server;

#[tokio::main]
async fn main() {
    start_server().await;
}
