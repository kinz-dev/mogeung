/**
 * What to actually spawn for a terminal pane. `R-B18`, `R-B33`, `R-I6`.
 *
 * A port of the argv-building half of `crates/mogeung-ui/src/term.rs`, kept
 * faithful because every line of it was paid for once already.
 *
 * The rule underneath: **mogeung never spawns an agent.** The Agent pane runs
 * `tmux attach`, and tmux — which already owns the session — hands over a view
 * of it. `claude` started in iTerm2 is owned by iTerm2 and cannot be attached
 * to at all, which is what `scripts/yolomo` exists to fix. See
 * [ADR-0010](../../../docs/decisions/0010-attach-a-terminal-never-own-one.md).
 */

/** Which machine the tmux we drive is running on. `R-I6`. */
export type Reach = { kind: "local" } | { kind: "ssh"; dest: string };

export const LOCAL: Reach = { kind: "local" };

/**
 * What the pane tells the program inside it that it is.
 *
 * Must be set explicitly, and must be `xterm-256color`. A window started from a
 * desktop launcher has no `TERM` in its environment, and tmux then fails with
 * `open terminal failed: terminal does not support clear` — which names a
 * capability and so reads like a terminfo problem rather than an empty
 * variable.
 */
export const TERM = "xterm-256color";

/**
 * Where package managers put binaries, for when a login profile did not say.
 *
 * Homebrew on Apple silicon, Homebrew on Intel, MacPorts, and the two Nix
 * shapes. **Appended** to `PATH`, never prepended: this is a fallback for "the
 * profile did not mention it", not an opinion about which `tmux` to run.
 */
const FALLBACK_PATH =
  "/opt/homebrew/bin:/usr/local/bin:/opt/local/bin:/run/current-system/sw/bin:$HOME/.nix-profile/bin";

