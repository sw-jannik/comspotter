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
    CameraRuntimeState {
        payload: CameraRuntimeState,
    },
    #[serde(other)]
    Other,
}

/// ChasePlane's continuously sent camera state (`camera_runtime_state`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CameraRuntimeState {
    #[serde(default)]
    pub camera_position: Option<CameraPosition>,
    /// Null when the camera isn't looking at anything.
    #[serde(default)]
    pub look_at_position: Option<LookAtPosition>,
    #[serde(default)]
    pub ai_traffic_tracking_id: Option<u64>,
    #[serde(default)]
    pub ai_traffic_auto_target_enabled: bool,
    #[serde(default)]
    pub master_enabled: bool,
}

/// Camera position; x = longitude, y = altitude, z = latitude (same convention as saved views).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CameraPosition {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub z: f64,
    #[serde(default)]
    pub pitch: f64,
    #[serde(default)]
    pub yaw: f64,
    #[serde(default)]
    pub roll: f64,
    #[serde(default)]
    pub zoom: f64,
    #[serde(default)]
    pub preset_zoom: f64,
    #[serde(default)]
    pub rendered_zoom: f64,
    #[serde(default)]
    pub agl_min: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LookAtPosition {
    #[serde(default)]
    pub longitude: f64,
    #[serde(default)]
    pub latitude: f64,
    #[serde(default)]
    pub altitude: f64,
    #[serde(default)]
    pub agl_min: f64,
    #[serde(default)]
    pub manual: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_camera_runtime_state() {
        let message: ServerMessage = serde_json::from_str(
            r#"{"message":"camera_runtime_state","status":200,"payload":{"camera_position":{"x":8.55816908772659,"y":182.92090910688185,"z":50.04525586995449,"pitch":-0.02531324395577457,"yaw":1.42027946761768,"roll":0.0,"zoom":0.9747225351483884,"preset_zoom":0.9747225351483884,"rendered_zoom":0.9747225351483884,"agl_min":0.0},"look_at_position":{"longitude":8.595819316591752,"latitude":50.048916478672865,"altitude":114.07841159445833,"agl_min":0.0,"manual":false},"ai_traffic_tracking_id":286359552,"ai_traffic_auto_target_enabled":false,"master_enabled":true}}"#,
        )
        .unwrap();
        let ServerMessage::CameraRuntimeState { payload } = message else {
            panic!("wrong variant");
        };
        assert_eq!(payload.ai_traffic_tracking_id, Some(286359552));
        assert!(!payload.ai_traffic_auto_target_enabled);
        assert!(payload.master_enabled);
        assert_eq!(payload.camera_position.unwrap().z, 50.04525586995449);
        assert_eq!(
            payload.look_at_position.unwrap().longitude,
            8.595819316591752
        );
    }

    #[test]
    fn camera_state_tolerates_null_tracking_id() {
        let message: ServerMessage = serde_json::from_str(
            r#"{"message":"camera_runtime_state","payload":{"ai_traffic_tracking_id":null,"camera_position":null,"look_at_position":null}}"#,
        )
        .unwrap();
        let ServerMessage::CameraRuntimeState { payload } = message else {
            panic!("wrong variant");
        };
        assert_eq!(payload.ai_traffic_tracking_id, None);
        assert!(payload.camera_position.is_none());
        assert!(payload.look_at_position.is_none());
    }
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
