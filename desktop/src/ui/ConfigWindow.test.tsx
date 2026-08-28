/**
 * The config editor's three refusals and one promise. `R-J79`.
 *
 * What is pinned here is not that a textarea renders. It is that every way
 * this can go wrong reaches the screen: a daemon that will not be edited, a
 * file that will not parse, and a save that has not happened yet. A settings
 * window that silently does nothing is worse than no settings window, because
 * you go away believing the setting is set.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { useStore, type ConfigFile } from "@/store";
import type { ClientMsg } from "@/wire/types";
import { ConfigWindow } from "./ConfigWindow";

const file = (over: Partial<ConfigFile> = {}): ConfigFile => ({
  path: "/home/you/.mogeung/config.toml",
  text: 'model_url = "http://127.0.0.1:8000/v1"\n',
  keys: ["listen", "model_url", "allow_remote_model"],
  readonly: null,
  error: null,
  savedAt: null,
  ...over,
});

const sent: ClientMsg[] = [];

/**
 * A message arriving from the daemon *after* the window is on screen.
 *
 * Wrapped in `act` because that is what it is: the store updates and React
 * re-renders and runs effects. Setting state bare leaves the effects unflushed
 * and the assertions reading the previous frame, which looks exactly like the
 * component ignoring the message.
 */
const arrives = (config: ConfigFile) => act(() => useStore.setState({ config }));

describe("the config editor", () => {
  beforeEach(() => {
    sent.length = 0;
    useStore.setState({ showConfig: true, config: null, send: (m) => void sent.push(m) });
  });

  it("asks the daemon for the file every time it opens", () => {
    render(<ConfigWindow />);
    expect(sent).toEqual([{ cmd: "config_get" }]);
  });

  it("shows the file and where it lives", () => {
    useStore.setState({ config: file() });
    render(<ConfigWindow />);
    expect(screen.getByRole("textbox")).toHaveValue('model_url = "http://127.0.0.1:8000/v1"\n');
    expect(screen.getByText("/home/you/.mogeung/config.toml")).toBeInTheDocument();
    // The daemon's own key list, which is the only discoverable one there is:
    // `deny_unknown_fields` makes a guess an error rather than a setting that
    // quietly does nothing.
    expect(screen.getByText("allow_remote_model")).toBeInTheDocument();
  });

  /**
   * Saving is disabled until something has changed, so the button is never a
   * way to write the file back over a change someone made in a terminal while
   * this was open.
   */
  it("only offers to save what you have actually changed", () => {
    useStore.setState({ config: file() });
    render(<ConfigWindow />);
    const save = screen.getByRole("button", { name: /save/i });
    expect(save).toBeDisabled();

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "poll_ms = 400\n" } });
    expect(save).toBeEnabled();
    expect(screen.getByText(/unsaved changes/i)).toBeInTheDocument();

    fireEvent.click(save);
    expect(sent).toContainEqual({ cmd: "config_save", text: "poll_ms = 400\n" });
  });

  /**
   * The parser's complaint, beside what you typed — **not** instead of it. A
   * refusal that also clears the box makes you retype a file to find the typo
   * in it.
   */
  it("keeps your text when the daemon refuses to parse it", () => {
    useStore.setState({ config: file() });
    render(<ConfigWindow />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "pol_ms = 400" } });
    arrives(file({ error: "unknown field `pol_ms`, expected one of ...", savedAt: null }));
    expect(screen.getByText(/unknown field/)).toBeInTheDocument();
    expect(screen.getByRole("textbox")).toHaveValue("pol_ms = 400");
  });

  /**
   * A daemon reachable beyond loopback holds outbound URLs somebody else could
   * aim. It says so, and the box goes read-only rather than accepting edits
   * that would be refused on arrival.
   */
  it("says why it is read-only rather than failing on save", () => {
    useStore.setState({
      config: file({ readonly: "this daemon is reachable beyond loopback, so its config is read-only here" }),
    });
    render(<ConfigWindow />);
    expect(screen.getByText(/read-only here/)).toBeInTheDocument();
    expect(screen.getByRole("textbox")).toHaveAttribute("readonly");
    expect(screen.getByRole("button", { name: /save/i })).toBeDisabled();
  });

  /**
   * After a save the editor shows what the daemon re-read, not what was sent —
   * so a file the daemon normalised, or one a save only half-applied, cannot
   * be reported by showing the text that did not land.
   */
  it("shows the file the daemon now has after a save", () => {
    useStore.setState({ config: file() });
    render(<ConfigWindow />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "poll_ms = 400" } });
    arrives(file({ text: "poll_ms = 400\n", savedAt: Date.now() }));
    expect(screen.getByRole("textbox")).toHaveValue("poll_ms = 400\n");
    expect(screen.getByText(/saved/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /save/i })).toBeDisabled();
  });
});
