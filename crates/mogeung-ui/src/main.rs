//! mogeung — the window, and a daemon when one is needed.
//!
//! A projection of daemon state. It holds no authoritative data of its own:
//! every mutation is a command to the daemon, and every change comes back on
//! the event stream. Closing this window does not stop any agent.
//!
//! Since ADR-0009 it will also *host* a daemon when none is running, so one
//! executable is enough. That does not change the authority model — the window
//! still talks to the daemon over the same websocket, whether that daemon is in
//! this process or another one.

mod app;
mod daemon;
mod diff;
mod filter;
mod hotkey;
mod keymap;
mod net;
mod prefs;
mod term;
mod ui;

use std::path::PathBuf;

struct Args {
    addr: String,
    url: Option<String>,
    /// System-wide shortcut that raises this window. `None` disables it.
    hotkey: Option<String>,
    /// Start a daemon if none is running.
    start_daemon: bool,
    db: Option<PathBuf>,
    notify: bool,
    push_url: Option<String>,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            addr: "127.0.0.1:7717".into(),
            url: None,
            hotkey: Some(hotkey::DEFAULT.to_string()),
            start_daemon: true,
            db: None,
            notify: false,
            push_url: None,
        }
    }
}

/// Tiny argument parsing, so the UI does not pull in clap.
fn parse_args() -> Args {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--addr" => {
                if let Some(v) = it.next() {
                    args.addr = v;
                }
            }
            "--url" => args.url = it.next(),
            "--hotkey" => {
                if let Some(v) = it.next() {
                    args.hotkey = Some(v);
                }
            }
            "--no-hotkey" => args.hotkey = None,
            "--no-daemon" => args.start_daemon = false,
            "--db" => args.db = it.next().map(PathBuf::from),
            "--notify" => args.notify = true,
            "--push-url" => args.push_url = it.next(),
            "-h" | "--help" => {
                println!(
"mogeung — the mogeung window

Starts a daemon if none is already watching, and attaches to one if there is.
A daemon this window started stops when the window closes; one that was already
running is left alone.

Options:
  --addr HOST:PORT daemon address (default 127.0.0.1:7717)
  --url URL        connect to this websocket instead, and never start a daemon
  --no-daemon      attach only; do not start one
  --db PATH        database for a daemon we start (default ~/.mogeung/mogeung.db)
  --notify         desktop notifications, for a daemon we start
  --push-url URL   push notifications, for a daemon we start
  --hotkey ACCEL   system-wide key that raises this window
                   default {default}; e.g. \"Alt+Space\", \"Shift+Cmd+J\", \"F13\"
  --no-hotkey      do not register a system-wide key
  -h, --help       this

A shortcut macOS reserves for itself (Cmd+Space, Cmd+Tab) will appear to
register and then never fire — the system consumes it first. If nothing
happens, try another combination rather than assuming it is broken.

For a daemon that outlives every window — so notifications and the phone client
keep working — run `mogeungd` separately instead.",
                    default = hotkey::DEFAULT
                );
                std::process::exit(0);
            }
            other => eprintln!("ignoring unknown argument: {other}"),
        }
    }
    args
}

fn main() -> eframe::Result<()> {
    let args = parse_args();

    // Daemon logs go to stderr only if we are the one running it.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mogeungd=info".into()),
        )
        .try_init();

    // An explicit --url means "talk to that", full stop: the user has pointed
    // us somewhere, possibly another machine, and starting a local daemon would
    // be answering a question nobody asked.
    let (mode, ws_url) = match &args.url {
        Some(url) => (daemon::Mode::None, url.clone()),
        None => {
            let (mode, listener) = daemon::acquire(&args.addr, args.start_daemon);
            if let Some(listener) = listener {
                daemon::host(
                    listener,
                    args.db.clone(),
                    mogeungd::notify::NotifyConfig {
                        desktop: args.notify,
                        push_url: args.push_url.clone(),
                    },
                );
            }
            (mode, format!("ws://{}/ws", args.addr))
        }
    };
    eprintln!("{}", mode.detail(&args.addr));

    let (hk, hk_error) = match &args.hotkey {
        None => (None, None),
        Some(accel) => match hotkey::Hotkey::register(accel) {
            Ok(h) => {
                eprintln!("global hotkey: {accel} raises this window");
                (Some(h), None)
            }
            Err(e) => {
                eprintln!("global hotkey unavailable: {e}");
                (None, Some(e))
            }
        },
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("mogeung"),
        ..Default::default()
    };
    let addr = args.addr.clone();
    eframe::run_native(
        "mogeung",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, ws_url, hk, hk_error, mode, addr)))),
    )
}
