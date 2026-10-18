use trackaudio::TrackAudioClient;

pub const DEFAULT_URL: &str = "ws://127.0.0.1:49080/ws";

/// Creates a TrackAudio client so its events can be subscribed to. Only an invalid URL fails here;
/// the client connects (and reconnects) in the background.
pub async fn connect(url: &str) -> trackaudio::Result<TrackAudioClient> {
    let client = TrackAudioClient::connect_url(url).await?;
    crate::app_info!("Monitoring radio activity at {url}...");
    Ok(client)
}
