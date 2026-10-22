//! Connects to ChasePlane and TrackAudio, keeps those connections alive, and runs the tracker.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use trackaudio::{ConnectionState, TrackAudioClient};

use crate::chaseplane::ChaseplaneClient;
use crate::options::Options;
use crate::state::{ConnStatus, Control, StateHandle};
use crate::tracking::AircraftTracker;
use crate::{app_error, app_info, trackaudio as ta, zoom};

const RETRY_DELAY: Duration = Duration::from_secs(3);
const STATUS_POLL: Duration = Duration::from_millis(500);

/// Whether switching between these options requires reconnecting.
fn needs_reconnect(old: &Options, new: &Options) -> bool {
    old.chaseplane.url != new.chaseplane.url
        || old.chaseplane.view_profile_theme != new.chaseplane.view_profile_theme
        || old.trackaudio.url != new.trackaudio.url
}

fn trackaudio_status(state: &ConnectionState) -> ConnStatus {
    match state {
        ConnectionState::Connected => ConnStatus::Connected,
        ConnectionState::Connecting { .. } | ConnectionState::Reconnecting { .. } => {
            ConnStatus::Connecting
        }
        other => ConnStatus::Failed(format!("{other:?}")),
    }
}

struct Engine {
    state: StateHandle,
    control: mpsc::UnboundedReceiver<Control>,
    options: Options,
}

/// Aborts the wrapped task when dropped.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Mirrors the TrackAudio client's connection state (which it manages itself, including
/// reconnects) into the shared state.
async fn watch_trackaudio_status(state: StateHandle, client: TrackAudioClient) {
    let mut poll = tokio::time::interval(STATUS_POLL);
    loop {
        poll.tick().await;
        let status = trackaudio_status(&client.connection_state());
        state.update(|s| {
            if s.trackaudio_status != status {
                s.trackaudio_status = status;
            }
        });
    }
}

/// Runs until the control channel's senders are all dropped.
pub async fn run(state: StateHandle, control: mpsc::UnboundedReceiver<Control>, options: Options) {
    let mut engine = Engine {
        state,
        control,
        options,
    };
    engine.run().await;
}

impl Engine {
    async fn run(&mut self) {
        loop {
            self.state.update(|s| {
                s.options = self.options.clone();
                s.chaseplane_status = ConnStatus::Connecting;
                s.trackaudio_status = ConnStatus::Connecting;
                // Their RX end events are lost along with the old connection.
                s.transmitting.clear();
                if let Some(last) = s.last_transmission.as_mut() {
                    last.ended.get_or_insert_with(Instant::now);
                }
            });

            let trackaudio = match ta::connect(&self.options.trackaudio.url).await {
                Ok(client) => client,
                Err(err) => {
                    app_error!("TrackAudio: {err}");
                    self.state
                        .update(|s| s.trackaudio_status = ConnStatus::Failed(err.to_string()));
                    if !self.idle_until_options_change().await {
                        return;
                    }
                    continue;
                }
            };

            let _status_watcher = AbortOnDrop(tokio::spawn(watch_trackaudio_status(
                self.state.clone(),
                trackaudio.clone(),
            )));

            let chaseplane = match self.connect_chaseplane().await {
                Connect::Connected(client) => client,
                Connect::OptionsChanged => {
                    trackaudio.shutdown();
                    continue;
                }
                Connect::Closed => return,
            };

            let keep_running = self.run_session(chaseplane, &trackaudio).await;
            trackaudio.shutdown();
            self.state.update(|s| s.chaseplane = None);
            if !keep_running {
                return;
            }
        }
    }

