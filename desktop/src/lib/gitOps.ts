/**
 * The git operations, as data. `R-D31`.
 *
 * IntelliJ's *VCS Operations Popup* is a numbered list over the verbs its tool
 * window already has, on one key. This is the same list over the verbs
 * [feature 0042](../../../docs/features/0042-git-tool-window.md) built — and
 * the reason it is a module rather than markup is that **all the interesting
 * parts are rules, not pixels**: which digit runs what, what is unavailable
 * right now and what it says instead, and what each one does to the store.
 * Those are assertions over a pure function; a popup that held them inside its
 * own JSX would be a popup you could only test by clicking.
 *
 * **Nothing here is a new capability.** Every entry routes to a verb that
 * exists — `lib/gitActions.ts` for the daemon, the store for the window — with
 * two exceptions that are stated rather than hidden:
 *
 *  - **Push is permanently unavailable**, by decision, not by omission:
 *    [ADR-0012](../../../docs/decisions/0012-write-locally-never-publish.md)
 *    and [ADR-0014](../../../docs/decisions/0014-fetch-is-not-publishing.md)
 *    draw the line at publishing and merging. It is drawn, disabled, with that
 *    sentence — an IntelliJ user will press `8`, and a row that is missing
 *    teaches nothing where a row that refuses teaches the rule once. `R-J92`
 *    made the same choice for a button it could not enable.
 *  - **Show Local History has no equivalent**, because IntelliJ records every
 *    edit and mogeung records none. The nearest honest answer is git's own
 *    memory, the reflog, and that is what the row says it is.
 */

import { useStore, type GitTab } from "@/store";
import type { SessionId } from "@/wire/types";
import { askLog, copyText, discard, selectPath, stage } from "@/lib/gitActions";
import { openFile } from "@/lib/explorer";
import { parseFilePaneId } from "@/lib/panes";

/**
 * What the popup is acting on.
 *
 * Read once, at the moment the popup opens or a key is pressed — never
 * subscribed to. An entry that became available while the list was on screen
 * would move the numbering under the hand already reaching for it.
 */
export interface OpsContext {
  session: SessionId | null;
  /** The session's repository, or `null` when it is not in one. */
  repoRoot: string | null;
  /** The file the Local changes tab has selected, if any. */
  selectedPath: string | null;
  /** The file the Code pane in front is showing, if any. */
  filePath: string | null;
  /** The current branch, for *Copy Branch Name*. */
  head: string | null;
}

export interface GitOp {
  id: string;
  /**
   * The key that runs it — `"1"`…`"9"`, `"0"`, or `null` for the rows below
   * the numbered block, which are reached with the arrows.
   */
  digit: string | null;
  label: string;
  /** A keymap action id, when mogeung binds one; the popup draws the live
   *  binding rather than IntelliJ's, which this window does not listen for. */
  action?: string;
  /** A word under the label, where the entry does something worth warning about. */
  note?: string;
  /** Why it cannot run, or `null` when it can. */
  unavailable(ctx: OpsContext): string | null;
  /**
   * Do it. `confirm` is how an entry asks the popup for a confirmation
   * instead of acting — the popup renders it and calls `run` again with the
   * answer, which keeps "never discard without naming the file" in one place.
   */
  run(ctx: OpsContext): OpAsk | void;
}

/** What an entry can ask the popup for instead of acting. */
export type OpAsk =
  | { ask: "branches" }
  | { ask: "confirm"; title: string; body: string; danger?: boolean; then: () => void };

/** The two refusals every entry shares, in the order they are reached. */
function needsRepo(ctx: OpsContext): string | null {
  if (!ctx.session) return "no session is selected — a repository belongs to one";
  if (!ctx.repoRoot) return "that session is not in a git repository";
  return null;
}

function needsPath(ctx: OpsContext): string | null {
  const gate = needsRepo(ctx);
  if (gate) return gate;
  if (!ctx.selectedPath && !ctx.filePath) return "no file is selected — pick one in Local changes, or open one";
  return null;
}

/** The file an entry acts on: the git selection first, then what you are reading. */
export function targetPath(ctx: OpsContext): string | null {
  return ctx.selectedPath ?? ctx.filePath;
}

/** Put the Git tool window in front, on a tab. The popup's one movement. */
export function openGit(tab: GitTab): void {
  const { setPrefs } = useStore.getState();
  useStore.setState({ gitTab: tab });
  setPrefs({ dock: "git" });
}

/** Ask the commit box for the keyboard. Consumed by `LocalChanges`. */
function focusCommit(): void {
  useStore.setState((s) => ({ commitFocus: s.commitFocus + 1 }));
}

