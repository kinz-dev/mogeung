//! mogeung-tray — the WAITING count, without the window (R-C2).
//!
//! A StatusNotifierItem whose whole job is one number: how many of your
//! Claude Code sessions are waiting for you to type. It is a client like
//! every other — it reads the daemon over the same websocket the window
//! uses, holds no authority, touches nothing on disk, and its only action
//! is raising the window. The observer's observer.

mod launch;
mod model;
#[cfg(target_os = "linux")]
mod tray;

struct Args {
    url: String,
}

/// Tiny argument parsing, same reasoning as the window: no clap for one flag.
fn parse_args() -> Args {
    let mut url = "ws://127.0.0.1:7717/ws".to_string();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--url" => {
                if let Some(v) = it.next() {
                    // A bare host:port is what the window's --addr takes;
                    // accept it here too and fill in the rest.
                    url = if v.contains("://") {
                        v
                    } else {
                        format!("ws://{v}/ws")
                    };
                }
            }
            "-h" | "--help" => {
                println!(
                    "mogeung-tray — the WAITING count in the system tray

Connects to a running mogeung daemon and shows how many sessions are
waiting for you. When the daemon is unreachable it says so instead of
showing a stale number, and keeps retrying. It never starts a daemon,
never reads ~/.claude, and its only action is opening the window.

Options:
  --url URL   daemon websocket (default ws://127.0.0.1:7717/ws);
              a bare HOST:PORT is also accepted
  -h, --help  this"
                );
                std::process::exit(0);
            }
            other => eprintln!("mogeung-tray: ignoring unknown argument: {other}"),
        }
    }
    Args { url }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    parse_args();
    // The spec ships the Linux half; the macOS menu-bar item is noted
    // untested and unbuilt there. Exiting cleanly beats pretending.
    eprintln!("mogeung-tray: only the Linux StatusNotifierItem is built (spec 0019)");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn main() {
    // Named rather than inferred: rustls picks its crypto provider from cargo
    // features, and anything other than exactly one panics on the first TLS
    // connection instead of failing the build. See `pin_tls_backend` in the
    // window, which carries the longer version of this note. `R-I10`.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args = parse_args();
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("mogeung-tray: could not start a runtime: {e}");
            std::process::exit(1);
        }
    };
    rt.block_on(run(args.url));
}

#[cfg(target_os = "linux")]
async fn run(url: String) {
    use ksni::TrayMethods;

    // No tray on this desktop is a fact to report, not a crash: GNOME needs
    // the AppIndicator extension before a StatusNotifierItem shows at all.
    let handle = match tray::MogeungTray::new().spawn().await {
        Ok(h) => h,
        Err(e) => {
            eprintln!(
                "mogeung-tray: no tray on this desktop ({e}); \
                 on GNOME, install the AppIndicator extension — exiting"
            );
            return;
        }
    };

    net_loop(url, handle).await;
    eprintln!("mogeung-tray: tray service closed — exiting");
}

/// Connect, subscribe, fold broadcasts into the model, and push changes to
/// the item. On any failure: flip the tray to its unreachable state and
/// retry with exponential backoff, forever. A daemon restarting under its
/// clients is normal here, not an error the user should act on.
#[cfg(target_os = "linux")]
async fn net_loop(url: String, handle: ksni::Handle<tray::MogeungTray>) {
    use futures_util::{SinkExt, StreamExt};
    use mogeung_core::{ClientMsg, ServerMsg};
    use std::time::Duration;
    use tokio_tungstenite::tungstenite::Message;

    let mut model = model::Model::default();
    let mut backoff = Duration::from_secs(1);

    loop {
        match tokio_tungstenite::connect_async(&url).await {
            Ok((ws, _)) => {
                let (mut sink, mut stream) = ws.split();
                let sub = serde_json::to_string(&ClientMsg::Subscribe).unwrap_or_default();
                if sink.send(Message::Text(sub.into())).await.is_ok() {
                    // Not "connected" yet: the tray flips to live only once
                    // the daemon has actually said what is waiting, so the
                    // unreachable state can never dress up an old count.
                    let mut live = false;
                    while let Some(res) = stream.next().await {
                        match res {
                            Ok(Message::Text(txt)) => {
                                // Malformed or future messages: skip. The
                                // wire can grow; the tray must not care.
                                let Ok(msg) = serde_json::from_str::<ServerMsg>(&txt) else {
                                    continue;
                                };
                                let changed = model.apply(&msg);
                                let is_state = matches!(
                                    msg,
                                    ServerMsg::Snapshot { .. } | ServerMsg::Queue { .. }
                                );
                                if changed || (is_state && !live) {
                                    if !live {
                                        eprintln!("mogeung-tray: connected to {url}");
                                        backoff = Duration::from_secs(1);
                                        live = true;
                                    }
                                    let waiting = model.waiting();
                                    if handle
                                        .update(|t| t.set_connected(waiting))
                                        .await
                                        .is_none()
                                    {
                                        return;
                                    }
                                }
                            }
                            Ok(Message::Close(_)) | Err(_) => break,
                            _ => {}
                        }
                    }
                }
                eprintln!("mogeung-tray: daemon connection closed");
            }
            Err(e) => eprintln!("mogeung-tray: cannot reach daemon at {url}: {e}"),
        }

        if handle.update(|t| t.set_unreachable()).await.is_none() {
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}
