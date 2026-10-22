//! Applies the user's zoom offset to ChasePlane's camera while it tracks an aircraft.
//!
//! ChasePlane's only zoom command starts zooming at a given rate until a rate of 0 is sent, so
//! this waits for ChasePlane's own zoom for the target to settle, then steers the reported zoom
//! towards that baseline plus the wanted offset with a proportional rate, and stops once it's
//! close enough.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use crate::chaseplane::{CameraRuntimeState, ChaseplaneClient};
use crate::state::StateHandle;

/// The zoom counts as settled once it hasn't changed by more than this for `SETTLE_TIME`.
const SETTLE_EPSILON: f64 = 0.002;
const SETTLE_TIME: Duration = Duration::from_millis(2000);

/// Zoom error below which zooming is stopped.
const STOP_BAND: f64 = 0.01;
/// Zoom error above which zooming is started again.
const START_BAND: f64 = 0.02;
/// Rate sent per unit of zoom error; the rate is capped to ±1.
const GAIN: f64 = 1.2;
/// Smallest rate sent while zooming, so small errors still converge.
const MIN_RATE: f64 = 0.01;
/// Smallest change of the sent rate worth another message.
const RATE_STEP: f64 = 0.05;

/// Distance in km from the camera to what it looks at, if both are known.
pub fn camera_distance_km(camera: &CameraRuntimeState) -> Option<f64> {
    let position = camera.camera_position.as_ref()?;
    let target = camera.look_at_position.as_ref()?;
    let ground =
        crate::chaseplane::haversine_km(position.z, position.x, target.latitude, target.longitude);
    let height = (position.y - target.altitude) / 1000.0;
    Some(ground.hypot(height))
}

/// The share of the slider that still applies at the given distance: 1 when close, fading
/// linearly to 0 at `falloff_km`. A falloff of 0 (or an unknown distance) keeps it at 1.
pub fn falloff(distance_km: Option<f64>, falloff_km: f64) -> f64 {
    match distance_km {
        Some(d) if falloff_km > 0.0 => (1.0 - d / falloff_km).clamp(0.0, 1.0),
        _ => 1.0,
    }
}

pub async fn watch_zoom_offset(chaseplane: Arc<ChaseplaneClient>, state: StateHandle) {
    let mut camera_state = chaseplane.subscribe_camera_state();
    let mut base: Option<f64> = None;
    // Last seen zoom and since when it hasn't moved; used to wait out ChasePlane's own zoom.
    let mut settling: Option<(f64, Instant)> = None;
    let mut last_target: Option<u64> = None;
    let mut last_view: Option<String> = None;
    let mut rate = 0.0_f64;

    while camera_state.changed().await.is_ok() {
        let Some(camera) = chaseplane.camera_state() else {
            continue;
        };
        let target = camera.ai_traffic_tracking_id.filter(|id| *id != 0);
        let view = chaseplane.current_view();
        // ChasePlane picks its own zoom again for a new target or view.
        if target != last_target || view != last_view {
            base = None;
            settling = None;
            last_target = target;
            last_view = view;
        }

        let wanted_rate = match (target, camera.camera_position.as_ref()) {
            (Some(_), Some(position)) => {
                if base.is_none() {
                    // Only the current zoom is known, and ChasePlane keeps zooming towards its
                    // own goal for a while, so take the baseline once the zoom stopped moving.
                    match settling {
                        Some((zoom, since)) if (position.zoom - zoom).abs() <= SETTLE_EPSILON => {
                            if since.elapsed() >= SETTLE_TIME {
                                base = Some(position.zoom);
                            }
                        }
                        _ => settling = Some((position.zoom, Instant::now())),
                    }
                }
                match base {
                    Some(base) => {
                        let snapshot = state.snapshot();
                        let distance = camera_distance_km(&camera);
                        let offset = snapshot.zoom_offset
                            * falloff(distance, snapshot.options.chaseplane.zoom_falloff_km);
                        let goal = (base + offset).clamp(0.0, 1.0);
                        next_rate(goal - position.zoom, rate)
                    }
                    None => 0.0,
                }
            }
            _ => {
                base = None;
                settling = None;
                0.0
            }
        };

        let stopping = wanted_rate == 0.0 && rate != 0.0;
        if !stopping && (wanted_rate - rate).abs() < RATE_STEP {
            continue;
        }
        match chaseplane.zoom_slider_move(wanted_rate).await {
            Ok(()) => rate = wanted_rate,
            Err(err) => crate::app_error!("🔍 Failed to adjust zoom: {err}"),
        }
    }
}

/// The zoom rate to use for the given remaining zoom error, with hysteresis between starting
/// (error above `START_BAND`) and stopping (error below `STOP_BAND`).
fn next_rate(error: f64, current_rate: f64) -> f64 {
    let band = if current_rate == 0.0 {
        START_BAND
    } else {
        STOP_BAND
    };
    if error.abs() < band {
        0.0
    } else {
        (error * GAIN).clamp(-1.0, 1.0).abs().max(MIN_RATE) * error.signum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_has_hysteresis_and_stops() {
        assert_eq!(next_rate(0.015, 0.0), 0.0);
        assert_eq!(next_rate(0.015, 0.3), 0.1);
        assert_eq!(next_rate(0.005, 0.3), 0.0);
        assert_eq!(next_rate(-0.9, 0.0), -1.0);
    }

    #[test]
    fn falloff_fades_linearly() {
        assert_eq!(falloff(Some(0.0), 5.0), 1.0);
        assert_eq!(falloff(Some(2.5), 5.0), 0.5);
        assert_eq!(falloff(Some(10.0), 5.0), 0.0);
        assert_eq!(falloff(Some(10.0), 0.0), 1.0);
        assert_eq!(falloff(None, 5.0), 1.0);
    }
}
