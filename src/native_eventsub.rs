//! The EventSub WebSocket chat stream on `tokio` and `tungstenite`.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Message as WsMessage, Utf8Bytes, http::Uri};
use tracing::{debug, trace};

use super::{ChatSubscriptionConfig, EventSubAuthError, send_prepared_request};
use crate::eventsub::{
    EventSubMessageType, EventSubStreamEvent, EventSubWebSocketSession,
    channel_chat_message_delete_subscription_request, channel_chat_message_subscription_request,
    decode_eventsub_websocket_message,
};
use crate::helix;

const EVENTSUB_WEBSOCKET_URL: &str = "wss://eventsub.wss.twitch.tv/ws";

/// How long past the session's keepalive timeout the stream waits before it
/// gives the connection up, for a keepalive that is merely late.
const KEEPALIVE_MARGIN: Duration = Duration::from_secs(5);

/// How long a new connection may take to send its welcome. Twitch sends it
/// at once.
const WELCOME_TIMEOUT: Duration = Duration::from_secs(15);

type EventSubSocket = WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

pub struct EventSubChatStream {
    ws: EventSubSocket,
    session: EventSubWebSocketSession,
}

impl EventSubChatStream {
    pub fn session_id(&self) -> &str {
        &self.session.id
    }

    /// The session's keepalive timeout: Twitch sends an event or a keepalive
    /// at least this often while the connection is alive.
    pub fn keepalive_timeout(&self) -> Option<Duration> {
        self.session
            .keepalive_timeout_seconds
            .map(|seconds| Duration::from_secs(u64::from(seconds)))
    }

    /// The next event. `Ok(None)` means Twitch closed the connection.
    ///
    /// An error also means the connection is gone and a new one is needed:
    /// among others, when nothing arrived for longer than the session's
    /// keepalive timeout, which is how a connection that died without a word
    /// (a sleeping laptop, a dropped NAT mapping) shows.
    pub async fn next_event(&mut self) -> Result<Option<EventSubStreamEvent>> {
        loop {
            let frame = match self.keepalive_timeout() {
                Some(keepalive) => {
                    let limit = keepalive + KEEPALIVE_MARGIN;
                    tokio::time::timeout(limit, next_text_frame(&mut self.ws))
                        .await
                        .map_err(|_| {
                            anyhow!(
                                "EventSub sent nothing for {} seconds: the connection is lost",
                                limit.as_secs()
                            )
                        })??
                }
                None => next_text_frame(&mut self.ws).await?,
            };
            let Some(text) = frame else {
                return Ok(None);
            };
            let envelope = decode_eventsub_websocket_message(&text)
                .context("failed to parse EventSub payload")?;
            if let Some(event) = envelope
                .stream_event()
                .context("failed to decode EventSub stream event")?
            {
                return Ok(Some(event));
            }
        }
    }

    pub async fn close(&mut self) -> Result<()> {
        self.ws
            .close(None)
            .await
            .context("failed to close EventSub WebSocket")
    }
}

pub async fn connect_chat_stream(
    http: &Client,
    config: ChatSubscriptionConfig,
) -> Result<EventSubChatStream> {
    let mut ws = connect_eventsub_websocket(
        EVENTSUB_WEBSOCKET_URL,
        &config.client_id,
        &config.access_token,
    )
    .await?;
    let session = tokio::time::timeout(WELCOME_TIMEOUT, wait_for_session_welcome(&mut ws))
        .await
        .context("EventSub sent no welcome message in time")??;
    debug!(
        session_id = %session.id,
        keepalive = session.keepalive_timeout_seconds,
        "received EventSub session welcome"
    );

    create_chat_subscription(http, &config, &session.id).await?;

    Ok(EventSubChatStream { ws, session })
}

/// Reads the next text frame, answering pings and skipping binary, pong, and
/// raw frames. `Ok(None)` means the server closed the socket.
async fn next_text_frame(ws: &mut EventSubSocket) -> Result<Option<Utf8Bytes>> {
    while let Some(message) = ws.next().await {
        match message.context("EventSub WebSocket error")? {
            WsMessage::Text(text) => return Ok(Some(text)),
            WsMessage::Ping(payload) => {
                ws.send(WsMessage::Pong(payload))
                    .await
                    .context("failed to reply to EventSub ping")?;
            }
            WsMessage::Binary(_) => debug!("dropping unexpected binary EventSub payload"),
            WsMessage::Close(frame) => {
                debug!(?frame, "EventSub WebSocket closed by server");
                return Ok(None);
            }
            WsMessage::Pong(_) | WsMessage::Frame(_) => {}
        }
    }
    Ok(None)
}

