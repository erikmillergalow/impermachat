use tower_http::services::ServeDir;
use tokio::{
    signal,
    net::TcpListener,
};
use listenfd::ListenFd;

use crate::public;
use crate::routes::rooms_router;

pub struct App {}

impl App {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self{})
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        // let cors_layer = CorsLayer::permissive();
        // let cors_layer = CorsLayer::new()
        //     .allow_headers(Any);
        //     .allow_headers([http::header::CONTENT_TYPE]);

        let app = public::router()
            .merge(rooms_router())
            .nest_service("/assets", ServeDir::new("assets"));

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
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        Ok(())
    }
}

async fn shutdown_signal() {
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
        _ = ctrl_c => { println!("Shutdown triggered by ctrl-c") },
        _ = terminate => { println!("Shutdown triggered by termination") },
    }
}