    /// Retries until connected. Returns early if the options change, so the new ones are used.
    async fn connect_chaseplane(&mut self) -> Connect {
        loop {
            let url = self.options.chaseplane.url.clone();
            let theme = self.options.chaseplane.view_profile_theme.clone();
            self.state.update(|s| {
                if !matches!(s.chaseplane_status, ConnStatus::Failed(_)) {
                    s.chaseplane_status = ConnStatus::Connecting;
                }
            });

            let result = tokio::select! {
                result = ChaseplaneClient::connect(&url, &theme) => result,
                ctl = self.control.recv() => match self.handle_idle(ctl) {
                    Idle::Continue => continue,
                    Idle::OptionsChanged => return Connect::OptionsChanged,
                    Idle::Closed => return Connect::Closed,
                },
            };

            match result {
                Ok(client) => {
                    app_info!("Connected to ChasePlane at {url}");
                    return Connect::Connected(client);
                }
                Err(err) => {
                    app_error!("ChasePlane: {err}");
                    self.state
                        .update(|s| s.chaseplane_status = ConnStatus::Failed(err.to_string()));
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(RETRY_DELAY) => {}
                ctl = self.control.recv() => match self.handle_idle(ctl) {
                    Idle::Continue => {}
                    Idle::OptionsChanged => return Connect::OptionsChanged,
                    Idle::Closed => return Connect::Closed,
                },
            }
        }
    }

    /// Blocks (e.g. on an invalid URL) until new options arrive. Returns false if the GUI is gone.
    async fn idle_until_options_change(&mut self) -> bool {
        loop {
            let ctl = self.control.recv().await;
            match self.handle_idle(ctl) {
                Idle::Continue => {}
                Idle::OptionsChanged => return true,
                Idle::Closed => return false,
            }
        }
    }

    /// Handles a control message while there is no tracker to forward it to.
    fn handle_idle(&mut self, ctl: Option<Control>) -> Idle {
        match ctl {
            None => Idle::Closed,
            Some(Control::SetTrackingPaused(paused)) => {
                self.state.update(|s| s.tracking_paused = paused);
                Idle::Continue
            }
            Some(Control::SetAutoSpotPaused(paused)) => {
                self.state.update(|s| s.auto_spot_paused = paused);
                Idle::Continue
            }
            Some(Control::StopZoom) => Idle::Continue,
            Some(Control::ApplyOptions(options)) => {
                self.options = options;
                Idle::OptionsChanged
            }
        }
    }

    /// Feeds TrackAudio events to a tracker until a connection is lost or the options require
    /// reconnecting. Returns false if the GUI is gone.
    async fn run_session(
        &mut self,
        chaseplane: Arc<ChaseplaneClient>,
        trackaudio: &TrackAudioClient,
    ) -> bool {
        let mut events = trackaudio.subscribe();
        let mut chaseplane_connected = chaseplane.subscribe_connected();
        let mut tracker =
            AircraftTracker::new(chaseplane.clone(), self.options.clone(), self.state.clone());
        let _zoom_watcher = AbortOnDrop(tokio::spawn(zoom::watch_zoom_offset(
            chaseplane.clone(),
            self.state.clone(),
        )));

        self.state.update(|s| {
            s.chaseplane_status = ConnStatus::Connected;
            s.chaseplane = Some(chaseplane.clone());
        });

        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Ok(event) => tracker.handle_event(event),
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return true,
                },
                changed = chaseplane_connected.changed() => {
                    if changed.is_err() || !*chaseplane_connected.borrow() {
                        app_error!("ChasePlane connection lost");
                        return true;
                    }
                }
                ctl = self.control.recv() => match ctl {
                    None => return false,
                    Some(Control::SetTrackingPaused(paused)) => {
                        self.state.update(|s| s.tracking_paused = paused);
                        tracker.set_tracking_paused(paused);
                    }
                    Some(Control::SetAutoSpotPaused(paused)) => {
                        self.state.update(|s| s.auto_spot_paused = paused);
                        tracker.set_auto_spot_paused(paused);
                    }
                    Some(Control::StopZoom) => {
                        if let Err(err) = chaseplane.zoom_slider_move(0.0).await {
                            app_error!("Failed to stop zooming: {err}");
                        }
                    }
                    Some(Control::ApplyOptions(options)) => {
                        let reconnect = needs_reconnect(&self.options, &options);
                        self.options = options.clone();
                        self.state.update(|s| s.options = options.clone());
                        if reconnect {
                            app_info!("Connection options changed, reconnecting");
                            return true;
                        }
                        tracker.apply_options(options);
                        app_info!("Options applied");
                    }
                },
            }
        }
    }
}

enum Connect {
    Connected(Arc<ChaseplaneClient>),
    OptionsChanged,
    Closed,
}

enum Idle {
    Continue,
    OptionsChanged,
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_connection_options_require_reconnect() {
        let old = Options::default();

        let mut new = old.clone();
        new.tracking.track_threshold += Duration::from_millis(1);
        new.chaseplane.view_switching = !new.chaseplane.view_switching;
        assert!(!needs_reconnect(&old, &new));

        let mut new = old.clone();
        new.chaseplane.url = "ws://other/".into();
        assert!(needs_reconnect(&old, &new));

        let mut new = old.clone();
        new.chaseplane.view_profile_theme = "OTHER".into();
        assert!(needs_reconnect(&old, &new));

        let mut new = old.clone();
        new.trackaudio.url = "other".into();
        assert!(needs_reconnect(&old, &new));
    }
}
