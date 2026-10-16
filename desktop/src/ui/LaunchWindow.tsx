/**
 * Start a session — in your terminal, not in mogeung. `R-B2`.
 *
 * This is the one place mogeung causes an agent to exist, and the shape is what
 * keeps it inside [ADR-0003](../../../docs/decisions/0003-observe-do-not-spawn.md):
 * the daemon opens **a real terminal window** in a directory and nothing more.
 * The conversation is not wrapped, not proxied and not readable except the way
 * every other session is — through the files Claude Code writes. Closing mogeung
 * leaves it running, exactly like a session you started yourself.
 *
 * The `--dangerously-skip-permissions` warning is stated up front rather than
 * left to be discovered. The flag is the agent's own and the daemon only passes
 * it, but it changes what pressing this costs, and a permission prompt that
 * never comes is not something to meet by surprise.
 *
 * **Which CLI, since `R-J51`** — asked 2026-08-25, once the window watched
 * three of them. It was a Claude launcher because there was one CLI; a window
 * that lists Qwen sessions in its queue and can only *start* Claude ones is
 * telling you to go back to a terminal for half of what it does. The daemon
 * owns the recipe per source ([ADR-0029](../../../docs/decisions/0029-an-agent-cli-is-a-variant-not-a-plugin.md)),
 * so this window's whole job is the choice and saying what it costs — the
 * approve-everything flag differs per CLI and the warning has to follow it, or
 * it becomes a sentence about a flag that is not being passed.
 *
 * **All three CLIs since `R-J72`.** Codex used to be absent rather than
 * disabled — there was no recipe for starting it, and a control you can see but
 * not press invites the question *why not*, every time. `codexmo` wrote the
 * recipe down, so the daemon has an answer and the choice belongs here.
 *
 * Codex's yolo line is the one that is **not** its CLI's most dangerous flag.
 * `--dangerously-bypass-approvals-and-sandbox` is the exact analogue of the
 * other two, and it does one thing more than either: it turns off a
 * **sandbox**, which neither `claude` nor `qwen` has to give up. Taking that
 * step because a click said yolo is not a choice this window makes for you.
 * What it starts is your usual Codex session minus the prompts.
 *
 * **Four since `R-I25`.** opencode's line is its own `--auto` — *"auto-approve
 * permissions that are not explicitly denied"* — so a `deny` in your opencode
 * config still denies.
 *
 * **Two lists since `R-J45`**, asked for 2026-08-24: *"I seldom need to open a
 * new claude session from a terminal"*. That is the sentence that changes this
 * window's job. It was built as an occasional shortcut, so a log of recent
 * repositories was a fair answer; used as the front door every morning it is
 * the wrong one, because the log grows with everything you have touched and
 * reorders itself while the two projects you actually work in sit somewhere
 * inside it. Favourites go **above** the recents and the recents keep their
 * place under them — nothing is taken away, and a folder you have not kept is
 * still one click from being started and two from being kept.
 *
 * **A folder picker beside the box, asked for 2026-08-28.** Typing an absolute
 * path was the slowest part of this window and always had been — the favourites
 * and recents lists exist because of it, and they only help for somewhere you
 * have already been. `chooseFolder` had been sitting in `lib/tauri` since
 * `R-J40` doing exactly this for the Files tool; this window simply never
 * asked it. It opens at whatever is half-typed in the box, so correcting a
 * path is not the same length as entering one, and it degrades to a prompt in
 * a browser tab where there is no shell to ask.
 *
 * **The daemon's picker, not the OS's, since `R-J142`**, asked 2026-10-16:
 * *"if I selected the remote machine that I follow. And when I click the
 * "open" directory button. It always show my own local machine folder."* An
 * OS picker can only show the disk the window runs on, so it was wrong for a
 * followed machine and for a watched daemon on another box alike. The button
 * now opens `FolderBrowser` under the box, answered by the chosen machine's
 * daemon — still opening at whatever is half-typed.
 *
 * **Headless since `R-J61`**, asked 2026-08-26: starting from here always
 * opened a terminal window too, and for a session you mean to drive from a
 * mogeung pane that window is a thing to go and close. Headless is `yolomo
 * -d` behind a checkbox — a detached tmux session, picked up by the next scan
 * — and the choice is remembered beside the CLI, for the same reason. The
 * subtitle and the button follow it the way the yolo warning follows the CLI:
 * a dialog promising a terminal it will not open is a sentence about a thing
 * that is not being done.
 *
 * **Three sections since `R-J130`**, in a window of its own (`R-J128`): the
 * agent, where, and how — each a card in its own colour, the warning a
 * callout rather than a line of amber, the button in a footer that stays put
 * while the lists scroll. Asked 2026-10-11: *"make it visual component more
 * visible so it can easily see the diff section"*.
 *
 * **Two cards since `R-J136`**, asked 2026-10-12 the day after: *"I like the
 * coloring of the refactored UI, but can we make it more compact, we
 * shouldn't need a separate banner for AGENT and HOW"*. The agent and the
 * shape of the session are five controls that explain themselves, so they
 * share one blue card with no header strip — the CLI on one row, the two
 * checkboxes on the next, the yolo line under them — and only *where*, with
 * its lists, keeps a banner.
 *
 * **On which machine, since `R-J135`**, asked the same day: *"if we are
 * following a remote mogeung daemon, I want to be able to start a remote
 * session directly from the New session popup"*. A follow that is up is
 * offered beside the CLI, by the host its daemon says it is; one with
 * changes off is grey and says where to turn them on. A launch on another
 * machine is **always headless** — the daemon forwards it so, because a
 * terminal window there would open where nobody is looking — and the
 * session comes back mirrored, as that machine's, attachable over ssh the
 * way every mirrored session is. The recent repos and the favourites follow
 * the choice, since a path is a machine's own: each followed machine keeps a
 * starred list of its own, and the star files a path under the machine it
 * was typed for — asked the same afternoon, once the picker was in use.
 */

