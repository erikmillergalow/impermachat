use askama::Template;
use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use axum_messages::{Message, Messages};
use serde::Deserialize;
// use axum_login::tracing::{event, Level};

use crate::models::users::{AuthSession, Credentials, RegisterCredentials};

#[derive(Template)]
#[template(path = "signin.html")]
pub struct SigninTemplate {
    messages: Vec<Message>,
    next: Option<String>,
}

#[derive(Template)]
#[template(path = "signup.html")]
pub struct SignupTemplate {
    messages: Vec<Message>,
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NextUrl {
    next: Option<String>,
}

pub fn router() -> Router<()> {
    Router::new()
        .route("/signin", post(self::post::signin))
        .route("/signin", get(self::get::signin))
        .route("/signup", post(self::post::signup))
        .route("/signup", get(self::get::signup))
        .route("/logout", get(self::get::logout))
}

mod post {
    use super::*;

    pub async fn signup(
        mut auth_session: AuthSession,
        messages: Messages,
        Form(register_creds): Form<RegisterCredentials>,
    ) -> impl IntoResponse {
        println!("{}", register_creds.username);

        // check if user is already logged in
        if auth_session.user.is_some() {
            return StatusCode::BAD_REQUEST.into_response();
        }

        // check if passwords match
        if register_creds.password != register_creds.confirm_password {
            return StatusCode::UNAUTHORIZED.into_response();
        }

        // create new user
        let user = match auth_session.backend.create_user(register_creds.clone()).await {
            Ok(user) => user,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

        // sign in new user
        if auth_session.login(&user).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        Redirect::to("/overview").into_response()
    }

    pub async fn signin(
        mut auth_session: AuthSession,
        messages: Messages,
        Form(creds): Form<Credentials>,
    ) -> impl IntoResponse {
        let user = match auth_session.authenticate(creds.clone()).await {
            Ok(Some(user)) => user,
            Ok(None) => {
                messages.error("Invalid credentials");

                let mut login_url = "/signin".to_string();
                if let Some(next) = creds.next {
                    login_url = format!("{}?next={}", login_url, next);
                };

                return Redirect::to(&login_url).into_response();
            }
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

        if auth_session.login(&user).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        messages.success(format!("Successfully logged in as {}", user.username));

        if let Some(ref next) = creds.next {
            Redirect::to(next)
        } else {
            Redirect::to("/overview")
        }
        .into_response()
    }
}

mod get {
    use super::*;

    // return signin page with any relevant status messages
    pub async fn signin(
        messages: Messages,
        Query(NextUrl { next }): Query<NextUrl>,
    ) -> SigninTemplate {
        SigninTemplate {
            messages: messages.into_iter().collect(),
            next,
        }
    }

    pub async fn signup(
        messages: Messages,
        Query(NextUrl { next }): Query<NextUrl>,
    ) -> SignupTemplate {
        SignupTemplate {
            messages: messages.into_iter().collect(),
            next,
        }
    }

    pub async fn logout(mut auth_session: AuthSession) -> impl IntoResponse {
        match auth_session.logout().await {
            Ok(_) => Redirect::to("/logout").into_response(),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}








