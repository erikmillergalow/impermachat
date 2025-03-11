use axum_login::tower_sessions::ExpiredDeletion;
use tower_http::services::ServeDir;
use sqlx::SqlitePool;
use tokio::{
    signal,
    task::AbortHandle,
    net::TcpListener,
};
use tower_sessions_sqlx_store::SqliteStore;
use listenfd::ListenFd;

use crate::routes::{
    public, rooms
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

        // let cors_layer = CorsLayer::permissive();
        // let cors_layer = CorsLayer::new()
        //     .allow_headers(Any);
        //     .allow_headers([http::header::CONTENT_TYPE]);

        let app = public::router()
            .merge(rooms::router())
            .nest_service("/assets", ServeDir::new("assets"));

        // let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
        let mut listenfd = ListenFd::from_env();
        let listener = match listenfd.take_tcp_listener(0).unwrap() {
            // use listen fd 0 if provided
            Some(listener) => {
                listener.set_nonblocking(true).unwrap();
                TcpListener::from_std(listener).unwrap()
            }
            // otherwise fall back to local listening
            None => TcpListener::bind("0.0.0.0:8080").await.unwrap(),
        };
        println!("listening on {}", listener.local_addr().unwrap());

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
