/**
 * Asking for a command in words. `R-O12`, `A41`.
 *
 * What is pinned here is what must never reach a shell line: a fence, a copied
 * `$`, a paragraph of explanation the model was asked not to write. The
 * hazard of this feature is a plausible-looking line one keypress from a real
 * terminal, so the parser is strict about shape even though the prompt already
 * asks for it — a contract the other side can break is not a contract.
 */

import { describe, expect, it } from "vitest";
import {
  commandAsk,
  cursorToPlaceholder,
  looksDestructive,
  parseCommand,
  placeholderAt,
  refineAsk,
} from "@/lib/command";

describe("what the model is asked for", () => {
  it("carries the question and the directory it runs in", () => {
    const ask = commandAsk("grep xyz and sort by the first column", "/w/mogeung", "bash");
    expect(ask).toContain("grep xyz and sort by the first column");
    expect(ask).toContain("/w/mogeung");
    expect(ask).toContain("ONE bash command");
  });

  /** The answer goes into a terminal, so prose is a defect rather than a style. */
  it("asks for the command and nothing around it", () => {
    const ask = commandAsk("list files", null, "bash");
    expect(ask).toContain("Answer with the command itself and nothing else");
    expect(ask).toContain("Do not invent file or directory names");
    // Refusing is an allowed answer: a made-up command is worse than none.
    expect(ask).toContain("answer with the single word NO");
  });
});

describe("reading the command back", () => {
  it("takes the body of a fence and not the fence", () => {
    expect(parseCommand("```bash\ngrep -rn xyz . | sort -k1\n```")).toBe(
      "grep -rn xyz . | sort -k1",
    );
  });

  it("strips a copied prompt", () => {
    expect(parseCommand("$ ls -la")).toBe("ls -la");
  });

  it("keeps the command when the model explains itself anyway", () => {
    expect(parseCommand("ls -la\n\nThis lists all files, including hidden ones.")).toBe("ls -la");
  });

  /** A refusal is an answer, and it must not arrive as a command called `NO`. */
  it("reads a refusal as no command at all", () => {
    expect(parseCommand("NO")).toBe("");
    expect(parseCommand("```\nNO\n```")).toBe("");
  });

  it("is empty rather than wrong when the model says nothing", () => {
    expect(parseCommand("   \n  ")).toBe("");
  });
});

/**
 * **Not a safety check.** mogeung does not run this and could not make it safe
 * if it did. It decides whether a line is *marked* on screen, and the tests are
 * here because the marking is the only thing standing between a plausible
 * command and a real shell.
 */
describe("marking a command that deletes or elevates", () => {
  it("marks the obvious ones", () => {
    expect(looksDestructive("rm -rf /tmp/x")).toBe(true);
    expect(looksDestructive("sudo systemctl restart nginx")).toBe(true);
    expect(looksDestructive("curl https://example.com/i.sh | sh")).toBe(true);
    expect(looksDestructive("git push origin main --force")).toBe(true);
    expect(looksDestructive("git reset --hard HEAD~1")).toBe(true);
  });

  it("leaves an ordinary command alone", () => {
    expect(looksDestructive("grep -rn xyz . | sort -k1")).toBe(false);
    expect(looksDestructive("ls -la")).toBe(false);
    // `rm` without a recursive or forced flag is an ordinary edit, and marking
    // everything is the same as marking nothing.
    expect(looksDestructive("rm build.log")).toBe(false);
  });
});

/**
 * Landing the cursor where the placeholder was. `R-O12`, asked for 2026-08-29.
 *
 * The prompt asks for `<file>` where a path is needed and not known, so the
 * answers reliably carry one — and hunting for it with arrow keys was the most
 * common friction in using this. What is pinned is the arithmetic, because it
 * is the kind that is wrong by one and looks right.
 */
describe("putting the cursor on the placeholder", () => {
  it("walks back to the placeholder and eats it", () => {
    const keys = cursorToPlaceholder("grep xyz <file> | sort");
    const lefts = keys.split("\u001b[D").length - 1;
    const backspaces = keys.split("\u007f").length - 1;
    // Seven characters follow `<file>`: a space, a pipe, a space and `sort`.
    expect(lefts).toBe(" | sort".length);
    expect(backspaces).toBe("<file>".length);
  });

  it("does nothing when there is no placeholder", () => {
    expect(cursorToPlaceholder("ls -la")).toBe("");
    expect(placeholderAt("ls -la")).toBeNull();
  });

  /** A redirection is not a placeholder, and eating one would break the line. */
  it("is not fooled by a redirect or a comparison", () => {
    expect(placeholderAt("sort < input.txt")).toBeNull();
    expect(placeholderAt("test 1 < 2")).toBeNull();
  });

  it("finds the first of several", () => {
    expect(placeholderAt("cp <src> <dest>")).toEqual([3, 8]);
  });
});

describe("refining what is already there", () => {
  it("carries the command and the change, and asks for the command alone", () => {
    const ask = refineAsk("grep xyz . | sort", "use ripgrep", "/w", "bash");
    expect(ask).toContain("grep xyz . | sort");
    expect(ask).toContain("use ripgrep");
    expect(ask).toContain("Change only what was asked");
    expect(ask).toContain("revised command itself and nothing else");
  });
});