import { useEffect, useMemo, useState, type ReactNode } from "react";
import { FolderOpen, Rocket, Star, TriangleAlert, X } from "lucide-react";
import { useStore } from "@/store";
import { useMachineNames } from "@/lib/useMachineNames";
import { addFavourite, isFavourite, normaliseDir, removeFavourite } from "@/lib/favourites";
import { Dialog } from "@/ui/Dialog";
import { FolderBrowser } from "@/ui/FolderBrowser";
import { Callout, Field, Section } from "@/ui/Form";
import { Button, Checkbox, Dim, IconButton, Input, Mono, Row, Segmented } from "@/ui/primitives";
import { SourceMark } from "@/ui/SourceMark";
import { sourceLabel, type PeerState, type SessionSource } from "@/wire/types";

/**
 * The CLIs this window can start, and the flag each one's yolo mode is spelled
 * with.
 *
 * The flag is quoted rather than described because it is the thing you would
 * search for, and because a warning that paraphrases the danger is a warning
 * you can read without noticing. It must stay the truth: these strings are
 * what `agent_command` actually runs, and a dialog quoting a flag the daemon
 * does not pass is worse than one saying nothing.
 */
const LAUNCHABLE: readonly { source: SessionSource; yolo: string }[] = [
  { source: "claude_code", yolo: "--dangerously-skip-permissions" },
  { source: "qwen_code", yolo: "--approval-mode yolo" },
  { source: "codex", yolo: "--ask-for-approval never --sandbox workspace-write" },
  { source: "opencode", yolo: "--auto" },
];

/** "a claude", "an opencode" — the one place a CLI's name follows an article. */
function withArticle(name: string): string {
  return `${/^[aeiou]/i.test(name) ? "an" : "a"} ${name}`;
}

/** A list of folders in a box of its own, so each list has an edge. */
function FolderList({ children, empty }: { children: ReactNode; empty?: ReactNode }) {
  return (
    <div className="w-full rounded-sm border border-[var(--border)] bg-[var(--bg)]">
      {empty ? (
        <Dim className="block px-2 py-1.5 text-xs italic">{empty}</Dim>
      ) : (
        <div className="max-h-40 overflow-y-auto py-0.5">{children}</div>
      )}
    </div>
  );
}

