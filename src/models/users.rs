use async_trait::async_trait;
use axum_login::{AuthUser, AuthnBackend, UserId};
use password_auth::{verify_password, generate_hash};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use tokio::task;

#[derive(Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    id: i64,
    pub username: String,
    password: String,
}

// implement debug as to not log the password hash
impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("password", &"[redacted]")
            .finish()
    }
}

impl AuthUser for User {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    // use password hash as auth hash - when the user changes the password
    // the auth session becomes invalid
    fn session_auth_hash(&self) -> &[u8] {
        self.password.as_bytes() 
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    pub next: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterCredentials {
    pub username: String,
    pub password: String,
    pub confirm_password: String,
    pub next: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Backend {
    db: SqlitePool,
}

impl Backend {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    pub async fn create_user(
        &self,
        register_creds: RegisterCredentials,
    ) -> Result<User, Error> {
        // generate hash from password
        let password_hash = tokio::task::spawn_blocking(|| {
            generate_hash(register_creds.password)
        }).await?;

        // add new user to db
        match sqlx::query_as::<_, User>("insert into users (username, password) values (?, ?) returning *")
            .bind(&register_creds.username)
            .bind(&password_hash)
            .fetch_one(&self.db)
            .await {
                Ok(user) => Ok(user),
                Err(e) => {
                    println!("Database error: {:?}", e);
                    Err(e.into())
                }
            }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    #[error(transparent)]
    TaskJoin(#[from] task::JoinError),
}

#[async_trait]
impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = Error;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user: Option<Self::User> = sqlx::query_as("select * from users where username = ? ")
            .bind(creds.username)
            .fetch_optional(&self.db)
            .await?;

        // verifing password is blocking/potentially slow, use 'spawn_blocking'
        task::spawn_blocking(|| {
            // password-based auth, compare form input with argon2 password hash
            Ok(user.filter(|user| verify_password(creds.password, &user.password).is_ok()))
        })
        .await?
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        let user = sqlx::query_as("select * from users where id = ?")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await?;

        Ok(user)
    }
}

pub type AuthSession = axum_login::AuthSession<Backend>;
