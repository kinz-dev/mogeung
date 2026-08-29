/**
 * Recurring failures grouped by meaning. `R-F4` by meaning, `R-O6`, `A38`.
 *
 * `--bin judge --clusters` over 232 literal groups of this machine's corpus is
 * what this list rests on: nine spellings of one shell error in a cluster, five
 * of a browser timeout, four of a command timeout — joins no normalisation can
 * make, because the members share not one distinctive word. What is pinned here
 * is the honesty of that claim rather than the grouping itself:
 *
 * - a cluster **expands to what it joined**, because *these nine errors are one
 *   error* is worth nothing if you cannot read the nine;
 * - a cluster of one is not shown as a cluster — it joined nothing, and saying
 *   otherwise would be claiming work that was not done;
 * - the literal list underneath is untouched, which is pillar K's refusal of a
 *   blend;
 * - and the grouping is asked for, because it costs a model call.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import type { ClientMsg, FailureCluster } from "@/wire/types";
import { InsightPane } from "@/panes/InsightPane";
import { PaneScope } from "@/lib/paneScope";

const sent: ClientMsg[] = [];

const member = (example: string, count = 1) => ({
  normalized: example.toLowerCase(),
  example,
  sessions: ["s1"],
  count,
});

const cluster = (over: Partial<FailureCluster> = {}): FailureCluster => ({
  label: "(eval):1: == not found",
  members: [member("(eval):1: == not found", 5), member("(eval):1: unmatched '", 3)],
  sessions: ["s1", "s2"],
  count: 8,
  ...over,
});

const show = () => {
  const r = render(
    <PaneScope id="insight">
      <InsightPane />
    </PaneScope>,
  );
  fireEvent.click(screen.getByText("Failures"));
  return r;
};

const insight = (over: Record<string, unknown>) =>
  act(() => useStore.setState((st) => ({ insight: { ...st.insight, ...over } as never })));

const asked = () => sent.filter((m) => m.cmd === "cluster_failures");

beforeEach(() => {
  cleanup();
  sent.length = 0;
  useStore.setState({
    prefs: { ...defaultPrefs(), dock: "insight" },
    sessions: {},
    send: (m: ClientMsg) => void sent.push(m),
  } as never);
  insight({
    failures: [member("(eval):1: == not found", 5)],
    failureClusters: null,
    failureClustersPending: false,
    clusterModel: "",
    clusterRefusal: null,
  });
});

describe("failures grouped by meaning", () => {
  /** It costs a model call, so it is asked for. */
  it("groups nothing until it is asked to", () => {
    show();
    expect(asked()).toHaveLength(0);
    fireEvent.click(screen.getByText("group by meaning"));
    expect(asked()).toEqual([{ cmd: "cluster_failures", min_sessions: 1 }]);
  });

  it("shows what a cluster joined, on demand", () => {
    insight({ failureClusters: [cluster()], clusterModel: "bge-m3" });
    show();
    expect(screen.getByText(/2 wordings · 2 session\(s\)/)).toBeInTheDocument();
    // The members are the evidence for the join, and they are one click away.
    expect(screen.queryByText(/unmatched/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByText(/what was joined/));
    expect(screen.getByText(/unmatched/)).toBeInTheDocument();
  });

  /** A cluster of one joined nothing; claiming otherwise would be a lie. */
  it("does not present an unjoined failure as a cluster", () => {
    insight({
      failureClusters: [cluster({ members: [member("only one", 2)], label: "only one" })],
    });
    show();
    expect(screen.queryByText(/wordings/)).not.toBeInTheDocument();
  });

  /** The literal list is the incumbent and is never replaced by this. */
  it("leaves the literal list underneath untouched", () => {
    insight({ failureClusters: [cluster()], clusterModel: "bge-m3" });
    show();
    // The literal row for the same error is still its own row below.
    expect(screen.getAllByText(/== not found/).length).toBeGreaterThan(1);
  });

  it("renders the daemon's refusal rather than an empty grouping", () => {
    insight({
      failureClusters: [],
      clusterRefusal: "no embed_model is configured, so there is nothing to embed with",
    });
    show();
    expect(screen.getByText(/no embed_model is configured/)).toBeInTheDocument();
  });
});
