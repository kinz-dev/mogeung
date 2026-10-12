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
//! # Closing one, and popping back in (`R-J108`)
//!
//! Asked 2026-09-25: *"After popout, there is no easy way to pop back in. I
//! have to drag and drop the pane and then manually close the empty pop-up
//! window."* Two faults under one complaint, and neither was dockview's:
//!
//! - **A script's `window.close()` does not close the window.** dockview
//!   closes a popout it is finished with — the last pane dragged out, or the
//!   group sent home — with `window.close()`, and wry answers that on
//!   WebKitGTK by destroying the *webview* (`connect_close` →
//!   `webview.destroy()`) and leaving the OS window standing, empty. WKWebView
//!   has no handler at all. So the window has to be closed from here:
//!   [`popout_close`], which the client calls whenever a popout group is gone.
//! - **The window's own close button lost the panes.** Closing the OS window
//!   destroys the webview without the page's `beforeunload`, which is the one
//!   event dockview listens for to send the group home — so the group stayed in
//!   its model, attached to a window that no longer existed. Now the first
//!   close request is **routed to the client** (`popout:close`), which sends
//!   the group home the way dockview's own close would, and then closes the
//!   window. A second close request goes through regardless, so a client that
//!   never answers cannot make a window uncloseable.
//!
//! Every window this module opens is told its own label, as
//! `window.__MOGEUNG_WINDOW_LABEL__`, by an initialization script — which is
//! how the client, holding only the popout's `Window`, knows which shell
//! window to close. **And the opener is told too** (`R-J130`): the label and
//! the URL asked for go to the main window as `popout:opened` the moment the
//! window exists, so a page that is closed before it loaded — or never loads
//! — can still be closed by label rather than with the `window.close()` the
//! first point above says leaves a window standing. The dialogs in windows
//! of their own (`R-J128`, `lib/dialogWindow.ts`) mark their URL so they can
//! tell the announcement apart from a dockview popout's.
//!
//! # Why these windows keep their decorations
//!
//! The main window draws its own title bar and has `decorations: false`. A
//! dockview popout draws a *group header* — tabs — and no window chrome, so a
//! popout without decorations would be a window you cannot move or close. It
//! gets the system's.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

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

/// Popouts whose close request has already been routed to the client once.
static ASKED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Whether a close request for `label` should be held and handed to the
/// client (`true`), or allowed to close the window (`false`). The first is
/// held; any after it goes through, so a client that never answers cannot
/// make a window uncloseable.
pub fn route_close(label: &str) -> bool {
    let mut asked = ASKED.lock().unwrap();
    asked.get_or_insert_with(HashSet::new).insert(label.to_string())
}

/// The script that tells a window its own label.
pub fn label_script(label: &str) -> String {
    format!(
        "window.__MOGEUNG_WINDOW_LABEL__ = {};",
        serde_json::to_string(label).unwrap_or_else(|_| "null".into())
    )
}

/// The content size a page asked for, carried in its own URL as `w` and `h`
/// in CSS pixels. `R-J138`.
///
/// `window.open`'s features string — `width=1500,height=1000` — is what the
/// web says a page should use, and on macOS it works. **On Linux it never
/// arrives**: wry's WebKitGTK path hands the new-window handler
/// `NewWindowFeatures { size: None, position: None }` whatever the page said
/// (wry 0.55.1, `webkitgtk/mod.rs`), so `window_features` had nothing to
/// apply, every popout opened at the toolkit's 800×600, and a dialog then
/// measured 800×553 — that, minus a title bar — on close and remembered it as
/// the size it was left at. Found 2026-10-12, when a bigger Teams window
/// (`R-J137`) opened no bigger. The URL is the one thing both platforms
/// deliver intact, so the page says its size there too; where the features
/// also arrive the two agree.
///
/// Bounded, so a malformed or mischievous URL opens a window rather than a
/// 1×1 or a hundred-thousand-pixel one; half an ask is no ask.
pub fn requested_size(url: &tauri::Url) -> Option<(f64, f64)> {
    let mut w = None;
    let mut h = None;
    for (k, v) in url.query_pairs() {
        match &*k {
            "w" => w = v.parse::<f64>().ok(),
            "h" => h = v.parse::<f64>().ok(),
            _ => {}
        }
    }
    let (w, h) = (w?, h?);
    let sane = |x: f64| x.is_finite() && (200.0..=8000.0).contains(&x);
    (sane(w) && sane(h)).then_some((w, h))
}

