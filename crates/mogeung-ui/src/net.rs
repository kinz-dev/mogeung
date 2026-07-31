//! WebSocket client, bridged into the egui frame loop.
//!
//! A dedicated OS thread runs a small tokio runtime holding the connection.
//! Incoming `ServerMsg`s cross into the UI over a plain std channel, and the
//! thread pokes egui to repaint. That keeps the whole UI synchronous and
//! immediate-mode with no async colouring.

use futures_util::{SinkExt, StreamExt};
use mogeung_core::{ClientMsg, ServerMsg};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

pub enum NetEvent {
    Connected,
    Disconnected(String),
    Msg(Box<ServerMsg>),
}

pub struct Net {
    rx: Receiver<NetEvent>,
    tx: Sender<ClientMsg>,
    pub connected: bool,
    pub last_error: Option<String>,
    pub url: String,
}

impl Net {
    pub fn connect(url: String, ctx: egui::Context) -> Net {
        let (ev_tx, ev_rx) = channel::<NetEvent>();
        let (cmd_tx, cmd_rx) = channel::<ClientMsg>();
        let ws_url = url.clone();

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ev_tx.send(NetEvent::Disconnected(e.to_string()));
                    return;
                }
            };
            rt.block_on(net_loop(ws_url, ev_tx, cmd_rx, ctx));
        });

        Net {
            rx: ev_rx,
            tx: cmd_tx,
            connected: false,
            last_error: None,
            url,
        }
    }

    pub fn send(&self, msg: ClientMsg) {
        let _ = self.tx.send(msg);
    }

    /// Drain everything the network thread has produced since the last frame.
    pub fn drain(&mut self) -> Vec<ServerMsg> {
        let mut out = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(NetEvent::Connected) => {
                    self.connected = true;
                    self.last_error = None;
                }
                Ok(NetEvent::Disconnected(e)) => {
                    self.connected = false;
                    self.last_error = Some(e);
                }
                Ok(NetEvent::Msg(m)) => out.push(*m),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }
}

/// Post an event, reporting whether anyone is still listening.
///
/// The `Net` that owns the receiving end is dropped when the window switches
/// daemons (`R-I7`), and this is how the thread finds out. It matters more than
/// it looks: without it a switch leaves the old thread reconnecting for ever —
/// waking every two seconds, failing to send, and calling `request_repaint` on
/// a context it has no business waking. Three switches in a session and there
/// are three of them.
fn post(ev_tx: &Sender<NetEvent>, ev: NetEvent, ctx: &egui::Context) -> bool {
    if ev_tx.send(ev).is_err() {
        return false;
    }
    ctx.request_repaint();
    true
}

