//! The embedded terminal panes: a real pty, running tmux.
//!
//! Two panes are built out of this one widget, and the difference between them
//! is the whole point of the module.
//!
//! # Agent — attached, never spawned
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
//!
//! # Terminal — spawned, and still not trapped
//!
//! The shell pane does own its process. It runs under tmux anyway, and not for
//! the code reuse: people type `claude` into a shell that sits next to a diff,
//! and a directly-owned pty would kill that session when the window closes —
//! the property above, defeated through the back door. `new-session -A` keeps
//! it, transitively, for anything started inside.
//!
//! Where tmux is missing the pane falls back to a bare pty and says so. See
//! [ADR-0011](../../../docs/decisions/0011-own-a-shell-never-an-agent.md).

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

/// Which machine the tmux we drive is running on. `R-I6`.
///
/// The pane always holds a local pty — that is what a pty is. What changes is
/// what runs in it: `tmux …` here, or `ssh -t host tmux …` when the daemon
/// being watched is somewhere else. Before this existed the panel ran tmux
/// locally regardless, rooted at a path that only existed on the *other*
/// machine, so a remote session's shell tab opened a local shell in a directory
/// that was not there.
///
/// Nothing about [ADR-0011] changes: it is still tmux that owns the session,
/// still reachable from any terminal on that host, and still outliving this
/// window. It simply outlives it over there.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Reach {
    /// tmux runs on this machine.
    #[default]
    Local,
    /// tmux runs on another machine, reached as this ssh destination —
    /// `user@host`, or a name from `~/.ssh/config`.
    Ssh(String),
}

impl Reach {
    /// Turn a tmux argv into the program and argv actually spawned.
    ///
    /// `-t` forces a pty on the remote side: without it ssh runs the command
    /// without a terminal and tmux refuses to start, reporting that it is not
    /// a terminal — which reads like a tmux fault rather than a missing flag.
    ///
    /// No `BatchMode`, deliberately. A key passphrase or a host-key prompt
    /// appears *in the pane* and can be answered there, because the pane is a
    /// real terminal. Failing fast instead would turn every first connection
    /// into an error with no way to act on it.
    fn spawn_as(&self, tmux_args: Vec<String>) -> (String, Vec<String>) {
        match self {
            Reach::Local => ("tmux".to_string(), tmux_args),
            Reach::Ssh(dest) => {
                // Each tmux argument is quoted because the remote shell parses
                // this line: a worktree path with a space in it is the ordinary
                // case, not the exotic one.
                let mut command = String::from("tmux");
                for a in &tmux_args {
                    command.push(' ');
                    command.push_str(&shell_quote(a));
                }
                // …and then the whole thing is quoted again, because it is run
                // through a **login** shell. `ssh host cmd` gets a
                // non-interactive, non-login shell, which on zsh sources only
                // `.zshenv` — so macOS never runs `path_helper` and Homebrew's
                // `/opt/homebrew/bin` is missing from PATH. tmux is installed
                // and the shell cannot see it:
                //
                //     zsh:1: command not found: tmux
                //
                // Reported from a real Apple-silicon box on 2026-07-31. `-l`
                // sources the profile, which is where every package manager
                // puts its PATH.
                let login = format!(
                    "exec ${{SHELL:-/bin/sh}} -l -c {}",
                    shell_quote(&command)
                );
                (
                    "ssh".to_string(),
                    vec!["-t".to_string(), dest.clone(), login],
                )
            }
        }
    }

    /// Where a shell started through this reach can be found afterwards.
    pub fn host_label(&self) -> Option<&str> {
        match self {
            Reach::Local => None,
            Reach::Ssh(dest) => Some(dest),
        }
    }
}

/// Wrap in single quotes for a POSIX shell, closing and reopening around any
/// single quote of its own.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
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

/// What the pty on the other end of this widget actually is.
///
/// Not cosmetic. It decides whether the wheel may drive tmux copy-mode, and
/// whether the "nothing here is trapped" promise the Agent pane makes still
/// holds — [`Kind::Bare`] is the one case where it does not.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A tmux client attached to a session someone else started. Mogeung is a
    /// second viewer and owns nothing.
    Attached,
    /// A shell mogeung started, inside tmux. Owned, but reachable from any
    /// terminal and outliving this window — see [ADR-0011].
    Shell,
    /// A shell mogeung started on a pty of its own, because tmux is not
    /// installed. Anything running here dies with the window, which is why the
    /// pane says so out loud instead of degrading quietly.
    Bare,
}

