//! `window.open`, so that dockview can put a group in its own OS window.
//! `R-B55`,
//! [ADR-0037](../../../docs/decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md)
//! and its 2026-09-08 amendment.
//!
//! # What this is, and what it replaced
//!
//! The first cut opened a **second client** — its own store, its own socket,
//! its own tmux attach — and could therefore never let you drag a pane between
//! the two windows: separate React trees, separate dockviews, and HTML5
//! drag-and-drop does not cross OS windows.
//!
//! dockview's own `addPopoutGroup` does span windows, because both are one
//! dockview rendering into two documents. It is built on `window.open`, which
//! ADR-0037 refused on the grounds that its behaviour in a Tauri webview was
//! unproven. **That was half right.** `window.open` in Tauri v2 is not absent,
//! it is **opt-in**: `tauri-runtime-wry` installs wry's handler only when the
//! app supplies one, and `WebviewWindowBuilder::on_new_window` is the public
//! way to supply it. mogeung never had, which is exactly why a `window.open`
//! would have done nothing at all. This module is that switch.
//!
//! # Why the window is labelled `popout-…`
//!
//! `capabilities/default.json` matches `popout-*` and gives it what `main` has.
//! A window that matches no capability has **no commands**, and a webview with
//! no commands fails as a blank pane rather than as an error — so the label
//! format is load-bearing and there is a test below that says so.
//!
//! # Why these windows keep their decorations
//!
//! The main window draws its own title bar and has `decorations: false`. A
//! dockview popout draws a *group header* — tabs — and no window chrome, so a
//! popout without decorations would be a window you cannot move or close. It
//! gets the system's.

use std::sync::atomic::{AtomicU32, Ordering};

use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Labels have to be unique for the lifetime of the process, and dockview may
/// open and close popouts all afternoon. A counter is enough and, unlike a
/// name derived from the panel, cannot collide with a window still closing.
static NEXT: AtomicU32 = AtomicU32::new(1);

/// The label for the next popout window.
pub fn next_label() -> String {
    format!("popout-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// Is this a window we opened for a dockview popout?
///
/// Used to keep the main window's own rules from applying to them — and to
/// state the naming contract in one place rather than in a `starts_with` at
/// each call site.
pub fn is_popout(label: &str) -> bool {
    label.starts_with("popout-")
}

/// Build the main window with `window.open` enabled.
///
/// The window is declared in `tauri.conf.json` with `"create": false` so that
/// Tauri does not make it before we get here: `on_new_window` is a *builder*
/// option, and a window Tauri has already created cannot be given one.
pub fn build_main(app: &tauri::AppHandle) -> tauri::Result<()> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .ok_or_else(|| tauri::Error::WindowNotFound)?;

    let handle = app.clone();
    WebviewWindowBuilder::from_config(app, &config)?
        .on_new_window(move |url, features| {
            let label = next_label();
            // `about:blank` to start: wry navigates the webview it is handed to
            // the URL that was actually requested. dockview asks for a
            // same-origin page of its own (`popout.html`), which is why this
            // does not need to know anything about the address.
            let builder = WebviewWindowBuilder::new(
                &handle,
                &label,
                WebviewUrl::External("about:blank".parse().expect("about:blank parses")),
            )
            .window_features(features)
            .title("mogeung")
            // dockview names the popout after the panel in it, through the
            // document title. Following it means the taskbar says which pane
            // this window holds rather than saying "mogeung" four times.
            .on_document_title_changed(|window, title| {
                let _ = window.set_title(&title);
            });

            match builder.build() {
                Ok(window) => tauri::webview::NewWindowResponse::Create { window },
                Err(e) => {
                    // Denied rather than panicking: a popout that cannot open
                    // is a pane that stays where it was, which is a
                    // disappointment and not a broken app.
                    eprintln!("could not open a window for {url}: {e}");
                    tauri::webview::NewWindowResponse::Deny
                }
            }
        })
        .build()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The label is what `capabilities/default.json` matches with `popout-*`.
    /// A label that stopped starting with it would produce a window with no
    /// commands, which fails as a blank pane rather than as an error.
    #[test]
    fn every_label_is_matched_by_the_capability_glob() {
        for _ in 0..5 {
            let label = next_label();
            assert!(is_popout(&label), "{label} would match no capability");
        }
    }

    /// dockview opens and closes these freely, and a label that came round
    /// again while the previous window was still closing would collide.
    #[test]
    fn labels_do_not_repeat() {
        let a = next_label();
        let b = next_label();
        assert_ne!(a, b);
    }

    /// The main window must not be mistaken for one of these: it is the one
    /// window that draws its own chrome and owns the dockview.
    #[test]
    fn the_main_window_is_not_a_popout() {
        assert!(!is_popout("main"));
        assert!(!is_popout("popout"));
        assert!(is_popout("popout-1"));
    }
}
