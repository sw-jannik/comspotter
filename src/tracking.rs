use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use trackaudio::Event;

use crate::chaseplane::{ChaseplaneClient, TrafficInfo};
use crate::options::Options;

/// Repeated RX starts from the same callsign within this window are treated as duplicates.
const RX_BEGIN_DEBOUNCE: Duration = Duration::from_millis(50);

/// Tracks known aircraft in ChasePlane once their radio transmission has been sustained for
/// `track_threshold`, and enables ChasePlane's auto-spot once `auto_spot_threshold` passes
/// without any known traffic transmitting.
pub struct AircraftTracker {
    chaseplane: Arc<ChaseplaneClient>,
    options: Options,
    // Last accepted RX start per callsign, used to drop duplicate events.
    last_rx_begin: HashMap<String, Instant>,
    // Pending threshold timers, keyed by callsign; aborted if RX ends before they fire.
    pending: HashMap<String, JoinHandle<()>>,
    // Pending auto-spot-enable timer, reset whenever known traffic transmits.
    auto_spot_timer: Option<JoinHandle<()>>,
    // Switches views when auto-spot follows a new aircraft; only runs if enabled in the options.
    auto_spot_view_watcher: Option<JoinHandle<()>>,
    // Time of the last successful scene change. A track task holds the lock while it waits out
    // the scene change cooldown, which serializes concurrent track attempts.
    last_scene_change: Arc<Mutex<Option<Instant>>>,
}

impl AircraftTracker {
    pub fn new(chaseplane: Arc<ChaseplaneClient>, options: Options) -> Self {
        let last_scene_change = Arc::new(Mutex::new(None));
        let auto_spot_view_watcher = (options.chaseplane.view_switching
            && options.chaseplane.auto_spot_view_switching)
            .then(|| {
                tokio::spawn(watch_auto_spot(
                    chaseplane.clone(),
                    options.chaseplane.auto_spot_continuous_view_switching,
                    options.tracking.scene_change_threshold,
                    last_scene_change.clone(),
                ))
            });

        // Start the countdown immediately so auto-spot also engages if no traffic ever transmits.
        let auto_spot_timer = Some(spawn_auto_spot_timer(
            chaseplane.clone(),
            options.tracking.auto_spot_threshold,
        ));

        Self {
            chaseplane,
            options,
            pending: HashMap::new(),
            auto_spot_timer,
            auto_spot_view_watcher,
            last_rx_begin: HashMap::new(),
            last_scene_change,
        }
    }

    /// Updates tracking/auto-spot state in response to a TrackAudio event.
    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::RxBegin(rx) => {
                // TrackAudio sometimes emits RxBegin twice in quick succession.
                let now = Instant::now();
                if let Some(last) = self.last_rx_begin.get(&rx.callsign)
                    && now.duration_since(*last) < RX_BEGIN_DEBOUNCE
                {
                    return;
                }

                self.last_rx_begin.insert(rx.callsign.clone(), now);
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
        let track_threshold = self.options.tracking.track_threshold;
        let scene_change_threshold = self.options.tracking.scene_change_threshold;
        let view_switching = self.options.chaseplane.view_switching;
        let force_view_switch = self.options.chaseplane.force_view_switch;
        let last_scene_change = self.last_scene_change.clone();
        let owned_callsign = callsign.to_string();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(track_threshold).await;
            // The task is aborted on RX end, so the station is still transmitting here.
            let mut last = last_scene_change.lock().await;
            if let Some(at) = *last {
                tokio::time::sleep_until(at + scene_change_threshold).await;
            }
            if track_if_known(
                &chaseplane,
                &owned_callsign,
                view_switching,
                force_view_switch,
            )
            .await
            {
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
            self.auto_spot_timer = Some(spawn_auto_spot_timer(
                self.chaseplane.clone(),
                self.options.tracking.auto_spot_threshold,
            ));
        }
    }
}

impl Drop for AircraftTracker {
    fn drop(&mut self) {
        if let Some(handle) = self.auto_spot_view_watcher.take() {
            handle.abort();
        }
        if let Some(handle) = self.auto_spot_timer.take() {
            handle.abort();
        }
    }
}

