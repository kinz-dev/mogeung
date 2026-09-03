/**
 * Scratch files. `R-L5`, ADR-0035.
 *
 * IntelliJ's gesture: `Ctrl+Alt+Shift+Insert`, pick a language, and a file
 * called `scratch-3.java` opens with the cursor in it. It is **a file, not a
 * note**: `R-L2`'s notes are markdown the daemon owns and mirrors one way, and
 * they live in the rail; a scratch file is a document in the Code pane, with
 * the colouring its extension earns, that anything else on the machine can
 * open from `~/.mogeung/scratch`.
 *
 * It is also the first thing Monaco has ever been allowed to write in this
 * window, and the reason that is not a change to pillar K is where the
 * writing goes: the daemon mints every name, refuses any that is not a bare
 * file name inside its own directory, and does the write itself. The pane
 * never holds a path — only a name — so there is nothing to aim elsewhere.
 *
 * **The pane id is `scratch:<name>`, and it survives the saved layout** on
 * purpose, unlike a `file:` pane. A file pane is stripped on restore because
 * its id names a session that may be gone; a scratch file names nothing but
 * itself, so a pane that comes back after a restart is the right behaviour —
 * it asks the daemon for the body on mount and says so if the file has gone.
 */

import { useStore } from "@/store";
import { getDock } from "@/lib/panes";

const PREFIX = "scratch:";

/** What the picker offers. The extension is what the daemon names the file by. */
export interface ScratchLanguage {
  label: string;
  ext: string;
}

/**
 * Ordered by the ask (`R-J88`: Java first, then the rest), then by what a
 * scratch file is usually for. Plain text is last rather than first: the
 * point of picking is the colouring.
 */
export const SCRATCH_LANGUAGES: readonly ScratchLanguage[] = [
  { label: "Java", ext: "java" },
  { label: "Python", ext: "py" },
  { label: "JavaScript", ext: "js" },
  { label: "TypeScript", ext: "ts" },
  { label: "JSON", ext: "json" },
  { label: "XML", ext: "xml" },
  { label: "YAML", ext: "yaml" },
  { label: "Markdown", ext: "md" },
  { label: "SQL", ext: "sql" },
  { label: "CSS", ext: "css" },
  { label: "HTML", ext: "html" },
  { label: "Shell", ext: "sh" },
  { label: "Rust", ext: "rs" },
  { label: "Kotlin", ext: "kt" },
  { label: "Go", ext: "go" },
  { label: "TOML", ext: "toml" },
  { label: "Plain text", ext: "txt" },
];

export function scratchPaneId(name: string): string {
  return `${PREFIX}${name}`;
}

/** The name back out of a pane id, or `null` if it is not a scratch pane. */
export function parseScratchPaneId(id: string): string | null {
  return id.startsWith(PREFIX) && id.length > PREFIX.length ? id.slice(PREFIX.length) : null;
}

/** Where the file is on the daemon's machine, for the tab's hint and the menu. */
export function scratchPath(name: string): string {
  return `~/.mogeung/scratch/${name}`;
}

/**
 * Ask the daemon for a new file. The pane opens when the answer comes back
 * (`scratch_content` with `fresh`), not here — the name is the daemon's to
 * choose, and a pane opened on a guessed name would be a pane on nothing.
 */
export function createScratch(ext: string): void {
  useStore.getState().send({ cmd: "scratch_create", ext });
}

/** Open an existing one, or bring its pane forward. */
export function openScratch(name: string): void {
  showScratchPane(name);
}

/**
 * The pane itself. Beside the active group when that group is an agent, the
 * same rule `showFilePane` uses — a document joins the documents.
 */
export function showScratchPane(name: string): void {
  const dock = getDock();
  if (!dock) return;
  const id = scratchPaneId(name);
  const existing = dock.getPanel(id);
  if (existing) {
    existing.api.setActive();
    return;
  }
  const active = dock.activeGroup;
  const activeIsDocument =
    active?.activePanel && (active.activePanel.id.startsWith("file:") || active.activePanel.id.startsWith(PREFIX));
  dock.addPanel({
    id,
    component: "scratch",
    title: name,
    ...(active && !activeIsDocument ? { position: { referenceGroup: active, direction: "right" as const } } : {}),
  });
}

/** Ask for the body once; the store fills it in when the daemon answers. */
export function fetchScratch(name: string): void {
  const { send, scratch } = useStore.getState();
  if (scratch.files[name]) return;
  useStore.setState({
    scratch: { ...scratch, files: { ...scratch.files, [name]: { content: null, saved: 0 } } },
  });
  send({ cmd: "scratch_read", name });
}

/** Let go of a body a closed pane no longer needs, so a reopen re-reads it. */
export function forgetScratch(name: string): void {
  const { scratch } = useStore.getState();
  if (!scratch.files[name]) return;
  const files = { ...scratch.files };
  delete files[name];
  useStore.setState({ scratch: { ...scratch, files } });
}
