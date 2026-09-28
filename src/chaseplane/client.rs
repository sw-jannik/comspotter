use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpStream;
use tokio::sync::{Mutex as TokioMutex, oneshot};
use tokio::time::timeout;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

use super::error::{Error, Result};
use super::message::{ApiReply, ServerMessage};
use super::traffic::TrafficInfo;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

const DEFAULT_URL: &str = "ws://127.0.0.1:8652/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// A connection to ChasePlane's websocket API, tracking known AI traffic and
/// letting callers request that a specific aircraft be tracked/viewed.
pub struct ChaseplaneClient {
    write: Arc<TokioMutex<SplitSink<WsStream, Message>>>,
    traffic: Arc<StdMutex<HashMap<u64, TrafficInfo>>>,
    pending: Arc<StdMutex<HashMap<String, oneshot::Sender<ApiReply>>>>,
    request_counter: AtomicU64,
}

impl ChaseplaneClient {
    pub async fn connect_default() -> Result<Self> {
        Self::connect(DEFAULT_URL).await
    }

    pub async fn connect(url: &str) -> Result<Self> {
        let (socket, _) = connect_async(url).await?;
        let (write, read) = socket.split();

        let client = Self {
            write: Arc::new(TokioMutex::new(write)),
            traffic: Arc::new(StdMutex::new(HashMap::new())),
            pending: Arc::new(StdMutex::new(HashMap::new())),
            request_counter: AtomicU64::new(0),
        };

        client
            .send_message(json!({
                "message": "api_connect",
                "payload": { "client_name": "comspotter" }
            }))
            .await?;

        client
            .send_message(json!({
                "message": "api_request",
                "request_id": client.next_request_id("request_ai_traffic"),
                "command": "request_ai_traffic",
                "payload": {}
            }))
            .await?;

        tokio::spawn(Self::read_loop(
            read,
            client.traffic.clone(),
            client.pending.clone(),
        ));

        Ok(client)
    }

    async fn read_loop(
        mut read: SplitStream<WsStream>,
        traffic: Arc<StdMutex<HashMap<u64, TrafficInfo>>>,
        pending: Arc<StdMutex<HashMap<String, oneshot::Sender<ApiReply>>>>,
    ) {
        while let Some(message) = read.next().await {
            let message = match message {
                Ok(message) => message,
                Err(err) => {
                    eprintln!("chaseplane: websocket error: {err}");
                    break;
                }
            };

            match message {
                Message::Text(text) => Self::handle_text(&text, &traffic, &pending),
                Message::Close(_) => break,
                _ => {}
            }
        }
        // Unblock any in-flight track_by_id/track_by_callsign calls with ReaderTaskEnded.
        pending.lock().unwrap().clear();
    }

    fn handle_text(
        text: &str,
        traffic: &StdMutex<HashMap<u64, TrafficInfo>>,
        pending: &StdMutex<HashMap<String, oneshot::Sender<ApiReply>>>,
    ) {
        let message: ServerMessage = match serde_json::from_str(text) {
            Ok(message) => message,
            Err(err) => {
                eprintln!("chaseplane: failed to parse message: {err}");
                return;
            }
        };

        match message {
            ServerMessage::AiTraffic { payload } => {
                let mut traffic = traffic.lock().unwrap();
                for entry in payload.added.into_iter().chain(payload.updated) {
                    traffic.insert(entry.uid, entry);
                }
                for uid in payload.removed {
                    traffic.remove(&uid);
                }
            }
            ServerMessage::ApiReply {
                request_id,
                status,
                payload,
            } => {
                if let Some(sender) = pending.lock().unwrap().remove(&request_id) {
                    let _ = sender.send(ApiReply {
                        request_id,
                        status,
                        payload,
                    });
                }
            }
            ServerMessage::Other => {}
        }
    }

    fn next_request_id(&self, command: &str) -> String {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let counter = self.request_counter.fetch_add(1, Ordering::Relaxed);
        format!("{command}_{millis}_{counter}")
    }

    async fn send_message(&self, message: serde_json::Value) -> Result<()> {
        let mut write = self.write.lock().await;
        write
            .send(Message::Text(message.to_string().into()))
            .await?;
        Ok(())
    }

    async fn send_request(
        &self,
        command: &'static str,
        payload: serde_json::Value,
    ) -> Result<ApiReply> {
        let request_id = self.next_request_id(command);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(request_id.clone(), tx);

        let message = json!({
            "message": "api_request",
            "request_id": request_id,
            "command": command,
            "payload": payload,
        });

        if let Err(err) = self.send_message(message).await {
            self.pending.lock().unwrap().remove(&request_id);
            return Err(err);
        }

        match timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(_)) => Err(Error::ReaderTaskEnded),
            Err(_) => {
                self.pending.lock().unwrap().remove(&request_id);
                Err(Error::Timeout(command))
            }
        }
    }

    /// Requests that ChasePlane track/follow the aircraft with the given uId.
    pub async fn track_by_id(&self, uid: u64) -> Result<ApiReply> {
        self.send_request("ai_traffic_track", json!({ "id": uid }))
            .await
    }

    /// Looks up the given callsign in the currently known traffic and tracks it.
    pub async fn track_by_callsign(&self, callsign: &str) -> Result<ApiReply> {
        let uid = self
            .find_by_callsign(callsign)
            .map(|traffic| traffic.uid)
            .ok_or_else(|| Error::UnknownCallsign(callsign.to_string()))?;
        self.track_by_id(uid).await
    }

    /// Snapshot of all currently known AI traffic.
    pub fn traffic(&self) -> Vec<TrafficInfo> {
        self.traffic.lock().unwrap().values().cloned().collect()
    }

    pub fn get_traffic(&self, uid: u64) -> Option<TrafficInfo> {
        self.traffic.lock().unwrap().get(&uid).cloned()
    }

    pub fn find_by_callsign(&self, callsign: &str) -> Option<TrafficInfo> {
        self.traffic
            .lock()
            .unwrap()
            .values()
            .find(|traffic| traffic.callsign.eq_ignore_ascii_case(callsign))
            .cloned()
    }
}
