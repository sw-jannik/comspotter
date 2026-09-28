use trackaudio::{Event, TrackAudioClient};

pub async fn connect() -> trackaudio::Result<()> {
    let ta_client = TrackAudioClient::connect_default().await?;
    let mut events = ta_client.subscribe();

    println!("Monitoring radio activity...");

    println!("Monitoring radio activity...");

    while let Ok(event) = events.recv().await {
        match event {
            Event::RxBegin(rx) => {
                println!("📻 RX Start: {} on {}", rx.callsign, rx.frequency);
            }
            Event::RxEnd(rx) => {
                println!("📻 RX End: {} on {}", rx.callsign, rx.frequency);
                if let Some(active) = rx.active_transmitters {
                    if !active.is_empty() {
                        println!("   Still transmitting: {}", active.join(", "));
                    }
                }
            }
            Event::TxBegin(_) => {
                println!("🎙️  TX Start");
            }
            Event::TxEnd(_) => {
                println!("🎙️  TX End");
            }
            // Event::StationStateUpdate(state) => {
            //     println!("📡 Station update: {}", state.callsign);
            // }
            _ => {}
        }
    }

    Ok(())
}
