use std::{
    sync::Arc,
    convert::Infallible,
    time::Instant,
    collections::HashMap,
};
use tokio::{
    sync::{
        Mutex,
        broadcast,
    },
    time::{
        Duration,
        sleep,
    },
};
use askama::Template;
use axum::{
    extract::{
        Path,
        State,
        Json,
        Query,
    },
    response::{
        IntoResponse,
        sse::{
            Event,
            Sse,
        },
        Response,
        Redirect,
    },
    http::{
        StatusCode,
        HeaderMap,
    },
    body::Body,
};
use tokio_stream::StreamExt as _;
use futures_util::stream::Stream;
use serde::Deserialize;
use async_stream::try_stream;
use tokio_stream::wrappers::BroadcastStream;

use crate::utils::{
    create_fragments_event,
    name_to_color,
    format_time,
    get_connection_cookie,
};

use crate::templates::{
    RoomTemplate,
    SubmitTemplate,
    SetNameTemplate,
    ShutdownTemplate,
    // StatusMessageTemplate,
    InitNameTemplate,
    TypingTemplate,
    MajorErrorTemplate,
    ChatInputTemplate,
    // MessageTemplate,
};

const MAX_MESSAGE_SIZE: usize = 4000;

#[derive(Clone, Debug)]
enum Action {
    Typing,
    Send,
    SetName,
    ShutdownRoom,
    UpdateTime,
    MajorError,
}

#[derive(Clone)]
struct ActionEvent {
    connection_id: String,
    action: Action,
}

#[derive(Clone)]
pub struct Room {
    tx: broadcast::Sender<ActionEvent>,
    message_history: Vec<Message>,
    typing_state: HashMap<String, Message>,
    join_count: u32,
    name_to_id: HashMap<String, String>,
    id_to_name: HashMap<String, String>,
    name_to_color: HashMap<String, String>,
    expiration: Instant,
}

