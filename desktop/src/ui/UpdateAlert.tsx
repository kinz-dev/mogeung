/**
 * The window's own update: the alert that offers it, the progress while it
 * builds and installs, and the question at the end — restart now, or later.
 * `R-J105`, [feature 0049](../../../docs/features/0049-update-in-place.md).
 *
 * Asked 2026-09-24: *"popup a screen to confirm if user want to update or
 * not … And once it is done popup a screen to ask user to restart the
 * application, it is either "OK" to restart it automatically, or "Later"."*
 *
 * Two parts of the ask are done differently, and the dialog says so where it
 * matters: an update is offered **when a commit lands** rather than on a timer,
 * and the password is typed into **the operating system's own dialog** —
 * mogeung shows text it did not write, so it is the last place a password
 * should pass through. See ADR-0041.
 */

import { useEffect, useRef } from "react";
import { ArrowUpCircle, RotateCw } from "lucide-react";
import { cn } from "@/lib/cn";
import { unsavedWork } from "@/lib/unsaved";
import { short } from "@/lib/update";
import { useStore } from "@/store";
import { standing, useUpdate } from "@/store/update";
import { Dialog } from "@/ui/Dialog";
import { Button, Dim } from "@/ui/primitives";
import { interactive } from "@/ui/styles";

/** Mounted once, beside the other overlays: starts watching, and draws the dialog when there is one. */
export function UpdateAlert() {
  const start = useUpdate((s) => s.start);
  const open = useUpdate((s) => s.open);
  const phase = useUpdate((s) => s.phase);
  useEffect(() => {
    void start();
  }, [start]);
  if (!open || phase === "idle") return null;
  return <UpdateDialog />;
}

const MORE = 12;

function UpdateDialog() {
  const s = useUpdate();
  const daemon = useStore((st) => st.daemonStatus);
  const logRef = useRef<HTMLPreElement>(null);
  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [s.log]);

  const branch = s.info?.branch ?? "main";
  const running = s.info?.commit ?? "";
  const linux = s.info?.platform !== "macos";
  const close = s.phase === "available" ? s.notNow : s.later;

  const log = (
    <pre
      ref={logRef}
      aria-label="update log"
      className="mt-2 max-h-56 overflow-auto rounded-sm border border-[var(--border)] bg-[var(--bg-faint)] p-2 font-mono text-2xs leading-snug text-[var(--dim)]"
    >
      {s.log.length ? s.log.join("\n") : "starting…"}
    </pre>
  );

  let title = "";
  let body: React.ReactNode = null;
  let actions: React.ReactNode = null;

  switch (s.phase) {
    case "available": {
      const commits = s.check?.commits ?? [];
      title = "Update available";
      body = (
        <>
          <p>
            You are running <code>{short(running)}</code>; {branch} is at <code>{short(s.target)}</code>
            {s.check?.behind ? `, ${s.check.behind} commit${s.check.behind === 1 ? "" : "s"} on` : ""}.
          </p>
          {commits.length > 0 && (
            <ul aria-label="what changed" className="mt-2 flex flex-col gap-0.5">
              {commits.slice(0, MORE).map((c) => (
                <li key={c.sha} className="truncate">
                  <code className="text-[var(--dim)]">{short(c.sha)}</code> {c.subject}
                </li>
              ))}
              {commits.length > MORE && <Dim>and {commits.length - MORE} more</Dim>}
            </ul>
          )}
          <p className="mt-3 text-[var(--dim)]">
            It builds in a worktree of its own, so this window and every agent keep running meanwhile — a few minutes the
            first time, less after.{" "}
            {linux
              ? "Installing asks for your password once, in the system's own dialog, for dpkg — mogeung never sees it."
              : "Installing copies the app into /Applications; no password is needed."}
          </p>
        </>
      );
      actions = (
        <>
          <Button variant="ghost" className="mr-auto" onClick={() => s.setEnabled(false)} title="this window stops looking; turn it back on from the status bar">
            stop checking
          </Button>
          <Button variant="outline" onClick={s.notNow}>
            Not now
          </Button>
          <Button variant="solid" onClick={() => void s.update()}>
            Update
          </Button>
        </>
      );
      break;
    }
    case "building":
    case "installing":
      title = s.phase === "building" ? `Building ${short(s.target)}` : "Installing";
      body = (
        <>
          <p>
            {s.phase === "building"
              ? "Building in ~/.mogeung/update/tree. You can close this — it keeps going, and the status bar says when it is done."
              : linux
                ? "Enter your password in the system's dialog when it asks."
                : "Copying the app into /Applications."}
          </p>
          {log}
        </>
      );
      actions = (
        <>
          <Button variant="outline" onClick={s.later}>
            Hide
          </Button>
          <Button variant="outline" onClick={() => void s.cancel()}>
            Cancel
          </Button>
        </>
      );
      break;
    case "failed":
      title = "The update did not finish";
      body = (
        <>
          <p className="text-[var(--red)]">{s.message}</p>
          {s.log.length > 0 && log}
        </>
      );
      actions = (
        <>
          <Button variant="outline" onClick={() => useUpdate.setState({ phase: "idle", open: false })}>
            Close
          </Button>
          <Button variant="solid" onClick={() => void s.update()}>
            Try again
          </Button>
        </>
      );
      break;
    case "installed": {
      const unsaved = unsavedWork();
      title = "Update installed";
      body = (
        <>
          <p>
            <code>{short(s.target)}</code> is installed. Restart mogeung to use it.
          </p>
          <ul className="mt-2 flex list-disc flex-col gap-1 pl-4 text-[var(--dim)]">
            <li>Agents keep running — they live in tmux, and their panes reattach when the window is back.</li>
            {daemon?.mode === "attached" && (
              <li className="text-[var(--amber)]">
                The daemon on this machine{daemon.pid ? ` (pid ${daemon.pid})` : ""} was started separately, and keeps running
                the build it was started with. Restart it yourself when it suits you.
              </li>
            )}
            {unsaved.map((u) => (
              <li key={u} className="text-[var(--amber)]">
                {u} has unsaved changes, which a restart would lose.
              </li>
            ))}
          </ul>
          {s.message && <p className="mt-2 text-[var(--red)]">{s.message}</p>}
        </>
      );
      actions = (
        <>
          <Button variant="outline" onClick={s.later}>
            Later
          </Button>
          <Button variant="solid" onClick={() => void s.restart()}>
            Restart now
          </Button>
        </>
      );
      break;
    }
  }

  return (
    <Dialog title={title} onClose={close}>
      <div className="flex flex-col text-xs" role="region" aria-label="the update">
        {body}
        <div className="mt-4 flex items-center justify-end gap-2">{actions}</div>
      </div>
    </Dialog>
  );
}

