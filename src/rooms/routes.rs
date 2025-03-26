use http::HeaderValue;
use axum::{
    Router,
    routing::{
        get,
        post,
    },
    middleware,
};
use tower_http::set_header::SetResponseHeaderLayer;
use tower::ServiceBuilder;

use super::handlers::{
    connect_to_room,
    submit_message,
    update_room,
    render_room,
    set_name,
    AllRooms,
};

use super::middleware::ensure_uid;

pub fn rooms_router() -> Router<()> {
    let rooms = AllRooms::new();

    let sse_router = Router::new()
        .route("/connect", get(connect_to_room))
        .layer(SetResponseHeaderLayer::overriding(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            http::header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        ));
        // .layer(SetResponseHeaderLayer::overriding(
        //     http::header::CONNECTION,
        //     HeaderValue::from_static("keep-alive"),
        // ));

    Router::new()
        .route("/room/:room_id", get(render_room))
        .nest("/room/:room_id", sse_router)
        .route("/room/:room_id/live", post(update_room))
        .route("/room/:room_id/submit", post(submit_message))
        .route("/room/:room_id/name", post(set_name))
        .layer(ServiceBuilder::new().layer(middleware::from_fn(ensure_uid)))
        .with_state(rooms)
}
