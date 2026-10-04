use serde::Deserialize;

use super::traffic::TrafficInfo;

#[derive(Debug, Deserialize)]
#[serde(tag = "message", rename_all = "snake_case")]
pub(crate) enum ServerMessage {
    AiTraffic {
        payload: AiTrafficPayload,
    },
    ApiReply {
        request_id: String,
        status: u16,
        payload: serde_json::Value,
    },
    AirportChanged {
        payload: AirportChangedPayload,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AirportChangedPayload {
    #[serde(rename = "activeIdent", default)]
    pub active_ident: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AiTrafficPayload {
    #[serde(default)]
    pub added: Vec<TrafficInfo>,
    #[serde(default)]
    pub updated: Vec<TrafficInfo>,
    #[serde(default)]
    pub removed: Vec<u64>,
}

/// The response to a request sent via `ChaseplaneClient::track_by_id`/`track_by_callsign`.
#[derive(Debug, Clone)]
pub struct ApiReply {
    pub request_id: String,
    pub status: u16,
    pub payload: serde_json::Value,
}

impl ApiReply {
    pub fn is_success(&self) -> bool {
        self.status == 200
    }
}
