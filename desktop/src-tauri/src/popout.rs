//! A pane in a window of its own. `R-B55`,
//! [ADR-0037](../../../docs/decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md).
//!
//! The window this opens is an **ordinary client**: it loads the same
//! `index.html`, runs the same store, opens its own socket to the same daemon
//! and attaches its own tmux client. It gets no capability the main window does
//! not have, and the daemon cannot tell it apart from any other.
//!
//! # Why the shell builds the URL
//!
//! The webview hands over a *kind* and a *session id*, never a URL. A command
//! that took a URL would be a window that could be pointed anywhere — at a
//! remote page, inside the app's own origin, with the app's own permissions —
//! and "the caller decides where this window goes" is not a sentence worth
//! writing. Both arguments are checked against an allowlist here, before
//! anything is created, the same posture `scratch::check_name` takes with a
//! name that arrived over a socket.
//!
//! # Why the daemon address is not passed
//!
//! Both windows load from one origin and therefore share `localStorage`, and
//! `defaultUrl` already falls back to `localStorage["mogeung.url"]`. Threading
//! the address through a query string would be a second source of truth, and
//! since `R-I16` an address can carry a token — which would then be in a window
//! label and a URL bar.

use serde::Serialize;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// The panes that may be popped out.
///
/// An allowlist rather than a check for absurdity: this string reaches a URL,
/// and the set of panes worth detaching is small, known, and grows by someone
/// deciding it should.
const POPPABLE: &[&str] = &["agent"];

/// A session id is a `String` from another program's file (`SessionId` is an
/// alias, not a newtype), so it is checked rather than trusted. Anything a
/// query string would have to escape is refused instead.
fn sane_session(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The window label, which is also what the capability file matches on.
///
/// `popout-*` is granted the same permissions as `main` in
/// `capabilities/default.json` — without that the new window has **no**
/// commands at all and its terminal cannot open a pty, which fails as an empty
/// black pane rather than as an error.
pub fn label_for(kind: &str, session: &str) -> String {
    format!("popout-{kind}-{session}")
}

/// Open the pane `kind` for `session` in its own OS window, or focus the one
/// that is already open for it.
///
/// Focusing rather than opening a second is the whole of the duplicate
/// handling: two windows attached to one tmux session is the two-head resize
/// churn ADR-0037 declines to build on purpose, and the gesture is easy to
/// repeat by accident.
#[tauri::command]
pub async fn popout_open(
    app: tauri::AppHandle,
    kind: String,
    session: String,
    title: Option<String>,
) -> Result<String, String> {
    if !POPPABLE.contains(&kind.as_str()) {
        return Err(format!("{kind} is not a pane that can be popped out"));
    }
    if !sane_session(&session) {
        return Err("that is not a session id".into());
    }

    let label = label_for(&kind, &session);
    if let Some(existing) = app.get_webview_window(&label) {
        // Both, and in this order: an unfocused window behind the main one
        // looks exactly like nothing happening.
        let _ = existing.unminimize();
        let _ = existing.set_focus();
        return Ok(label);
    }

    // Built here, from checked parts. The webview never names a page.
    let url = format!("index.html?popout={kind}&session={session}");
    let heading = title.unwrap_or_else(|| "mogeung".into());

    let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
        .title(heading)
        .inner_size(900.0, 600.0)
        .min_inner_size(420.0, 260.0)
        .resizable(true)
        // The main window draws its own chrome, and a popout that arrived with
        // the system's would not match it — nor would its own close button be
        // where every other one in this app is.
        .decorations(false)
        .build()
        .map_err(|e| format!("could not open a window for that pane: {e}"))?;

    // The other half of *moving* a pane. The main window closed its pane when
    // this one opened, so something has to tell it when to take the pane back —
    // and only the shell knows a window has gone. Emitted app-wide, like
    // `pty:data`: the window that cares filters, and this one is being
    // destroyed as it fires.
    //
    // `Destroyed` rather than `CloseRequested`: the second can be cancelled, and
    // a pane handed back while its window is still on screen would be the pane
    // in two places, which is the state this whole design exists to avoid.
    let handle = app.clone();
    let returned = Returned { kind, session };
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            let _ = handle.emit("popout:closed", returned.clone());
        }
    });

    Ok(label)
}

/// What the main window needs to put the pane back where it came from.
#[derive(Serialize, Clone)]
pub struct Returned {
    pub kind: String,
    pub session: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The id reaches a query string. Anything that would have to be escaped is
    /// refused instead of escaped, because refusing is checkable and escaping
    /// is a thing you get subtly wrong once.
    #[test]
    fn a_session_id_that_could_break_out_of_the_url_is_refused() {
        assert!(sane_session("0b3f9c2a-1d4e-4f77-9a2b-6c5d8e1f0a3b"));
        assert!(sane_session("abc_123"));

        assert!(!sane_session(""));
        assert!(!sane_session("a&popout=evil"));
        assert!(!sane_session("a?b"));
        assert!(!sane_session("../../etc/passwd"));
        assert!(!sane_session("a b"));
        assert!(!sane_session("a#b"));
        assert!(!sane_session(&"x".repeat(129)));
    }

    /// The label is what `capabilities/default.json` matches with `popout-*`.
    /// A label that stopped starting with it would produce a window with no
    /// commands, which fails as a black pane rather than as an error.
    #[test]
    fn the_label_is_matched_by_the_capability_glob() {
        let label = label_for("agent", "0b3f9c2a");
        assert!(label.starts_with("popout-"), "{label}");
        assert_eq!(label, "popout-agent-0b3f9c2a");
    }

    /// One window per session per kind, so the gesture is idempotent.
    #[test]
    fn one_label_per_pane_and_session() {
        assert_eq!(label_for("agent", "a"), label_for("agent", "a"));
        assert_ne!(label_for("agent", "a"), label_for("agent", "b"));
    }

    #[test]
    fn only_named_panes_may_be_popped_out() {
        assert!(POPPABLE.contains(&"agent"));
        assert!(!POPPABLE.contains(&"git"));
        assert!(!POPPABLE.contains(&""));
    }
}
