/**
 * The change-summary protocol, client side.
 *
 * The daemon stopped broadcasting full diffs from its scan loop — hunk
 * bodies grew with the session's whole diff and went to every window, every
 * move. What travels now is a summary; the hunks are pulled only by the pane
 * drawing them. These tests pin the client half: the summary map is kept
 * current by either message, the disagreement check that triggers a re-fetch
 * answers correctly, and a pruned session takes its heap with it.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "@/store";
import { summarize, summaryDisagrees } from "@/store/changes";
import type { Change, ChangeSummary } from "@/wire/types";

const change = (reviewed: boolean): Change => ({
  files: [
    {
      path: "src/a.rs",
      old_path: null,
      status: "modified",
      insertions: 2,
      deletions: 1,
      hunks: [
        {
          anchor: "x",
          header: "@@ -1 +1 @@",
          lines: ["+two", "-one"],
          insertions: 2,
          deletions: 1,
          flags: [],
          score: 0,
          reviewed,
        },
      ],
      flags: [],
      score: 0,
      truncated: false,
    },
  ],
  insertions: 2,
  deletions: 1,
  error: null,
});

describe("summarize", () => {
  it("carries the tallies and none of the lines", () => {
    const s = summarize(change(true));
    expect(s.files).toEqual([
      {
        path: "src/a.rs",
        status: "modified",
        insertions: 2,
        deletions: 1,
        hunks: 1,
        reviewed_hunks: 1,
        score: 0,
      },
    ]);
    expect(JSON.stringify(s)).not.toContain("+two");
  });
});

describe("summaryDisagrees", () => {
  it("agrees with the change it was derived from", () => {
    const c = change(false);
    expect(summaryDisagrees(summarize(c), c)).toBe(false);
  });

  it("notices a moved diff and a review mark alike", () => {
    const held = change(false);
    const moved: ChangeSummary = {
      ...summarize(held),
      insertions: 9,
    };
    expect(summaryDisagrees(moved, held)).toBe(true);
    // A hunk marked read elsewhere must also pull fresh hunks, or the pane
    // keeps showing it unread.
    expect(summaryDisagrees(summarize(change(true)), held)).toBe(true);
  });
});

describe("the summary map", () => {
  beforeEach(() =>
    useStore.setState({ changes: {}, changeSummaries: {}, events: {}, sessions: {} }),
  );

  it("is fed by change_summary broadcasts", () => {
    useStore
      .getState()
      .ingest({ ev: "change_summary", session_id: "s1", summary: summarize(change(false)) });
    expect(useStore.getState().changeSummaries.s1.insertions).toBe(2);
    // No full change appeared out of a summary — hunks arrive only on ask.
    expect(useStore.getState().changes.s1).toBeUndefined();
  });

  it("is kept in step by a full change_updated", () => {
    useStore.getState().ingest({ ev: "change_updated", session_id: "s1", change: change(true) });
    expect(useStore.getState().changes.s1.files[0].hunks.length).toBe(1);
    expect(useStore.getState().changeSummaries.s1.files[0].reviewed_hunks).toBe(1);
  });

  it("goes with the session when the daemon prunes it", () => {
    useStore.getState().ingest({ ev: "change_updated", session_id: "s1", change: change(false) });
    useStore.getState().ingest({
      ev: "events",
      events: [
        {
          session_id: "s1",
          seq: 1,
          ts: new Date().toISOString(),
          kind: { t: "assistant_text", text: "hi" },
        },
      ],
    });
    useStore.getState().ingest({ ev: "session_removed", session_id: "s1" });
    const st = useStore.getState();
    expect(st.changes.s1).toBeUndefined();
    expect(st.changeSummaries.s1).toBeUndefined();
    expect(st.events.s1).toBeUndefined();
  });
});
