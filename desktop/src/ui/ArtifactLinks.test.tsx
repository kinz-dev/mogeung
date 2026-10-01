/**
 * A session's artifacts as links in the pane header. `R-J115`.
 *
 * What can go wrong here is the choice of *which* page a click opens: the
 * daemon sends newest first, and a header that showed the list's last entry,
 * or opened a different one from the title it shows, would take you to an
 * older draft while reading as the current one.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ArtifactLinks } from "@/ui/ArtifactLinks";
import type { Artifact } from "@/wire/types";

const opened = vi.hoisted(() => [] as string[]);
vi.mock("@/lib/tauri", async (orig) => ({
  ...(await orig<typeof import("@/lib/tauri")>()),
  openArtifact: vi.fn(async (url: string) => void opened.push(url)),
}));

const newest: Artifact = { id: "n", title: "The map", url: "https://claude.ai/artifact/VzCr55rMjnsNhymTNAA12P" };
const older: Artifact = { id: "o", title: "First draft", url: "https://claude.ai/artifact/9RrtdqMgCWL5qa1DYUCbo6" };

afterEach(() => {
  cleanup();
  opened.length = 0;
});

describe("ArtifactLinks", () => {
  it("says nothing for a session that published nothing", () => {
    const { container } = render(<ArtifactLinks artifacts={undefined} />);
    expect(container.innerHTML).toBe("");
    render(<ArtifactLinks artifacts={[]} />);
    expect(container.innerHTML).toBe("");
  });

  it("opens the one artifact it names", () => {
    render(<ArtifactLinks artifacts={[newest]} />);
    fireEvent.click(screen.getByText("The map"));
    expect(opened).toEqual([newest.url]);
  });

  it("names the newest of several and counts the rest", () => {
    render(<ArtifactLinks artifacts={[newest, older]} />);
    expect(screen.getByText("The map")).toBeTruthy();
    expect(screen.getByText("+1")).toBeTruthy();
    expect(screen.queryByText("First draft")).toBeNull();
  });
});
