use trackaudio::TrackAudioClient;

pub const DEFAULT_URL: &str = "ws://127.0.0.1:49080/ws";

/// Connects to a running TrackAudio instance so its events can be subscribed to.
pub async fn connect(url: &str) -> trackaudio::Result<TrackAudioClient> {
    let client = TrackAudioClient::connect_url(url).await?;
    println!("Monitoring radio activity...");
    Ok(client)
}
