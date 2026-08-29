/**
 * The **similar** list. `R-O6`, and what `--bin judge --recall` decided about it.
 *
 * The harness measured the two engines against each other over 337 lines of a
 * real corpus: on paraphrased queries the index found 7 of 11 where grep found
 * 0, and on literal slices of the corpus grep found 10 of 11 where the index
 * found 5. **Complementary, and neither dominates.** Every assertion here is
 * one of the consequences of that:
 *
 * - the two lists stay two lists — a blend would be worse than either, which is
 *   pillar K's refusal with a measurement behind it;
 * - the list is labelled **similar** and never *matches*, because the index has
 *   been shown to rank away half of what grep matches;
 * - an index older than the corpus says so rather than answering as current;
 * - and the index is **built on a click**, never by asking a question.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import type { ClientMsg, SemanticHit } from "@/wire/types";
import { InsightPane } from "@/panes/InsightPane";
import { PaneScope } from "@/lib/paneScope";

const sent: ClientMsg[] = [];

const hit = (over: Partial<SemanticHit> = {}): SemanticHit => ({
  session_id: "sess1234",
  line: 12,
  role: "user",
  timestamp: "2026-08-01T10:00:00Z",
  preview: "how do I find the ip address of a pod",
  score: 0.81,
  ...over,
});

/**
 * The pane opens on Analytics, so every test clicks through to Search first —
 * the list under test lives there, beside the grep results it must never mix
 * with.
 */
const show = () => {
  const r = render(
    <PaneScope id="insight">
      <InsightPane />
    </PaneScope>,
  );
  fireEvent.click(screen.getByText("Search"));
  return r;
};

/** Only the commands this list sends; the pane fetches its own data on mount. */
const asked = () =>
  sent.filter((m) => m.cmd === "build_semantic_index" || m.cmd === "semantic_search");

/** The insight slice, with only what this list reads. */
const insight = (over: Record<string, unknown>) =>
  act(() =>
    useStore.setState((st) => ({ insight: { ...st.insight, ...over } as never })),
  );

beforeEach(() => {
  cleanup();
  sent.length = 0;
  useStore.setState({
    prefs: { ...defaultPrefs(), dock: "insight" },
    sessions: {},
    send: (m: ClientMsg) => void sent.push(m),
  } as never);
  insight({
    query: "",
    results: null,
    similar: null,
    similarPending: false,
    similarRefusal: null,
    indexModel: "",
    indexBuiltMs: 0,
    indexStale: false,
    indexBuilding: false,
  });
});

describe("the similar list", () => {
  /** No index is a state with an action, not an error and not an empty list. */
  it("offers to build an index when there is none, and asks nothing until told", () => {
    // Nothing searched yet, deliberately: a feature you can only reach by
    // already having used it is one nobody finds.
    show();
    expect(asked()).toHaveLength(0);
    fireEvent.click(screen.getByText("build the index"));
    expect(asked()).toEqual([{ cmd: "build_semantic_index" }]);
  });

  it("shows the model and when it was built, beside the grep results", () => {
    insight({
      query: "pod",
      results: ["pod", { hits: [], files_scanned: 3 }],
      indexModel: "bge-m3",
      indexBuiltMs: Date.parse("2026-08-29T09:00:00Z"),
      similar: ["pod", [hit()]],
    });
    show();
    expect(screen.getByText(/bge-m3/)).toBeInTheDocument();
    // Labelled *similar*, never *matches*: the harness measured the index
    // ranking away half of what grep found on literal queries.
    expect(screen.getByText(/similar —/)).toBeInTheDocument();
    expect(screen.getByText(/how do I find the ip address/)).toBeInTheDocument();
    expect(screen.getByText("0.81")).toBeInTheDocument();
  });

  /** A photograph of a corpus that kept growing has to say so. */
  it("says when the index is older than the corpus", () => {
    insight({
      query: "pod",
      results: ["pod", { hits: [], files_scanned: 3 }],
      indexModel: "bge-m3",
      indexBuiltMs: 1,
      indexStale: true,
      similar: ["pod", [hit()]],
    });
    show();
    expect(screen.getByText(/corpus has changed since this index was built/)).toBeInTheDocument();
  });

  /** Nothing similar is an answer, and a different one from grep's silence. */
  it("tells an empty index apart from an empty answer", () => {
    insight({
      query: "pod",
      results: ["pod", { hits: [], files_scanned: 3 }],
      indexModel: "bge-m3",
      indexBuiltMs: 1,
      similar: ["pod", []],
    });
    show();
    expect(screen.getByText(/different answer from grep finding nothing/)).toBeInTheDocument();
  });

  /** The daemon's own sentence, rendered rather than reinterpreted. */
  it("renders a refusal instead of an empty list", () => {
    insight({
      query: "pod",
      results: ["pod", { hits: [], files_scanned: 3 }],
      indexModel: "bge-m3",
      indexBuiltMs: 1,
      similarRefusal: "no embed_model is configured, so there is nothing to embed with",
      similar: ["pod", []],
    });
    show();
    expect(screen.getByText(/no embed_model is configured/)).toBeInTheDocument();
  });
});