export function LaunchWindow() {
  const open = useStore((s) => s.showLaunch);
  const sessions = useStore((s) => s.sessions);
  const setScoped = useStore((s) => s.setScoped);
  const send = useStore((s) => s.send);
  const [dir, setDir] = useState("");
  // The folder browser under the box (`R-J142`), open or not.
  const [browsing, setBrowsing] = useState(false);
  const [worktree, setWorktree] = useState(false);
  // Where it starts (`R-J135`): here — the daemon you are watching — or a
  // daemon this one follows, by machine id. Offered only while the follow is
  // up; one with changes off is on screen and grey, with the reason, rather
  // than missing. Local to the window, not a preference: which machine is a
  // decision per session, where the CLI is a habit.
  const peers = useStore((s) => s.peers);
  const [machine, setMachine] = useState<string | null>(null);
  const remotes = useMemo(() => (peers ?? []).filter((p) => p.state === "up" && !!p.machine_id), [peers]);
  const remote = machine ? (remotes.find((p) => p.machine_id === machine) ?? null) : null;
  // The name you gave its connection, else its host (`R-J140`).
  const names = useMachineNames();
  const nameOf = (p: PeerState) => (p.machine_id && names[p.machine_id]) || p.host || p.name;
  const remoteName = remote ? nameOf(remote) : null;
  // A follow that went down while this was open falls back to here, rather
  // than starting nowhere on the click.
  useEffect(() => {
    if (machine && !remote) setMachine(null);
  }, [machine, remote]);
  // The favourites are the chosen machine's: a path kept for one box is not
  // a path on another, so each keeps its own list and the star files what
  // you typed under the machine you typed it for.
  const favourites = useStore((s) => s.scoped(machine).favouriteDirs);
  // Remembered, not local: this is the front door since `R-J45`, and picking
  // the same CLI every morning is the kind of click you stop noticing and
  // never stop paying. `setPrefs` writes through to the file.
  const source = useStore((s) => s.prefs.launchSource);
  const headless = useStore((s) => s.prefs.launchHeadless);
  const health = useStore((s) => s.health);
  const setPrefs = useStore((s) => s.setPrefs);
  // The window of its own this opens in (`R-J128`), the size it was left at.
  const launchSize = useStore((s) => s.prefs.launchSize);
  const chosen = LAUNCHABLE.find((a) => a.source === source) ?? LAUNCHABLE[0];

  // Somewhere you have already worked is nearly always where you want to work
  // again, and typing a path is the slowest part of this window.
  //
  // Minus whatever you have kept: a folder in both lists is the same row twice,
  // and the copy you would click is the one with the ✕ next to it.
  const repos = useMemo(() => {
    const seen = new Set<string>();
    for (const s of Object.values(sessions)) {
      // A path is a machine's own: here, the sessions with no origin; on a
      // followed machine, the ones mirrored from it (`R-J135`).
      const theirs = machine ? s.origin?.machine_id === machine : !s.origin;
      if (theirs && s.repo_root) seen.add(s.repo_root);
    }
    return [...seen].filter((r) => !isFavourite(favourites, r)).sort();
  }, [sessions, favourites, machine]);

  // Only here: the trusted list is this daemon's, and the far daemon refuses
  // a headless Codex launch of its own accord (`R-J74`).
  const untrusted = useMemo(() => {
    const typedDir = normaliseDir(dir);
    if (machine || source !== "codex" || !typedDir) return false;
    const dirs = health?.agents?.find((a) => a.source === "codex")?.trusted_dirs;
    if (!dirs) return false;
    return !dirs.includes(typedDir);
  }, [machine, source, dir, health]);

  // Headless as asked — never into a directory the CLI has not been granted
  // (`R-J74`), and always on another machine (`R-J135`).
  const headlessNow = machine !== null || (headless && !untrusted);

  if (!open) return null;
  const close = () => {
    setBrowsing(false);
    useStore.setState({ showLaunch: false });
  };

  const keep = (path: string) => setScoped({ favouriteDirs: addFavourite(favourites, path) }, machine);
  const drop = (path: string) => setScoped({ favouriteDirs: removeFavourite(favourites, path) }, machine);

  const typed = normaliseDir(dir);
  const typedIsKept = isFavourite(favourites, typed);

  /**
   * Would a headless launch here create a session nobody can see? `R-J74`.
   *
   * Codex asks *"do you trust this directory?"* the first time it works in one
   * and opens no thread until you answer — so with no terminal window the
   * prompt has nowhere to appear, and the session is invisible to you and to
   * the daemon alike, which reads Codex's own bookkeeping and finds none.
   *
   * Exact-path, because that is how Codex matches: trusting `/home/you` does
   * not trust `/home/you/repo`, which is how this was reported. A daemon that
   * does not send the list at all (older, or a CLI with no such notion) warns
   * about nothing rather than guessing — the daemon refuses as the backstop.
   */

  const go = () => {
    if (!typed) return;
    send({
      cmd: "launch_terminal",
      dir: typed,
      worktree,
      source: chosen.source,
      headless: headlessNow,
      // Only when it is somewhere else: a client before `R-J135` sent no
      // such key, and the wire a daemon reads stays the same for a launch
      // here.
      ...(machine ? { machine } : {}),
    });
    close();
  };

  return (
    <Dialog
      title="New session"
      subtitle={
        remoteName
          ? `starts a real interactive ${sourceLabel(chosen.source)} under tmux on ${remoteName} — no terminal window`
          : headless
            ? `starts a real interactive ${sourceLabel(chosen.source)} under tmux — no terminal window`
            : `opens a real interactive ${sourceLabel(chosen.source)} in your terminal`
      }
      onClose={close}
      detached={{ name: "launch", size: launchSize, onResized: (size) => setPrefs({ launchSize: size }) }}
    >
      <div className="flex h-full min-h-0 min-w-[30rem] flex-col">
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3">
          <Dim className="block text-xs leading-snug">
            mogeung does not wrap the conversation — you drive it exactly as usual, and it shows up in the queue like
            any other session.
          </Dim>

          {/* One compact card for the choices (`R-J136`), no banner over any of
              them: which CLI, on which machine, and the shape of the session.
              The CLI comes first, above the directory, because it changes what
              the rest of this window means — the flag below it and the name in
              the button at the bottom — so it is answered before you read either. */}
          <Section tone="blue" testId="launch-choices">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
              <Segmented
                size="md"
                value={chosen.source}
                onChange={(v) => setPrefs({ launchSource: v })}
                options={LAUNCHABLE.map((a) => ({
                  value: a.source,
                  title: `start ${sourceLabel(a.source)} — ${a.yolo}`,
                  label: (
                    <span className="flex items-center gap-1.5">
                      <SourceMark source={a.source} />
                      {sourceLabel(a.source)}
                    </span>
                  ),
                }))}
              />
              {/* Only with a follow that is up (`R-J135`); the watched daemon
                  is always the first choice, and the default. */}
              {remotes.length > 0 && (
                <span className="flex items-center gap-1.5">
                  <Dim className="text-xs">on</Dim>
                  <Segmented
                    size="md"
                    value={machine ?? ""}
                    onChange={(v) => setMachine(v || null)}
                    options={[
                      { value: "", label: "this machine", title: "the daemon you are watching — its machine, as before" },
                      ...remotes.map((p) => {
                        const name = nameOf(p);
                        const where = p.host && p.host !== name ? ` (${p.host})` : "";
                        return {
                          value: p.machine_id ?? "",
                          label: name,
                          disabled: !p.allow_writes,
                          title: p.allow_writes
                            ? `start on ${name}${where} — headless, a detached tmux session there; it joins the queue as that machine's`
                            : `changes on ${name} are off for this follow — turn on “allow changes” for it in Connections`,
                        };
                      }),
                    ]}
                  />
                </span>
              )}
            </div>
            <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
              <Checkbox
                className="text-sm"
                checked={worktree}
                onChange={setWorktree}
                label="in a fresh git worktree"
                title="a new branch mogeung/<timestamp> in its own checkout, so this session cannot collide with the one already running there"
              />
              <Checkbox
                className="text-sm"
                checked={headlessNow}
                disabled={untrusted || machine !== null}
                onChange={(v) => setPrefs({ launchHeadless: v })}
                label="headless — no terminal window"
                title={
                  machine !== null
                    ? "always, on another machine: a terminal window there would open where nobody is looking"
                    : untrusted
                      ? "not available here yet: codex will stop on a trust prompt, and a headless session has no window to show it in"
                      : "a detached tmux session, hosted in a mogeung pane; reach it from any terminal later with tmux attach. Needs tmux on the daemon's machine"
                }
              />
            </div>
            {/* The flag, on one line: it is a sentence to read once, not a box
                to skim past. */}
            <div className="flex items-start gap-1.5 text-xs leading-snug text-[var(--dim)]">
              <TriangleAlert size={13} className="mt-0.5 shrink-0 text-[var(--amber)]" />
              <span>
                yolo mode, <Mono>{chosen.yolo}</Mono>: the agent will not ask before editing files or running commands
              </span>
            </div>
            {/* Said before the click, not after. The daemon refuses this launch
                too, but a refusal you read once the session already failed to
                appear is a worse version of the same sentence. */}
            {untrusted && (
              <Callout tone="amber" icon={<TriangleAlert size={14} />}>
                codex has not been trusted in this directory. It asks once, and a headless session has no window to
                ask in — so start it with a terminal this time. Headless works here from then on.
              </Callout>
            )}
          </Section>

          <Section title="Where" tone="green" icon={<FolderOpen size={14} />} hint="the folder the session starts in">
            <Field label="directory" hint="type or paste a path, or browse">
              <div className="flex w-full items-center gap-1">
                <Input
                  value={dir}
                  mono
                  className="flex-1"
                  onChange={setDir}
                  placeholder="~/projects/foo"
                  onKeyDown={(e) => e.key === "Enter" && go()}
                />
                {/*
                  Beside the star rather than replacing the box: the box is still
                  the fastest route for a path you can type or paste, and a picker
                  that is the only way in makes that route longer.
                */}
                <IconButton
                  title={browsing ? "close the folder browser" : `browse for a folder on ${remoteName ?? "this machine"}`}
                  active={browsing}
                  onClick={() => setBrowsing((b) => !b)}
                >
                  <FolderOpen className="h-3.5 w-3.5" />
                </IconButton>
                {/*
                  The star is a toggle rather than an add, so a folder already kept
                  says so instead of offering to keep it a second time — which is
                  the only state `addFavourite` has no visible answer for.
                */}
                <IconButton
                  disabled={!typed}
                  active={typedIsKept}
                  title={
                    !typed
                      ? "type a folder first"
                      : typedIsKept
                        ? "stop keeping this folder"
                        : "keep this folder in the list below"
                  }
                  onClick={() => (typedIsKept ? drop(typed) : keep(typed))}
                >
                  <Star className="h-3.5 w-3.5" fill={typedIsKept ? "currentColor" : "none"} />
                </IconButton>
              </div>
            </Field>

            {/* The daemon's disk, not the window's (`R-J142`): the OS picker
                here could only show this computer's, which is not where a
                session on a followed machine starts — nor, when the watched
                daemon is on another box, where one "here" does. */}
            {browsing && (
              <Field label="browse" hint={`${remoteName ?? "this machine"}'s folders`} align="start">
                <FolderBrowser
                  machine={machine}
                  machineName={remoteName ?? "this machine"}
                  start={dir}
                  onChoose={(path) => {
                    setDir(path);
                    setBrowsing(false);
                  }}
                />
              </Field>
            )}

            <Field label="favourites" hint="kept with the star" align="start">
              {/* An empty list that says nothing looks like a list that failed to
                  load. `R-J5`, and here it doubles as the only place the star is
                  explained. */}
              <FolderList empty={favourites.length === 0 && "none yet — type a folder above and press the star to keep it here"}>
                {favourites.map((f) => (
                  <Row key={f} selected={typed === f} onClick={() => setDir(f)} className="flex items-center gap-1 px-2 py-0.5">
                    <Mono className="flex-1 truncate text-xs">{f}</Mono>
                    <IconButton
                      title={`stop keeping ${f}`}
                      onClick={(e) => {
                        // The row sets the directory; the button must not do that
                        // on its way to removing the row it sits in.
                        e.stopPropagation();
                        drop(f);
                      }}
                    >
                      <X className="h-3 w-3" />
                    </IconButton>
                  </Row>
                ))}
              </FolderList>
            </Field>

            {repos.length > 0 && (
              <Field label="recent repos" hint="where a session has run" align="start">
                <FolderList>
                  {repos.map((r) => (
                    <Row key={r} selected={typed === r} onClick={() => setDir(r)} className="flex items-center gap-1 px-2 py-0.5">
                      <Mono className="flex-1 truncate text-xs">{r}</Mono>
                      <IconButton
                        title={`keep ${r}`}
                        onClick={(e) => {
                          e.stopPropagation();
                          keep(r);
                        }}
                      >
                        <Star className="h-3 w-3" />
                      </IconButton>
                    </Row>
                  ))}
                </FolderList>
              </Field>
            )}
          </Section>

        </div>

        {/* The one verb, in a footer that stays while the lists scroll. */}
        <div className="flex shrink-0 items-center gap-3 border-t border-[var(--border)] bg-[var(--bg-panel)] px-3 py-2">
          <Button variant="solid" size="lg" onClick={go} disabled={!typed}>
            <Rocket size={13} />{" "}
            {remoteName
              ? `start a headless ${sourceLabel(chosen.source)} on ${remoteName}`
              : headlessNow
                ? `start a headless ${sourceLabel(chosen.source)}`
                : `open ${withArticle(sourceLabel(chosen.source))} terminal`}
          </Button>
          <Dim className="text-xs leading-snug">
            {remoteName
              ? `it starts on ${remoteName} and joins the queue as that machine's session; its pane attaches over ssh once that daemon has said how`
              : "it opens where you are watching — the daemon's machine, not necessarily this one"}
          </Dim>
        </div>
      </div>
    </Dialog>
  );
}
