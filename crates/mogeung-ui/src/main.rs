//! mogeung — native client.
//!
//! A projection of daemon state. It holds no authoritative data of its own:
//! every mutation is a command to the daemon, and every change comes back on
//! the event stream. Closing this window does not stop any agent.

mod app;
mod diff;
mod filter;
mod hotkey;
mod keymap;
mod net;
mod prefs;
mod ui;

struct Args {
    url: String,
    /// System-wide shortcut that raises this window. `None` disables it.
    hotkey: Option<String>,
}

/// Tiny argument parsing, so the UI does not pull in clap for three flags.
fn parse_args() -> Args {
    let mut args = Args {
        url: "ws://127.0.0.1:7717/ws".to_string(),
        hotkey: Some(hotkey::DEFAULT.to_string()),
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--url" => {
                if let Some(v) = it.next() {
                    args.url = v;
                }
            }
            "--hotkey" => {
                if let Some(v) = it.next() {
                    args.hotkey = Some(v);
                }
            }
            "--no-hotkey" => args.hotkey = None,
            "-h" | "--help" => {
                println!(
"mogeung — the mogeung window

Options:
  --url URL        daemon websocket (default ws://127.0.0.1:7717/ws)
  --hotkey ACCEL   system-wide key that raises this window
                   default {default}; e.g. \"Alt+Space\", \"Shift+Cmd+J\", \"F13\"
  --no-hotkey      do not register a system-wide key
  -h, --help       this

A shortcut macOS reserves for itself (Cmd+Space, Cmd+Tab) will appear to
register and then never fire — the system consumes it first. If nothing
happens, try another combination rather than assuming it is broken.",
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

    // Registered before the event loop starts, and on the main thread, which
    // macOS requires. A failure here is reported into the window rather than
    // being fatal — someone else owning a shortcut must not stop mogeung from
    // opening.
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
    eframe::run_native(
        "mogeung",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, args.url, hk, hk_error)))),
    )
}
