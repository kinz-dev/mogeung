import { describe, expect, it } from "vitest";
import { attachArgs, reachFor, shellArgs, shellQuote, spawnAs } from "./tmux";

describe("attachArgs", () => {
  /**
   * The exact-match prefix. Without it, attaching to `mogeung-api` also matches
   * `mogeung-api-v2` and puts you in front of the wrong agent — the worst
   * failure available in a tool whose job is telling sessions apart.
   */
  it("keeps tmux's exact-match prefix", () => {
    expect(attachArgs("mog:0.0")).toEqual([
      "set-option",
      "-t",
      "mog:0.0",
      "mouse",
      "on",
      ";",
      "attach-session",
      "-t",
      "=mog:0.0",
    ]);
  });

  /**
   * The wheel belongs to tmux. `R-J84`.
   *
   * With `mouse off`, tmux enables no mouse reporting, so xterm.js falls back
   * to alternate-scroll and turns the wheel into **Up/Down arrows** — command
   * history at a prompt, and nothing scrolling because tmux owns the
   * scrollback and never heard about it.
   */
  it("hands the wheel to tmux before attaching", () => {
    const args = attachArgs("mog-0");
    expect(args.slice(0, 6)).toEqual(["set-option", "-t", "mog-0", "mouse", "on", ";"]);
    // Before the attach, because the attach takes over the client.
    expect(args.indexOf("attach-session")).toBeGreaterThan(args.indexOf("mouse"));
  });

  /**
   * Per session, never `-g`. A global set would reach every tmux session on
   * the machine, including ones mogeung has nothing to do with.
   */
  it("scopes the option to one session", () => {
    expect(attachArgs("mog-0")).not.toContain("-g");
    // And the bare name, because `set-option -t` rejects the `=` prefix
    // outright — it is exact anyway.
    expect(attachArgs("mog-0")[2]).toBe("mog-0");
  });
});

describe("shellArgs", () => {
  /** `-A` is the persistence: attach if it exists, create if it does not.
   * Losing it would look like the pane simply forgetting your shell. */
  it("attaches to its session rather than replacing it", () => {
    const args = shellArgs("mog-0", "/repo");
    expect(args[0]).toBe("new-session");
    expect(args).toContain("-A");
  });

  /** `-s` names a session literally, so the `=` that attach needs would become
   * part of the name here. */
  it("does not borrow attach's exact-match prefix", () => {
    expect(shellArgs("mog-0", "/repo").some((a) => a.startsWith("="))).toBe(false);
  });

  /**
   * The option goes **after** the session here, unlike `attachArgs`. `-A` may
   * be creating it in this very invocation, and setting an option on a session
   * that does not exist yet fails — which would take the whole command with it.
   */
  it("sets mouse mode after the session it may be creating", () => {
    const args = shellArgs("mog-0", "/repo");
    expect(args.indexOf("mouse")).toBeGreaterThan(args.indexOf("new-session"));
    expect(args.slice(-6)).toEqual([";", "set-option", "-t", "mog-0", "mouse", "on"]);
  });
});

describe("spawnAs", () => {
  it("runs tmux directly when the daemon is on this machine", () => {
    expect(spawnAs({ kind: "local" }, attachArgs("mog:0.0"))).toEqual([
      "tmux",
      ...attachArgs("mog:0.0"),
    ]);
  });

  /** `-t` forces a pty on the far side. Without it tmux refuses to start and
   * blames the terminal, which reads like a tmux fault rather than a flag. */
  it("forces a remote pty over ssh", () => {
    const argv = spawnAs({ kind: "ssh", dest: "box" }, attachArgs("mog:0.0"));
    expect(argv[0]).toBe("ssh");
    expect(argv[1]).toBe("-t");
    expect(argv[2]).toBe("box");
  });

  /** The tmux verb, and the exact-match target, have to survive the trip
   * through two layers of shell quoting. */
  it("carries the tmux command through the ssh wrapper", () => {
    const argv = spawnAs({ kind: "ssh", dest: "box" }, attachArgs("mog:0.0"));
    expect(argv[3]).toContain("attach-session");
    expect(argv[3]).toContain("=mog:0.0");
    // The command separator is quoted so the *remote shell* leaves it alone
    // and hands it to tmux, which is what reads it. Verified against a real
    // shell: `sh -c "tmux 'set-option' … ';' 'has-session' …"` sets the option.
    expect(argv[3]).toContain("';'");
  });

  /**
   * A login shell, then an explicit PATH fallback. Both are needed, and the
   * bug that proved it printed `command not found: tmux` on a Mac with tmux
   * installed: `ssh host cmd` is non-login, and a login shell run with `-c` is
   * still non-interactive.
   */
  it("sources the profile and still appends a PATH fallback", () => {
    const argv = spawnAs({ kind: "ssh", dest: "box" }, attachArgs("x"));
    expect(argv[3]).toContain("-l -c");
    expect(argv[3]).toContain("/opt/homebrew/bin");
    // Appended, not prepended: a tmux the user chose stays chosen.
    expect(argv[3]).toContain('PATH="$PATH:');
  });

  /** A worktree path with a space in it is the ordinary case, not the exotic
   * one, and the remote shell parses this line. */
  it("quotes an argument containing a space", () => {
    const argv = spawnAs({ kind: "ssh", dest: "box" }, shellArgs("mog-0", "/my repo"));
    expect(argv[3]).toContain(`'/my repo'`);
  });

  /** Close the quote, emit an escaped one, reopen — the only way a POSIX
   * shell takes a literal quote inside single quotes. */
  it("survives a single quote in a path", () => {
    expect(shellQuote("it's")).toBe("'it'\\''s'");
  });
});

describe("reachFor", () => {
  /**
   * The bug `R-I5` exists to kill: `ssh -L 7717:localhost:7717` makes a remote
   * daemon answer on `127.0.0.1`, and an address-based guess reads that as
   * local — running tmux here for files that are somewhere else entirely.
   */
  it("does not read a tunnelled daemon as local", () => {
    const reach = reachFor({ machine_id: "devbox", ssh_target: "devbox" }, "laptop");
    expect(reach).toEqual({ kind: "ssh", dest: "devbox" });
  });

  it("is local when the identities match, whatever the route", () => {
    expect(reachFor({ machine_id: "same" }, "same")).toEqual({ kind: "local" });
  });

  /**
   * Unknown on either side means **not this machine**. Refusing prints a
   * sentence; guessing wrong opens a shell in the wrong place. The costs are
   * not symmetric, so the doubt resolves the safe way.
   */
  it("refuses rather than guessing when a remote daemon has no ssh target", () => {
    expect(reachFor({ machine_id: "devbox" }, "laptop")).toBeNull();
    expect(reachFor({ machine_id: null }, "laptop")).toBeNull();
    expect(reachFor({ machine_id: "devbox" }, null)).toBeNull();
  });
});
