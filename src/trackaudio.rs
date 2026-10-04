use trackaudio::TrackAudioClient;

/// Connects to a running TrackAudio instance so its events can be subscribed to.
pub async fn connect() -> trackaudio::Result<TrackAudioClient> {
    let client = TrackAudioClient::connect_default().await?;
    println!("Monitoring radio activity...");
    Ok(client)
}
