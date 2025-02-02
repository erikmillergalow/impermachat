use askama::Template;
use axum::{
    Router,
    routing::get,
};

#[derive(Template)]
#[template(path="index.html")]
pub struct LandingTemplate {}

pub fn router() -> Router<()> {
    Router::new()
        .route("/", get(self::get::landing))
}

mod get {
    use super::*;

    pub async fn landing() -> LandingTemplate {
        LandingTemplate{}
    }
}
