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
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver};

/// What the embedded pane tells the program inside it that it is.
///
/// This must be set explicitly. alacritty_terminal sets no `TERM` at all, so
/// the child inherits the *window's* environment — and a window started from a
/// desktop launcher, a `.app`, or the daemon has no `TERM` in it. tmux then
/// fails with `open terminal failed: terminal does not support clear`, which
/// names a capability and so reads like a font or terminfo problem rather than
/// an empty variable. Because the pane re-attaches whenever tmux exits, the
/// failure arrives once per frame and the tab appears to flicker.
///
/// Not `alacritty`, which is what the emulator actually is: that terminfo entry
/// ships with alacritty rather than with ncurses, so on a machine without
/// alacritty installed it is missing and tmux refuses the same way. Every
/// machine that has tmux has `xterm-256color`, and the widget implements it.
const TERM: &str = "xterm-256color";

/// The environment the attached tmux client runs with.
fn child_env() -> HashMap<String, String> {
    HashMap::from([("TERM".to_string(), TERM.to_string())])
}

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
    /// Sub-line wheel remainder for the scrollback interception below.
    scroll_pixels: f32,
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
                env: child_env(),
            },
        )?;
        Ok(Self {
            target: target.to_string(),
            backend,
            events,
            exited: false,
            scroll_pixels: 0.0,
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
    pub fn ui(&mut self, ui: &mut egui::Ui, focused: bool, font_px: f32) -> egui::Response {
        self.wheel_scrolls_tmux_history(ui);
        let view = TerminalView::new(ui, &mut self.backend)
            .set_focus(focused)
            .set_size(ui.available_size())
            .set_font(egui_term::TerminalFont::new(egui_term::FontSettings {
                font_type: egui::FontId::monospace(font_px),
            }))
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

    /// Make the wheel scroll tmux's scrollback instead of the agent's history.
    ///
    /// A fullscreen app with tmux's `mouse` option **off** leaves the wheel in
    /// "alternate scroll": the emulator converts it to arrow keys, which Claude
    /// Code reads as prompt-history navigation — the one thing a wheel over a
    /// transcript should never do. Not platform behaviour, tmux configuration;
    /// a Mac with `set -g mouse on` in `~/.tmux.conf` never sees it, the same
    /// machine without that line does.
    ///
    /// We know this pane is tmux, so when the pane has *not* asked for mouse
    /// reporting (`mouse on` handles itself — the widget speaks the mouse
    /// protocol), wheel events over it are consumed and turned into tmux
    /// copy-mode scrolling. Still observer-safe: `send-keys -X` drives tmux's
    /// *view* of the pane and writes nothing to the process inside — the agent
    /// sees no input at all, which is the entire point.
    fn wheel_scrolls_tmux_history(&mut self, ui: &egui::Ui) {
        if self.exited {
            return;
        }
        let mode = self.backend.last_content().terminal_mode;
        if mode.intersects(TerminalMode::MOUSE_MODE) || !mode.contains(TerminalMode::ALT_SCREEN) {
            self.scroll_pixels = 0.0;
            return;
        }

        let rect = ui.available_rect_before_wrap();
        let row = ui.text_style_height(&egui::TextStyle::Monospace).max(1.0);
        let mut lines = 0i32;
        ui.input_mut(|i| {
            if !i.pointer.hover_pos().is_some_and(|p| rect.contains(p)) {
                return;
            }
            i.events.retain(|e| match e {
                // Ctrl+wheel is the pane-zoom gesture, not scrollback —
                // leave it alone or zooming over the terminal scrolls tmux.
                egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } if !modifiers.ctrl && !modifiers.command => {
                    lines += wheel_lines(*unit, delta.y, row, &mut self.scroll_pixels);
                    false // consumed — or it becomes arrow keys downstream
                }
                _ => true,
            });
        });
        if lines != 0 {
            run_tmux(scroll_args(&self.target, lines));
        }
    }
}

/// Wheel movement → whole lines, carrying the sub-line remainder in `accum`.
/// Positive is towards history, matching the terminal's own convention.
fn wheel_lines(unit: egui::MouseWheelUnit, dy: f32, row: f32, accum: &mut f32) -> i32 {
    match unit {
        egui::MouseWheelUnit::Line => (dy.signum() * dy.abs().ceil()) as i32,
        egui::MouseWheelUnit::Point => {
            *accum += dy;
            let l = (*accum / row).trunc();
            *accum %= row;
            l as i32
        }
        egui::MouseWheelUnit::Page => 0,
    }
}

/// The tmux command for one batch of wheel movement.
///
/// Up enters copy-mode first — a no-op when already there — with `-e` so
/// scrolling back to the bottom leaves it again. Down deliberately does *not*
/// enter copy-mode: at the live view there is nothing below to scroll to, and
/// `send-keys -X` outside copy-mode is a harmless error.
fn scroll_args(target: &str, lines: i32) -> Vec<String> {
    let t = format!("={target}");
    let n = lines.unsigned_abs().to_string();
    let mut args: Vec<String> = Vec::new();
    if lines > 0 {
        for a in ["copy-mode", "-e", "-t", &t, ";"] {
            args.push(a.to_string());
        }
    }
    let dir = if lines > 0 { "scroll-up" } else { "scroll-down" };
    for a in ["send-keys", "-X", "-N", &n, "-t", &t, dir] {
        args.push(a.to_string());
    }
    args
}

/// Fire and forget, reaped off-frame: `.spawn()` alone would leak zombies and
/// `.status()` inline would block the frame on a subprocess.
fn run_tmux(args: Vec<String>) {
    std::thread::spawn(move || {
        let _ = std::process::Command::new("tmux")
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
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

    /// The child must be told what terminal it is talking to. Nothing else
    /// sets `TERM` — not alacritty_terminal, not the window process when it was
    /// started from a launcher or the tray — and tmux handed an empty one dies
    /// with `open terminal failed: terminal does not support clear`.
    #[test]
    fn the_child_is_given_a_term() {
        let env = child_env();
        let term = env.get("TERM").expect("TERM must be set explicitly");
        assert!(!term.is_empty(), "an empty TERM is what tmux refuses");
        assert_ne!(term, "dumb", "dumb has no clear capability");
    }

    /// And it must name an entry this machine actually has. `alacritty` — what
    /// the emulator really is — ships with alacritty rather than with ncurses,
    /// so it is absent wherever alacritty is not installed and tmux refuses it
    /// exactly as it refuses an empty one. Skipped where `infocmp` is missing:
    /// the point is the terminfo database, and without one there is nothing to
    /// check.
    #[test]
    fn the_term_we_claim_has_a_terminfo_entry_with_clear() {
        let Ok(out) = std::process::Command::new("infocmp").arg(TERM).output() else {
            return;
        };
        if !out.status.success() {
            panic!("no terminfo entry for {TERM}");
        }
        let caps = String::from_utf8_lossy(&out.stdout);
        assert!(
            caps.contains("clear="),
            "{TERM} has no clear capability — tmux will refuse it"
        );
    }

    #[test]
    fn each_target_gets_its_own_widget_id() {
        assert_ne!(id_for("mogeung-app:0.0"), id_for("mogeung-api:0.0"));
        assert_eq!(id_for("mogeung-app:0.0"), id_for("mogeung-app:0.0"));
    }

    /// Scrolling up must enter copy-mode (with `-e`, so the bottom exits it);
    /// scrolling down must not — at the live view, entering copy-mode on a
    /// down-tick would flash the mode in and out for nothing.
    #[test]
    fn only_an_upward_scroll_enters_copy_mode() {
        let up = scroll_args("mog:0.0", 3);
        assert_eq!(up[..5], ["copy-mode", "-e", "-t", "=mog:0.0", ";"]);
        assert!(up.contains(&"scroll-up".to_string()));
        assert!(up.contains(&"3".to_string()), "repeat count carried: {up:?}");

        let down = scroll_args("mog:0.0", -2);
        assert!(!down.contains(&"copy-mode".to_string()));
        assert!(down.contains(&"scroll-down".to_string()));
        assert!(down.contains(&"2".to_string()));
    }

    /// The exact-match `=` prefix matters here as much as in attach: scrolling
    /// the wrong session's pane would be silent and bizarre.
    #[test]
    fn scroll_targets_use_the_exact_match_prefix() {
        for args in [scroll_args("mog", 1), scroll_args("mog", -1)] {
            assert!(args.contains(&"=mog".to_string()), "{args:?}");
        }
    }

    /// Pixel-unit wheels accumulate into whole lines, keeping the remainder —
    /// three 6px ticks over a 14px row must scroll one line, not zero.
    #[test]
    fn pixel_scrolls_accumulate_across_events() {
        let mut accum = 0.0;
        let row = 14.0;
        let mut total = 0;
        for _ in 0..3 {
            total += wheel_lines(egui::MouseWheelUnit::Point, 6.0, row, &mut accum);
        }
        assert_eq!(total, 1, "18px over a 14px row is one whole line");
        assert!((accum - 4.0).abs() < 0.001, "remainder kept: {accum}");
        // And the sign convention: up is positive, towards history.
        assert!(wheel_lines(egui::MouseWheelUnit::Line, 2.0, row, &mut accum) > 0);
        assert!(wheel_lines(egui::MouseWheelUnit::Line, -2.0, row, &mut accum) < 0);
    }
}

