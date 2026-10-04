use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use trackaudio::Event;

use crate::chaseplane::ChaseplaneClient;

/// Tracks known aircraft in ChasePlane once their radio transmission has been sustained for
/// `track_threshold`, and enables ChasePlane's auto-spot once `auto_spot_threshold` passes
/// without any known traffic transmitting.
pub struct AircraftTracker {
    chaseplane: Arc<ChaseplaneClient>,
    track_threshold: Duration,
    auto_spot_threshold: Duration,
    // Pending threshold timers, keyed by callsign; aborted if RX ends before they fire.
    pending: HashMap<String, JoinHandle<()>>,
    // Pending auto-spot-enable timer, reset whenever known traffic transmits.
    auto_spot_timer: Option<JoinHandle<()>>,
}

impl AircraftTracker {
    pub fn new(
        chaseplane: Arc<ChaseplaneClient>,
        track_threshold: Duration,
        auto_spot_threshold: Duration,
    ) -> Self {
        Self {
            chaseplane,
            track_threshold,
            auto_spot_threshold,
            pending: HashMap::new(),
            auto_spot_timer: None,
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
        let track_threshold = self.track_threshold;
        let owned_callsign = callsign.to_string();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(track_threshold).await;
            track_if_known(&chaseplane, &owned_callsign).await;
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
            let auto_spot_threshold = self.auto_spot_threshold;
            self.auto_spot_timer = Some(tokio::spawn(async move {
                tokio::time::sleep(auto_spot_threshold).await;
                enable_auto_spot(&chaseplane).await;
            }));
        }
    }
}

/// Tracks `callsign` in ChasePlane if it's currently known AI traffic; no-op otherwise.
async fn track_if_known(chaseplane: &ChaseplaneClient, callsign: &str) {
    let Some(traffic) = chaseplane.find_by_callsign(callsign) else {
        return;
    };

    match chaseplane.track_by_id(traffic.uid).await {
        Ok(_reply) => println!(
            "🎯 Tracking {} after sustained transmission",
            traffic.callsign
        ),
        Err(err) => eprintln!("🎯 Failed to track {}: {err}", traffic.callsign),
    }
}

/// Enables ChasePlane's auto-spot after known traffic has gone quiet for a while.
async fn enable_auto_spot(chaseplane: &ChaseplaneClient) {
    match chaseplane.set_auto_target(true).await {
        Ok(_reply) => println!("🔭 No known traffic transmitting — enabling auto-spot"),
        Err(err) => eprintln!("🔭 Failed to enable auto-spot: {err}"),
    }
}
