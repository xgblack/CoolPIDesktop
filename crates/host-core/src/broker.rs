use crate::task::{HostEvent, TaskSnapshot};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    net::TcpListener,
    sync::{OnceCell, RwLock, broadcast},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use uuid::Uuid;

#[cfg(test)]
const OBSERVER_BUFFER: usize = 512;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObserverInfo {
    pub url: String,
    pub token: String,
}

/// Loopback-only, read-only event stream. The first WebSocket message must be
/// `{ "token": "..." }`; all later client messages are ignored as controls.
#[derive(Clone)]
pub struct EventBroker {
    tx: broadcast::Sender<HostEvent>,
    token: String,
    listener: Arc<OnceCell<ObserverInfo>>,
}

impl EventBroker {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity.max(1));
        Self {
            tx,
            token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
            listener: Arc::new(OnceCell::new()),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HostEvent> {
        self.tx.subscribe()
    }
    pub fn publish(&self, event: HostEvent) {
        let _ = self.tx.send(event);
    }
    pub fn recover(snapshot: &TaskSnapshot, after_seq: u64) -> Vec<HostEvent> {
        snapshot
            .events
            .iter()
            .filter(|event| event.seq > after_seq)
            .cloned()
            .collect()
    }

    pub async fn observer_info(
        &self,
        snapshots: Arc<RwLock<HashMap<String, Arc<RwLock<TaskSnapshot>>>>>,
    ) -> std::io::Result<ObserverInfo> {
        let broker = self.clone();
        let info = self
            .listener
            .get_or_try_init(|| async move {
                let listener = TcpListener::bind("127.0.0.1:0").await?;
                let address = listener.local_addr()?;
                let info = ObserverInfo {
                    url: format!("ws://{address}"),
                    token: broker.token.clone(),
                };
                tokio::spawn(serve(listener, broker, snapshots));
                Ok::<_, std::io::Error>(info)
            })
            .await?;
        Ok(info.clone())
    }
}

async fn snapshot_payload(
    snapshots: &Arc<RwLock<HashMap<String, Arc<RwLock<TaskSnapshot>>>>>,
) -> Value {
    let values: Vec<_> = snapshots.read().await.values().cloned().collect();
    let mut output = Vec::with_capacity(values.len());
    for snapshot in values {
        output.push(snapshot.read().await.clone());
    }
    json!({"type":"snapshot", "tasks":output})
}

async fn recovery_payload(
    snapshots: &Arc<RwLock<HashMap<String, Arc<RwLock<TaskSnapshot>>>>>,
    after_seq: &HashMap<String, u64>,
) -> Value {
    let values: Vec<_> = snapshots.read().await.values().cloned().collect();
    let mut events = Vec::new();
    for snapshot in values {
        let snapshot = snapshot.read().await;
        events.extend(EventBroker::recover(
            &snapshot,
            *after_seq.get(&snapshot.task_id).unwrap_or(&snapshot.seq),
        ));
    }
    json!({"type":"recovery", "events":events})
}

async fn serve(
    listener: TcpListener,
    broker: EventBroker,
    snapshots: Arc<RwLock<HashMap<String, Arc<RwLock<TaskSnapshot>>>>>,
) {
    while let Ok((stream, _)) = listener.accept().await {
        let broker = broker.clone();
        let snapshots = snapshots.clone();
        tokio::spawn(async move {
            let Ok(mut socket) = accept_async(stream).await else {
                return;
            };
            let Some(Ok(message)) = socket.next().await else {
                return;
            };
            let auth = message
                .into_text()
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok());
            let supplied = auth.as_ref().and_then(|value| value["token"].as_str());
            if supplied != Some(&broker.token) {
                let _ = socket
                    .send(Message::Text(
                        json!({"type":"error", "code":"unauthorized"})
                            .to_string()
                            .into(),
                    ))
                    .await;
                let _ = socket.close(None).await;
                return;
            }
            let after_seq = auth
                .as_ref()
                .and_then(|value| value["afterSeq"].as_object())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|(task, seq)| seq.as_u64().map(|seq| (task.clone(), seq)))
                        .collect()
                })
                .unwrap_or_default();
            if socket
                .send(Message::Text(
                    snapshot_payload(&snapshots).await.to_string().into(),
                ))
                .await
                .is_err()
            {
                return;
            }
            let recovery = recovery_payload(&snapshots, &after_seq).await;
            if recovery["events"]
                .as_array()
                .is_some_and(|events| !events.is_empty())
                && socket
                    .send(Message::Text(recovery.to_string().into()))
                    .await
                    .is_err()
            {
                return;
            }
            let mut events = broker.subscribe();
            loop {
                tokio::select! {
                    event = events.recv() => match event {
                        Ok(event) => if socket.send(Message::Text(json!({"type":"event", "event":event}).to_string().into())).await.is_err() { return; },
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            if socket.send(Message::Text(snapshot_payload(&snapshots).await.to_string().into())).await.is_err() { return; }
                        },
                        Err(broadcast::error::RecvError::Closed) => return,
                    },
                    incoming = socket.next() => match incoming {
                        Some(Ok(Message::Close(_))) | None => return,
                        Some(Ok(_)) => if socket.send(Message::Text(json!({"type":"error", "code":"read_only"}).to_string().into())).await.is_err() { return; },
                        Some(Err(_)) => return,
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    #[tokio::test]
    async fn loopback_observer_requires_token_and_never_accepts_commands() {
        let broker = EventBroker::new(OBSERVER_BUFFER);
        let snapshots = Arc::new(RwLock::new(HashMap::new()));
        let info = broker.observer_info(snapshots).await.unwrap();

        let (mut denied, _) = connect_async(&info.url).await.unwrap();
        denied.send(Message::Text("{}".into())).await.unwrap();
        assert!(
            denied
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap()
                .contains("unauthorized")
        );

        let (mut allowed, _) = connect_async(&info.url).await.unwrap();
        allowed
            .send(Message::Text(
                json!({"token":info.token}).to_string().into(),
            ))
            .await
            .unwrap();
        assert!(
            allowed
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap()
                .contains("snapshot")
        );
        allowed
            .send(Message::Text(json!({"type":"prompt"}).to_string().into()))
            .await
            .unwrap();
        assert!(
            allowed
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap()
                .contains("read_only")
        );
    }
}
