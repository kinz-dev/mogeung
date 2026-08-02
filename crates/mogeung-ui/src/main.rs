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
mod connections;
mod cli;
mod daemon;
mod diff;
mod explorer;
mod filter;
mod font;
mod gitview;
mod hotkey;
mod keymap;
mod layout;
mod net;
mod palette;
mod prefs;
mod search;
mod shells;
mod symbols;
mod term;
mod theme;
mod ui;

use std::path::PathBuf;

struct Args {
    addr: String,
    url: Option<String>,
    /// Shared token a remote daemon requires (`R-I4`); rides the ws URL as a
    /// query parameter.
    token: Option<String>,
    /// System-wide shortcut that raises this window. `None` disables it.
    hotkey: Option<String>,
    /// Start a daemon if none is running.
    start_daemon: bool,
    db: Option<PathBuf>,
    notify: bool,
    push_url: Option<String>,
    /// Stay attached to the terminal instead of detaching.
    foreground: bool,
    /// Where the detached process appends its console output. `None` discards.
    log: Option<PathBuf>,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            addr: connections::DEFAULT_ADDR.into(),
            url: None,
            token: None,
            hotkey: Some(hotkey::DEFAULT.to_string()),
            start_daemon: true,
            db: None,
            notify: false,
            push_url: None,
            foreground: false,
            log: None,
        }
    }
}

/// The file's answers, under the built-in ones. `R-J3`.
///
/// The merge is this way round — file first, command line over it — because
/// `parse_args` only ever *writes* what it is given, so anything the user
/// typed lands on top for free. A field is a preference or an invocation:
/// `--foreground`, `--log` and `--no-daemon` are the latter and stay out of
/// the file deliberately.
fn args_from_config(cfg: &mogeung_core::config::Config) -> Args {
    let mut args = Args::default();
    if let Some(addr) = &cfg.addr {
        args.addr = addr.clone();
    }
    args.url = cfg.url.clone().or(args.url);
    args.token = cfg.token.clone().or(args.token);
    args.db = cfg.db.clone().or(args.db);
    args.push_url = cfg.push_url.clone().or(args.push_url);
    args.notify |= cfg.notify.unwrap_or(false);
    if let Some(h) = cfg.hotkey_setting() {
        args.hotkey = h;
    }
    args
}

/// Tiny argument parsing, so the UI does not pull in clap.
fn parse_args() -> Args {
    let (cfg, warning) = mogeung_core::config::Config::load();
    if let Some(w) = warning {
        eprintln!("{w}");
    }
    let mut args = args_from_config(&cfg);
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--addr" => {
                if let Some(v) = it.next() {
                    args.addr = v;
                }
            }
            "--url" => args.url = it.next(),
            "--token" => args.token = it.next(),
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
            "--foreground" | "-f" => args.foreground = true,
            "--log" | "-log" => args.log = it.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!(
"mogeung — the mogeung window

Starts a daemon if none is already watching, and attaches to one if there is.
A daemon this window started stops when the window closes; one that was already
running is left alone.

Always starts on the local daemon. Daemons on other machines are saved in the
window and chosen there (Alt+D) — a remote is never dialled automatically.

Detaches from the terminal by default, like nohup: the prompt comes back at
once and closing the terminal does not close the window. Console output is
discarded unless --log names a file.

Options:
  --addr HOST:PORT daemon address (default 127.0.0.1:7717)
  --url URL        connect to this websocket instead, and never start a daemon
  --token TOKEN    shared token the daemon requires (see mogeungd --token);
                   sent as a query parameter — clear text, trusted networks only
  --no-daemon      attach only; do not start one
  --db PATH        database for a daemon we start (default ~/.mogeung/mogeung.db)
  --notify         desktop notifications, for a daemon we start
  --push-url URL   push notifications, for a daemon we start
  --log PATH       append console output to PATH instead of discarding it
  --foreground     stay attached to the terminal; output goes there as before
  --hotkey ACCEL   system-wide key that raises this window
                   default {default}; e.g. \"Alt+Space\", \"Shift+Cmd+J\", \"F13\"
  --no-hotkey      do not register a system-wide key
  -h, --help       this

A shortcut macOS reserves for itself (Cmd+Space, Cmd+Tab) will appear to
register and then never fire — the system consumes it first. If nothing
happens, try another combination rather than assuming it is broken.

For a daemon that outlives every window — so notifications keep firing and a
window elsewhere can attach — run `mogeungd` separately instead.",
                    default = hotkey::DEFAULT
                );
                std::process::exit(0);
            }
            other => eprintln!("ignoring unknown argument: {other}"),
        }
    }
    args
}

/// Marks the re-exec'd copy so it knows not to detach again.
const DETACHED_MARKER: &str = "MOGEUNG_DETACHED";

