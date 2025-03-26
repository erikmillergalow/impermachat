use std::collections::HashMap;
use askama::Template;

use crate::rooms::Message;

#[derive(Template)]
#[template(path="room.html")]
pub struct RoomTemplate {
    pub room_id: String,
}

// #[derive(Template)]
// #[template(path = "message.html")]
// pub struct MessageTemplate {
//     message: String,
//     person: String,
// }

#[derive(Template)]
#[template(path = "shutdown_room.html")]
pub struct ShutdownTemplate {}

#[derive(Template)]
#[template(path = "submit_message.html")]
pub struct SubmitTemplate {
    pub messages: Vec<Message>,
    pub connection_id: String,
}

#[derive(Template)]
#[template(path = "typing_messages.html")]
pub struct TypingTemplate {
    pub messages: HashMap<String, Message>,
    pub connection_id: String,
}

#[derive(Template)]
#[template(path = "init_name.html")]
pub struct InitNameTemplate {
    pub room_id: String,
}

#[derive(Template)]
#[template(path = "major_error.html")]
pub struct MajorErrorTemplate {}

#[derive(Template)]
#[template(path = "chat_input.html")]
pub struct ChatInputTemplate {
    pub room_id: String,
    pub person: String,
}

#[derive(Template)]
#[template(path = "set_name.html")]
pub struct SetNameTemplate {
    pub room_id: String,
    pub message: String,
}

// #[derive(Template)]
// #[template(path = "status_message.html")]
// pub struct StatusMessageTemplate {
//     target_id: String,
//     message: String,
// }