/// Close a popout window for good — the one step a page cannot take for
/// itself here (see the module notes). Only ever a `popout-*` window: the
/// label comes from the page, and the main window is not a page's to close.
#[tauri::command]
pub fn popout_close(app: tauri::AppHandle, label: String) {
    if !is_popout(&label) {
        return;
    }
    if let Some(set) = ASKED.lock().unwrap().as_mut() {
        set.remove(&label);
    }
    if let Some(window) = app.get_webview_window(&label) {
        // `destroy`, not `close`: close raises another close request, which
        // would come straight back here.
        let _ = window.destroy();
    }
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
            let mut builder = WebviewWindowBuilder::new(
                &handle,
                &label,
                WebviewUrl::External("about:blank".parse().expect("about:blank parses")),
            )
            .window_features(features)
            .initialization_script(&label_script(&label))
            .title("mogeung")
            // dockview names the popout after the panel in it, through the
            // document title. Following it means the taskbar says which pane
            // this window holds rather than saying "mogeung" four times.
            .on_document_title_changed(|window, title| {
                let _ = window.set_title(&title);
            });
            // The size the page asked for, read off its URL rather than off
            // `features`, which arrives empty on Linux. See `requested_size`.
            if let Some((w, h)) = requested_size(&url) {
                builder = builder.inner_size(w, h);
            }

            match builder.build() {
                Ok(window) => {
                    // Which window this became, to the page that asked. The
                    // result is ignored on purpose: a main window that is
                    // gone has no dialog to close.
                    let _ = handle.emit_to(
                        "main",
                        "popout:opened",
                        serde_json::json!({ "label": label, "url": url.as_str() }),
                    );
                    // The close button sends the group home first; see the
                    // module notes.
                    let events = handle.clone();
                    let own = label.clone();
                    window.on_window_event(move |event| {
                        if let WindowEvent::CloseRequested { api, .. } = event {
                            if route_close(&own) {
                                api.prevent_close();
                                let _ = events.emit("popout:close", own.clone());
                            }
                        }
                    });
                    tauri::webview::NewWindowResponse::Create { window }
                }
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

    /// `R-J138`: the size a page asks for comes off its URL, bounded, and
    /// half an ask or nonsense is no ask at all.
    #[test]
    fn a_page_says_its_size_in_its_url_and_nonsense_is_no_size() {
        let u = |s: &str| tauri::Url::parse(s).unwrap();
        assert_eq!(requested_size(&u("tauri://localhost/popout.html?dialog=teams&w=1500&h=1000")), Some((1500.0, 1000.0)));
        assert_eq!(requested_size(&u("tauri://localhost/arcade.html?w=1280&h=860&theme=dark")), Some((1280.0, 860.0)));
        assert_eq!(requested_size(&u("tauri://localhost/popout.html")), None, "no ask, no size");
        assert_eq!(requested_size(&u("tauri://localhost/x?w=1500")), None, "half an ask is none");
        assert_eq!(requested_size(&u("tauri://localhost/x?w=1&h=1")), None, "too small to be a window");
        assert_eq!(requested_size(&u("tauri://localhost/x?w=100000&h=800")), None, "too big to be one");
        assert_eq!(requested_size(&u("tauri://localhost/x?w=abc&h=800")), None);
        assert_eq!(requested_size(&u("tauri://localhost/x?w=NaN&h=800")), None);
    }

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

    /// The first close request is held for the client; a second is not, so a
    /// window can always be closed.
    #[test]
    fn a_close_is_held_once_and_never_twice() {
        let label = next_label();
        assert!(route_close(&label), "the first is routed to the client");
        assert!(!route_close(&label), "the second closes the window");
        assert!(route_close(&next_label()), "another window has its own first");
    }

    /// The label reaches the page as a JSON string, so no label can break
    /// out of the script.
    #[test]
    fn the_page_is_told_its_label_as_a_string() {
        assert_eq!(label_script("popout-3"), r#"window.__MOGEUNG_WINDOW_LABEL__ = "popout-3";"#);
        assert_eq!(label_script(r#"a"b"#), r#"window.__MOGEUNG_WINDOW_LABEL__ = "a\"b";"#);
    }
}
