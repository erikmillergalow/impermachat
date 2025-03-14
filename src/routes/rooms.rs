use std::{
    sync::Arc,
    convert::Infallible,
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
        IntoResponse,
        sse::{
            Event,
            Sse,
        },
        Response,
    },
    http::{
        Request,
        StatusCode,
        header::{COOKIE, SET_COOKIE},
        HeaderMap,
    },
    middleware::{self, Next},
    body::Body,
};
use uuid::Uuid;
use tokio_stream::StreamExt as _;
use futures_util::stream::{self, Stream};
use serde::Deserialize;
use async_stream::try_stream;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::set_header::SetResponseHeaderLayer;
use tower::ServiceBuilder;

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
        .route("/room/:room_id/live", post(update_room))
        .route("/room/:room_id/submit", post(submit_message))
        .route("/room/:room_id/name", post(set_name))
        .layer(ServiceBuilder::new().layer(middleware::from_fn(ensure_uid)))
        .with_state(rooms)
}

fn name_to_color(name: &str) -> String {
    let mut hash: u32 = 0;
    for byte in name.bytes() {
        hash = hash.wrapping_add(byte as u32);
        hash = hash.wrapping_mul(31);
    }

    let r = (hash % 200) + 55; // +55 to avoid going too dark
    let g = ((hash >> 8) % 200) + 55;
    let b = ((hash >> 16) % 200) + 55;

    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

async fn ensure_uid(
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {

    // check for cookie
    let has_browser_id = request
        .headers()
        .get(COOKIE)
        .and_then(|cookie| {
            cookie
                .to_str()
                .ok()
                .and_then(|c| c.split(';')
                    .find(|s| s.trim().starts_with("impermachat_id=")))
        })
        .is_some();

    let mut response = next.run(request).await;

    // bestow an ID if none found
    if !has_browser_id {
        let new_browser_id = Uuid::new_v4().to_string();
        let cookie = format!("impermachat_id={}; Path=/; HttpOnly", new_browser_id);
        response.headers_mut().insert(
            SET_COOKIE,
            cookie.parse().unwrap()
        );
    }

    Ok(response)
}

#[derive(Template)]
#[template(path="room.html")]
pub struct RoomTemplate {
    room_id: String,
    messages: Vec<Message>,
    person: u32,
}

async fn render_room(
    Path(RoomParams { room_id }): Path<RoomParams>,
    State(state): State<Arc<AllRooms>>,
) -> RoomTemplate { 
    println!("room id: {room_id}");

    let mut rooms = state.rooms.lock().await;
    if let Some(room) = rooms.get_mut(&room_id) {
        RoomTemplate{
            room_id: room_id.clone(),
            messages: room.message_history.clone(),
            person: room.join_count,
        }
    } else {
        let (tx, _rx) = broadcast::channel(100);
        rooms.insert(room_id.clone(), Room{
            tx,
            message_history: Vec::new(),
            join_count: 1,
            name_to_id: HashMap::new(),
            id_to_name: HashMap::new(),
            name_to_color: HashMap::new(),
            typing_state: HashMap::new(),
        });
        RoomTemplate{
            room_id: room_id.clone(),
            messages: Vec::new(),
            person: 1,
        }
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

#[derive(Clone, Debug)]
enum Action {
    Typing,
    Send,
    SetName,
}

// #[derive(Default)]
#[derive(Clone)]
struct Room {
    tx: broadcast::Sender<(String, String, Action)>,
    message_history: Vec<Message>,
    // message_history: Vec<(String, String)>,
    typing_state: HashMap<String, Message>,
    // typing_state: HashMap<String, String>,
    join_count: u32,
    name_to_id: HashMap<String, String>,
    id_to_name: HashMap<String, String>,
    name_to_color: HashMap<String, String>,
}

#[derive(Clone)]
struct Message {
    name: String,
    connection_id: String,
    color: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct RoomParams {
    room_id: String,
}

#[derive(Template)]
#[template(path = "message.html")]
pub struct MessageTemplate {
    message: String,
    person: String,
}

#[derive(Template)]
#[template(path = "submit_message.html")]
pub struct SubmitTemplate {
    messages: Vec<Message>,
    connection_id: String,
}

#[derive(Template)]
#[template(path = "typing_messages.html")]
pub struct TypingTemplate {
    messages: HashMap<String, Message>,
}

#[derive(Template)]
#[template(path = "init_name.html")]
pub struct InitNameTemplate {
    room_id: String,
}

fn get_connection_cookie(headers: &HeaderMap) -> Option<String> {
    headers.get("cookie")
        .and_then(|c| c.to_str().ok())
        .and_then(|c| c.split(';')
            .find(|s| s.trim().starts_with("impermachat_id="))
            .map(|s| s.trim_start_matches("impermachat_id=").to_string()))
}

#[axum::debug_handler]
async fn connect_to_room(
    // cookies: TypedHeader<Cookie>,
    // jar: CookieJar,
    headers: HeaderMap,
    Path(RoomParams { room_id }): Path<RoomParams>,
    State(state): State<Arc<AllRooms>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    println!("connect to room");
    println!("incoming param: {:?}", room_id);

    // get this person's uid
    let connection_id = match get_connection_cookie(&headers) {
        Some(id) => id,
        None => {
            println!("Uh oh!");
            Uuid::new_v4().to_string()
            // let error_stream = try_stream! {
            //     yield Event::default()
            //         .event("datastar-merge-fragments")
            //         .data(MessageTemplate {
            //             message: "Error setting up connection_id, please refresh the page.".to_string(),
            //             person: "System".to_string(),
            //         }.render().unwrap());
            // };
            // return Sse::new(error_stream);
        }
    };

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
                message_history: Vec::new(),
                join_count: 1,
                name_to_id: HashMap::new(),
                id_to_name: HashMap::new(),
                name_to_color: HashMap::new(),
                typing_state: HashMap::new(),
            });
            rx
        }
    };
   
    let stream = try_stream! {
        // flush
        yield Event::default().data("");

        let room = state.rooms.lock().await
            .get(&room_id)
            .cloned()
            .expect("Room should exist by now");

        yield Event::default()
            .event("datastar-merge-fragments")
            .data(SubmitTemplate {
                    messages: room.message_history.clone(),
                    connection_id: connection_id.clone(),
            }.render().unwrap());

        // let person_name = match 
        match room.id_to_name.get(&connection_id) {
            Some(name) => {
                yield Event::default()
                    .event("datastar-merge-fragments")
                    .data(ChatInputTemplate {
                        room_id: room_id.clone(),
                        person: name.to_string(),
                    }.render().unwrap())
            },
            None => {
                yield Event::default()
                    .event("datastar-merge-fragments")
                    .data(InitNameTemplate {
                        room_id: room_id.clone(),
                    }.render().unwrap());
            }
        }

        let mut broadcast_stream = BroadcastStream::new(rx);
        while let Some(Ok(update)) = broadcast_stream.next().await {
            println!("Processing update: {:?} for room: {}", update.2, room_id);
            println!("New broadcast!");

            match update.2 {
                Action::Typing => {
                    let mut rooms = state.rooms.lock().await;
                    if let Some(room) = rooms.get_mut(&room_id) {
                        yield Event::default()
                            .event("datastar-merge-fragments")
                            .data(TypingTemplate {
                                messages: room.typing_state.clone(),
                            }.render().unwrap());
                    }
                },
                Action::Send => {
                    let mut rooms = state.rooms.lock().await;
                    if let Some(room) = rooms.get_mut(&room_id) {
                        yield Event::default()
                            .event("datastar-merge-fragments")
                            .data(SubmitTemplate {
                                messages: room.message_history.clone(),
                                connection_id: connection_id.clone(),
                            }.render().unwrap());
                    }
                    // clear user chat input
                    if update.0 == connection_id {
                        yield Event::default()
                            .event("datastar-merge-signals")
                            .data("signals {message: ''}")
                    }
                },
                Action::SetName => {
                    if update.1 == connection_id {
                        // name = Some(update.0.clone());
                        yield Event::default()
                            .event("datastar-merge-fragments")
                            .data(ChatInputTemplate {
                                room_id: room_id.clone(),
                                person: update.0.clone(),
                            }.render().unwrap())
                    }
                },
            }
        }
    };

    Sse::new(stream)
}

#[derive(Debug, Deserialize)]
struct TypingRequest {
    message: String,
}

// #[debug_handler]
async fn update_room(
    headers: HeaderMap,
    State(state): State<Arc<AllRooms>>,
    Path(RoomParams { room_id }): Path<RoomParams>,
    Json(payload): Json<TypingRequest>,
) -> impl IntoResponse {
    // println!("typing");
    let connection_id = match get_connection_cookie(&headers) {
        Some(id) => id,
        None => {
            println!("Uh oh!");
            Uuid::new_v4().to_string()
        }
    };

    let mut rooms = state.rooms.lock().await;
    if let Some(room) = rooms.get_mut(&room_id) {
        let person_name = room.id_to_name.get(&connection_id).cloned().expect("Person should have a name");
        room.typing_state.insert(String::from(person_name.clone()), Message{
            name: person_name.clone(),
            content: payload.message.clone(),
            color: name_to_color(&person_name),
            connection_id: connection_id.clone(),
        });
        if let Err(e) = room.tx.send((connection_id.clone(), payload.message, Action::Typing)) {
            println!("Error broadcasting: {}", e);
        }
    }
    StatusCode::OK
}

async fn submit_message(
    headers: HeaderMap,
    State(state): State<Arc<AllRooms>>,
    Path(RoomParams { room_id }): Path<RoomParams>,
    Json(payload): Json<TypingRequest>,
) -> impl IntoResponse {
    println!("submit");
    println!("incoming param: {:?}", room_id);
    println!("incoming data: {:?}", payload);

    let connection_id = match get_connection_cookie(&headers) {
        Some(id) => id,
        None => {
            println!("Uh oh!");
            Uuid::new_v4().to_string()
        }
    };

    let mut rooms = state.rooms.lock().await;
    if let Some(room) = rooms.get_mut(&room_id) {
        // replace these expects() with a new error broadcast event handler as to not crash the
        // entire server
        let person_name = room.id_to_name.get(&connection_id).cloned().expect("Person should have a name");
        room.message_history.push(Message{
            name: person_name.clone(),
            content: payload.message.clone(),
            color: name_to_color(&person_name),
            connection_id: connection_id.clone(),
        });
        room.typing_state.insert(String::from(person_name.clone()), Message{
            name: person_name.clone(),
            content: String::from(""),
            color: name_to_color(&person_name),
            connection_id: connection_id.clone(),
        });
        if let Err(e) = room.tx.send((connection_id.clone(), payload.message, Action::Send)) {
            println!("Error broadcasting: {}", e);
        }
        if let Err(e) = room.tx.send((connection_id.clone(), String::from(""), Action::Typing)) {
            println!("Error broadcasting: {}", e);
        }
    }
    StatusCode::OK
}


#[derive(Debug, Deserialize)]
struct SetNameRequest {
    name: String,
}
#[derive(Template)]
#[template(path = "chat_input.html")]
pub struct ChatInputTemplate {
    room_id: String,
    person: String,
}
#[derive(Template)]
#[template(path = "set_name.html")]
pub struct SetNameTemplate {
    room_id: String,
    connection_id: String,
    message: String,
}

#[derive(Template)]
#[template(path = "status_message.html")]
pub struct StatusMessageTemplate {
    target_id: String,
    message: String,
}
async fn set_name(
    headers: HeaderMap,
    State(state): State<Arc<AllRooms>>,
    Path(RoomParams { room_id }): Path<RoomParams>,
    Json(payload): Json<SetNameRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    println!("submit");
    println!("incoming param: {:?}", room_id);
    println!("incoming data: {:?}", payload);

    let connection_id = match get_connection_cookie(&headers) {
        Some(id) => id,
        None => {
            println!("Uh oh!");
            Uuid::new_v4().to_string()
        }
    };

    let event = {
        let mut rooms = state.rooms.lock().await;
        if let Some(room) = rooms.get_mut(&room_id) {
            if !room.name_to_id.contains_key(&payload.name) {
                room.name_to_id.insert(payload.name.clone(), connection_id.clone());
                room.id_to_name.insert(connection_id.clone(), payload.name.clone());
                room.name_to_color.insert(payload.name.clone(), name_to_color(&payload.name));
                if let Err(e) = room.tx.send((
                    payload.name.clone(),
                    connection_id.clone(),
                    Action::SetName,
                )) {
                    println!("Error broadcasting name change: {}", e);
                }
                Event::default().data("")
            } else {
                Event::default()
                    .event("datastar-merge-fragments")
                    .data(SetNameTemplate {
                        room_id,
                        connection_id: connection_id.clone(),
                        message: "Name already taken".to_string(),
                    }.render().unwrap())
            }
        } else {
            Event::default()
                .event("datastar-merge-fragments")
                .data(SetNameTemplate {
                    room_id,
                    connection_id: connection_id.clone(),
                    message: "Room not found".to_string(),
                }.render().unwrap())
        }
    };

    let stream = stream::once(async move { Ok(event) });
    Sse::new(stream)
}
