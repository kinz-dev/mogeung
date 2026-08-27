/**
 * Copying a turn, and where the words go.
 *
 * Asked 2026-08-27: a clipboard button beside the one that copies into a note.
 * The two sit next to each other and the pane has to keep them **telling
 * different truths**, which is the whole of what these pin:
 *
 * - a turn goes to the clipboard as its own **words**, with none of the note's
 *   heading — that heading is what you want months later and exactly what you
 *   do not want when pasting into an issue;
 * - a conversation keeps the note's shape, because bare text with no speakers
 *   is not a readable conversation;
 * - and it says so out loud, because a clipboard write has no visible effect
 *   and a button with none is one you press twice and then distrust.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { TranscriptPane } from "@/panes/TranscriptPane";
import { useStore } from "@/store";
import { PaneScope } from "@/lib/paneScope";
import { defaultPrefs } from "@/store/prefs";
import type { TranscriptEvent } from "@/wire/types";

/**
 * The turn list is virtualised, and jsdom has no layout — so the real
 * virtualiser measures a viewport of zero and renders no rows at all, leaving
 * a pane that reports "1 turns" and shows none of them. Every row is rendered
 * here instead. What that gives up is honest: these tests say nothing about
 * windowing, only about the buttons on a row once it is on screen.
 */
vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 90,
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({ key: index, index, start: index * 90, size: 90 })),
    measureElement: () => {},
    scrollToIndex: () => {},
  }),
}));

const written: string[] = [];

const ev = (seq: number, t: "user_prompt" | "assistant_text", text: string): TranscriptEvent =>
  ({ session_id: "s1", seq, ts: "2026-08-27T10:00:00Z", kind: { t, text } }) as TranscriptEvent;

function show(events: TranscriptEvent[]) {
  useStore.setState({
    selected: "s1",
    events: { s1: events },
    sessions: {},
    prefs: defaultPrefs(),
    notices: [],
  } as never);
  return render(
    <PaneScope id="transcript">
      <TranscriptPane />
    </PaneScope>,
  );
}

beforeEach(() => {
  cleanup();
  written.length = 0;
  vi.stubGlobal("navigator", {
    clipboard: {
      writeText: (t: string) => {
        written.push(t);
        return Promise.resolve();
      },
    },
  });
});

describe("copying to the clipboard", () => {
  it("copies a turn as its own words, without the note's heading", async () => {
    show([ev(1, "user_prompt", "make the backoff jittered")]);

    fireEvent.click(screen.getByTitle("copy this turn's text to the clipboard"));

    await waitFor(() => expect(written).toHaveLength(1));
    expect(written[0]).toBe("make the backoff jittered");
    expect(written[0]).not.toContain("#");
  });

  /** A conversation without speakers is not a conversation. */
  it("copies the conversation in the note's shape, with its speakers", async () => {
    show([ev(1, "user_prompt", "make the backoff jittered"), ev(2, "assistant_text", "done")]);

    fireEvent.click(screen.getByTitle(/copy this conversation to the clipboard/));

    await waitFor(() => expect(written).toHaveLength(1));
    expect(written[0]).toContain("make the backoff jittered");
    expect(written[0]).toContain("done");
    expect(written[0]).toMatch(/you|agent/i);
  });

  it("says it copied, because nothing else on screen would", async () => {
    show([ev(1, "user_prompt", "make the backoff jittered")]);
    fireEvent.click(screen.getByTitle("copy this turn's text to the clipboard"));

    await waitFor(() =>
      expect(useStore.getState().notices.some((n) => /copied/i.test(n.text))).toBe(true),
    );
  });

  /** A webview that refuses the clipboard is the one case where nothing
   *  happens and nothing is wrong with what you asked for. */
  it("reports a refusal rather than failing silently", async () => {
    vi.stubGlobal("navigator", {
      clipboard: { writeText: () => Promise.reject(new Error("no clipboard here")) },
    });
    show([ev(1, "user_prompt", "make the backoff jittered")]);

    fireEvent.click(screen.getByTitle("copy this turn's text to the clipboard"));

    await waitFor(() =>
      expect(useStore.getState().notices.some((n) => /could not copy/i.test(n.text))).toBe(true),
    );
  });
});
