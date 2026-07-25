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

async fn net_loop(
    url: String,
    ev_tx: Sender<NetEvent>,
    cmd_rx: Receiver<ClientMsg>,
    ctx: egui::Context,
) {
    loop {
        match tokio_tungstenite::connect_async(&url).await {
            Ok((ws, _)) => {
                let _ = ev_tx.send(NetEvent::Connected);
                ctx.request_repaint();
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
                                let _ = ev_tx.send(NetEvent::Msg(Box::new(msg)));
                                ctx.request_repaint();
                            }
                        }
                        Ok(Some(Ok(Message::Close(_)))) | Ok(None) => break,
                        Ok(Some(Err(e))) => {
                            let _ = ev_tx.send(NetEvent::Disconnected(e.to_string()));
                            break;
                        }
                        // Timeout or a frame type we do not care about.
                        _ => {}
                    }
                }
                let _ = ev_tx.send(NetEvent::Disconnected("connection closed".into()));
                ctx.request_repaint();
            }
            Err(e) => {
                let _ = ev_tx.send(NetEvent::Disconnected(format!(
                    "cannot reach daemon: {e}"
                )));
                ctx.request_repaint();
            }
        }
        // Reconnect forever: the daemon outliving or restarting under the UI is
        // normal, not an error state the user should have to act on.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
