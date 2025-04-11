mod api;
mod config;
mod get;

mod calculate;

#[tokio::main]
async fn main() {
    println!(
        "total invesment value: {}",
        calculate::calculate_total().await
    );
}
