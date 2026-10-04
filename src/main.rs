mod chaseplane;
mod trackaudio;
mod tracking;

use std::sync::Arc;
use std::time::Duration;

use chaseplane::ChaseplaneClient;
use tracking::AircraftTracker;

/// Minimum continuous transmission time before a station is tracked in ChasePlane.
const TRACK_THRESHOLD: Duration = Duration::from_millis(500);
/// Idle time with no known traffic transmitting before ChasePlane's auto-spot is enabled.
const AUTO_SPOT_THRESHOLD: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let chaseplane = Arc::new(ChaseplaneClient::connect_default().await?);

    tokio::try_join!(run_tracking(chaseplane.clone()))?;

    Ok(())
}

/// Feeds TrackAudio events to an `AircraftTracker` until the connection ends.
async fn run_tracking(chaseplane: Arc<ChaseplaneClient>) -> Result<(), Box<dyn std::error::Error>> {
    let ta_client = trackaudio::connect().await?;
    let mut events = ta_client.subscribe();
    let mut tracker = AircraftTracker::new(chaseplane, TRACK_THRESHOLD, AUTO_SPOT_THRESHOLD);

    while let Ok(event) = events.recv().await {
        tracker.handle_event(event);
    }

    Ok(())
}
