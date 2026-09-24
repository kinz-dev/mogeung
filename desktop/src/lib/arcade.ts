/**
 * Where the easter-egg games keep their history: one JSON line per finished
 * session, in `~/.mogeung/<game>.jsonl` through the shell, and in
 * `localStorage` when there is no shell (a browser tab, the tests).
 *
 * The file is yours — to copy, read or delete — so a reader must be tolerant:
 * each game parses its own lines and skips the ones it cannot read.
 */

import { isTauri } from "@/lib/tauri";

export type ArcadeGame = "mental-math" | "trading-game";

const localKey = (game: ArcadeGame) => `mogeung.arcade.${game}`;

/** Every line recorded for a game. Never throws: no history is an answer. */
export async function loadLines(game: ArcadeGame): Promise<string[]> {
  try {
    const text = isTauri()
      ? await (await import("@tauri-apps/api/core")).invoke<string>("arcade_read", { game })
      : (localStorage.getItem(localKey(game)) ?? "");
    return text
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);
  } catch {
    return [];
  }
}

/** Add one session. Returns whether it was kept, so a screen can say if not. */
export async function appendLine(game: ArcadeGame, record: unknown): Promise<boolean> {
  const line = JSON.stringify(record);
  try {
    if (isTauri()) {
      await (await import("@tauri-apps/api/core")).invoke("arcade_append", { game, line });
      return true;
    }
    localStorage.setItem(localKey(game), `${localStorage.getItem(localKey(game)) ?? ""}${line}\n`);
    return true;
  } catch {
    return false;
  }
}

/** For a history screen's footer: where the sessions are. */
export function historyPlace(game: ArcadeGame): string {
  return isTauri() ? `~/.mogeung/${game}.jsonl` : "this browser's storage";
}
