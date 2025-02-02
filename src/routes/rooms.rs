use std::{
    // time::Duration,
    sync::Arc,
    convert::Infallible
};
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
        IntoResponse,
        sse::{
            Event,
            // KeepAlive,
            Sse,
        },
    },
    http::StatusCode,
    // Error,
    // Result<(), Error>,
};
use tokio_stream::StreamExt as _;
use futures_util::stream::{self, Stream};
use serde::Deserialize;
use async_stream::try_stream;
use tokio_stream::wrappers::BroadcastStream;

pub fn router() -> Router<()> {
    let rooms = AllRooms::new();
    Router::new()
        .route("/room/:room_id", get(render_room))
        .route("/room/:room_id/connect", get(connect_to_room))
        .route("/room/:room_id/live", post(update_room))
        // .route("/sse", post(connect_to_room))
        .with_state(rooms)
}

#[derive(Template)]
#[template(path="room.html")]
pub struct RoomTemplate {
    room_id: String,
    messages: Vec<String>,
}

async fn render_room(
    Path(RoomParams { room_id }): Path<RoomParams>,
    State(state): State<Arc<AllRooms>>,
) -> RoomTemplate { 
    println!("room id: {room_id}");
    RoomTemplate{
        room_id: room_id.clone(),
        messages: state.rooms.lock().await
            .get(&room_id)
            .map(|r| r.data.clone())
            .unwrap_or_default()
    }
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

struct Room {
    tx: broadcast::Sender<String>,
    data: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RoomParams {
    room_id: String,
}

#[derive(Template)]
#[template(path = "message.html")]
pub struct MessageTemplate {
    message: String,
}

async fn connect_to_room(
    Path(RoomParams { room_id }): Path<RoomParams>,
    State(state): State<Arc<AllRooms>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    println!("connect to room");
    println!("incoming param: {:?}", room_id);
    
    // check for existing room or create one
    let rx = {
        let mut rooms = state.rooms.lock().await;
        if let Some(room) = rooms.get(&room_id) {
            room.tx.subscribe()
        } else {
            let (tx, rx) = broadcast::channel(100);
            rooms.insert(room_id.clone(), Room{
                tx,
                data: Vec::new(),
            });
            rx
        }
    };
    
    let stream = try_stream! {
        let initial = state.rooms.lock().await
            .get(&room_id)
            .map(|r| r.data.clone())
            .unwrap_or_default();
        yield Event::default().data(initial.join("\n"));

        let mut broadcast_stream = BroadcastStream::new(rx);
        while let Some(Ok(update)) = broadcast_stream.next().await {
            println!("New broadcast!");
//         .event("datastar-merge-fragments")
//         .data(CountTemplate{ count: new_count }.render().unwrap());
            yield Event::default()
                .event("datastar-merge-fragments")
                .data(MessageTemplate {
                        message: update,
                }.render().unwrap());
        }
    };

    Sse::new(stream)
}

#[derive(Debug, Deserialize)]
struct UpdateRoomRequest {
    message: String,
}

// #[debug_handler]
async fn update_room(
    State(state): State<Arc<AllRooms>>,
    Path(RoomParams { room_id }): Path<RoomParams>,
    Json(payload): Json<UpdateRoomRequest>,
) -> impl IntoResponse {
    println!("SSE");
    println!("incoming param: {:?}", room_id);
    println!("incoming data: {:?}", payload);
    
    // check for existing room or create one
    // let rx = {
    //     let mut rooms = state.rooms.lock().await;
    //     if let Some(room) = rooms.get(&room_id) {
    //         room.tx.subscribe()
    //     } else {
    //         let (tx, rx) = broadcast::channel(100);
    //         rooms.insert(room_id.clone(), Room{
    //             tx,
    //             data: Vec::new(),
    //         });
    //         rx
    //     }
    // };
    let mut rooms = state.rooms.lock().await;
    if let Some(room) = rooms.get_mut(&room_id) {
        room.data.push(payload.message.clone());
        if let Err(e) = room.tx.send(payload.message) {
            println!("Error broadcasting: {}", e);
        }
        // room.tx.send(payload.message)?;
    }
    StatusCode::OK
    // Ok(())
}

// post function (user interacts with textarea or something)

#[derive(Debug, Deserialize)]
struct CountRequest {
    count: u32,
}

#[derive(Template)]
#[template(path = "count_response.html")]
pub struct CountTemplate {
    count: u32,
}

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
