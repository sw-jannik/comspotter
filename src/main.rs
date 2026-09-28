mod chaseplane;
mod trackaudio_interface;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chaseplane::ChaseplaneClient;

/// Minimum continuous transmission time before a station is tracked in ChasePlane.
const TRACK_THRESHOLD: Duration = Duration::from_millis(500);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let chaseplane = Arc::new(ChaseplaneClient::connect_default().await?);

    tokio::try_join!(
        async {
            trackaudio_interface::connect(chaseplane.clone(), TRACK_THRESHOLD)
                .await
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        },
        // track_new_traffic(&chaseplane),
    )?;

    Ok(())
}

async fn track_new_traffic(
    chaseplane: &ChaseplaneClient,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tracked = HashSet::new();
    loop {
        for traffic in chaseplane.traffic() {
            if tracked.insert(traffic.uid) {
                println!("Tracking {} (uid {})", traffic.callsign, traffic.uid);
                match chaseplane.track_by_callsign(&traffic.callsign).await {
                    Ok(reply) => println!("  -> {reply:?}"),
                    Err(err) => eprintln!("  -> failed to track: {err}"),
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
