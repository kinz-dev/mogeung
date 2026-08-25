/**
 * Close one tab, the others, or the lot. `R-J47`.
 *
 * Asked for 2026-08-25: the centre accumulates tabs — a pane per file since
 * `R-B53`, a pane per agent since `R-B49`, and one arrives on every queue click
 * that lands nowhere — and the only way back to a clean header was to press ✕
 * once per tab, left to right, which is the gesture every editor stopped asking
 * for a decade ago.
 *
 * **A module of its own, because closing a tab is two different acts.** A file
 * pane is also a row in `explorer`'s open list, so it closes through
 * [`closeFile`]; an Agent pane holds an anchor that has to be dropped with it,
 * so it closes through [`closeAgentPane`]. `explorer.ts` already imports
 * `panes.ts`, so the dispatch cannot live in either without making the two
 * import each other — a cycle that resolves to `undefined` at module init, and
 * the one failure shape that never shows up in a test.
 */

import { closeFile } from "@/lib/explorer";
import { closeAgentPane, getDock, groupPanes, parseFilePaneId } from "@/lib/panes";
import { paneKind } from "@/lib/paneScope";

/** One tab, through whichever door its kind needs. */
export function closeTab(id: string): void {
  const file = parseFilePaneId(id);
  if (file) {
    closeFile(file.session, file.path, file.rev);
    return;
  }
  if (paneKind(id) === "agent") {
    closeAgentPane(id);
    return;
  }
  getDock()?.getPanel(id)?.api.close();
}

/**
 * Every tab in this one's group except this one.
 *
 * The commoner half of the pair by some distance: it is what you reach for
 * having found the pane you meant among nine you did not.
 */
export function closeOtherTabs(id: string): void {
  for (const other of groupPanes(id)) {
    if (other === id) continue;
    closeTab(other);
  }
}

/**
 * Every tab in this one's group, this one included.
 *
 * It can leave the centre empty, and that is allowed on purpose — the same
 * decision `closeAgentPane` took when slot 1 stopped being special. There are
 * four ways back to an Agent pane (`Alt+A`, the palette, `Alt+0`, the next
 * launch), so a gesture that refused the last tab would be protecting nothing
 * and would read as a bug the first time it declined.
 */
export function closeAllTabs(id: string): void {
  for (const other of groupPanes(id)) closeTab(other);
}
