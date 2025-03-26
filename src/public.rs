use askama::Template;
use axum::{
    Router,
    routing::{get, post},
    response::{IntoResponse, Redirect},
    Form,
};
use serde::Deserialize;

#[derive(Template)]
#[template(path="index.html")]
pub struct IndexTemplate {
    show_message: bool,
    message: String,
}

pub fn router() -> Router<()> {
    Router::new()
        .route("/", get(self::get::index))
        .route("/", post(self::post::create_room))
}

mod get {
    use super::*;

    pub async fn index() -> IndexTemplate {
        IndexTemplate{
            show_message: false,
            message: "".to_string(),
        }
    }
}

mod post {
    use super::*;

    #[derive(Debug, Clone, Deserialize)]
    pub struct CreateRoomForm {
        pub room_name: String,
        pub hours: u64,
        pub minutes: u64,
    }

    fn sanitize_room_name(name: &str) -> String {
        name.chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect::<String>()
            .to_lowercase()
    }

    pub async fn create_room(
        Form(create_room_form): Form<CreateRoomForm>,
    ) -> impl IntoResponse {
        let room_path = format!("/room/{}?hours={}&minutes={}", sanitize_room_name(&create_room_form.room_name), create_room_form.hours, create_room_form.minutes);

        if create_room_form.room_name.trim().is_empty() {
            return IndexTemplate{
                show_message: true,
                message: "Enter a room name".to_string(),
            }.into_response()
        }

        Redirect::to(&room_path).into_response()
    }
}
