mod api;
mod config;
mod get;

mod calculate;
mod stream;

#[tokio::main]
async fn main() {
    println!("testing stream function");
    /*
    println!(
        "total invesment value: {}",
        calculate::calculate_total().await
    );
    */

    // stream::stream(5).await;
    stream::lazy_stream().await;
}
