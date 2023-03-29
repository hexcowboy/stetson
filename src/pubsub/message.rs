use serde::Deserialize;

#[derive(PartialEq, Eq, Hash, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PubSubRequest {
    Subscribe {
        topics: Vec<String>,
    },
    Publish {
        topics: Vec<String>,
        message: String,
    },
}

pub fn process_subscription_message(message: impl ToString) -> serde_json::Result<PubSubRequest> {
    serde_json::from_str(&message.to_string())
}
