#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    turf_net::cli::main_cli().await
}
