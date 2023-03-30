use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

#[derive(PartialEq, Eq, Hash, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PubSubRequest {
    Subscribe {
        topics: Vec<String>,
    },
    Unsubscribe {
        topics: Vec<String>,
    },
    Publish {
        topics: Vec<String>,
        message: String,
        key: Option<String>,
    },
}

#[derive(PartialEq, Eq, Hash, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PubSubResponse {
    Message { topic: String, message: String },
    Error { message: String },
}

pub fn process_subscription_message(message: impl ToString) -> serde_json::Result<PubSubRequest> {
    serde_json::from_str(&message.to_string())
}

/// Serializes a message and sends it to the given sender.
pub fn send_message(
    topic: &impl ToString,
    message: &impl ToString,
    sender: &broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let message = PubSubResponse::Message {
        topic: topic.to_string(),
        message: message.to_string(),
    };
    let message = serde_json::to_string(&message)?;
    sender.send(message)?;
    Ok(())
}

/// Serializes an error message and sends it to the given sender.
pub async fn send_error(
    message: impl ToString,
    sender: mpsc::Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let message = PubSubResponse::Error {
        message: message.to_string(),
    };
    let message = serde_json::to_string(&message)?;
    sender.send(message).await?;
    Ok(())
}
