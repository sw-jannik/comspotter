use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (mut socket, _) = connect_async("ws://127.0.0.1:8652/").await?;

    let connect_message = json!({
        "message": "api_connect",
        "payload": { "client_name": "comspotter" }
    });
    socket
        .send(Message::Text(connect_message.to_string().into()))
        .await?;

    let request_id = format!(
        "request_ai_traffic_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let traffic_request = json!({
        "message": "api_request",
        "request_id": request_id,
        "command": "request_ai_traffic",
        "payload": {}
    });
    socket
        .send(Message::Text(traffic_request.to_string().into()))
        .await?;

    while let Some(message) = socket.next().await {
        let message = message?;

        if let Message::Text(text) = &message {
            let message_object: serde_json::Value = match serde_json::from_str(text) {
                Ok(obj) => obj,
                Err(err) => {
                    eprintln!("Failed to parse JSON: {err}");
                    continue;
                }
            };

            let message_type = message_object.get("message").and_then(|v| v.as_str());
            // println!("Received text message of type: {message_type:?}");

            if !(matches!(message_type, Some("ai_traffic"))) {
                continue;
            }

            let payload = message_object.get("payload");

            let first_updated = payload
                .and_then(|payload| payload.get("updated"))
                .and_then(|updated| updated.get(0));

            if first_updated.is_none() {
                continue;
            }

            let first_updated = first_updated.unwrap();
            let callsign = first_updated.get("callsign").and_then(|v| v.as_str());
            let uid = first_updated.get("uId").and_then(|v| v.as_u64());

            println!("Callsign: {callsign:#?}");
            println!("uId: {uid:#?}");

            if let Some(uid) = uid {
                track_aircraft_by_id(&mut socket, uid).await?;
            }
        }

        if matches!(message, Message::Close(_)) {
            break;
        }
    }

    Ok(())
}

async fn track_aircraft_by_id(
    socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    uid: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let track_payload = json!({
        "message":"api_request",
        "request_id":"ai_traffic_track_1790619017187_e4xpfam5o",
        "command":"ai_traffic_track",
        "payload": {
            "id":uid
        }
    });

    socket
        .send(Message::Text(track_payload.to_string().into()))
        .await?;

    Ok(())
}
