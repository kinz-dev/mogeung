/**
 * The Insight views got a filter and two of them got a shape. `R-F10`, `R-F11`.
 *
 * `rank.test.ts` owns the matching. What these pin is the wiring — that the box
 * is actually there on the views that need one, that it cuts the list down and
 * says which engine did it, and that the chart is counts rather than the one
 * axis label ADR-0024 keeps off this view.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { InsightPane } from "@/panes/InsightPane";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";

// recharts measures its container, and in jsdom every box is zero by zero — so
// a ResponsiveContainer renders nothing at all. The chart's *content* is not
// what is under test here; that it is asked for, with the right numbers and no
// money in it, is.
vi.mock("recharts", async () => {
  const passthrough = ({ children }: { children?: React.ReactNode }) => <div>{children}</div>;
  return {
    ResponsiveContainer: passthrough,
    CartesianGrid: () => null,
    XAxis: () => null,
    YAxis: () => null,
    Tooltip: () => null,
    Area: () => null,
    AreaChart: passthrough,
    Bar: () => null,
    // The series rides on an **attribute**, not as text. As text it would be
    // found by every `getByText` in this file — the chart repeats the list's
    // own labels by design — and each assertion about a row would silently be
    // an assertion about the chart instead.
    BarChart: ({ data }: { data: { label: string; count: number }[] }) => (
      <div data-testid="bars" data-series={data.map((d) => `${d.label}=${d.count}`).join("|")} />
    ),
  };
});

const PROMPTS = [
  { representative: "run the tests again", count: 40, first_used: "2026-08-01T09:00:00Z", last_used: "2026-08-06T09:00:00Z" },
  { representative: "fix the egress path", count: 3, first_used: "2026-08-02T09:00:00Z", last_used: "2026-08-05T09:00:00Z" },
];

const FAILURES = [
  { example: "error: cannot borrow as mutable", count: 9, sessions: ["s1"] },
  { example: "warning: unused import", count: 2, sessions: ["s1"] },
];

beforeEach(() => {
  cleanup();
  useStore.setState({
    prefs: defaultPrefs(),
    selected: null,
    send: (() => {}) as never,
    insight: {
      ...useStore.getState().insight,
      prompts: PROMPTS as never,
      failures: FAILURES as never,
    },
  });
});

const show = (label: string) => {
  render(<InsightPane />);
  fireEvent.click(screen.getByText(label));
};

describe("filtering an Insight view", () => {
  it("offers a box on the Prompts view", () => {
    show("Prompts");
    expect(screen.getByLabelText("filter this view")).toBeInTheDocument();
  });

  it("cuts the list down and counts what is left", () => {
    show("Prompts");
    fireEvent.change(screen.getByLabelText("filter this view"), { target: { value: "egress" } });
    expect(screen.getByText("1/2")).toBeInTheDocument();
    expect(screen.getByText(/fix the egress path/)).toBeInTheDocument();
    expect(screen.queryByText(/run the tests again/)).toBeNull();
  });

  /** The rule the whole search module exists for. */
  it("names the engine that won", () => {
    show("Prompts");
    fireEvent.change(screen.getByLabelText("filter this view"), { target: { value: "egress" } });
    expect(screen.getByTitle(/which engine won/i)).toBeInTheDocument();
  });

  it("says nothing matches rather than nothing found", () => {
    show("Prompts");
    fireEvent.change(screen.getByLabelText("filter this view"), { target: { value: "zzzzz" } });
    expect(screen.getByText("nothing matches")).toBeInTheDocument();
  });

  it("clears on Escape", () => {
    show("Prompts");
    const box = screen.getByLabelText("filter this view");
    fireEvent.change(box, { target: { value: "egress" } });
    fireEvent.keyDown(box, { key: "Escape" });
    expect(screen.getByText(/run the tests again/)).toBeInTheDocument();
  });

  it("filters the Failures view by its error text", () => {
    show("Failures");
    fireEvent.change(screen.getByLabelText("filter this view"), { target: { value: "borrow" } });
    expect(screen.getByText("1/2")).toBeInTheDocument();
  });
});

describe("the shape of a counted list", () => {
  it("charts the prompts by count, most-repeated first", () => {
    show("Prompts");
    expect(screen.getByTestId("bars")).toHaveAttribute(
      "data-series",
      "run the tests again=40|fix the egress path=3",
    );
  });

  it("charts the failures the same way", () => {
    show("Failures");
    expect(screen.getByTestId("bars").getAttribute("data-series")).toContain(
      "error: cannot borrow as mutable=9",
    );
  });

  /**
   * ADR-0024, asserted rather than remembered. Dollars now exist in this
   * product — on the **Analytics** view, labelled and dated — which makes this
   * test more load-bearing rather than less: the rule it pins is that they do
   * not leak from there onto a frequency chart three lines away, where a money
   * axis would look perfectly reasonable in a screenshot.
   */
  it("never puts money on an axis", () => {
    show("Prompts");
    // Currency and units only. An earlier version of this grepped for the word
    // "cost" and failed on the chart's own hint — *"never by cost (ADR-0024)"* —
    // which is the disclaimer, not a violation. A test that cannot tell a rule
    // from its statement is a test that punishes writing the rule down.
    expect(document.body.textContent ?? "").not.toMatch(/[$£€]|\bUSD\b|\bdollars?\b/i);
    expect(screen.getByTestId("bars").getAttribute("data-series") ?? "").not.toMatch(/[$£€]/);
  });

  it("draws no chart at all when there is nothing counted", () => {
    useStore.setState({
      insight: { ...useStore.getState().insight, prompts: [] as never },
    });
    show("Prompts");
    expect(screen.queryByTestId("bars")).toBeNull();
  });
});