#[derive(Clone)]
pub struct Message {
    pub name: String,
    pub connection_id: String,
    pub color: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct RoomParams {
    pub room_id: String,
}

#[derive(Debug, Deserialize)]
pub struct TypingRequest {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct SetNameRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ExpirationParams {
    #[serde(default)]
    pub hours: Option<u64>,
    #[serde(default)]
    pub minutes: Option<u64>,
}

pub struct AllRooms {
    pub rooms: Mutex<HashMap<String, Room>>,
}

impl AllRooms {
    pub fn new() -> Arc<Self> {
        let rooms = Arc::new(Self {
            rooms: Mutex::new(HashMap::new()),
        });

        let rooms_cleanup = rooms.clone();
        tokio::spawn(async move {
            cleanup_rooms(rooms_cleanup).await;
        });

        rooms
    }
}

pub async fn render_room(
    Path(RoomParams { room_id }): Path<RoomParams>,
    Query(ExpirationParams { hours, minutes }): Query<ExpirationParams>,
    State(state): State<Arc<AllRooms>>,
) -> impl IntoResponse { 
    let mut rooms = state.rooms.lock().await;
    if let Some(_room) = rooms.get_mut(&room_id) {
        RoomTemplate{
            room_id: room_id.clone(),
        }.into_response()
    } else {
        if hours.is_none() && minutes.is_none() {
            return Redirect::to("/").into_response();
        }

        let (tx, _rx) = broadcast::channel(100);
        rooms.insert(room_id.clone(), Room{
            tx,
            message_history: Vec::new(),
            join_count: 1,
            name_to_id: HashMap::new(),
            id_to_name: HashMap::new(),
            name_to_color: HashMap::new(),
            typing_state: HashMap::new(),
            expiration: Instant::now() + Duration::from_secs(hours.unwrap_or(0) * 60 * 60) + Duration::from_secs(minutes.unwrap_or(1) * 60),
        });
        RoomTemplate{
            room_id: room_id.clone(),
        }.into_response()
    }
}

async fn cleanup_rooms(all_rooms: Arc<AllRooms>) {
    loop {
        sleep(Duration::from_secs(1)).await;

        let mut rooms = all_rooms.rooms.lock().await;
        let mut to_remove = Vec::new();

        for (room_id, room) in rooms.iter_mut() {
            if Instant::now() > room.expiration {
                // broadcast room shutdown
                let _ = room.tx.send(ActionEvent {
                    connection_id: "System".to_string(),
                    action: Action::ShutdownRoom,
                });
                to_remove.push(room_id.clone());
            } else {
                let _ = room.tx.send(ActionEvent {
                    connection_id: "System".to_string(),
                    action: Action::UpdateTime,
                });
            }
        }

        for room_id in to_remove {
            rooms.remove(&room_id);
        }
    }
}

pub async fn connect_to_room(
    headers: HeaderMap,
    Path(RoomParams { room_id }): Path<RoomParams>,
    State(state): State<Arc<AllRooms>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {

    // get this person's uid
    let connection_id = get_connection_cookie(&headers)
        .expect("Middleware should have bestowed UUID by now.");

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
                expiration: Instant::now() + Duration::from_secs(30),
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

        // render typing state
        yield Event::default()
            .event("datastar-merge-fragments")
            .data(TypingTemplate {
                messages: room.typing_state.clone(),
                connection_id: connection_id.clone(),
            }.render().unwrap());

        // populate existing messages
        let initial_messages = SubmitTemplate {
            messages: room.message_history.clone(),
            connection_id: connection_id.clone(),
        }.render().unwrap();
        let mut raw_event = String::from("");
        for line in initial_messages.lines() {
            raw_event.push_str(&format!("fragments {}\n", line));
        }
        yield Event::default()
            .event("datastar-merge-fragments")
            .data(raw_event);

        // check if person has already selected a name in this room
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

        // main handler loop to send SSE to update UI
        let mut broadcast_stream = BroadcastStream::new(rx);
        while let Some(Ok(event)) = broadcast_stream.next().await {
            match event.action {
                Action::Typing => {
                    let mut rooms = state.rooms.lock().await;
                    if let Some(room) = rooms.get_mut(&room_id) {
                        let rendered_typing = TypingTemplate {
                            messages: room.typing_state.clone(),
                            connection_id: connection_id.clone(),
                        }.render().unwrap();
                        yield Event::default()
                            .event("datastar-merge-fragments")
                            .data(create_fragments_event(rendered_typing));
                    }
                },
                Action::Send => {
                    let mut rooms = state.rooms.lock().await;
                    if let Some(room) = rooms.get_mut(&room_id) {
                        let rendered_submit = SubmitTemplate {
                            messages: room.message_history.clone(),
                            connection_id: connection_id.clone(),
                        }.render().unwrap();

                        yield Event::default()
                            .event("datastar-merge-fragments")
                            .data(create_fragments_event(rendered_submit));

                        // clear user chat input
                        if event.connection_id == connection_id {
                            yield Event::default()
                                .event("datastar-merge-signals")
                                .data("signals {message: ''}")
                        }

                        let rendered_typing = TypingTemplate {
                            messages: room.typing_state.clone(),
                            connection_id: connection_id.clone(),
                        }.render().unwrap();
                        let typing_converted = create_fragments_event(rendered_typing);
                        yield Event::default()
                            .event("datastar-merge-fragments")
                            .data(typing_converted);
                    }
                },
                Action::SetName => {
                    let rooms = state.rooms.lock().await;
                    if let Some(room) = rooms.get(&room_id) {
                        if let Some(name) = room.id_to_name.get(&connection_id) {
                            if event.connection_id == connection_id {
                                yield Event::default()
                                    .event("datastar-merge-fragments")
                                    .data(ChatInputTemplate {
                                        room_id: room_id.clone(),
                                        person: name.clone(),
                                    }.render().unwrap())
                            }

                            // render new person's typing box
                            let rendered_typing = TypingTemplate {
                                messages: room.typing_state.clone(),
                                connection_id: connection_id.clone(),
                            }.render().unwrap();
                            let typing_converted = create_fragments_event(rendered_typing);
                            yield Event::default()
                                .event("datastar-merge-fragments")
                                .data(typing_converted);
                            }
                    }
                },
                Action::ShutdownRoom => {
                    yield Event::default()
                        .event("datastar-merge-fragments")
                        .data(ShutdownTemplate {
                        }.render().unwrap());
                },
                Action::UpdateTime => {
                    let mut rooms = state.rooms.lock().await;
                    if let Some(room) = rooms.get_mut(&room_id) {
                        yield Event::default()
                            .event("datastar-merge-signals")
                            .data(format!("signals {{remaining: '{}'}}", format_time(room.expiration.duration_since(Instant::now()))));
                    }
                },
                Action::MajorError => {
                    if event.connection_id == connection_id {
                        yield Event::default()
                            .event("datastar-merge-fragments")
                            .data(MajorErrorTemplate{}.render().unwrap());
                    }
                }
            }
        }
    };

    Sse::new(stream)
}

pub async fn update_room(
    headers: HeaderMap,
    State(state): State<Arc<AllRooms>>,
    Path(RoomParams { room_id }): Path<RoomParams>,
    Json(payload): Json<TypingRequest>,
) -> impl IntoResponse {
    let mut rooms = state.rooms.lock().await;
    if let Some(room) = rooms.get_mut(&room_id) {
        let connection_id = match get_connection_cookie(&headers) {
            Some(id) => id,
            None => {
                return (
                    StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                    "event: datastar-merge-fragments\ndata:fragments <div id='chat-container'><h1 class='major-error-message'>Unable to find connection ID cookie - refresh to attempt to recover</h1><div class='button-center'><button class='big' onclick='window.location.reload()'>Refresh</button></div></div>\n\n"
                ).into_response();
            }
        };

        let person_name = match room.id_to_name.get(&connection_id).cloned() {
            Some(name) => name,
            None => {
                if let Err(e) = room.tx.send(ActionEvent {
                    connection_id: connection_id.clone(),
                    action: Action::MajorError,
                }) {
                    println!("Error broadcasting error event {}", e);
                }
                return StatusCode::OK.into_response();
            }
        };

        let mut new_message = payload.message.clone();
        if payload.message.len() > MAX_MESSAGE_SIZE {
            new_message = "This message was too long! Keep it under 4,000 characters".to_string();
        }

        room.typing_state.insert(String::from(person_name.clone()), Message{
            name: person_name.clone(),
            content: new_message,
            color: name_to_color(&person_name),
            connection_id: connection_id.clone(),
        });
        if let Err(e) = room.tx.send(ActionEvent{
            connection_id: connection_id.clone(), 
            action: Action::Typing
        }) {
            println!("Error broadcasting: {}", e);
        }
    }
    StatusCode::OK.into_response()
}

pub async fn submit_message(
    headers: HeaderMap,
    State(state): State<Arc<AllRooms>>,
    Path(RoomParams { room_id }): Path<RoomParams>,
    Json(payload): Json<TypingRequest>,
) -> impl IntoResponse {
    let connection_id = match get_connection_cookie(&headers) {
        Some(id) => id,
        None => {
            return (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                "event: datastar-merge-fragments\ndata:fragments <div id='chat-container'><h1 class='major-error-message'>Unable to find connection ID cookie - refresh to attempt to recover</h1><div class='button-center'><button class='big' onclick='window.location.reload()'>Refresh</button></div></div>\n\n"
            ).into_response();
        }
    };

    let mut rooms = state.rooms.lock().await;
    if let Some(room) = rooms.get_mut(&room_id) {
        let person_name = match room.id_to_name.get(&connection_id).cloned() {
            Some(name) => name,
            None => {
                if let Err(e) = room.tx.send(ActionEvent {
                    connection_id: connection_id.clone(),
                    action: Action::MajorError,
                }) {
                    println!("Error broadcasting error event {}", e);
                }
                return StatusCode::OK.into_response();
            }
        };

        let mut new_message = payload.message.clone();
        if payload.message.len() > MAX_MESSAGE_SIZE {
            new_message = "This message was too long! Keep it under 4,000 characters".to_string();
        }

        room.message_history.push(Message{
            name: person_name.clone(),
            content: new_message,
            color: name_to_color(&person_name),
            connection_id: connection_id.clone(),
        });
        room.typing_state.insert(String::from(person_name.clone()), Message{
            name: person_name.clone(),
            content: String::from(""),
            color: name_to_color(&person_name),
            connection_id: connection_id.clone(),
        });
        if let Err(e) = room.tx.send(ActionEvent {
            connection_id: connection_id.clone(),
            action: Action::Send,
        }) {
            println!("Error broadcasting: {}", e);
        }
    }
    StatusCode::OK.into_response()
}

pub async fn set_name(
    headers: HeaderMap,
    State(state): State<Arc<AllRooms>>,
    Path(RoomParams { room_id }): Path<RoomParams>,
    Json(payload): Json<SetNameRequest>,
) -> Response<Body> {
    let connection_id = match get_connection_cookie(&headers) {
        Some(id) => id,
        None => {
            return (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                "event: datastar-merge-fragments\ndata: fragments <div class='error-message'>Missing connection ID cookie</div>\n\n"
            ).into_response();
        }
    };

    let mut rooms = state.rooms.lock().await;
    if let Some(room) = rooms.get_mut(&room_id) {
        if !room.name_to_id.contains_key(&payload.name) {

            // set name if it's available
            room.name_to_id.insert(payload.name.clone(), connection_id.clone());
            room.id_to_name.insert(connection_id.clone(), payload.name.clone());
            room.name_to_color.insert(payload.name.clone(), name_to_color(&payload.name));

            room.typing_state.insert(String::from(payload.name.clone()), Message {
                name: payload.name.clone(),
                content: "".to_string(),
                color: name_to_color(&payload.name),
                connection_id: connection_id.clone(),
            });

            if let Err(e) = room.tx.send(ActionEvent {
                connection_id: connection_id.clone(),
                action: Action::SetName,
            }) {
                println!("Error broadcasting name change: {}", e);
            }
            return (StatusCode::OK, "").into_response();
        } else {
            // name already taken
            let template = SetNameTemplate {
                room_id,
                message: "Name already taken".to_string(),
            }.render().unwrap();

            return (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                format!("event: datastar-merge-fragments\ndata: fragments {}\n\n", template)
            ).into_response();
        }
    } else {
        // room not found
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
            "event: datastar-merge-fragments\ndata: fragments <div class='error-message'>Room not found</div>\n\n"
        ).into_response();
    }
}
