use askama::Template;
use axum::{
    Router,
    routing::{get, post},
    extract,
    response::sse::{Event, KeepAlive, Sse},
};
use std::{time::Duration, convert::Infallible};
use tokio_stream::StreamExt as _;
use futures_util::stream::{self, Stream};
use serde::Deserialize;

#[derive(Template)]
#[template(path="index.html")]
pub struct LandingTemplate {}

pub fn router() -> Router<()> {
    Router::new()
        .route("/", get(self::get::landing))
        .route("/sse", post(sse_handler))
}

#[derive(Debug, Deserialize)]
struct CountRequest {
    count: u32,
}

#[derive(Template)]
#[template(path = "count_response.html")]
pub struct CountTemplate {
    count: u32,
}

async fn sse_handler(extract::Json(payload): extract::Json<CountRequest>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    println!("SSE");
    println!("incoming json: {:?}", payload);

    let new_count = payload.count + 1;

    let event = Event::default()
        .event("datastar-merge-fragments")
        .data(CountTemplate{ count: new_count }.render().unwrap());

    let stream = stream::once(async move { Ok(event) });

    Sse::new(stream)//.keep_alive(KeepAlive::default())
}

mod get {
    use super::*;

    pub async fn landing() -> LandingTemplate {
        LandingTemplate{}
    }
}
