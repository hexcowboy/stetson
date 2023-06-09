use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::Message;
use axum::extract::{ws::WebSocket, State};
use axum::extract::{ConnectInfo, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::stream::SplitStream;
use futures::{sink::SinkExt, stream::StreamExt};
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::task::JoinSet;

use crate::pubsub::message::{send_error, send_message};

use super::message::PubSubRequest;
use super::process_subscription_message;
use super::subscriber::subscribe;

#[derive(Debug)]
pub struct PubSubState {
    publisher_key: Option<String>,
    // maps topic to a broadcast channel
    pub topics: Mutex<HashMap<String, broadcast::Sender<String>>>,
    // maps topic to receiver and it's a subscription routine
    pub subscriptions: Mutex<HashMap<String, HashMap<SocketAddr, tokio::task::JoinHandle<()>>>>,
}

impl Default for PubSubState {
    fn default() -> Self {
        Self {
            publisher_key: None,
            topics: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
        }
    }
}

impl PubSubState {
    pub fn with_publisher_key(self, publisher_key: Option<String>) -> Self {
        Self {
            publisher_key,
            ..Default::default()
        }
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
    while let Some(Ok(Message::Text(text))) = stream.next().await {
        match process_subscription_message(text) {
            Ok(result) => {
                tracing::info!("received message from {}: {:?}", socket_address, result);

                handle_message(result, state_clone.clone(), sender.clone(), socket_address).await;
            }
            Err(e) => {
                tracing::trace!("error parsing message from {}: {}", socket_address, e);

                // close connection if we can't send error to client
                if send_error(e.to_string(), recv_task_sender.clone())
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    }
}

/// Creates a new subscription for each topic in the request or publishes a message to each topic.
async fn handle_message(
    result: PubSubRequest,
    state: Arc<PubSubState>,
    sender: mpsc::Sender<String>,
    socket_address: SocketAddr,
) {
    let sender = sender.clone();

    match result {
        PubSubRequest::Subscribe { topics } => {
            for topic in topics {
                tracing::info!("subscribing {} to {}", socket_address, topic);

                let receiver = get_or_create_topic_channel(&mut state.clone(), topic.clone()).await;
                let routine = tokio::spawn(subscribe(receiver, sender.clone()));
                let mut current_subscriptions = state.subscriptions.lock().await;
                match current_subscriptions.get_mut(&topic) {
                    Some(subscriptions) => {
                        subscriptions.insert(socket_address, routine);
                    }
                    None => {
                        let mut new_subscriptions = HashMap::new();
                        new_subscriptions.insert(socket_address, routine);
                        current_subscriptions.insert(topic, new_subscriptions);
                    }
                }
            }
        }
        PubSubRequest::Unsubscribe { topics } => {
            for topic in topics {
                let mut subscriptions = state.subscriptions.lock().await;
                match subscriptions.get_mut(&topic) {
                    Some(topics) => {
                        if let Some(routine) = topics.get(&socket_address) {
                            tracing::info!("unsubscribing {} from {}", socket_address, topic);

                            routine.abort();
                            topics.remove(&socket_address);
                        }

                        if topics.is_empty() {
                            tracing::trace!(
                                "deleting {} since there are no more subscribers",
                                &topic
                            );
                            subscriptions.remove(&topic);
                        }
                    }
                    None => {
                        tracing::trace!(
                            "{} tried unsubscribing from {} but was not subscribed",
                            socket_address,
                            topic
                        );
                        continue;
                    }
                }
            }
        }
        PubSubRequest::Publish {
            topics,
            message,
            key,
        } => {
            if key != state.publisher_key {
                tracing::trace!("invalid publisher key");
                if send_error("invalid publisher key", sender).await.is_err() {
                    tracing::trace!("error sending error message to {}", socket_address);
                }
                return;
            }

            for topic in topics {
                match state.topics.lock().await.get(&topic) {
                    Some(transceiver) => {
                        tracing::info!("publishing to {}: {}", topic, message);

                        if send_message(&topic, &message, transceiver).is_err() {
                            tracing::trace!("error sending message to {}", topic);
                        }
                    }
                    None => {
                        tracing::trace!("topic {} does not have any subscribers", topic);
                    }
                };
            }
        }
    }
}

/// Returns a broadcast channel for the given topic. If the topic does not exist, it will be created.
async fn get_or_create_topic_channel(
    state: &mut Arc<PubSubState>,
    topic: String,
) -> broadcast::Receiver<String> {
    let mut topics = state.topics.lock().await;

    match topics.get(&topic) {
        Some(tx) => tx.subscribe(),
        None => {
            tracing::trace!("creating new topic {}", topic);

            let (tx, _rx) = broadcast::channel(1000);
            topics.insert(topic, tx.clone());
            tx.subscribe()
        }
    }
}
