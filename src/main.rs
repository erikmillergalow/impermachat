mod app;
mod rooms;
mod public;
mod templates;
mod utils;
mod middleware;
mod routes;

use crate::app::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new().await?.serve().await
}
