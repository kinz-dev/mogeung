//! mogeung — native client.
//!
//! A projection of daemon state. It holds no authoritative data of its own:
//! every mutation is a command to the daemon, and every change comes back on
//! the event stream. Closing this window does not stop any agent.

mod app;
mod net;
mod ui;

/// Tiny argument parsing, so the UI does not pull in clap for one flag.
fn parse_url() -> String {
    let mut url = "ws://127.0.0.1:7717/ws".to_string();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--url" => {
                if let Some(v) = it.next() {
                    url = v;
                }
            }
            "-h" | "--help" => {
                println!("mogeung [--url ws://host:port/ws]");
                std::process::exit(0);
            }
            other => eprintln!("ignoring unknown argument: {other}"),
        }
    }
    url
}

fn main() -> eframe::Result<()> {
    let url = parse_url();
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
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, url)))),
    )
}
