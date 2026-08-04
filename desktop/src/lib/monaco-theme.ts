/**
 * Monaco, wearing mogeung's palette.
 *
 * The syntax colours are the ones `theme.rs` already chose and tested — keeping
 * them means the Code pane and the diff read the same in both clients while
 * they run side by side, and it inherits `R-J6`'s contrast work rather than
 * inventing a second opinion about legibility.
 */

import type { Monaco } from "@monaco-editor/react";
import type { ThemeMode } from "@/store/prefs";

let defined = false;

export function defineMogeungThemes(monaco: Monaco): void {
  if (defined) return;
  defined = true;

  monaco.editor.defineTheme("mogeung-dark", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "C592E8" },
      { token: "string", foreground: "B6D78B" },
      { token: "comment", foreground: "777C88", fontStyle: "italic" },
      { token: "number", foreground: "E8B075" },
      { token: "type", foreground: "7EC0E0" },
      { token: "type.identifier", foreground: "7EC0E0" },
      { token: "delimiter", foreground: "A8A8B0" },
    ],
    colors: {
      "editor.background": "#1A1C20",
      "editor.foreground": "#D4D7DD",
      "editorLineNumber.foreground": "#5A5A60",
      "editorLineNumber.activeForeground": "#8A8A90",
      "editor.selectionBackground": "#2A4E78",
      "editor.lineHighlightBackground": "#22252A",
      "editorCursor.foreground": "#4C8EDA",
      "editorIndentGuide.background1": "#2C2F35",
      "editorGutter.background": "#1A1C20",
      "editorWidget.background": "#22252A",
      "editorWidget.border": "#33373E",
      "editorHoverWidget.background": "#22252A",
      "editorSuggestWidget.background": "#22252A",
      "minimap.background": "#1A1C20",
      "scrollbarSlider.background": "#43485288",
      "diffEditor.insertedTextBackground": "#143A2166",
      "diffEditor.removedTextBackground": "#451A1D66",
      "diffEditor.insertedLineBackground": "#143A2144",
      "diffEditor.removedLineBackground": "#451A1D44",
    },
  });

  monaco.editor.defineTheme("mogeung-light", {
    base: "vs",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "7B28A8" },
      { token: "string", foreground: "25621B" },
      { token: "comment", foreground: "6B707B", fontStyle: "italic" },
      { token: "number", foreground: "994E00" },
      { token: "type", foreground: "0E5E7E" },
      { token: "type.identifier", foreground: "0E5E7E" },
      { token: "delimiter", foreground: "3C4048" },
    ],
    colors: {
      "editor.background": "#F7F8FA",
      "editor.foreground": "#25282E",
      "editorLineNumber.foreground": "#999EA8",
      "editorLineNumber.activeForeground": "#676C76",
      "editor.selectionBackground": "#C5DCF5",
      "editor.lineHighlightBackground": "#E4E7EC",
      "editorCursor.foreground": "#1B64B0",
      "editorGutter.background": "#F7F8FA",
      "editorWidget.background": "#FFFFFF",
      "editorWidget.border": "#C6CBD4",
      "minimap.background": "#F7F8FA",
      "diffEditor.insertedTextBackground": "#DDF4E366",
      "diffEditor.removedTextBackground": "#FBDFE166",
      "diffEditor.insertedLineBackground": "#DDF4E344",
      "diffEditor.removedLineBackground": "#FBDFE144",
    },
  });
}

export function monacoTheme(mode: ThemeMode): string {
  const resolved =
    mode === "system"
      ? window.matchMedia("(prefers-color-scheme: light)").matches
        ? "light"
        : "dark"
      : mode;
  return resolved === "light" ? "mogeung-light" : "mogeung-dark";
}
