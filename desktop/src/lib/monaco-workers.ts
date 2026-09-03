/**
 * Which worker a Monaco language asks for. `R-J88`.
 *
 * Monaco's bundle registers four language *services* — JSON, CSS, HTML and
 * TypeScript/JavaScript — and each runs in a worker of its own. The editor
 * asks `MonacoEnvironment.getWorker(_, label)` for one, by label, and until
 * 2026-09-03 this window answered every label with the **base** editor
 * worker. That is not "no services": the service is still registered, still
 * asks its worker to load the JSON module, and the base worker throws
 * `Cannot read properties of undefined (reading 'toUrl')` into the console
 * on every JSON file opened — found by opening one and reading the console,
 * which is the first thing the row said to do.
 *
 * Kept as a pure map so it can be tested without a worker or a DOM. The
 * labels are Monaco's own; anything it has no service for gets the base
 * worker, which is what colours and folds every other language.
 */

export type WorkerKind = "json" | "css" | "html" | "typescript" | "editor";

export function workerKind(label: string): WorkerKind {
  switch (label) {
    case "json":
      return "json";
    case "css":
    case "scss":
    case "less":
      return "css";
    case "html":
    case "handlebars":
    case "razor":
      return "html";
    case "typescript":
    case "javascript":
      return "typescript";
    default:
      return "editor";
  }
}

/**
 * What the TypeScript service is allowed to say about a file. `R-J88`.
 *
 * **Syntax only.** A worktree file is read here without its `tsconfig`, its
 * `node_modules` or the rest of its project, so semantic checking would
 * paint every import red and call it a finding — noise in a viewer, and
 * worse than noise beside an agent's diff. Syntax errors are real whatever
 * the project, so those stay. Suggestions (`unused`, `unreachable`) are the
 * project's business too.
 */
export const TS_DIAGNOSTICS = {
  noSemanticValidation: true,
  noSyntaxValidation: false,
  noSuggestionDiagnostics: true,
} as const;