export const GIT_OPS: readonly GitOp[] = [
  {
    id: "commit",
    digit: "1",
    label: "Commit…",
    unavailable: needsRepo,
    run: () => {
      openGit("local");
      focusCommit();
    },
  },
  {
    id: "commit_file",
    digit: "2",
    label: "Commit File…",
    // IntelliJ greys this one out when the editor has no file; here the same
    // rule reads off whichever of the two selections exists.
    unavailable: needsPath,
    run: (ctx) => {
      const path = targetPath(ctx);
      if (!ctx.session || !path) return;
      // Staged first, because *commit this file* is two verbs in git and the
      // popup promising one of them would commit whatever was staged already.
      stage(ctx.session, [path]);
      selectPath(ctx.session, path, false);
      openGit("local");
      focusCommit();
    },
  },
  {
    id: "rollback",
    digit: "3",
    label: "Rollback…",
    note: "throws the file's changes away",
    unavailable: needsPath,
    run: (ctx) => {
      const path = targetPath(ctx);
      if (!ctx.session || !path) return;
      const session = ctx.session;
      // The confirmation is the feature, not politeness: `R-D19`'s rule is
      // that a discard names every file it is about to lose, and a popup that
      // skipped it would be the one place in the window where it did.
      return {
        ask: "confirm",
        title: "Roll back this file?",
        body: `${path}\n\nUncommitted changes to it are thrown away. This cannot be undone from here.`,
        danger: true,
        then: () => {
          discard(session, [path]);
          openGit("local");
        },
      };
    },
  },
  {
    id: "history",
    digit: "4",
    label: "Show History",
    unavailable: needsRepo,
    run: (ctx) => {
      if (!ctx.session) return;
      // With a file, the log scoped to it — which is what *file history* is.
      // Without one, the log as it stands, unfiltered.
      askLog(ctx.session, 0, { path: targetPath(ctx) ?? "" });
      openGit("log");
    },
  },
  {
    id: "annotate",
    digit: "5",
    label: "Annotate with Git Blame",
    unavailable: needsPath,
    run: (ctx) => {
      const path = targetPath(ctx);
      if (!ctx.session || !path) return;
      const { scoped, setScoped } = useStore.getState();
      const on = scoped().editorBlame.includes(path);
      setScoped({
        editorBlame: on
          ? scoped().editorBlame.filter((p) => p !== path)
          : [...scoped().editorBlame, path],
      });
      // Opened as well as annotated: turning a gutter on in a pane that is not
      // on screen is a keystroke with no visible effect.
      if (!on) openFile(ctx.session, path, { pin: true });
    },
  },
  {
    id: "diff",
    digit: "6",
    label: "Show Diff",
    unavailable: needsRepo,
    run: (ctx) => {
      if (!ctx.session) return;
      const path = targetPath(ctx);
      if (path) {
        selectPath(ctx.session, path, false);
        openGit("local");
        return;
      }
      // No file in hand: the session's own diff, which is the Changes tool and
      // the question *what has this agent done* rather than *what is in this
      // file*.
      useStore.getState().setPrefs({ dock: "changes" });
    },
  },
  {
    id: "branches",
    digit: "7",
    label: "Branches…",
    action: "git.branches",
    unavailable: needsRepo,
    run: () => ({ ask: "branches" }),
  },
  {
    id: "push",
    digit: "8",
    label: "Push…",
    note: "mogeung never publishes — ADR-0012",
    // Not `needsRepo`: this one is unavailable in a bare directory and in a
    // perfect repository alike, and saying so the same way in both is the
    // point. The rule is the product's, not the situation's.
    unavailable: () =>
      "mogeung writes locally and never publishes (ADR-0012) — push from the terminal beside it",
    run: () => {},
  },
  {
    id: "stash",
    digit: "9",
    label: "Stash Changes…",
    unavailable: needsRepo,
    run: () => openGit("stash"),
  },
  {
    id: "unstash",
    digit: "0",
    label: "Unstash Changes…",
    unavailable: needsRepo,
    run: () => openGit("stash"),
  },
  {
    id: "fetch",
    digit: null,
    // IntelliJ's own name for the row, with the verb it really is in brackets:
    // `Ctrl+T` is *Update Project* there and `git fetch` here, and the note
    // says which half of that command this product will ever have.
    label: "Update Project (fetch)",
    action: "sync",
    note: "reads the remote; merges nothing",
    unavailable: needsRepo,
    run: (ctx) => {
      if (!ctx.session) return;
      useStore.getState().send({ cmd: "git_fetch", session_id: ctx.session });
      openGit("log");
    },
  },
  {
    id: "worktrees",
    digit: null,
    label: "Worktrees…",
    unavailable: needsRepo,
    run: () => {
      useStore.setState({ gitMore: "worktrees" });
      openGit("more");
    },
  },
  {
    id: "copy_branch",
    digit: null,
    label: "Copy Branch Name",
    unavailable: (ctx) => needsRepo(ctx) ?? (ctx.head ? null : "this worktree has no branch checked out"),
    run: (ctx) => {
      if (ctx.head) copyText(ctx.head);
    },
  },
  {
    id: "reflog",
    digit: null,
    // IntelliJ's *Show Local History* records every edit in the IDE. mogeung
    // records none, and says which question it can answer instead.
    label: "Show Local History (git reflog)",
    unavailable: needsRepo,
    run: () => {
      useStore.setState({ gitMore: "reflog" });
      openGit("more");
    },
  },
];

/** The entry a digit runs, or `undefined`. */
export function opForDigit(digit: string): GitOp | undefined {
  return GIT_OPS.find((o) => o.digit === digit);
}

/** The context, read off the store. One reader, so the popup and its tests
 *  cannot disagree about what *the selected file* means. */
export function opsContext(): OpsContext {
  const s = useStore.getState();
  const session = s.selected;
  const sess = session ? s.sessions[session] : undefined;
  const git = session ? s.git[session] : undefined;
  return {
    session,
    repoRoot: sess?.repo_root ?? null,
    selectedPath: git?.selectedPath ?? null,
    filePath: filePathInFront(),
    head: git?.refs?.head ?? null,
  };
}

/**
 * The path of the Code pane that is forward, if one is.
 *
 * `activePane` is mirrored into the store for the rail (`R-J25`), and a
 * `file:` id carries its path — so *annotate this file* means the file you are
 * looking at, without the popup reaching into dockview. Parsed by
 * `parseFilePaneId`, because there is one id grammar and it lives there.
 */
function filePathInFront(): string | null {
  const id = useStore.getState().activePane;
  return (id ? parseFilePaneId(id)?.path : null) ?? null;
}