/** Single-quote for a POSIX shell, closing and reopening around any quote. */
export function shellQuote(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`;
}

/**
 * The tmux argv that attaches to an existing session.
 *
 * `=` is tmux's exact-match prefix. Without it, attaching to `mogeung-api`
 * would also match `mogeung-api-v2` and put you in front of the wrong agent —
 * which, in a tool whose whole job is telling sessions apart, is the worst
 * failure available.
 */
export function attachArgs(target: string): string[] {
  return [...mouseOn(target), "attach-session", "-t", `=${target}`];
}

/**
 * Hand the mouse wheel to tmux, for the session we are about to show. `R-J84`.
 *
 * Reported 2026-08-29: the wheel in a terminal pane **printed command
 * history** instead of scrolling, and scrolling did not work at all. One
 * cause. tmux draws on the alternate screen, and with `mouse off` it enables
 * no mouse reporting — so xterm.js falls back to its alternate-scroll mode and
 * converts the wheel into **Up/Down arrow keys**, which at a shell prompt is
 * the history. Nothing scrolls because tmux owns the scrollback and never
 * heard about the wheel.
 *
 * With `mouse on`, tmux enables reporting, xterm forwards the wheel as a real
 * mouse event, and tmux scrolls its own history — which is the scrollback the
 * pane exists to show.
 *
 * **Per session, never `-g`.** A global set would reach every tmux session on
 * the machine, including ones mogeung has nothing to do with. Verified that
 * the option lands on the named session and the global default stays as it
 * was.
 *
 * **The bare name, not `=name`.** `set-option -t` rejects the exact-match
 * prefix outright — *no such session: =mogtest* — and the bare form is exact
 * anyway: setting it on `mogtest` was verified to leave `mogtest-2` alone.
 *
 * The cost, stated because it is shared: a session is one thing, so a terminal
 * of your own attached to it also gets mouse reporting, and selecting text
 * there needs Shift. That is the same trade the pane already makes for any
 * program that turns reporting on.
 */
function mouseOn(target: string): string[] {
  return ["set-option", "-t", target, "mouse", "on", ";"];
}

/**
 * The tmux argv that opens a worktree's shell, creating it the first time.
 *
 * `-A` is the whole feature: attach when the session exists, create when it
 * does not. It is what makes the pane the *same* shell across restarts rather
 * than a fresh one each launch — so a build still running from an hour ago is
 * still there, and closing the window detaches instead of killing.
 *
 * `-s` takes the name literally, so no `=` here; `-c` roots a *new* session in
 * the worktree and is ignored for one that already exists, which is the
 * behaviour every terminal has.
 */
export function shellArgs(name: string, cwd: string): string[] {
  // The option goes **after** here, not before: `-A` may be creating the
  // session in this very invocation, and setting an option on a session that
  // does not exist yet fails. Verified that a command chained after an
  // attaching `new-session` still runs.
  return ["new-session", "-A", "-s", name, "-c", cwd, ";", "set-option", "-t", name, "mouse", "on"];
}

/**
 * Turn a tmux argv into the program and argv actually spawned.
 *
 * `-t` forces a pty on the remote side: without it ssh runs the command with no
 * terminal and tmux refuses to start, reporting that it is not a terminal —
 * which reads like a tmux fault rather than a missing flag.
 *
 * No batch mode, deliberately. A key passphrase or a host-key prompt appears
 * *in the pane* and can be answered there, because the pane is a real terminal.
 * Failing fast would turn every first connection into an error with no way to
 * act on it.
 */
export function spawnAs(reach: Reach, tmuxArgs: string[]): string[] {
  if (reach.kind === "local") return ["tmux", ...tmuxArgs];

  // Each argument is quoted because the remote shell parses this line: a
  // worktree path with a space in it is the ordinary case, not the exotic one.
  const command = ["tmux", ...tmuxArgs.map(shellQuote)].join(" ");

  // …and then the whole thing is wrapped, because finding `tmux` over ssh is
  // harder than it looks:
  //
  //     zsh:1: command not found: tmux
  //
  // — from an Apple-silicon Mac with tmux installed, twice. Two separate gaps
  // produce that same line:
  //
  //  1. `ssh host cmd` runs a **non-login** shell, so zsh sources only
  //     `.zshenv`; macOS never runs `path_helper`. `-l` fixes that.
  //  2. A login shell run with `-c` is still **non-interactive**, so `.zshrc`
  //     is not sourced either — and Homebrew's own installer puts
  //     `brew shellenv` in `.zshrc` at least as often as in `.zprofile`.
  //
  // Chasing (2) with `-i` invites job-control noise into a pane that has to
  // stay clean, so the fallback is explicit instead.
  const inner = `PATH="$PATH:${FALLBACK_PATH}"; exec ${command}`;
  const login = `exec \${SHELL:-/bin/sh} -l -c ${shellQuote(inner)}`;
  return ["ssh", "-t", reach.dest, login];
}

/**
 * Where a shell started through this reach can be found afterwards.
 *
 * Naming the host matters: `tmux attach -t …` is advice you act on, and run on
 * the wrong machine it finds nothing — or something else entirely.
 */
export function hostLabel(reach: Reach): string | null {
  return reach.kind === "ssh" ? reach.dest : null;
}

/**
 * Which machine to drive tmux on, given who the daemon says it is.
 *
 * Identity, never the address dialled — an `ssh -L` tunnel makes a remote
 * daemon answer on `127.0.0.1`, and guessing from the address would run tmux
 * here for a session whose files are a thousand miles away. `R-I5`.
 *
 * A remote daemon that has not been told an `ssh_target` gets `null`: a refusal
 * that prints a sentence, rather than a shell on the wrong machine.
 */
export function reachFor(
  daemon: { machine_id?: string | null; ssh_target?: string | null } | null,
  localMachineId: string | null,
): Reach | null {
  if (!daemon) return LOCAL;
  const same = !!daemon.machine_id && !!localMachineId && daemon.machine_id === localMachineId;
  if (same) return LOCAL;
  return daemon.ssh_target ? { kind: "ssh", dest: daemon.ssh_target } : null;
}

/** The tmux session name for the panel's n-th shell. Shared with the egui
 * client on purpose: they are two views of the same shells, not two sets. */
export function shellSessionName(index: number): string {
  return `mog-${index}`;
}
