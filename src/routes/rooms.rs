use std::{
    // time::Duration,
    sync::Arc,
    convert::Infallible,
    // boxed::Box,
};
use http::HeaderValue;
use std::collections::HashMap;
use tokio::sync::{
    Mutex,
    broadcast,
};
use askama::Template;
use axum::{
    Router,
    routing::{
        get,
        post,
    },
    extract::{
        Path,
        State,
        Json,
    },
    response::{
        // AppendHeaders,
        IntoResponse,
        sse::{
            Event,
            Sse,
            // KeepAlive,
        },
    },
    http::{
        StatusCode,
        // HeaderMap,
    },
};
use tokio_stream::StreamExt as _;
use futures_util::stream::{self, Stream};
use serde::Deserialize;
use async_stream::{
    try_stream,
    // AsyncStream,
};
use tokio_stream::wrappers::BroadcastStream;
use tower_http::set_header::SetResponseHeaderLayer;

pub fn router() -> Router<()> {
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
        // .route("/room/:room_id/connect", get(connect_to_room))
        .route("/room/:room_id/live", post(update_room))
        .route("/room/:room_id/submit", post(submit_message))
        .with_state(rooms)
}

#[derive(Template)]
#[template(path="room.html")]
pub struct RoomTemplate {
    room_id: String,
    messages: Vec<String>,
    person: u32,
}

async fn render_room(
    Path(RoomParams { room_id }): Path<RoomParams>,
    State(state): State<Arc<AllRooms>>,
) -> RoomTemplate { 
    println!("room id: {room_id}");
    // let mut room = state.rooms.lock().await.get(&room_id).unwrap_or_default();
    // room.join_count = room.join_count + 1;
    // check for existing room or create one
    let mut rooms = state.rooms.lock().await;
    if let Some(room) = rooms.get_mut(&room_id) {
        room.join_count = room.join_count + 1;
        // room.tx.subscribe();
        RoomTemplate{
            room_id: room_id.clone(),
            messages: room.data.clone(),
            // messages: room
            //     .get(&room_id)
            //     .map(|r| r.data.clone())
            //     .unwrap_or_default(),
            person: room.join_count,
        }
    } else {
        let (tx, _rx) = broadcast::channel(100);
        rooms.insert(room_id.clone(), Room{
            tx,
            data: Vec::new(),
            join_count: 1,
        });
        RoomTemplate{
            room_id: room_id.clone(),
            messages: Vec::new(),
            person: 1,
        }
        // rx
    }
    // RoomTemplate{
    //     room_id: room_id.clone(),
    //     // messages: room.data.clone(),
    //     // messages: room
    //     //     .get(&room_id)
    //     //     .map(|r| r.data.clone())
    //     //     .unwrap_or_default(),
    //     person: room.join_count,
    // }
}

pub struct AllRooms {
    rooms: Mutex<HashMap<String, Room>>,
}
impl AllRooms {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            rooms: Mutex::new(HashMap::new()),
        })
    }
}

#[derive(Clone, Debug)]
enum Action {
    Typing,
    Send,
    // Join(person),
    // Leave(person),
}

// #[derive(Default)]
struct Room {
    tx: broadcast::Sender<(u32, String, Action)>,
    data: Vec<String>,
    join_count: u32,
    // people: Vec<Person>,
}

// struct Person {
//     name: String,
//     current_message: String,
// }

#[derive(Debug, Deserialize)]
struct RoomParams {
    room_id: String,
}

#[derive(Template)]
#[template(path = "message.html")]
pub struct MessageTemplate {
    message: String,
    person: u32,
}

#[derive(Template)]
#[template(path = "submit_message.html")]
pub struct SubmitTemplate {
    messages: Vec<String>,
}

