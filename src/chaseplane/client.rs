use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, Weak};
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
use super::view::{self, GetViewsReply, View};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

const DEFAULT_URL: &str = "ws://127.0.0.1:8652/";
pub const DEFAULT_VIEW_THEME: &str = "WORLD_TOWER";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// A connection to ChasePlane's websocket API, tracking known AI traffic and
/// letting callers request that a specific aircraft be tracked/viewed.
pub struct ChaseplaneClient {
    write: Arc<TokioMutex<SplitSink<WsStream, Message>>>,
    traffic: Arc<StdMutex<HashMap<u64, TrafficInfo>>>,
    pending: Arc<StdMutex<HashMap<String, oneshot::Sender<ApiReply>>>>,
    request_counter: AtomicU64,
    // ICAO of the airport currently active in ChasePlane, from `airport_changed`.
    active_icao: StdMutex<Option<String>>,
    // Saved views for the active airport and configured theme only.
    views: StdMutex<Vec<View>>,
    view_theme: String,
}

impl ChaseplaneClient {
    pub async fn connect_default(view_theme: &str) -> Result<Arc<Self>> {
        Self::connect(DEFAULT_URL, view_theme).await
    }

    /// Only views whose `profile_theme` equals `view_theme` are kept.
    pub async fn connect(url: &str, view_theme: &str) -> Result<Arc<Self>> {
        let (socket, _) = connect_async(url).await?;
        let (write, read) = socket.split();

        let client = Self {
            write: Arc::new(TokioMutex::new(write)),
            traffic: Arc::new(StdMutex::new(HashMap::new())),
            pending: Arc::new(StdMutex::new(HashMap::new())),
            request_counter: AtomicU64::new(0),
            active_icao: StdMutex::new(None),
            views: StdMutex::new(Vec::new()),
            view_theme: view_theme.to_string(),
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

        let client = Arc::new(client);
        tokio::spawn(Self::read_loop(read, Arc::downgrade(&client)));
        client.spawn_refresh_views();

        Ok(client)
    }

    async fn read_loop(mut read: SplitStream<WsStream>, client: Weak<Self>) {
        while let Some(message) = read.next().await {
            let message = match message {
                Ok(message) => message,
                Err(err) => {
                    eprintln!("chaseplane: websocket error: {err}");
                    break;
                }
            };

            let Some(client) = client.upgrade() else {
                return;
            };
            match message {
                Message::Text(text) => client.handle_text(&text),
                Message::Close(_) => break,
                _ => {}
            }
        }
        // Unblock any in-flight track_by_id/track_by_callsign calls with ReaderTaskEnded.
        if let Some(client) = client.upgrade() {
            client.pending.lock().unwrap().clear();
        }
    }

    fn handle_text(self: &Arc<Self>, text: &str) {
        let message: ServerMessage = match serde_json::from_str(text) {
            Ok(message) => message,
            Err(err) => {
                eprintln!("chaseplane: failed to parse message: {err}");
                return;
            }
        };

        match message {
            ServerMessage::AiTraffic { payload } => {
                let mut traffic = self.traffic.lock().unwrap();
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
                if let Some(sender) = self.pending.lock().unwrap().remove(&request_id) {
                    let _ = sender.send(ApiReply {
                        request_id,
                        status,
                        payload,
                    });
                }
            }
            ServerMessage::AirportChanged { payload } => {
                let Some(ident) = payload.active_ident.filter(|ident| !ident.is_empty()) else {
                    return;
                };
                {
                    let mut active = self.active_icao.lock().unwrap();
                    if active.as_deref() != Some(ident.as_str()) {
                        println!("🛫 Active airport: {ident}");
                        self.views.lock().unwrap().clear();
                    }
                    *active = Some(ident);
                }
                self.spawn_refresh_views();
            }
            ServerMessage::Other => {}
        }
    }

    fn spawn_refresh_views(self: &Arc<Self>) {
        let client = self.clone();
        tokio::spawn(async move {
            if let Err(err) = client.refresh_views().await {
                eprintln!("chaseplane: failed to load views: {err}");
            }
        });
    }

    /// Fetches all views from ChasePlane and keeps those for the active airport and configured theme.
    /// If the airport isn't known yet, nothing is kept; the next `airport_changed` refreshes again.
    pub async fn refresh_views(&self) -> Result<()> {
        let reply = self.send_request("get_views", json!({})).await?;
        if !reply.is_success() {
            eprintln!("chaseplane: get_views failed with status {}", reply.status);
            return Ok(());
        }
        let parsed: GetViewsReply = serde_json::from_value(reply.payload)?;

        let Some(icao) = self.active_icao.lock().unwrap().clone() else {
            return Ok(());
        };
        let views = view::filter_views(parsed.payload.views, &icao, &self.view_theme);
        println!("🎥 Loaded {} {} view(s) for {icao}", views.len(), self.view_theme);
        *self.views.lock().unwrap() = views;
        Ok(())
    }

    /// Switches ChasePlane to the saved view with the given guid.
    pub async fn set_view_by_guid(&self, guid: &str) -> Result<ApiReply> {
        self.send_request("set_view_by_guid", json!({ "guid": guid }))
            .await
    }

    /// The saved view (active airport, configured theme) closest horizontally to the given position.
    pub fn closest_view(&self, lat: f64, lon: f64) -> Option<View> {
        view::closest(&self.views.lock().unwrap(), lat, lon).cloned()
    }

    /// Snapshot of the saved views for the active airport.
    pub fn views(&self) -> Vec<View> {
        self.views.lock().unwrap().clone()
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

    /// Enables or disables ChasePlane's auto-spot (automatically track whichever AI traffic is active).
    pub async fn set_auto_target(&self, enabled: bool) -> Result<ApiReply> {
        self.send_request("ai_traffic_auto_target_set", json!({ "enabled": enabled }))
            .await
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
