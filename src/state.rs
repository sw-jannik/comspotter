//! State shared between the engine (producer) and the GUI (consumer).

use std::sync::{Arc, Mutex};
use tokio::time::Instant;

use crate::applog;
use crate::chaseplane::ChaseplaneClient;
use crate::options::Options;

#[derive(Clone, Debug, PartialEq)]
pub enum ConnStatus {
    Connecting,
    Connected,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct Transmission {
    pub callsign: String,
    pub frequency: String,
    pub since: Instant,
}

#[derive(Clone, Debug)]
pub struct LastTransmission {
    pub callsign: String,
    pub frequency: String,
    /// Whether the station is a known AI aircraft in ChasePlane.
    pub known_traffic: bool,
    pub started: Instant,
    /// `None` while still transmitting.
    pub ended: Option<Instant>,
}

#[derive(Clone)]
pub struct AppState {
    pub chaseplane_status: ConnStatus,
    pub trackaudio_status: ConnStatus,
    pub tracking_paused: bool,
    pub auto_spot_paused: bool,
    pub transmitting: Vec<Transmission>,
    pub last_transmission: Option<LastTransmission>,
    /// Aircraft ComSpotter last asked ChasePlane to track, and when.
    pub last_tracked: Option<(String, Instant)>,
    /// The options currently in effect.
    pub options: Options,
    /// Present while connected to ChasePlane.
    pub chaseplane: Option<Arc<ChaseplaneClient>>,
}

impl AppState {
    pub fn new(options: Options) -> Self {
        Self {
            chaseplane_status: ConnStatus::Connecting,
            trackaudio_status: ConnStatus::Connecting,
            tracking_paused: false,
            auto_spot_paused: false,
            transmitting: Vec::new(),
            last_transmission: None,
            last_tracked: None,
            options,
            chaseplane: None,
        }
    }
}

#[derive(Clone)]
pub struct StateHandle {
    inner: Arc<Mutex<AppState>>,
}

impl StateHandle {
    pub fn new(options: Options) -> Self {
        Self {
            inner: Arc::new(Mutex::new(AppState::new(options))),
        }
    }

    /// Modifies the state and wakes up the GUI.
    pub fn update<R>(&self, f: impl FnOnce(&mut AppState) -> R) -> R {
        let result = f(&mut self.inner.lock().unwrap());
        applog::request_repaint();
        result
    }

    pub fn snapshot(&self) -> AppState {
        self.inner.lock().unwrap().clone()
    }
}

/// Requests sent from the GUI to the engine.
#[derive(Debug)]
pub enum Control {
    SetTrackingPaused(bool),
    SetAutoSpotPaused(bool),
    /// Apply new options; the caller is responsible for persisting them.
    ApplyOptions(Options),
}
