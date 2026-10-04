mod chaseplane;
mod options;
mod trackaudio;
mod tracking;

use std::sync::Arc;

use chaseplane::ChaseplaneClient;
use tracking::AircraftTracker;
use tracking::Options;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = options::load();
    println!("Loaded options: {:?}", options);

    let chaseplane = ChaseplaneClient::connect_default(&options.view_profile_theme).await?;

    tokio::try_join!(run_tracking(chaseplane.clone(), options))?;

    Ok(())
}

/// Feeds TrackAudio events to an `AircraftTracker` until the connection ends.
async fn run_tracking(
    chaseplane: Arc<ChaseplaneClient>,
    options: Options,
) -> Result<(), Box<dyn std::error::Error>> {
    let ta_client = trackaudio::connect().await?;
    let mut events = ta_client.subscribe();
    let mut tracker = AircraftTracker::new(chaseplane, options);

    while let Ok(event) = events.recv().await {
        tracker.handle_event(event);
    }

    Ok(())
}
