//! The embedded terminal pane: a real pty, running `tmux attach`.
//!
//! # Why this attaches rather than spawns
//!
//! A pty has exactly one master, held by whichever terminal created it. A
//! `claude` started in iTerm2 is owned by iTerm2 and **cannot** be attached to
//! by anything else — no ioctl, no IPC, no injection. That is not a gap in
//! mogeung; it is how ptys work.
//!
//! So this pane never spawns `claude`. It spawns `tmux attach`, and tmux — which
//! does own the pty, and is built for several clients at once — renders the same
//! live session here and in your terminal simultaneously.
//!
//! The consequence worth keeping in view: **the session is never trapped in
//! mogeung.** Close this window, or let this widget fail entirely, and the
//! session is untouched and still reachable from any terminal. That is what
//! makes hosting a session additive rather than a repeat of the v0.1 failure
//! ([ADR-0003]), and it is why the widget being immature is survivable.

use egui_term::{
    Binding, BindingAction, BackendSettings, InputKind, PtyEvent, TerminalBackend, TerminalMode,
    TerminalView,
};
use std::sync::mpsc::{channel, Receiver};

/// Shift+Enter must insert a newline rather than submit the turn.
///
/// A terminal has no way to express Shift+Enter — Return is one byte, and the
/// shift is lost before the pty ever sees it — so Claude Code's
/// `/terminal-setup` works around it by having the *terminal* send a different
/// byte. On this machine it wrote an iTerm2 mapping of Shift+Return to `\n`,
/// which is where this value comes from: read from
/// `com.googlecode.iterm2.plist` rather than guessed, because guessing between
/// `\n`, `\x1b\r` and `\x1b[13;2u` costs a round trip each and they all look
/// equally plausible.
///
/// Upstream binds Shift+Enter to `\r`, the same byte as plain Enter, so without
/// this the two keys are indistinguishable and a multi-line prompt is
/// impossible to type.
fn shift_enter_newline() -> Vec<(Binding<InputKind>, BindingAction)> {
    vec![(
        Binding {
            target: InputKind::KeyCode(egui::Key::Enter),
            modifiers: egui::Modifiers::SHIFT,
            terminal_mode_include: TerminalMode::empty(),
            terminal_mode_exclude: TerminalMode::empty(),
        },
        BindingAction::Char('\n'),
    )]
}

/// One attached view of a tmux session.
pub struct Term {
    target: String,
    backend: TerminalBackend,
    events: Receiver<(u64, PtyEvent)>,
    /// Set once tmux exits — detached, or the session ended.
    exited: bool,
}

/// A stable widget id per target, so switching between two sessions does not
/// hand the second one the first one's scroll and selection state.
fn id_for(target: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    target.hash(&mut h);
    h.finish()
}

impl Term {
    pub fn attach(ctx: &egui::Context, target: &str) -> anyhow::Result<Self> {
        let (tx, events) = channel();
        let backend = TerminalBackend::new(
            id_for(target),
            ctx.clone(),
            tx,
            BackendSettings {
                shell: "tmux".to_string(),
                // `=` is tmux's exact-match prefix. Without it, attaching to
                // `mogeung-api` would also match `mogeung-api-v2` and put you in
                // front of the wrong agent — which, in a tool whose whole job is
                // telling sessions apart, is the worst possible failure.
                args: vec![
                    "attach-session".to_string(),
                    "-t".to_string(),
                    format!("={target}"),
                ],
                working_directory: None,
            },
        )?;
        Ok(Self {
            target: target.to_string(),
            backend,
            events,
            exited: false,
        })
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn exited(&self) -> bool {
        self.exited
    }

    /// Drain pty events. Must run every frame, or an exit goes unnoticed and the
    /// pane keeps drawing a dead terminal.
    pub fn poll(&mut self) {
        while let Ok((_, event)) = self.events.try_recv() {
            if matches!(event, PtyEvent::Exit) {
                self.exited = true;
            }
        }
    }

    /// Draw. `focused` decides whether keystrokes go to the agent or to mogeung;
    /// see `App::handle_keys`, which yields entirely while this has focus.
    ///
    /// Returns the widget's own response. The caller **must** use this one to
    /// detect the click that takes focus: the response of an enclosing
    /// `allocate_ui` senses hover only, so testing `clicked()` on it silently
    /// never fires and the pane can never be typed into.
    pub fn ui(&mut self, ui: &mut egui::Ui, focused: bool) -> egui::Response {
        let view = TerminalView::new(ui, &mut self.backend)
            .set_focus(focused)
            .set_size(ui.available_size())
            .add_bindings(shift_enter_newline());
        let response = ui.add(view);

        // Without this, egui's focus navigation eats the four keys Claude Code
        // needs most: arrows move focus to a neighbouring widget, Tab and
        // Shift+Tab cycle it, and Escape drops it altogether. A permission
        // prompt would be unanswerable — the exact thing this pane exists for.
        //
        // egui only honours the filter for a widget that already held focus on
        // the previous frame, so this is a no-op on the frame of the click and
        // live from the next one.
        if focused {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two sessions must not share widget state. They did in the first version,
    /// because the backend id was hardcoded to 0 following the upstream example
    /// — so switching sessions carried the previous one's selection across.
    /// Upstream binds Shift+Enter to `\r` — the same byte plain Enter sends —
    /// so the override must differ from it or multi-line input is impossible
    /// and nothing about the app looks broken while it is.
    #[test]
    fn shift_enter_does_not_send_the_same_byte_as_enter() {
        let binding = shift_enter_newline();
        assert_eq!(binding.len(), 1);
        assert_eq!(binding[0].1, BindingAction::Char('\n'));
        assert_ne!(binding[0].1, BindingAction::Char('\r'));
        assert_eq!(binding[0].0.modifiers, egui::Modifiers::SHIFT);
    }

    #[test]
    fn each_target_gets_its_own_widget_id() {
        assert_ne!(id_for("mogeung-app:0.0"), id_for("mogeung-api:0.0"));
        assert_eq!(id_for("mogeung-app:0.0"), id_for("mogeung-app:0.0"));
    }
}
