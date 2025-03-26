use axum::{
    Router,
    routing::{get, post},
};

use super::handlers::{
    get::index,
    post::create_room,
};

pub fn public_router() -> Router<()> {
    Router::new()
        .route("/", get(index))
        .route("/", post(create_room))
}

