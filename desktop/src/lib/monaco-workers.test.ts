/**
 * `R-J88`: every label Monaco has a service for gets that service's worker,
 * and nothing else gets anything but the base one. The failure this guards
 * was silent — a console exception per JSON file, and no squiggle — so the
 * test is the only place the mapping is stated out loud.
 */

import { describe, expect, it } from "vitest";
import { TS_DIAGNOSTICS, workerKind } from "@/lib/monaco-workers";

describe("which worker a language gets", () => {
  it("hands each service its own worker", () => {
    expect(workerKind("json")).toBe("json");
    expect(workerKind("css")).toBe("css");
    expect(workerKind("scss")).toBe("css");
    expect(workerKind("less")).toBe("css");
    expect(workerKind("html")).toBe("html");
    expect(workerKind("handlebars")).toBe("html");
    expect(workerKind("typescript")).toBe("typescript");
    expect(workerKind("javascript")).toBe("typescript");
  });

  it("gives everything Monaco has no service for the base worker — Java included", () => {
    for (const l of ["java", "python", "sql", "yaml", "xml", "markdown", "rust", "plaintext", ""]) {
      expect(workerKind(l)).toBe("editor");
    }
  });

  it("lets TypeScript report syntax and never semantics — a worktree file has no project here", () => {
    expect(TS_DIAGNOSTICS.noSyntaxValidation).toBe(false);
    expect(TS_DIAGNOSTICS.noSemanticValidation).toBe(true);
  });
});
