use axum_login::{
    login_required,
    tower_sessions::{ExpiredDeletion, Expiry, SessionManagerLayer},
    AuthManagerLayerBuilder,
};
use tower_http::services::ServeDir;
use axum_messages::MessagesManagerLayer;
use sqlx::SqlitePool;
use time::Duration;
use tokio::{signal, task::AbortHandle};
use tower_sessions::cookie::Key;
use tower_sessions_sqlx_store::SqliteStore;

// mod models;
// mod routes;
use crate::{
    models::users::Backend,
    routes::{auth, protected, public},
};


pub struct App {
    db: SqlitePool,
}

impl App {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let db = SqlitePool::connect(":memory:").await?;
        sqlx::migrate!().run(&db).await?;

        Ok(Self { db })
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        // session layer
        // uses tower-sessions to establish a layer that will provide the session
        // as a request extension
        let session_store = SqliteStore::new(self.db.clone());
        session_store.migrate().await?;

        // session expiry management
        let deletion_task = tokio::task::spawn(
            session_store
                .clone()
                .continuously_delete_expired(tokio::time::Duration::from_secs(60)),
        );

        // generate key to sign the session cookie
        let key = Key::generate();

        // configure the session layer
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_expiry(Expiry::OnInactivity(Duration::days(1)))
            .with_signed(key);

        // auth service
        // combines the session layer with our backend to establish the 
        // auth service which will provide the auth session as a request
        // extension
        let backend = Backend::new(self.db);
        let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

        // let cors_layer = CorsLayer::permissive();
        // let cors_layer = CorsLayer::new()
        //     .allow_headers(Any);
        //     .allow_headers([http::header::CONTENT_TYPE]);

        let app = protected::router()
            // signed in routes like /dashboard go here
            .route_layer(login_required!(Backend, login_url = "/signin"))
            .merge(auth::router())
            .merge(public::router())
            // public routes like login page can go here
            .layer(MessagesManagerLayer)
            .layer(auth_layer)
            // .layer(cors_layer)
            .nest_service("/assets", ServeDir::new("assets"));

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

        // ensure we have a shutdown signal to abort the deletion task
        axum::serve(listener, app.into_make_service())
            .with_graceful_shutdown(shutdown_signal(deletion_task.abort_handle()))
            .await?;

        deletion_task.await?;

        Ok(())
    }
}

async fn shutdown_signal(deletion_task_abort_handle: AbortHandle) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to insert ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { deletion_task_abort_handle.abort() },
        _ = terminate => { deletion_task_abort_handle.abort() },
    }
}
