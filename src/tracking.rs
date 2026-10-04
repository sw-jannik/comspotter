use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use trackaudio::Event;

use crate::chaseplane::ChaseplaneClient;

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub track_threshold: Duration,
    pub auto_spot_threshold: Duration,
    pub scene_change_threshold: Duration,
}

/// Tracks known aircraft in ChasePlane once their radio transmission has been sustained for
/// `track_threshold`, and enables ChasePlane's auto-spot once `auto_spot_threshold` passes
/// without any known traffic transmitting.
pub struct AircraftTracker {
    chaseplane: Arc<ChaseplaneClient>,
    options: Options,
    // Pending threshold timers, keyed by callsign; aborted if RX ends before they fire.
    pending: HashMap<String, JoinHandle<()>>,
    // Pending auto-spot-enable timer, reset whenever known traffic transmits.
    auto_spot_timer: Option<JoinHandle<()>>,
    // Time of the last successful scene change. A track task holds the lock while it waits out
    // the scene change cooldown, which serializes concurrent track attempts.
    last_scene_change: Arc<Mutex<Option<Instant>>>,
}

impl AircraftTracker {
    pub fn new(chaseplane: Arc<ChaseplaneClient>, options: Options) -> Self {
        Self {
            chaseplane,
            options,
            pending: HashMap::new(),
            auto_spot_timer: None,
            last_scene_change: Arc::new(Mutex::new(None)),
        }
    }

    /// Updates tracking/auto-spot state in response to a TrackAudio event.
    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::RxBegin(rx) => {
                println!("📻 RX Start: {} on {}", rx.callsign, rx.frequency);
                self.on_rx_begin(&rx.callsign);
            }
            Event::RxEnd(rx) => {
                println!("📻 RX End: {} on {}", rx.callsign, rx.frequency);
                if let Some(active) = rx.active_transmitters
                    && !active.is_empty()
                {
                    println!("   Still transmitting: {}", active.join(", "));
                }
                self.on_rx_end(&rx.callsign);
            }
            Event::TxBegin(_) => println!("🎙️  TX Start"),
            Event::TxEnd(_) => println!("🎙️  TX End"),
            _ => {}
        }
    }

    /// Call when a station starts transmitting.
    fn on_rx_begin(&mut self, callsign: &str) {
        if let Some(handle) = self.pending.remove(callsign) {
            handle.abort();
        }

        if self.chaseplane.find_by_callsign(callsign).is_some()
            && let Some(handle) = self.auto_spot_timer.take()
        {
            handle.abort();
        }

        let chaseplane = self.chaseplane.clone();
        let track_threshold = self.options.track_threshold;
        let scene_change_threshold = self.options.scene_change_threshold;
        let last_scene_change = self.last_scene_change.clone();
        let owned_callsign = callsign.to_string();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(track_threshold).await;
            // The task is aborted on RX end, so the station is still transmitting here.
            let mut last = last_scene_change.lock().await;
            if let Some(at) = *last {
                tokio::time::sleep_until(at + scene_change_threshold).await;
            }
            if track_if_known(&chaseplane, &owned_callsign).await {
                *last = Some(Instant::now());
            }
        });
        self.pending.insert(callsign.to_string(), handle);
    }

    /// Call when a station stops transmitting.
    fn on_rx_end(&mut self, callsign: &str) {
        if let Some(handle) = self.pending.remove(callsign) {
            handle.abort();
        }

        if self.chaseplane.find_by_callsign(callsign).is_some() {
            if let Some(handle) = self.auto_spot_timer.take() {
                handle.abort();
            }
            let chaseplane = self.chaseplane.clone();
            let auto_spot_threshold = self.options.auto_spot_threshold;
            self.auto_spot_timer = Some(tokio::spawn(async move {
                tokio::time::sleep(auto_spot_threshold).await;
                enable_auto_spot(&chaseplane).await;
            }));
        }
    }
}

/// Tracks `callsign` in ChasePlane if it's currently known AI traffic; no-op otherwise.
/// Returns whether the track succeeded.
async fn track_if_known(chaseplane: &ChaseplaneClient, callsign: &str) -> bool {
    let Some(traffic) = chaseplane.find_by_callsign(callsign) else {
        return false;
    };

    match chaseplane.track_by_id(traffic.uid).await {
        Ok(_reply) => {
            println!(
                "🎯 Tracking {} after sustained transmission",
                traffic.callsign
            );
            true
        }
        Err(err) => {
            eprintln!("🎯 Failed to track {}: {err}", traffic.callsign);
            false
        }
    }
}

/// Enables ChasePlane's auto-spot after known traffic has gone quiet for a while.
async fn enable_auto_spot(chaseplane: &ChaseplaneClient) {
    match chaseplane.set_auto_target(true).await {
        Ok(_reply) => println!("🔭 No known traffic transmitting — enabling auto-spot"),
        Err(err) => eprintln!("🔭 Failed to enable auto-spot: {err}"),
    }
}
