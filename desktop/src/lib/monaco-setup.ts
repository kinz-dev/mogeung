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
 */

import * as monaco from "monaco-editor";
import { loader } from "@monaco-editor/react";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";

// The base worker only. The language services — TypeScript, JSON, CSS — are
// what give completions and diagnostics, and this is a **viewer**: it colours
// and folds, it does not advise. Skipping them keeps the bundle honest about
// what the pane actually does.
// `monaco-editor` declares `MonacoEnvironment` globally already, so this
// assigns to that declaration rather than making a second one.
(self as unknown as { MonacoEnvironment: { getWorker: () => Worker } }).MonacoEnvironment = {
  getWorker: () => new EditorWorker(),
};

loader.config({ monaco });