fn spawn_auto_spot_timer(chaseplane: Arc<ChaseplaneClient>, threshold: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        tokio::time::sleep(threshold).await;
        enable_auto_spot(&chaseplane).await;
    })
}

/// Tracks `callsign` in ChasePlane if it's currently known AI traffic; no-op otherwise. If
/// `view_switching` is set, also jumps to the closest saved view afterwards (see
/// `switch_to_closest_view` for `force_view_switch`).
/// Returns whether the track succeeded.
async fn track_if_known(
    chaseplane: &ChaseplaneClient,
    callsign: &str,
    view_switching: bool,
    force_view_switch: bool,
) -> bool {
    let Some(traffic) = chaseplane.find_by_callsign(callsign) else {
        return false;
    };

    match chaseplane.track_by_id(traffic.uid).await {
        Ok(_reply) => {
            println!("🎯 Tracking {}", traffic.callsign);
            if view_switching {
                // Re-read the traffic entry: the position may have been updated while tracking.
                if let Some(traffic) = chaseplane.find_by_callsign(callsign) {
                    switch_to_closest_view(chaseplane, &traffic, force_view_switch).await;
                }
            }
            true
        }
        Err(err) => {
            eprintln!("🎯 Failed to track {}: {err}", traffic.callsign);
            false
        }
    }
}

/// Switches to the saved view closest to the aircraft's position. Does nothing if that view is
/// already the current one, unless `force` is set. Returns whether a switch happened.
async fn switch_to_closest_view(
    chaseplane: &ChaseplaneClient,
    traffic: &TrafficInfo,
    force: bool,
) -> bool {
    let Some(view) = chaseplane.closest_view(traffic.lat, traffic.lon) else {
        return false;
    };

    if !force && chaseplane.current_view().as_deref() == Some(view.guid.as_str()) {
        return false;
    }

    match chaseplane.set_view_by_guid(&view.guid).await {
        Ok(reply) if reply.is_success() => {
            println!(
                "🎥 Switched to view \"{}\" for {}",
                view.name, traffic.callsign
            );
            true
        }
        Ok(reply) => {
            eprintln!(
                "🎥 Failed to switch to view \"{}\": status {}",
                view.name, reply.status
            );
            false
        }
        Err(err) => {
            eprintln!("🎥 Failed to switch to view \"{}\": {err}", view.name);
            false
        }
    }
}

/// The aircraft ChasePlane's auto-spot is currently following, if auto-spot is active.
fn auto_spot_target(chaseplane: &ChaseplaneClient) -> Option<u64> {
    chaseplane
        .camera_state()
        .filter(|state| state.ai_traffic_auto_target_enabled)
        .and_then(|state| state.ai_traffic_tracking_id)
        .filter(|id| *id != 0)
}

/// Watches ChasePlane's camera state and switches to the closest view while auto-spot follows an
/// aircraft: on every camera state update if `continuous`, otherwise only when it starts following
/// a different aircraft. Never forces a switch to the current view. Shares the scene change
/// cooldown with explicit tracking.
async fn watch_auto_spot(
    chaseplane: Arc<ChaseplaneClient>,
    continuous: bool,
    scene_change_threshold: Duration,
    last_scene_change: Arc<Mutex<Option<Instant>>>,
) {
    let mut camera_state = chaseplane.subscribe_camera_state();
    let mut handled: Option<u64> = None;

    while camera_state.changed().await.is_ok() {
        let Some(id) = auto_spot_target(&chaseplane) else {
            handled = None;
            continue;
        };
        // Unknown traffic is retried on the next camera state message.
        if (!continuous && handled == Some(id)) || chaseplane.get_traffic(id).is_none() {
            continue;
        }

        let mut last = last_scene_change.lock().await;
        if let Some(at) = *last {
            tokio::time::sleep_until(at + scene_change_threshold).await;
        }

        // The target may have changed while waiting out the cooldown.
        let Some(id) = auto_spot_target(&chaseplane) else {
            handled = None;
            continue;
        };
        let Some(traffic) = chaseplane.get_traffic(id) else {
            continue;
        };
        handled = Some(id);
        if switch_to_closest_view(&chaseplane, &traffic, false).await {
            *last = Some(Instant::now());
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