/// One view of a pty: an attached tmux session, or a shell we started.
pub struct Term {
    target: String,
    kind: Kind,
    backend: TerminalBackend,
    events: Receiver<(u64, PtyEvent)>,
    /// Set once the child exits — detached, the session ended, or `exit`.
    exited: bool,
    /// Sub-line wheel remainder for the scrollback interception below.
    scroll_pixels: f32,
}

/// The shell to spawn when there is no tmux to spawn it inside.
///
/// `-l` because the window may have been started from a launcher, a `.app` or
/// the tray, none of which have read a login profile — so without it `cargo`
/// and friends are missing from `PATH` for reasons that look nothing like the
/// cause. tmux runs a login shell by default, so the fallback matching it
/// keeps the two modes behaving the same.
fn user_shell() -> (String, Vec<String>) {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "/bin/sh".to_string());
    (shell, vec!["-l".to_string()])
}

/// The tmux session name for a worktree's shell.
///
/// Keyed by path: a single global shell would be in the wrong directory every
/// time the selection moved, and keying by session id would strand a shell
/// every time a session ended.
///
/// Readable half plus a hash, because both matter. tmux forbids `.` and `:` in
/// a name and a bare hash would leave `tmux ls` full of things nobody can
/// identify — while two worktrees called `mogeung` in different checkouts must
/// not collide onto one shell.
///
/// `ordinal` distinguishes several shells in one worktree, and **0 produces the
/// name this function returned when there could only be one**. That is not
/// tidiness: the shells people already have open are named that way, and a
/// suffix on the first one would silently strand every one of them.
pub fn shell_session_name(root: &str, ordinal: u32) -> String {
    let base: String = std::path::Path::new(root)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '-' })
        .take(24)
        .collect();
    let hash = id_for(root) as u32;
    let base = base.trim_matches('-');
    let suffix = if ordinal == 0 {
        String::new()
    } else {
        format!("-{ordinal}")
    };
    if base.is_empty() {
        format!("mogeung-shell-{hash:08x}{suffix}")
    } else {
        format!("mogeung-shell-{base}-{hash:08x}{suffix}")
    }
}