async fn net_loop(
    url: String,
    ev_tx: Sender<NetEvent>,
    cmd_rx: Receiver<ClientMsg>,
    ctx: egui::Context,
) {
    loop {
        match tokio_tungstenite::connect_async(&url).await {
            Ok((ws, _)) => {
                if !post(&ev_tx, NetEvent::Connected, &ctx) {
                    return;
                }
                let (mut sink, mut stream) = ws.split();

                loop {
                    // Forward any commands the UI queued since the last poll.
                    let mut dead = false;
                    loop {
                        match cmd_rx.try_recv() {
                            Ok(cmd) => {
                                let txt = serde_json::to_string(&cmd).unwrap_or_default();
                                if sink.send(Message::Text(txt.into())).await.is_err() {
                                    dead = true;
                                    break;
                                }
                            }
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => return,
                        }
                    }
                    if dead {
                        break;
                    }

                    // Then take whatever arrived, without blocking the command
                    // pump for longer than a frame.
                    match tokio::time::timeout(Duration::from_millis(50), stream.next()).await {
                        Ok(Some(Ok(Message::Text(t)))) => {
                            if let Ok(msg) = serde_json::from_str::<ServerMsg>(&t) {
                                if !post(&ev_tx, NetEvent::Msg(Box::new(msg)), &ctx) {
                                    return;
                                }
                            }
                        }
                        Ok(Some(Ok(Message::Close(_)))) | Ok(None) => break,
                        Ok(Some(Err(e))) => {
                            if !post(&ev_tx, NetEvent::Disconnected(e.to_string()), &ctx) {
                                return;
                            }
                            break;
                        }
                        // Timeout or a frame type we do not care about.
                        _ => {}
                    }
                }
                if !post(&ev_tx, NetEvent::Disconnected("connection closed".into()), &ctx) {
                    return;
                }
            }
            Err(e) => {
                // Through `redacted` because this string is rendered, and a
                // transport error is free to quote the URI it failed on — which
                // is the dialled one, token and all. It is the identity
                // function on anything without a `token=` parameter, so this
                // costs nothing on the errors that do not.
                let msg = NetEvent::Disconnected(crate::connections::redacted(&format!(
                    "cannot reach daemon: {e}"
                )));
                if !post(&ev_tx, msg, &ctx) {
                    return;
                }
            }
        }
        // Reconnect forever: the daemon outliving or restarting under the UI is
        // normal, not an error state the user should have to act on.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[cfg(test)]
mod lifetime_tests {
    use super::*;

    /// The network thread must die when its `Net` does.
    ///
    /// `R-I7` lets the window switch daemons at runtime, which means dropping a
    /// `Net` and building another — and the old thread reconnects for ever
    /// unless something tells it to stop. Nothing does, explicitly: the signal
    /// is that both channels have lost their far end. This asserts the loop
    /// actually returns on that, rather than spinning invisibly for the rest of
    /// the session.
    ///
    /// The url points at a closed port on purpose. That is the hard case — a
    /// thread that never connects sits in the retry path, which is exactly
    /// where a missed check would leave it.
    #[test]
    fn the_network_thread_stops_when_its_owner_is_dropped() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let (ev_tx, ev_rx) = channel::<NetEvent>();
            let (cmd_tx, cmd_rx) = channel::<ClientMsg>();
            drop(ev_rx);
            drop(cmd_tx);

            let loop_done = net_loop(
                "ws://127.0.0.1:1/ws".to_string(),
                ev_tx,
                cmd_rx,
                egui::Context::default(),
            );

            // Generous: the loop sleeps two seconds between attempts, and the
            // point is that it returns at all rather than how fast.
            tokio::time::timeout(Duration::from_secs(10), loop_done)
                .await
                .expect("net_loop must return once nobody is listening");
        });
    }
}

#[cfg(test)]
mod tls_tests {
    use tokio::io::AsyncReadExt;

    /// The window can actually speak `wss://`.
    ///
    /// It could not until 2026-07-31: `tokio-tungstenite` was built with no TLS
    /// feature, so a `wss://` URL failed with *"TLS support not compiled in"*
    /// no matter what was on the other end. That closed off the ordinary answer
    /// to having no TLS of our own — put a reverse proxy in front — because the
    /// client could not dial the proxy. See `R-I10`.
    ///
    /// Proven by what goes on the wire rather than by which error comes back.
    /// A plain TCP listener accepts, and the first bytes must be a TLS
    /// handshake record: `0x16` (handshake), then `0x03` (the SSL 3.x-derived
    /// version byte every TLS ClientHello still carries). Nothing but a real
    /// TLS client sends that.
    ///
    /// **This test is load-bearing beyond the feature flag.** rustls 0.23 picks
    /// its crypto provider from features, and with none selected it does not
    /// fail to compile — it *panics at runtime* on the first TLS connection.
    /// A dependency bump that drops `ring` would therefore turn every `wss://`
    /// connection into a panic, silently, and this is what catches it.
    #[test]
    fn the_window_speaks_wss_not_just_ws() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let first_bytes = rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();

            let server = tokio::spawn(async move {
                let (mut sock, _) = listener.accept().await.expect("a connection");
                let mut buf = [0u8; 2];
                sock.read_exact(&mut buf).await.ok();
                buf
            });

            // Fails — there is no TLS server here — but only after the client
            // has spoken, which is the whole point.
            let _ = tokio_tungstenite::connect_async(format!("wss://127.0.0.1:{port}/ws")).await;
            server.await.expect("server task")
        });

        assert_eq!(
            first_bytes[0], 0x16,
            "expected a TLS handshake record, got {first_bytes:02x?} — \
             is the tokio-tungstenite TLS feature still enabled?"
        );
        assert_eq!(
            first_bytes[1], 0x03,
            "expected a TLS version byte, got {first_bytes:02x?}"
        );
    }
}
