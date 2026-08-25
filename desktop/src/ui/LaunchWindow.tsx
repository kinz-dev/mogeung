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
 * **Codex is absent rather than disabled**: mogeung has no recipe for starting
 * it, and a control you can see but not press invites the question *why not*,
 * every time. The daemon refuses it in words if a client asks anyway.
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
 */

import { useMemo, useState } from "react";
import { Rocket, Star, X } from "lucide-react";
import { useStore } from "@/store";
import { addFavourite, isFavourite, normaliseDir, removeFavourite } from "@/lib/favourites";
import { Dialog } from "@/ui/Dialog";
import { Button, Checkbox, Dim, IconButton, Input, Mono, Row, Segmented } from "@/ui/primitives";
import { SourceMark } from "@/ui/SourceMark";
import { sourceLabel, type SessionSource } from "@/wire/types";

/**
 * The CLIs this window can start, and the flag each one's yolo mode is spelled
 * with. Codex is deliberately not here — see the header note.
 *
 * The flag is quoted rather than described because it is the thing you would
 * search for, and because a warning that paraphrases the danger is a warning
 * you can read without noticing.
 */
const LAUNCHABLE: readonly { source: SessionSource; yolo: string }[] = [
  { source: "claude_code", yolo: "--dangerously-skip-permissions" },
  { source: "qwen_code", yolo: "--approval-mode yolo" },
];

export function LaunchWindow() {
  const open = useStore((s) => s.showLaunch);
  const sessions = useStore((s) => s.sessions);
  const favourites = useStore((s) => s.scoped().favouriteDirs);
  const setScoped = useStore((s) => s.setScoped);
  const send = useStore((s) => s.send);
  const [dir, setDir] = useState("");
  const [worktree, setWorktree] = useState(false);
  // Remembered, not local: this is the front door since `R-J45`, and picking
  // the same CLI every morning is the kind of click you stop noticing and
  // never stop paying. `setPrefs` writes through to the file.
  const source = useStore((s) => s.prefs.launchSource);
  const setPrefs = useStore((s) => s.setPrefs);
  const chosen = LAUNCHABLE.find((a) => a.source === source) ?? LAUNCHABLE[0];

  // Somewhere you have already worked is nearly always where you want to work
  // again, and typing a path is the slowest part of this window.
  //
  // Minus whatever you have kept: a folder in both lists is the same row twice,
  // and the copy you would click is the one with the ✕ next to it.
  const repos = useMemo(() => {
    const seen = new Set<string>();
    for (const s of Object.values(sessions)) if (s.repo_root) seen.add(s.repo_root);
    return [...seen].filter((r) => !isFavourite(favourites, r)).sort();
  }, [sessions, favourites]);

  if (!open) return null;
  const close = () => useStore.setState({ showLaunch: false });

  const keep = (path: string) => setScoped({ favouriteDirs: addFavourite(favourites, path) });
  const drop = (path: string) => setScoped({ favouriteDirs: removeFavourite(favourites, path) });

  const typed = normaliseDir(dir);
  const typedIsKept = isFavourite(favourites, typed);

  const go = () => {
    if (!typed) return;
    send({ cmd: "launch_terminal", dir: typed, worktree, source: chosen.source });
    close();
  };

  return (
    <Dialog
      title="New session"
      subtitle={`opens a real interactive ${sourceLabel(chosen.source)} in your terminal`}
      onClose={close}
    >
      <div className="min-w-[30rem]">
        <Dim className="block text-2xs">
          mogeung does not wrap the conversation — you drive it exactly as usual, and it shows
          up in the queue like any other session.
        </Dim>

        {/* First, above the directory: which CLI changes what the rest of this
            window means — the flag below it, and the name in the button at the
            bottom — so it is answered before you read either. */}
        <div className="mt-2 flex items-center gap-2">
          <Dim className="text-2xs">agent</Dim>
          <Segmented
            value={chosen.source}
            onChange={(v) => setPrefs({ launchSource: v })}
            options={LAUNCHABLE.map((a) => ({
              value: a.source,
              title: `start ${sourceLabel(a.source)} — ${a.yolo}`,
              label: (
                <span className="flex items-center gap-1">
                  <SourceMark source={a.source} />
                  {sourceLabel(a.source)}
                </span>
              ),
            }))}
          />
        </div>

        <div className="mt-1 text-2xs text-[var(--amber)]">
          Started in yolo mode: <Mono>{chosen.yolo}</Mono>. The agent will not
          ask before editing files or running commands.
        </div>

        <div className="mt-2">
          <Dim className="mb-0.5 block text-2xs">directory</Dim>
          <div className="flex items-center gap-1">
            <Input
              value={dir}
              mono
              className="flex-1"
              onChange={setDir}
              placeholder="~/projects/foo"
              onKeyDown={(e) => e.key === "Enter" && go()}
            />
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
        </div>

        <Dim className="mt-2 mb-0.5 block text-2xs">favourites</Dim>
        {favourites.length === 0 ? (
          // An empty list that says nothing looks like a list that failed to
          // load. `R-J5`, and here it doubles as the only place the star is
          // explained.
          <Dim className="block text-2xs italic">
            none yet — type a folder above and press the star to keep it here
          </Dim>
        ) : (
          <div className="max-h-40 overflow-y-auto">
            {favourites.map((f) => (
              <Row key={f} selected={typed === f} onClick={() => setDir(f)} className="flex items-center gap-1 px-1 py-0.5">
                <Mono className="flex-1 truncate text-2xs">{f}</Mono>
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
          </div>
        )}

        {repos.length > 0 && (
          <>
            <Dim className="mt-2 mb-0.5 block text-2xs">recent repos</Dim>
            <div className="max-h-40 overflow-y-auto">
              {repos.map((r) => (
                <Row key={r} selected={typed === r} onClick={() => setDir(r)} className="flex items-center gap-1 px-1 py-0.5">
                  <Mono className="flex-1 truncate text-2xs">{r}</Mono>
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
            </div>
          </>
        )}

        <div className="mt-2">
          <Checkbox
            checked={worktree}
            onChange={setWorktree}
            label="in a fresh git worktree"
            title="a new branch mogeung/<timestamp> in its own checkout, so this session cannot collide with the one already running there"
          />
        </div>

        <div className="mt-3 flex items-center gap-2">
          <Button variant="solid" onClick={go} disabled={!typed}>
            <Rocket size={11} /> open a {sourceLabel(chosen.source)} terminal
          </Button>
          <Dim className="text-2xs">
            it opens where you are watching — the daemon's machine, not necessarily this one
          </Dim>
        </div>
      </div>
    </Dialog>
  );
}