/**
 * The status bar's word on it: the commit this window runs, how far behind
 * the branch that is, and — once one is installed — *restart to update*,
 * until you do.
 */
export function UpdateMark() {
  const info = useUpdate((s) => s.info);
  const check = useUpdate((s) => s.check);
  const phase = useUpdate((s) => s.phase);
  const enabled = useUpdate((s) => s.enabled);
  const show = useUpdate((s) => s.show);
  const setEnabled = useUpdate((s) => s.setEnabled);
  if (!info) return null;

  const where = standing({ info, check });
  const behind = !!check?.head && check.head !== info.commit;
  const tip = [
    `this window was built from ${info.commit || "an unknown commit"}`,
    info.installed ? where : "a build run from a checkout — updates are for installed builds",
    check?.error,
    info.installed && !enabled ? "not checking for updates — click to start again" : null,
    info.installed && !info.check ? "update_check = false in ~/.mogeung/config.toml" : null,
  ]
    .filter(Boolean)
    .join("\n");

  if (phase === "installed") {
    return (
      <button type="button" onClick={show} title={tip} className={cn("flex items-center gap-1 rounded-sm text-[var(--amber)] hover:underline", interactive)}>
        <RotateCw className="h-3 w-3" /> restart to update
      </button>
    );
  }
  if (phase === "building" || phase === "installing") {
    return (
      <button type="button" onClick={show} title={tip} className={cn("rounded-sm text-[var(--blue)] hover:underline", interactive)}>
        updating…
      </button>
    );
  }
  return (
    <button
      type="button"
      aria-label="this build"
      title={tip}
      onClick={() => (info.installed && !enabled ? setEnabled(true) : behind ? show() : undefined)}
      className={cn(
        "flex items-center gap-1 rounded-sm font-mono",
        interactive,
        behind && info.installed ? "text-[var(--blue)] hover:underline" : "text-[var(--dim)]",
      )}
    >
      {behind && info.installed && <ArrowUpCircle className="h-3 w-3" />}
      {info.installed ? "" : "dev "}
      {short(info.commit)}
      {behind && info.installed && check?.behind ? ` · ${check.behind} behind` : ""}
    </button>
  );
}