/// Whether tmux is on `PATH` at all.
///
/// Asked once per pane open, never per frame: it forks a process, and the
/// answer cannot change between two frames in any way worth catching.
pub fn tmux_available() -> bool {
    std::process::Command::new("tmux")
        .arg("-V")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// A stable widget id per target, so switching between two sessions does not
/// hand the second one the first one's scroll and selection state.
fn id_for(target: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    target.hash(&mut h);
    h.finish()
}

/// The tmux argv that attaches to an existing session.
///
/// `=` is tmux's exact-match prefix. Without it, attaching to `mogeung-api`
/// would also match `mogeung-api-v2` and put you in front of the wrong agent —
/// which, in a tool whose whole job is telling sessions apart, is the worst
/// possible failure.
fn attach_args(target: &str) -> Vec<String> {
    vec![
        "attach-session".to_string(),
        "-t".to_string(),
        format!("={target}"),
    ]
}

/// The tmux argv that opens the worktree's shell, creating it if this is the
/// first time.
///
/// `-A` is the whole feature: attach when the session exists, create when it
/// does not. It is what makes the pane the *same* shell across restarts rather
/// than a fresh one each launch, so a build still running from an hour ago is
/// still there — and it means closing the window detaches instead of killing.
///
/// `-s` takes the name literally, so no `=` prefix here; and `-c` roots the
/// new session in the worktree, which is only read on creation. A shell that
/// already exists keeps whatever directory you left it in, which is the
/// behaviour every terminal has.
fn shell_args(name: &str, cwd: &str) -> Vec<String> {
    vec![
        "new-session".to_string(),
        "-A".to_string(),
        "-s".to_string(),
        name.to_string(),
        "-c".to_string(),
        cwd.to_string(),
    ]
}

impl Term {
    pub fn attach(ctx: &egui::Context, target: &str, reach: &Reach) -> anyhow::Result<Self> {
        let (program, args) = reach.spawn_as(attach_args(target));
        Self::spawn(ctx, target, Kind::Attached, program, args, None)
    }

    /// A shell rooted in `root`. `R-B31`, one per tab in the panel (`R-B33`).
    ///
    /// Under tmux when there is a tmux, on a bare pty when there is not — the
    /// difference is [`Kind`], and the pane reports it, because only one of
    /// the two survives this window closing ([ADR-0011]).
    pub fn shell(ctx: &egui::Context, root: &str, ordinal: u32, reach: &Reach) -> anyhow::Result<Self> {
        let name = shell_session_name(root, ordinal);
        if let Reach::Ssh(_) = reach {
            // `working_directory` stays `None`: `root` names a directory on the
            // *other* machine, and handing it to the local pty would fail the
            // spawn for a path that was never meant to be local. tmux's `-c`
            // does the rooting, over there, where the path exists.
            //
            // No bare-pty fallback either. Locally that fallback trades tmux's
            // survival for a shell; here it would trade the *right machine* for
            // a shell on the wrong one, silently — which is the bug this row
            // exists to fix, reintroduced as a degradation.
            let (program, args) = reach.spawn_as(shell_args(&name, root));
            return Self::spawn(ctx, &name, Kind::Shell, program, args, None);
        }
        if tmux_available() {
            Self::spawn(
                ctx,
                &name,
                Kind::Shell,
                "tmux".to_string(),
                shell_args(&name, root),
                Some(root.into()),
            )
        } else {
            let (shell, args) = user_shell();
            Self::spawn(ctx, &name, Kind::Bare, shell, args, Some(root.into()))
        }
    }

    fn spawn(
        ctx: &egui::Context,
        target: &str,
        kind: Kind,
        shell: String,
        args: Vec<String>,
        working_directory: Option<std::path::PathBuf>,
    ) -> anyhow::Result<Self> {
        let (tx, events) = channel();
        let backend = TerminalBackend::new(
            id_for(target),
            ctx.clone(),
            tx,
            BackendSettings {
                shell,
                args,
                working_directory,
                env: child_env(),
            },
        )?;
        Ok(Self {
            target: target.to_string(),
            kind,
            backend,
            events,
            exited: false,
            scroll_pixels: 0.0,
        })
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn kind(&self) -> Kind {
        self.kind
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
    /// `font` is the chosen terminal family at the pane's zoom — see
    /// [`crate::font`]. Passed in rather than built here because the family is
    /// a preference and the size is the pane's, and neither is this widget's
    /// business.
    pub fn ui(&mut self, ui: &mut egui::Ui, focused: bool, font: egui::FontId) -> egui::Response {
        self.wheel_scrolls_tmux_history(ui);
        let view = TerminalView::new(ui, &mut self.backend)
            .set_focus(focused)
            .set_size(ui.available_size())
            .set_font(egui_term::TerminalFont::new(egui_term::FontSettings {
                font_type: font,
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
        // A bare pty has no tmux to drive, and its scrollback belongs to the
        // emulator, which handles the wheel itself. Sending `send-keys -X`
        // here would target a session that does not exist.
        if self.exited || self.kind == Kind::Bare {
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

    /// `-A` is the persistence, and losing it would look like the pane simply
    /// working: you would get a shell, in the right directory, every time —
    /// just never the *same* shell, so the build you left running would be
    /// gone and nothing would say why.
    #[test]
    fn the_shell_attaches_to_its_session_rather_than_replacing_it() {
        let args = shell_args("mogeung-shell-repo-0badcafe", "/home/k/repo");
        assert_eq!(args[0], "new-session");
        assert!(args.contains(&"-A".to_string()), "{args:?}");
        assert!(args.contains(&"mogeung-shell-repo-0badcafe".to_string()));
        // `-c` roots a *new* session; an existing one keeps its own cwd.
        let c = args.iter().position(|a| a == "-c").expect("rooted");
        assert_eq!(args[c + 1], "/home/k/repo");
    }

    /// `-s` names a session literally, so the exact-match `=` that attach
    /// needs must **not** appear here — tmux would create a session actually
    /// called `=mogeung-shell-…` and the next launch would make another.
    #[test]
    fn the_shell_name_is_not_written_as_a_match_pattern() {
        let args = shell_args("mogeung-shell-x-1", "/tmp");
        assert!(
            !args.iter().any(|a| a.starts_with('=')),
            "= belongs to attach-session, not new-session: {args:?}"
        );
        assert!(attach_args("mog:0.0").contains(&"=mog:0.0".to_string()));
    }

    /// Two checkouts of the same repo are the case this has to get right —
    /// same basename, different worktree, and sharing one shell between them
    /// would put your commands in the wrong tree.
    #[test]
    fn each_worktree_gets_its_own_shell_even_with_the_same_name() {
        let a = shell_session_name("/home/k/work/mogeung", 0);
        let b = shell_session_name("/home/k/review/mogeung", 0);
        assert_ne!(a, b);
        assert!(a.contains("mogeung") && b.contains("mogeung"), "still readable in `tmux ls`");
        assert_eq!(a, shell_session_name("/home/k/work/mogeung", 0), "stable across launches");
    }

    /// The first shell in a worktree must keep the name it had when a worktree
    /// could only have one. Suffixing it would leave every shell anyone
    /// currently has open — and whatever is running in them — under a name
    /// this build never asks tmux for again.
    #[test]
    fn the_first_shell_in_a_worktree_is_named_as_it_always_was() {
        let name = shell_session_name("/home/k/repo", 0);
        let hash = name.strip_prefix("mogeung-shell-repo-").expect("{name}");
        assert_eq!(hash.len(), 8, "the name grew a part it did not have: {name}");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "{name}");
        assert_ne!(shell_session_name("/home/k/repo", 1), name);
    }

    /// Extra shells in one worktree address different tmux sessions, and the
    /// names stay legal — an ordinal is no use if tmux reads it as a window.
    #[test]
    fn extra_shells_in_one_worktree_are_distinct_and_legal() {
        let names: Vec<String> = (0..4).map(|n| shell_session_name("/home/k/repo", n)).collect();
        let mut uniq = names.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), names.len(), "two shells share a session: {names:?}");
        for n in &names {
            assert!(!n.contains('.') && !n.contains(':'), "{n}");
            assert!(n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "{n}");
        }
    }

    /// tmux gives `.` and `:` meaning inside a target, so a name carrying
    /// either addresses a window or a pane instead of the session.
    #[test]
    fn a_shell_name_is_legal_as_a_tmux_session_name() {
        for root in ["/home/k/my.repo", "/home/k/a:b", "/home/k/-weird-", "/", ""] {
            let name = shell_session_name(root, 0);
            assert!(name.starts_with("mogeung-shell-"), "{name}");
            assert!(!name.contains('.') && !name.contains(':'), "{name}");
            assert!(
                name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "{name}"
            );
        }
    }

    /// The fallback runs a *login* shell for the same reason `TERM` is set
    /// explicitly above: a window started from a launcher has read no profile,
    /// so without it `PATH` is missing everything the user installed.
    #[test]
    fn the_fallback_shell_is_a_login_shell() {
        let (shell, args) = user_shell();
        assert!(!shell.is_empty());
        assert_eq!(args, vec!["-l".to_string()]);
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


#[cfg(test)]
mod reach_tests {
    use super::*;

    /// Local is the identity case: the argv reaches tmux untouched, with no
    /// quoting applied, because nothing re-parses it.
    #[test]
    fn local_spawns_tmux_directly() {
        let (program, args) = Reach::Local.spawn_as(shell_args("mog-0", "/home/k/repo"));
        assert_eq!(program, "tmux");
        assert_eq!(args[0], "new-session");
        assert!(
            args.iter().all(|a| !a.starts_with('\'')),
            "local args must not be quoted: {args:?}"
        );
    }

    /// `R-I6`: the same tmux command, carried to the machine that has the files.
    #[test]
    fn ssh_wraps_the_same_tmux_command() {
        let (program, args) = Reach::Ssh("dev@box".into()).spawn_as(shell_args("mog-0", "/srv/w"));
        assert_eq!(program, "ssh");
        assert_eq!(args[0], "-t", "tmux needs a pty on the far side");
        assert_eq!(args[1], "dev@box");
        assert_eq!(args.len(), 3, "one command word after the destination");
        assert!(args[2].contains("new-session"), "the tmux verb must survive: {args:?}");
        assert!(args[2].contains("/srv/w"), "the remote root must survive: {args:?}");
    }

    /// `ssh host cmd` runs a non-login shell, which on macOS+zsh means
    /// `path_helper` never runs and Homebrew is absent from PATH — tmux is
    /// installed and cannot be found. Reported from a real box:
    /// `zsh:1: command not found: tmux`.
    #[test]
    fn the_remote_command_runs_through_a_login_shell() {
        let (_, args) = Reach::Ssh("box".into()).spawn_as(attach_args("mog:0.0"));
        let cmd = &args[2];
        assert!(cmd.contains(" -l -c "), "must be a login shell: {cmd}");
        assert!(
            cmd.starts_with("exec ${SHELL:-/bin/sh}"),
            "must use the remote user's own shell, with a fallback: {cmd}"
        );
    }

    /// ssh joins its trailing arguments with spaces and hands the result to a
    /// remote shell, so anything unquoted is word-split over there. A worktree
    /// path with a space is ordinary, and unquoted it would root the shell in
    /// the wrong directory — or, with `-c /My` failing, in the wrong place
    /// entirely.
    #[test]
    fn a_path_with_a_space_survives_the_remote_shell() {
        let (_, args) = Reach::Ssh("box".into()).spawn_as(shell_args("mog-0", "/My Code/repo"));
        assert!(
            args[2].contains("/My Code/repo"),
            "the path must survive quoting: {args:?}"
        );
    }

    /// The nastier half of quoting: a single quote inside the value must close,
    /// escape and reopen rather than ending the quoting early.
    #[test]
    fn a_single_quote_cannot_break_out_of_the_remote_command() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        let (_, args) = Reach::Ssh("box".into()).spawn_as(vec!["a'; rm -rf /; echo '".into()]);
        assert!(
            args[2].contains(r"'\''"),
            "an embedded quote must be escaped, not left to close the string: {args:?}"
        );
    }

    /// Attaching to a session by name has the same problem and the same fix:
    /// the exact-match `=` prefix must not be eaten by the remote shell.
    #[test]
    fn an_attach_target_keeps_its_exact_match_prefix_over_ssh() {
        let (_, args) = Reach::Ssh("box".into()).spawn_as(attach_args("mog:0.0"));
        assert!(
            args[2].contains("=mog:0.0"),
            "the = prefix decides which session: {args:?}"
        );
    }

    #[test]
    fn only_a_remote_reach_names_a_host() {
        assert_eq!(Reach::Local.host_label(), None);
        assert_eq!(Reach::Ssh("dev@box".into()).host_label(), Some("dev@box"));
    }
}

#[cfg(test)]
mod remote_shell_tests {
    use super::*;

    /// Hand the composed command to actual POSIX shells and count the words
    /// that come out.
    ///
    /// Every other quoting test here asserts what I believe a shell does with
    /// the string. This one asks. And it asks **twice**, because the command
    /// is parsed twice on the far side: once by the shell `ssh` starts, and
    /// again by the login shell that one execs. A quoting mistake at either
    /// level is silent — a mis-split path does not error, it roots the remote
    /// shell somewhere else and the tab looks like it worked.
    #[test]
    fn the_remote_shells_split_the_command_the_way_we_intended() {
        let root = "/My Code/re'po";
        let (_, args) = Reach::Ssh("box".into()).spawn_as(shell_args("mog-0", root));

        // Round one: the shell ssh starts parses the whole command line. Take
        // its argv, standing in for `exec $SHELL -l -c …`.
        let outer = words_of(&args[2]);
        assert_eq!(outer.first().map(String::as_str), Some("exec"));
        assert_eq!(outer.get(1).map(String::as_str), Some("/bin/sh"), "SHELL is unset here, so the fallback shows");
        assert_eq!(outer.get(2).map(String::as_str), Some("-l"));
        assert_eq!(outer.get(3).map(String::as_str), Some("-c"));
        assert_eq!(outer.len(), 5, "the command must arrive as ONE word: {outer:?}");

        // Round two: the login shell parses that single word.
        let inner = words_of(&outer[4]);
        assert_eq!(inner[0], "tmux");
        assert_eq!(inner[1], "new-session");
        assert_eq!(
            inner.last().map(String::as_str),
            Some(root),
            "the root must survive both rounds, quote and space intact: {inner:?}"
        );
        assert_eq!(inner.len(), 7, "tmux + six tmux arguments: {inner:?}");
    }

    /// What a POSIX shell makes of a command line, as a word list.
    fn words_of(command: &str) -> Vec<String> {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf '%s\\n' {command}"))
            .env_remove("SHELL")
            .output()
            .expect("sh must exist");
        assert!(out.status.success(), "shell rejected: {command}");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    }
}
