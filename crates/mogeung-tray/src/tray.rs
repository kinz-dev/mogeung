//! The StatusNotifierItem itself: rendering tray state ksni asks for.
//!
//! Holds only what the item shows — connectedness and the waiting rows.
//! All derivation happens in `model`; all launching in `launch`. Icons are
//! freedesktop theme names, so the tray borrows whatever theme the desktop
//! already has instead of shipping pixmaps.

use crate::launch;
use crate::model::WaitingEntry;
use ksni::menu::{MenuItem, StandardItem};
use ksni::{Status, ToolTip};

pub struct MogeungTray {
    connected: bool,
    waiting: Vec<WaitingEntry>,
}

impl MogeungTray {
    pub fn new() -> Self {
        MogeungTray {
            connected: false,
            waiting: Vec::new(),
        }
    }

    /// The daemon spoke: show this waiting list as live.
    pub fn set_connected(&mut self, waiting: Vec<WaitingEntry>) {
        self.connected = true;
        self.waiting = waiting;
    }

    /// The connection dropped. The old count is discarded, not dimmed —
    /// a stale number shown as live is the one lie this tray must never
    /// tell (spec 0019).
    pub fn set_unreachable(&mut self) {
        self.connected = false;
        self.waiting.clear();
    }

    fn summary(&self) -> String {
        if !self.connected {
            "mogeung: daemon unreachable".to_string()
        } else {
            match self.waiting.len() {
                0 => "mogeung: nothing waiting".to_string(),
                1 => "mogeung: 1 session waiting".to_string(),
                n => format!("mogeung: {n} sessions waiting"),
            }
        }
    }

    fn open_window() {
        if let Err(e) = launch::open_window() {
            eprintln!("mogeung-tray: {e}");
        }
    }
}

impl ksni::Tray for MogeungTray {
    fn id(&self) -> String {
        "mogeung-tray".into()
    }

    fn title(&self) -> String {
        self.summary()
    }

    fn icon_name(&self) -> String {
        if !self.connected {
            // Distinct from every live state, including "0 waiting".
            "network-offline".into()
        } else if self.waiting.is_empty() {
            "user-available".into()
        } else {
            "dialog-warning".into()
        }
    }

    fn attention_icon_name(&self) -> String {
        "dialog-warning".into()
    }

    fn status(&self) -> Status {
        if self.connected && !self.waiting.is_empty() {
            Status::NeedsAttention
        } else {
            Status::Active
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let description = if !self.connected {
            "no live count — retrying".to_string()
        } else if self.waiting.is_empty() {
            "no session is waiting for you".to_string()
        } else {
            self.waiting
                .iter()
                .map(|w| w.label.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        };
        ToolTip {
            icon_name: self.icon_name(),
            icon_pixmap: Vec::new(),
            title: self.summary(),
            description,
        }
    }

    /// Left click: straight to the window. The count says whether to come;
    /// the window says everything else.
    fn activate(&mut self, _x: i32, _y: i32) {
        Self::open_window();
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = Vec::new();

        if !self.connected {
            items.push(
                StandardItem {
                    label: "daemon unreachable — retrying".into(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        } else if self.waiting.is_empty() {
            items.push(
                StandardItem {
                    label: "nothing waiting".into(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        } else {
            for w in &self.waiting {
                items.push(
                    StandardItem {
                        label: w.label.clone(),
                        // Raising the window is the only action the tray
                        // has — it observes the observer, nothing more.
                        activate: Box::new(|_this: &mut Self| Self::open_window()),
                        ..Default::default()
                    }
                    .into(),
                );
            }
        }

        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Open mogeung".into(),
                activate: Box::new(|_this: &mut Self| Self::open_window()),
                ..Default::default()
            }
            .into(),
        );
        items.push(
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|_this: &mut Self| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}
