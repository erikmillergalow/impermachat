use askama::Template;

#[derive(Template)]
#[template(path="index.html")]
pub struct IndexTemplate {
    pub show_message: bool,
    pub message: String,
}

