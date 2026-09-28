use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use trackaudio::{Event, TrackAudioClient};

use crate::chaseplane::ChaseplaneClient;

/// Listens for TrackAudio RX events and tells ChasePlane to track a station once it has
/// been transmitting continuously for at least `threshold`, cancelling if it stops first.
pub async fn connect(
    chaseplane: Arc<ChaseplaneClient>,
    threshold: Duration,
) -> trackaudio::Result<()> {
    let ta_client = TrackAudioClient::connect_default().await?;
    let mut events = ta_client.subscribe();

    println!("Monitoring radio activity...");

    // Pending threshold timers, keyed by callsign; aborted if RX ends before they fire.
    let mut pending: HashMap<String, JoinHandle<()>> = HashMap::new();

    while let Ok(event) = events.recv().await {
        match event {
            Event::RxBegin(rx) => {
                println!("📻 RX Start: {} on {}", rx.callsign, rx.frequency);

                if let Some(handle) = pending.remove(&rx.callsign) {
                    handle.abort();
                }

                let chaseplane = chaseplane.clone();
                let callsign = rx.callsign.clone();
                let handle = tokio::spawn(async move {
                    tokio::time::sleep(threshold).await;
                    track_if_known(&chaseplane, &callsign).await;
                });
                pending.insert(rx.callsign, handle);
            }
            Event::RxEnd(rx) => {
                println!("📻 RX End: {} on {}", rx.callsign, rx.frequency);
                if let Some(active) = rx.active_transmitters
                    && !active.is_empty()
                {
                    println!("   Still transmitting: {}", active.join(", "));
                }

                if let Some(handle) = pending.remove(&rx.callsign) {
                    handle.abort();
                }
            }
            Event::TxBegin(_) => {
                println!("🎙️  TX Start");
            }
            Event::TxEnd(_) => {
                println!("🎙️  TX End");
            }
            _ => {}
        }
    }

    Ok(())
}

/// Tracks `callsign` in ChasePlane if it's currently known AI traffic; no-op otherwise.
async fn track_if_known(chaseplane: &ChaseplaneClient, callsign: &str) {
    let Some(traffic) = chaseplane.find_by_callsign(callsign) else {
        return;
    };

    match chaseplane.track_by_id(traffic.uid).await {
        Ok(reply) => println!(
            "🎯 Tracking {} after sustained transmission: {reply:?}",
            traffic.callsign
        ),
        Err(err) => eprintln!("🎯 Failed to track {}: {err}", traffic.callsign),
    }
}
