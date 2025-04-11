mod api;
mod config;
mod get;

mod calculate;
mod stream;

#[tokio::main]
async fn main() {
    /*
    println!(
        "total invesment value: {}",
        calculate::calculate_total().await
    );
    */

    stream::stream(5).await;
}
