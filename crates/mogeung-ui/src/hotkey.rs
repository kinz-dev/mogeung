//! A system-wide key that brings the mogeung window to the front.
//!
//! The other half of `R-B2`. Jump-to-terminal solves *going* — the queue tells
//! you which session needs you and `enter` puts you in front of it. Getting
//! **back** was still a mouse hunt through whatever is on screen, and a round
//! trip is only as fast as its slower half.
//!
//! Deliberately not an in-app terminal. Embedding a running session is
//! impossible (its PTY master belongs to the terminal that created it) and
//! spawning our own would mean writing a worse iTerm2 inside mogeung — the same
//! trade that made v0.1 a worse Claude Code
//! ([ADR-0003](../../../docs/decisions/0003-observe-do-not-spawn.md)). This
//! costs two keys and nothing else.

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Low collision risk, and reachable one-handed.
///
/// A global hotkey is stolen from **every** application, so the default has to
/// be something almost nothing else claims. `Cmd+Shift+M` would have been the
/// obvious mnemonic and is used by several editors; `Ctrl+Cmd+M` is not.
///
/// Note that registering a shortcut macOS reserves for itself — `Cmd+Space`,
/// `Cmd+Tab` — **succeeds** and then never fires, because the system consumes
/// the key before any application sees it. There is no way to detect that from
/// here, so it is documented in `--help` instead.
pub const DEFAULT: &str = "Ctrl+Cmd+M";

pub struct Hotkey {
    /// Dropping the manager unregisters the key, so it has to be held for the
    /// lifetime of the app even though nothing reads it again.
    _manager: GlobalHotKeyManager,
    id: u32,
    pub accel: String,
    /// Set by the waker thread, cleared by the UI thread.
    pending: Arc<AtomicBool>,
}

impl Hotkey {
    /// Register `accel` system-wide.
    ///
    /// Returns a description of the problem rather than panicking: a hotkey
    /// another application already owns is a completely ordinary thing to hit,
    /// and it must not stop the window from opening.
    pub fn register(accel: &str) -> Result<Self, String> {
        let key = HotKey::from_str(accel)
            .map_err(|e| format!("{accel:?} is not a valid shortcut: {e}"))?;

        // Must be constructed on the main thread on macOS.
        let manager = GlobalHotKeyManager::new()
            .map_err(|e| format!("could not start the hotkey listener: {e}"))?;

        manager.register(key).map_err(|e| {
            format!("could not register {accel} — another application probably owns it ({e})")
        })?;

        Ok(Hotkey {
            _manager: manager,
            id: key.id,
            accel: accel.to_string(),
            pending: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start listening, waking the UI as soon as the key is pressed.
    ///
    /// Polling the receiver from the frame loop alone is not enough: a
    /// backgrounded window repaints roughly once a second, so a press would
    /// take up to a second to raise anything. For the one feature whose entire
    /// job is *get me back here now*, that reads as broken.
    ///
    /// So a thread blocks on the event channel and pokes egui, and the frame
    /// loop only reads a flag.
    pub fn start_waker(&self, ctx: egui::Context) {
        let pending = self.pending.clone();
        let id = self.id;
        std::thread::spawn(move || {
            // Only `Pressed` counts — every hotkey also reports a `Released`,
            // and acting on both would fire twice per press.
            while let Ok(ev) = GlobalHotKeyEvent::receiver().recv() {
                if ev.id == id && ev.state == HotKeyState::Pressed {
                    pending.store(true, Ordering::SeqCst);
                    ctx.request_repaint();
                }
            }
        });
    }

    /// True if the key was pressed since the last call, clearing the flag.
    ///
    /// A flag rather than a count: holding the key down must not build a
    /// backlog that keeps re-raising the window after you have let go.
    pub fn pressed(&self) -> bool {
        self.pending.swap(false, Ordering::SeqCst)
    }
}

/// Bring this window to the front from wherever the user currently is.
///
/// Three commands, because "not visible" has three different causes and only
/// handling the last one leaves the window stubbornly hidden: minimised to the
/// Dock, hidden, or simply behind something else.
pub fn raise(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default must parse, or the first run of a fresh checkout greets you
    /// with an error about a shortcut nobody chose.
    #[test]
    fn the_default_accelerator_parses() {
        assert!(HotKey::from_str(DEFAULT).is_ok(), "{DEFAULT} does not parse");
    }

    #[test]
    fn common_spellings_are_accepted() {
        // Users will type these; all must work without a lookup table in the
        // documentation.
        for s in [
            "Ctrl+Cmd+M",
            "ctrl+cmd+m",
            "Control+Command+KeyM",
            "CmdOrCtrl+Shift+M",
            "Alt+Space",
            "F13",
        ] {
            assert!(HotKey::from_str(s).is_ok(), "{s} should parse");
        }
    }

    #[test]
    fn nonsense_is_rejected_rather_than_silently_ignored() {
        // A shortcut that silently does not register is worse than one that
        // refuses: you would press it for a week wondering why nothing happens.
        for s in ["", "Ctrl+", "NotAKey", "Ctrl+Shift", "Ctrl+A+B"] {
            assert!(HotKey::from_str(s).is_err(), "{s:?} should be rejected");
        }
    }
}
