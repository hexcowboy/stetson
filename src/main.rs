mod pubsub;

use std::env;
use std::panic;
use std::{net::SocketAddr, sync::Arc};

use axum::{routing::get, Router};
use dotenvy::dotenv;
use tokio::task::JoinSet;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::pubsub::{websocket_handler, PubSubState};

#[tokio::main]
async fn main() {
    // load environment variables from .env file
    dotenv().ok();

    // suppress panic messages
    panic::set_hook(Box::new(|_info| {
        // do nothing
    }));

    // enable logging to console
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or("stetson=trace,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // load publish key or warn use if it's not set
    let publish_key: Option<String> = env::var("PUBLISH_KEY").ok();
    if publish_key.is_none() {
        tracing::warn!("PUBLISH_KEY environment variable not set. Any client can publish messages to the server.");
    }

    // create global state for web server
    let state = Arc::new(PubSubState::default().with_publisher_key(publish_key));

    // define application routes
    let app = Router::new()
        .route("/", get(websocket_handler))
        .with_state(state.clone())
        // enable tracing for all tower http requests
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );

    // create joinset for all tasks
    let mut set = JoinSet::new();

    // run the server
    set.spawn(async move {
        let port: u16 = env::var("PORT")
            .unwrap_or("3000".to_string())
            .parse()
            .unwrap_or(3000);
        let addr: SocketAddr = env::var("ADDRESS")
            .unwrap_or(format!("0.0.0.0:{}", port))
            .parse()
            .unwrap_or(([0, 0, 0, 0], port).into());
        tracing::info!("listening on {}", addr);

        match axum::Server::try_bind(&addr) {
            Ok(server) => {
                tracing::info!("web server started");
                if let Err(axum_error) = server
                    .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                    .await
                {
                    tracing::error!("web server shut down: {}", axum_error);
                }
            }
            Err(e) => {
                tracing::error!("web server failed to start: {}", e);
            }
        }
    });

    // wait for all tasks to complete
    while let Some(res) = set.join_next().await {
        match res {
            Ok(_) => tracing::trace!("task completed"),
            Err(e) => tracing::error!("task failed: {}", e),
        }
    }

    tracing::error!("server shutting down unexpectedly");
}