async fn connect_to_room(
    Path(RoomParams { room_id }): Path<RoomParams>,
    State(state): State<Arc<AllRooms>>,
// ) -> impl IntoResponse {
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    println!("connect to room");
    println!("incoming param: {:?}", room_id);

    // let headers = [
    //     (http::header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream")),
    //     (http::header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
    //     (http::header::CONNECTION, HeaderValue::from_static("keep-alive")),
    // ];

    // check for existing room or create one
    let rx = {
        let mut rooms = state.rooms.lock().await;
        if let Some(room) = rooms.get_mut(&room_id) {
            room.join_count = room.join_count + 1;
            room.tx.subscribe()
        } else {
            let (tx, rx) = broadcast::channel(100);
            rooms.insert(room_id.clone(), Room{
                tx,
                data: Vec::new(),
                join_count: 1,
            });
            rx
        }
    };
   
    // let stream: Box<dyn Stream<Item = Result<Event, Infallible>> + Send> = Box::pin(try_stream! {
    let stream = try_stream! {
        // flush
        yield Event::default().data("");
        
        // let mut broadcast_stream = BroadcastStream::new(tx.subscribe());
        // let mut broadcast_stream = BroadcastStream::new(rx);
        let initial = state.rooms.lock().await
            .get(&room_id)
            .map(|r| r.data.clone())
            .unwrap_or_default();
        yield Event::default()
            .event("datastar-merge-fragments")
            .data(SubmitTemplate {
                    messages: initial,
            }.render().unwrap());

        // let room = state.rooms.lock().await.get(&room_id).unwrap();
        let mut broadcast_stream = BroadcastStream::new(rx);
        while let Some(Ok(update)) = broadcast_stream.next().await {
            println!("Processing update: {:?} for room: {}", update.2, room_id);
            let message_history = state.rooms.lock().await
                .get(&room_id)
                .map(|r| r.data.clone())
                .unwrap_or_default();
            println!("New broadcast!");

            match update.2 {
                Action::Typing => {
                    yield Event::default()
                        .event("datastar-merge-fragments")
                        .data(MessageTemplate {
                                message: update.1,
                                person: update.0,
                        }.render().unwrap());
                },
                Action::Send => {
                    yield Event::default()
                        .event("datastar-merge-fragments")
                        .data(SubmitTemplate {
                                messages: message_history,
                        }.render().unwrap());
                }

            }
        }
    };

    // Sse::new(stream)
    //     .keep_alive(KeepAlive::default())
    //     .headers(AppendHeaders([
    //         ("Cache-Control", "no-cache"),
    //         ("Connection", "keep-alive"),
    //     ]))
    Sse::new(stream)
    // (AppendHeaders([
    //         ("Cache-Control", "no-cache"),
    //         ("Connection", "keep-alive"),
    //     ]), Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[derive(Debug, Deserialize)]
struct UpdateRoomRequest {
    person: u32,
    message: String,
}

// #[debug_handler]
async fn update_room(
    State(state): State<Arc<AllRooms>>,
    Path(RoomParams { room_id }): Path<RoomParams>,
    Json(payload): Json<UpdateRoomRequest>,
) -> impl IntoResponse {
    println!("typing");
    println!("incoming param: {:?}", room_id);
    println!("incoming data: {:?}", payload);
    
    // check for existing room or create one
    
    let mut rooms = state.rooms.lock().await;
    if let Some(room) = rooms.get_mut(&room_id) {
        // room.data.push(payload.message.clone());
        if let Err(e) = room.tx.send((payload.person, payload.message, Action::Typing)) {
            println!("Error broadcasting: {}", e);
        }
    }
    StatusCode::OK
    // Ok(())
}

async fn submit_message(
    State(state): State<Arc<AllRooms>>,
    Path(RoomParams { room_id }): Path<RoomParams>,
    Json(payload): Json<UpdateRoomRequest>,
) -> impl IntoResponse {
    println!("submit");
    println!("incoming param: {:?}", room_id);
    println!("incoming data: {:?}", payload);

    // check for existing room or create one

    let mut rooms = state.rooms.lock().await;
    if let Some(room) = rooms.get_mut(&room_id) {
        room.data.push(payload.message.clone());
        if let Err(e) = room.tx.send((payload.person, payload.message, Action::Send)) {
            println!("Error broadcasting: {}", e);
        }
    }
    StatusCode::OK
    // Ok(())
}
