use serde::Deserialize;

/// A single AI traffic entry as reported by ChasePlane's `ai_traffic` message.
#[derive(Debug, Clone, Deserialize)]
pub struct TrafficInfo {
    #[serde(rename = "uId")]
    pub uid: u64,
    pub callsign: String,
    pub display_name: String,
    pub sim_title: String,
    pub phase: String,
    pub on_ground: bool,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub heading: f64,
    pub speed_knots: f64,
    /// Every other field ChasePlane sends (runway info, provider matches, etc.), kept for forward compatibility.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
