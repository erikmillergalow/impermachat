use std::{
    time::Duration,
    // sync::Arc,
    convert::Infallible
};
use std::collections::HashMap;
// use tokio::sync::Mutex;
use askama::Template;
use axum::{
    Router,
    routing::post,
    extract,
    response::sse::{Event, KeepAlive, Sse},
};
// use tokio_stream::StreamExt as _;
use futures_util::stream::{self, Stream};
use serde::Deserialize;
// use async_stream::stream;
// use tokio_stream::wrappers;

pub fn router() -> Router<()> {
    Router::new()
        .route("/sse", post(connect_to_room))
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

async fn connect_to_room(extract::Json(payload): extract::Json<CountRequest>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // room ID from request
    // create broadcast + room state
    // listen on broadcast until disconnected


    println!("SSE");
    println!("incoming json: {:?}", payload);

    let new_count = payload.count + 1;

    let event = Event::default()
        .event("datastar-merge-fragments")
        .data(CountTemplate{ count: new_count }.render().unwrap());

    let stream = stream::once(async move { Ok(event) });

    Sse::new(stream)//.keep_alive(KeepAlive::default())
}

// post function (user interacts with textarea or something)

// async fn sse_handler(extract::Json(payload): extract::Json<CountRequest>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
//     println!("SSE");
//     println!("incoming json: {:?}", payload);
//
//     let new_count = payload.count + 1;
//
//     let event = Event::default()
//         .event("datastar-merge-fragments")
//         .data(CountTemplate{ count: new_count }.render().unwrap());
//
//     let stream = stream::once(async move { Ok(event) });
//
//     Sse::new(stream)//.keep_alive(KeepAlive::default())
// }
