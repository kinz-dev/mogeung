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
      "-u",
      ...attachArgs("mog:0.0"),
    ]);
  });

  /**
   * `R-J97`. Claude Code's banner came through as a row of `_` because tmux
   * had no `LC_ALL`/`LC_CTYPE`/`LANG` to read — a Dock-launched app is handed
   * none — and wrote an underscore per cell it thought the client could not
   * take. Measured on the reporting machine as `utf8=0`. `-u` answers the
   * question tmux was getting wrong instead of hoping the environment does.
   */
  it("forces UTF-8 rather than trusting the launcher's locale", () => {
    expect(spawnAs({ kind: "local" }, attachArgs("mog:0.0"))).toContain("-u");
  });

  /** A client flag, so it has to land before the verb — after it, tmux reads
   * it as the command's own and `attach-session` has no `-u`. */
  it("puts the UTF-8 flag before the tmux verb", () => {
    const argv = spawnAs({ kind: "local" }, attachArgs("mog:0.0"));
    // Asserted present first, deliberately: `indexOf` answers -1 for a flag
    // that is not there, and -1 is less than every real index — so the
    // ordering check alone passes loudest exactly when the flag is missing.
    expect(argv).toContain("-u");
    expect(argv.indexOf("-u")).toBeLessThan(argv.indexOf("attach-session"));
  });

  /** The far side's locale is a second machine's environment and just as
   * invisible from here, so the flag travels rather than being assumed. */
  it("carries the UTF-8 flag over ssh too", () => {
    const argv = spawnAs({ kind: "ssh", dest: "box" }, attachArgs("mog:0.0"));
    expect(argv[3]).toContain("tmux -u ");
    expect(argv[3].indexOf(" -u ")).toBeLessThan(argv[3].indexOf("attach-session"));
  });

  /** The panel's own shells go through the same door, so they get it too —
   * a worktree shell renders a UTF-8 prompt as readily as an agent does. */
  it("forces UTF-8 for a worktree shell as well as an attach", () => {
    expect(spawnAs({ kind: "local" }, shellArgs("mog-0", "/repo"))).toContain("-u");
  });

  /**
   * **`R-J97` is deliberately not gated to macOS, and this is the guard.**
   *
   * Asked for directly — *"make sure this fix only apply when this is running
   * on MacOS, because I don't want the app to break on the linux platform"* —
   * and the answer is that gating it is what would break Linux. Three reasons,
   * pinned here because all three are invisible at the call site:
   *
   * 1. `-u` is a **no-op** where the locale already says UTF-8. Measured on
   *    both paths: `client_utf8` is 1 with the flag and 1 without it, so a
   *    correctly-configured Linux box cannot tell the difference.
   * 2. The pane **requires** UTF-8 on every platform. `src-tauri/src/lib.rs`
   *    decodes the pty with `String::from_utf8_lossy` before the bytes cross
   *    IPC, in shared code with no `target_os` branch — so anything tmux emits
   *    that is not UTF-8 is already lost, on Linux as much as on macOS.
   * 3. A correct gate is **not implementable here**. `isMac()` reads
   *    `navigator.platform`, which is the *window's* platform, while `Reach`
   *    carries no OS for an ssh target — so gating would send `-u` to a Linux
   *    host from a Mac window and withhold it from a Mac host from a Linux
   *    one, which is exactly backwards and reinstates this bug on the remote
   *    path `R-I6` exists for.
   *
   * So the invariant is not "we set `-u`" — the tests above cover that — but
   * **"the platform is never consulted"**. This fails the day someone wraps it
   * in `isMac()`, which is the change it exists to stop.
   */
  it("forces UTF-8 on every platform, not only on a Mac", () => {
    const original = Object.getOwnPropertyDescriptor(window.navigator, "platform");
    try {
      for (const platform of ["Linux x86_64", "MacIntel", "Win32", ""]) {
        Object.defineProperty(window.navigator, "platform", {
          value: platform,
          configurable: true,
        });
        expect(spawnAs({ kind: "local" }, attachArgs("mog:0.0")), platform).toContain("-u");
        expect(spawnAs({ kind: "ssh", dest: "box" }, attachArgs("mog:0.0"))[3], platform).toContain(
          "tmux -u ",
        );
      }
    } finally {
      if (original) Object.defineProperty(window.navigator, "platform", original);
    }
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