async fn connect_eventsub_websocket(
    endpoint: &str,
    client_id: &str,
    access_token: &str,
) -> Result<EventSubSocket> {
    ensure_rustls_crypto_provider()?;

    let uri: Uri = endpoint
        .parse()
        .context("failed to parse EventSub WebSocket URL")?;
    let mut request = uri
        .into_client_request()
        .context("failed to build EventSub websocket request")?;
    request.headers_mut().insert(
        "Client-Id",
        client_id.parse().context("invalid client id header")?,
    );
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {access_token}")
            .parse()
            .context("invalid authorization header")?,
    );

    let (ws, response) = connect_async(request)
        .await
        .context("failed to connect to EventSub WebSocket")?;
    trace!(status = ?response.status(), endpoint, "connected to EventSub");
    Ok(ws)
}

fn ensure_rustls_crypto_provider() -> Result<()> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }

    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
        && rustls::crypto::CryptoProvider::get_default().is_none()
    {
        bail!("failed to install the Rustls process-level crypto provider");
    }

    Ok(())
}

async fn wait_for_session_welcome(ws: &mut EventSubSocket) -> Result<EventSubWebSocketSession> {
    while let Some(text) = next_text_frame(ws).await? {
        let envelope = decode_eventsub_websocket_message(&text)
            .context("failed to parse EventSub welcome payload")?;
        match envelope.message_type()? {
            EventSubMessageType::SessionWelcome => {
                return envelope
                    .session()
                    .cloned()
                    .ok_or_else(|| anyhow!("EventSub welcome payload missing session"));
            }
            EventSubMessageType::SessionReconnect => {
                let url = envelope
                    .session()
                    .and_then(|session| session.reconnect_url.clone())
                    .unwrap_or_else(|| "<missing reconnect URL>".to_string());
                bail!("EventSub requested reconnect before welcome: {url}");
            }
            EventSubMessageType::SessionDisconnect => {
                let status = envelope
                    .session()
                    .and_then(|session| session.status.clone())
                    .unwrap_or_else(|| "disconnected".to_string());
                let reason = envelope
                    .session()
                    .and_then(|session| session.disconnect_reason.clone());
                bail!("EventSub disconnected before welcome (status {status}): {reason:?}");
            }
            _ => {}
        }
    }

    bail!("EventSub WebSocket closed before welcome message was received")
}

async fn create_chat_subscription(
    http: &Client,
    config: &ChatSubscriptionConfig,
    session_id: &str,
) -> Result<()> {
    let message_request = channel_chat_message_subscription_request(
        &config.broadcaster_id,
        &config.user_id,
        session_id,
    );
    let delete_request = channel_chat_message_delete_subscription_request(
        &config.broadcaster_id,
        &config.user_id,
        session_id,
    );
    let (message_result, delete_result) = tokio::join!(
        create_subscription(http, config, &message_request),
        create_subscription(http, config, &delete_request),
    );
    message_result?;
    if let Err(err) = delete_result {
        debug!(
            ?err,
            "failed to subscribe to channel.chat.message_delete; continuing without delete events"
        );
    }
    Ok(())
}

async fn create_subscription(
    http: &Client,
    config: &ChatSubscriptionConfig,
    subscription: &crate::eventsub::CreateEventSubSubscriptionRequest,
) -> Result<()> {
    let prepared = helix::create_eventsub_subscription_request(
        &config.client_id,
        &config.access_token,
        subscription,
    )
    .context("failed to serialize EventSub subscription request")?;
    let response = send_prepared_request(http, prepared).await?;

    if response.status == 202 {
        return Ok(());
    }

    match response.status {
        401 => Err(EventSubAuthError.into()),
        403 => {
            bail!(
                "Twitch reported insufficient permissions for EventSub subscription: {}",
                response.body
            );
        }
        _ => {
            bail!(
                "Twitch EventSub subscription request failed (status {}): {}",
                response.status,
                response.body
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustls_provider_init_is_idempotent() {
        ensure_rustls_crypto_provider().expect("first provider init should succeed");
        ensure_rustls_crypto_provider().expect("second provider init should succeed");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn eventsub_connect_error_path_does_not_panic_for_missing_provider() {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            connect_eventsub_websocket("wss://127.0.0.1:1/ws", "test-client-id", "test-token"),
        )
        .await
        .expect("local websocket connect test should fail quickly");

        assert!(
            result.is_err(),
            "expected connect failure against a closed localhost port"
        );
    }
}
