mod app;
mod rooms;
mod public;

use crate::app::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new().await?.serve().await
}
