/**
 * The other agent CLIs mogeung is watching, as the Health window shows them.
 *
 * A daemon at `R-I15` or later sends `agents`, one entry per non-Claude CLI.
 * An older one sends the four flat `codex_*` fields and nothing else, so this
 * reconstructs a single Codex entry from them. Both shapes exist on the wire at
 * once — the daemon still fills the old fields — and the client must not show a
 * Codex install twice because of it.
 */

import { SOURCE_COLOR, type AgentHealth, type Health, type SessionSource } from "@/wire/types";

/// `AgentHealth.source` is the `SessionSource`'s *label* (`codex`, `qwen`),
/// not its wire variant (`codex`, `qwen_code`) — the daemon sends
/// `SessionSource::label()`. Mapping back keeps one colour per CLI across the
/// Health window and the session chips, instead of two tables drifting apart.
const BY_LABEL: Record<string, SessionSource> = {
  claude: "claude_code",
  codex: "codex",
  qwen: "qwen_code",
};

/// The colour for a health slot. Falls back rather than throwing: the daemon
/// may be newer than this client and name a CLI it has never heard of.
export function agentColor(label: string): string {
  const wire = BY_LABEL[label];
  return (wire && SOURCE_COLOR[wire]) ?? "var(--fg-dim)";
}

/// Every watched non-Claude CLI, newest wire shape first.
///
/// Only falls back when `agents` is absent, not when it is empty: an empty list
/// from a new daemon is a real answer (no other CLI is installed), and reading
/// the deprecated fields over the top of it would resurrect a Codex chip the
/// daemon just told us not to show.
export function agentSlots(health: Health): AgentHealth[] {
  if (health.agents !== undefined) return health.agents;
  if (!health.codex_present) return [];
  return [
    {
      source: "codex",
      present: true,
      threads: health.codex_threads ?? 0,
      error: health.codex_error ?? null,
      unknown: health.codex_unknown ?? [],
    },
  ];
}