/// Re-run ourselves detached from the terminal, nohup-style: a new process
/// group, stdin from /dev/null, and stdout/stderr appended to `log` or
/// discarded. The re-exec'd copy sees `DETACHED_MARKER` and skips this.
#[cfg(unix)]
fn detach(log: Option<&std::path::Path>) -> std::io::Result<u32> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let (out, err) = match log {
        Some(path) => {
            let f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
            (Stdio::from(f.try_clone()?), Stdio::from(f))
        }
        None => (Stdio::null(), Stdio::null()),
    };
    let child = Command::new(std::env::current_exe()?)
        .args(std::env::args_os().skip(1))
        .env(DETACHED_MARKER, "1")
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .process_group(0)
        .spawn()?;
    Ok(child.id())
}

#[cfg(not(unix))]
fn detach(_log: Option<&std::path::Path>) -> std::io::Result<u32> {
    Err(std::io::Error::other("detaching is only supported on unix"))
}

/// Name the TLS backend rather than letting rustls infer one.
///
/// rustls 0.23 resolves its crypto provider from cargo features, and when the
/// answer is not exactly one it does **not** fail to build — it panics on the
/// first TLS connection, deep inside the network thread. So a future dependency
/// that happens to pull in a second provider would turn every `wss://` URL into
/// a crash, with nothing at build time to warn anyone.
///
/// Installing it explicitly makes that impossible: this wins regardless of what
/// else appears in the tree. The `Err` means it is already installed, which is
/// not a problem worth reporting. `R-I10`.
fn pin_tls_backend() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn main() -> eframe::Result<()> {
    pin_tls_backend();
    // Before anything else, including detaching: a subcommand is an answer on
    // stdout, and detaching would take the terminal away from it. `R-J4`.
    match cli::parse(std::env::args().skip(1)) {
        cli::Outcome::Window => {}
        cli::Outcome::Say(msg, code) => {
            if code == 0 {
                println!("{msg}");
            } else {
                eprintln!("{msg}");
            }
            std::process::exit(code);
        }
        cli::Outcome::Run(cmd, opts) => std::process::exit(cli::run(cmd, opts)),
    }

    let args = parse_args();

    // Hand the window over to a detached copy of ourselves and give the
    // terminal back, unless asked to stay (--foreground) or we *are* that
    // copy. Failure is not fatal — running attached is strictly less broken
    // than not running at all.
    if !args.foreground && std::env::var_os(DETACHED_MARKER).is_none() {
        match detach(args.log.as_deref()) {
            Ok(pid) => {
                match &args.log {
                    Some(path) => eprintln!("mogeung: detached (pid {pid}), logging to {}", path.display()),
                    None => eprintln!("mogeung: detached (pid {pid}); --log PATH keeps the output"),
                }
                return Ok(());
            }
            Err(e) => eprintln!("mogeung: could not detach ({e}); staying in the foreground"),
        }
    }

    // Daemon logs go to stderr only if we are the one running it.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mogeungd=info".into()),
        )
        .try_init();

    // Every launch starts on the local daemon.
    //
    // `R-I7` used to reopen the connection last chosen in the window, and that
    // was wrong in the one way a default must never be wrong: it was sticky and
    // invisible. Connect to a dev box once and every later launch dialled it —
    // including from a train, and including after that box was turned off — and
    // because the remembered URL was applied *here*, before `daemon::acquire`,
    // the window also stopped honouring ADR-0009. It never checked the local
    // port and never hosted a daemon, so the machine you were sitting in front
    // of was not being watched at all, and nothing said so.
    //
    // So the remembered choice no longer reaches this far. LOCAL is always the
    // start, always in the list, and another daemon is one keypress away in the
    // window (Alt+D) — a choice you make when you mean it, per session.
    //
    // An explicit --url means "talk to that", full stop: the user has pointed
    // us somewhere, possibly another machine, and starting a local daemon would
    // be answering a question nobody asked.
    let (mode, mut ws_url) = match &args.url {
        Some(url) => (daemon::Mode::None, url.clone()),
        None => {
            let (mode, listener) = daemon::acquire(&args.addr, args.start_daemon);
            if let Some(listener) = listener {
                // The hosted daemon refuses the same binds `mogeungd` refuses
                // (R-I10), but it refuses them on a background thread, where a
                // message is a line nobody reads before the window opens over
                // it. Ask the same question here, while stderr is still the
                // thing the user is looking at.
                if let Ok(bound) = listener.local_addr() {
                    if let Err(e) = mogeungd::server::admit(&bound, args.token.as_deref()) {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                }
                daemon::host(
                    listener,
                    args.db.clone(),
                    mogeungd::notify::NotifyConfig {
                        desktop: args.notify,
                        push_url: args.push_url.clone(),
                    },
                    args.token.clone(),
                );
            }
            (mode, format!("ws://{}/ws", args.addr))
        }
    };
    if let Some(t) = &args.token {
        // Query parameter, not a header: the ws client cannot set headers,
        // and the daemon accepts either (R-I4).
        ws_url.push_str(if ws_url.contains('?') { "&token=" } else { "?token=" });
        ws_url.push_str(t);
    }
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

    // The embedded icon covers X11; on Wayland the compositor ignores it and
    // resolves the icon from a desktop entry matching the app id, which
    // scripts/install.sh puts in place.
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/mogeung.png"))
        .expect("embedded icon is a valid PNG");

    // Geometry is read here rather than in `App::new`, which runs after the
    // window already exists: restoring a size *after* the first frame is a
    // visible jump. The App loads prefs again for everything else — two reads
    // of one file, a millisecond apart, rather than a second store. `R-J1`.
    let geometry = prefs::Prefs::load().0.window.and_then(|w| w.usable(None));
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(match geometry {
            Some(w) => [w.width, w.height],
            None => [1440.0, 900.0],
        })
        .with_min_inner_size([prefs::MIN_SIZE.0, prefs::MIN_SIZE.1])
        .with_title("mogeung")
        .with_app_id("mogeung")
        .with_icon(icon);
    if let Some(w) = geometry {
        if let (Some(x), Some(y)) = (w.x, w.y) {
            viewport = viewport.with_position([x, y]);
        }
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    // What the App displays and reasons about: an explicit --url is the
    // address that matters (it may be another machine — R-I4's remote check
    // reads this), otherwise the local addr.
    let addr = args.url.clone().unwrap_or_else(|| args.addr.clone());
    eframe::run_native(
        "mogeung",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, ws_url, hk, hk_error, mode, addr)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::config::Config;

    /// The window icon is `include_bytes!`d and decoded with an `expect`, so a
    /// file that is not a PNG is a panic on launch rather than a build error.
    /// That is a live risk here: the artwork arrives as a `.jpg`, and copying
    /// one to `mogeung.png` would look right in every file listing and crash
    /// the window for everyone.
    #[test]
    fn the_embedded_window_icon_is_a_png_eframe_can_decode() {
        let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/mogeung.png"))
            .expect("assets/mogeung.png must be a real PNG");
        assert_eq!(icon.width, icon.height, "an app icon has to be square");
        assert!(icon.width >= 256, "too small to install as a 512px icon: {}", icon.width);
        assert_eq!(
            icon.rgba.len(),
            (icon.width * icon.height * 4) as usize,
            "RGBA, so the transparent background survives"
        );
        // The corner is transparent: the source art is a sticker on white, and
        // a white square is exactly what an icon must not be on a dark dock.
        assert_eq!(icon.rgba[3], 0, "the top-left pixel must be transparent");
    }

    /// `R-J3`. The file supplies what the command line did not, and nothing
    /// more. A config that quietly won over a flag would be the worst kind of
    /// bug here: everything looks configured, and the thing you typed is
    /// ignored.
    #[test]
    fn the_file_fills_in_defaults_and_a_flag_still_wins() {
        let cfg = Config {
            addr: Some("10.0.0.4:7717".into()),
            hotkey: Some("Ctrl+Alt+K".into()),
            notify: Some(true),
            ..Config::default()
        };
        let from_file = args_from_config(&cfg);
        assert_eq!(from_file.addr, "10.0.0.4:7717");
        assert_eq!(from_file.hotkey.as_deref(), Some("Ctrl+Alt+K"));
        assert!(from_file.notify);
        // Untouched by the file, so still the built-in answer.
        assert!(from_file.start_daemon);
        assert!(!from_file.foreground, "an invocation flag is not a preference");

        // What `parse_args` does with a flag afterwards: overwrite. The order
        // is the whole mechanism, so assert it rather than trusting it.
        let mut args = from_file;
        args.addr = "127.0.0.1:9999".into();
        assert_eq!(args.addr, "127.0.0.1:9999");
    }

    /// An empty `hotkey` in the file means off — the file's `--no-hotkey`.
    /// Without this, disabling the global shortcut needs a flag every launch,
    /// which is exactly what a config file exists to stop.
    #[test]
    fn an_empty_hotkey_in_the_file_disables_it() {
        let cfg = Config { hotkey: Some(String::new()), ..Config::default() };
        assert_eq!(args_from_config(&cfg).hotkey, None);
        // And saying nothing leaves the default shortcut alone.
        assert_eq!(
            args_from_config(&Config::default()).hotkey.as_deref(),
            Some(hotkey::DEFAULT)
        );
    }
}
