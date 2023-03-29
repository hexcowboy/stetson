use tokio::sync::{broadcast::Receiver, mpsc::Sender};

pub async fn subscribe(mut receiver: Receiver<String>, sender: Sender<String>) {
    while let Ok(msg) = receiver.recv().await {
        if sender.send(msg).await.is_err() {
            break;
        }
    }
}
