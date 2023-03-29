use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::Message;
use axum::extract::{ws::WebSocket, State};
use axum::extract::{ConnectInfo, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::stream::SplitStream;
use futures::{sink::SinkExt, stream::StreamExt};
use tokio::sync::mpsc::error::SendError;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinSet;

use super::message::PubSubRequest;
use super::process_subscription_message;
use super::subscriber::subscribe;

pub struct PubSubState {
    // maps topic to a broadcast channel
    pub tx: HashMap<String, broadcast::Sender<String>>,
}

impl Default for PubSubState {
    fn default() -> Self {
        Self { tx: HashMap::new() }
    }
}

/// Upgrades an HTTP(s) connection to a websocket connection.
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<PubSubState>>,
    ConnectInfo(socket_address): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| websocket(socket, state, socket_address))
}

/// Handles websocket connections by processing incoming messages as subscription requests and
/// sending outgoing messages to subscribed users.
async fn websocket(stream: WebSocket, state: Arc<PubSubState>, socket_address: SocketAddr) {
    // split the stream to allow for simultaneous sending and receiving
    let (mut sink, stream) = stream.split();

    // create an mpsc so we can send messages to the stream from multiple threads
    let (sender, mut receiver) = mpsc::channel::<String>(1000);

    let mut socket_join_set = JoinSet::new();

    // since SplitSinks are not thread safe, we create an mpsc channel that will forward
    // messages to the SplitSink (`sink`)
    socket_join_set.spawn(async move {
        while let Some(message) = receiver.recv().await {
            if let Err(e) = sink.send(Message::Text(message)).await {
                tracing::trace!("error sending message to {}: {}", socket_address, e);
                break;
            }
        }
    });

    // listens for new messages from the user to update their subscription
    let recv_task_sender = sender.clone();
    let state_clone = state.clone();
    socket_join_set.spawn(listen_for_messages(
        stream,
        state_clone,
        sender,
        socket_address,
        recv_task_sender,
    ));

    // blocks until any task finishes
    socket_join_set.join_next().await;
    socket_join_set.abort_all();

    tracing::trace!("websocket closed for {}", socket_address);
}

/// Listens for new messages from the client.
async fn listen_for_messages(
    mut stream: SplitStream<WebSocket>,
    state_clone: Arc<PubSubState>,
    sender: mpsc::Sender<String>,
    socket_address: SocketAddr,
    recv_task_sender: mpsc::Sender<String>,
) {
    let mut join_set = JoinSet::new();

    while let Some(Ok(Message::Text(text))) = stream.next().await {
        match process_subscription_message(text) {
            Ok(result) => {
                tracing::info!("received message from {}: {:?}", socket_address, result);

                // remove old subscriptions
                join_set.abort_all();

                // create new subscriptions
                join_set =
                    match create_subscriptions(result, state_clone.clone(), sender.clone()).await {
                        Ok(join_set) => join_set,
                        Err(e) => {
                            tracing::trace!("error creating subscriptions: {}", e);
                            break;
                        }
                    }
            }
            Err(e) => {
                tracing::trace!("error parsing message from {}: {}", socket_address, e);

                // close connection if we can't send error to client
                if recv_task_sender.send(e.to_string()).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// Creates a new subscription for each topic in the request or publishes a message to each topic.
async fn create_subscriptions(
    result: PubSubRequest,
    state: Arc<PubSubState>,
    sender: mpsc::Sender<String>,
) -> Result<JoinSet<()>, SendError<String>> {
    let mut join_set = JoinSet::new();
    let sender = sender.clone();

    match result {
        PubSubRequest::Subscribe { topics } => {
            for topic in topics {
                let receiver = get_or_create_topic_channel(&mut state.clone(), topic.clone());

                tracing::info!("subscribing to {}", topic);
                join_set.spawn(subscribe(receiver, sender.clone()));
            }
        }
        PubSubRequest::Publish { topics, message } => {
            for topic in topics {
                tracing::info!("publishing to {}: {}", topic, message);
                sender.send(message.clone()).await?;
            }
        }
    }

    Ok(join_set)
}

/// Returns a broadcast channel for the given topic. If the topic does not exist, it will be created.
fn get_or_create_topic_channel(
    state: &mut Arc<PubSubState>,
    topic: String,
) -> broadcast::Receiver<String> {
    match state.tx.get(&topic) {
        Some(tx) => tx.subscribe(),
        None => {
            let (tx, _rx) = broadcast::channel(1000);
            state.tx.clone().insert(topic, tx.clone());
            tx.subscribe()
        }
    }
}
