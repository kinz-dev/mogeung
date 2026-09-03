/**
 * The chords, and the one that must not become a chord.
 *
 * Every case here is a byte that either reaches the pty or does not, and the
 * expensive one to get wrong is `Ctrl+C`: a terminal that copies instead of
 * interrupting is one you cannot stop a runaway process in, and it fails only
 * when you most need it to work.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import {
  clipboardIntent,
  decodeOsc52,
  writeClipboard,
  type ClipboardIntent,
} from "@/lib/clipboard";

const shellWrite = vi.fn<(text: string) => Promise<void>>();
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: (text: string) => shellWrite(text),
}));

function key(init: Partial<KeyboardEvent> & { key: string }): KeyboardEvent {
  return {
    type: "keydown",
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ...init,
  } as KeyboardEvent;
}

function intentOf(init: Partial<KeyboardEvent> & { key: string }): ClipboardIntent {
  return clipboardIntent(key(init));
}

describe("what a chord means to a terminal", () => {
  it("leaves Ctrl+C to the pty, selection or no selection", () => {
    expect(intentOf({ key: "c", ctrlKey: true })).toBeNull();
    expect(intentOf({ key: "C", ctrlKey: true })).toBeNull();
  });

  it("copies on Ctrl+Shift+C and on Cmd+C", () => {
    // Shift makes the browser report the upper-case letter; a handler keyed on
    // "c" alone reads Ctrl+Shift+C as an unrelated key and does nothing.
    expect(intentOf({ key: "C", ctrlKey: true, shiftKey: true })).toBe("copy");
    expect(intentOf({ key: "c", metaKey: true })).toBe("copy");
    expect(intentOf({ key: "Insert", ctrlKey: true })).toBe("copy");
  });

  it("hands the bound paste chords to the webview rather than reading", () => {
    expect(intentOf({ key: "v", ctrlKey: true })).toBe("paste-native");
    expect(intentOf({ key: "v", metaKey: true })).toBe("paste-native");
    expect(intentOf({ key: "Insert", shiftKey: true })).toBe("paste-native");
  });

  it("reads the clipboard only for the chord nothing else is bound to", () => {
    expect(intentOf({ key: "V", ctrlKey: true, shiftKey: true })).toBe("paste-read");
  });

  it("ignores keyup, so one press is not two pastes", () => {
    expect(intentOf({ key: "v", ctrlKey: true, type: "keyup" })).toBeNull();
    expect(intentOf({ key: "C", ctrlKey: true, shiftKey: true, type: "keyup" })).toBeNull();
  });

  it("leaves everything else alone — the pty owns every other byte", () => {
    expect(intentOf({ key: "c" })).toBeNull();
    expect(intentOf({ key: "v" })).toBeNull();
    expect(intentOf({ key: "d", ctrlKey: true })).toBeNull();
    expect(intentOf({ key: "Enter", shiftKey: true })).toBeNull();
    // Alt is how tmux and readline reach half their bindings.
    expect(intentOf({ key: "v", ctrlKey: true, altKey: true })).toBeNull();
    expect(intentOf({ key: "c", ctrlKey: true, shiftKey: true, altKey: true })).toBeNull();
  });
});

describe("OSC 52 — the program asking for the clipboard itself", () => {
  const b64 = (s: string) => btoa(String.fromCharCode(...new TextEncoder().encode(s)));

  it("decodes a write, whatever targets it names", () => {
    expect(decodeOsc52(`c;${b64("hello")}`)).toEqual({ kind: "write", text: "hello" });
    expect(decodeOsc52(`;${b64("hello")}`)).toEqual({ kind: "write", text: "hello" });
    expect(decodeOsc52(`p;${b64("primary")}`)).toEqual({ kind: "write", text: "primary" });
  });

  it("decodes as UTF-8, not latin-1", () => {
    // A path with an accent in it is the ordinary case, and latin-1 turns it
    // into mojibake on the clipboard rather than failing loudly.
    expect(decodeOsc52(`c;${b64("café — ✓")}`)).toEqual({ kind: "write", text: "café — ✓" });
  });

  it("reports a read request as a read, so it can be refused rather than answered", () => {
    expect(decodeOsc52("c;?")).toEqual({ kind: "read" });
  });

  it("degrades on anything malformed rather than throwing inside the parser", () => {
    expect(decodeOsc52("no-semicolon")).toBeNull();
    expect(decodeOsc52("c;not!valid!base64")).toBeNull();
    expect(decodeOsc52(`c;${"A".repeat(5 * 1024 * 1024)}`)).toBeNull();
  });
});

/**
 * Where a write goes, and why it is not always the webview.
 *
 * The case that matters is the first one: it is what a mouse selection in a
 * pane with tmux's mouse on produced on 2026-09-03 — an `OSC 52` handled with
 * no gesture in flight, which WebKit refuses. A fix that only worked when a
 * hand was on the keyboard would pass every chord test above and still show
 * the popup on every drag.
 */
describe("writing to the clipboard from the shell", () => {
  const notAllowed = () =>
    Promise.reject(
      new Error(
        "NotAllowedError: The request is not allowed by the user agent or the platform in the current context",
      ),
    );

  function webview(writeText: ((t: string) => Promise<void>) | undefined) {
    Object.defineProperty(navigator, "clipboard", {
      value: writeText ? { writeText } : undefined,
      configurable: true,
    });
  }

  afterEach(() => {
    shellWrite.mockReset();
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    webview(undefined);
  });

  it("goes through the shell in the desktop window, where the webview would refuse a write with no gesture", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const webviewWrite = vi.fn(notAllowed);
    webview(webviewWrite);
    shellWrite.mockResolvedValue(undefined);

    await expect(writeClipboard("from tmux")).resolves.toBeUndefined();
    expect(shellWrite).toHaveBeenCalledWith("from tmux");
    expect(webviewWrite).not.toHaveBeenCalled();
  });

  it("falls back to the webview when the shell refuses — a build without the permission still copies on a chord", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const webviewWrite = vi.fn(() => Promise.resolve());
    webview(webviewWrite);
    shellWrite.mockRejectedValue(new Error("clipboard-manager.write_text not allowed"));

    await expect(writeClipboard("on a chord")).resolves.toBeUndefined();
    expect(webviewWrite).toHaveBeenCalledWith("on a chord");
  });

  it("reports the shell's refusal when both routes fail, since the shell is the one that should have worked", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    webview(notAllowed);
    shellWrite.mockRejectedValue(new Error("clipboard-manager.write_text not allowed"));

    await expect(writeClipboard("x")).rejects.toThrow("write_text not allowed");
  });

  it("uses the webview in a browser tab, which has no shell to ask", async () => {
    const webviewWrite = vi.fn(() => Promise.resolve());
    webview(webviewWrite);

    await writeClipboard("in a tab");
    expect(webviewWrite).toHaveBeenCalledWith("in a tab");
    expect(shellWrite).not.toHaveBeenCalled();
  });

  it("still rejects rather than resolving quietly when nothing can write", async () => {
    webview(undefined);
    await expect(writeClipboard("x")).rejects.toThrow("exposes no clipboard");
  });
});
