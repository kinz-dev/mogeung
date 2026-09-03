/**
 * Monaco, bundled — not fetched.
 *
 * `@monaco-editor/react` defaults to loading Monaco from a CDN at runtime. Two
 * things are wrong with that here, and the second is the one that matters:
 *
 * 1. **It is a version we never installed.** The loader pins its own default
 *    (0.55.1) regardless of the `monaco-editor` in `package.json`, so the
 *    lockfile describes a dependency the app does not actually run.
 * 2. **It is an outbound network call.** `architecture.md` says plainly that
 *    exactly one exists in this product — `git fetch`, on an explicit
 *    keystroke, admitted by
 *    [ADR-0014](../../../docs/decisions/0014-fetch-is-not-publishing.md).
 *    Everything else is localhost, the local filesystem, or the user's own LAN.
 *    A CDN fetch on every launch is a second one, in the client, silently, and
 *    it means no Code pane on a train.
 *
 * Importing this once, before the first render, points the loader at the copy
 * in `node_modules` instead. It costs bundle size, which a desktop application
 * does not care about.
 *
 * **The language workers, since `R-J88` (2026-09-03).** The comment that used
 * to sit below said the services were *"skipped to keep the bundle honest"*.
 * They were not skipped: `monaco-editor`'s main entry registers them, and the
 * one thing missing was the worker each one runs in — every label got the
 * base editor worker, which cannot load a language module and said so in the
 * console on every JSON file. So JSON, CSS, HTML and TypeScript now get their
 * own workers, which is what gives a `.json` file its squiggle on a stray
 * comma and a `.ts` file its syntax errors. Java, Python, SQL, YAML, XML and
 * the rest have **no** in-browser service to wire; for those, colouring and
 * folding from the Monarch grammars is all Monaco has, and anything more is a
 * language server — `R-J88`'s (c), and its own spec.
 */

import * as monaco from "monaco-editor";
import { loader } from "@monaco-editor/react";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import CssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker";
import HtmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker";
import TsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";
import { TS_DIAGNOSTICS, workerKind } from "@/lib/monaco-workers";

// `monaco-editor` declares `MonacoEnvironment` globally already, so this
// assigns to that declaration rather than making a second one.
(self as unknown as { MonacoEnvironment: { getWorker: (id: string, label: string) => Worker } }).MonacoEnvironment = {
  getWorker: (_id, label) => {
    switch (workerKind(label)) {
      case "json":
        return new JsonWorker();
      case "css":
        return new CssWorker();
      case "html":
        return new HtmlWorker();
      case "typescript":
        return new TsWorker();
      default:
        return new EditorWorker();
    }
  },
};

// A viewer with no project in front of it: syntax, never semantics. See
// `TS_DIAGNOSTICS` for why.
monaco.languages.typescript.typescriptDefaults.setDiagnosticsOptions(TS_DIAGNOSTICS);
monaco.languages.typescript.javascriptDefaults.setDiagnosticsOptions(TS_DIAGNOSTICS);

loader.config({ monaco });
