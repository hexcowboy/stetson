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
    dotenv().ok();

    // suppress panic messages
    panic::set_hook(Box::new(|_info| {
        // do nothing
    }));

    // enable logging to console
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "stetson=trace,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // create global state for web server
    let state = Arc::new(PubSubState::default());

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

        if let Err(axum_error) = axum::Server::bind(&addr)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
        {
            tracing::error!("web server shut down: {}", axum_error);
        }
    });

    // wait for all tasks to complete
    while let Some(res) = set.join_next().await {
        match res {
            Ok(_) => tracing::trace!("task completed"),
            Err(e) => tracing::error!("task failed: {}", e),
        }
    }

    tracing::info!("server shutting down after all tasks finished");
}
