/**
 * The wire protocol, in TypeScript.
 *
 * A hand-written mirror of `crates/mogeung-core/src/wire.rs` and the types it
 * carries. Hand-written rather than generated on purpose: the protocol is the
 * contract this client is written against, and reading it should not require a
 * build step. `docs/design/wire-protocol.md` is the prose half.
 *
 * Two rules that decide every shape below:
 *
 * - Rust enums here are `#[serde(tag = "...", rename_all = "snake_case")]`, so
 *   they arrive as discriminated unions keyed on `cmd`, `ev`, `t` or `kind`.
 * - `DateTime<Utc>` is an RFC 3339 string. It is typed as `string` rather than
 *   `Date` because nothing should silently parse a timestamp it is only going
 *   to pass along.
 *
 * **Unknown fields are ignored and unknown variants must not throw.** The
 * daemon and the window upgrade separately — that is tested on the Rust side
 * (`a_snapshot_survives_a_version_gap_in_both_directions`) and the same
 * tolerance is this client's job at the other end.
 */

export type SessionId = string;
/** RFC 3339, always UTC. */
export type Timestamp = string;

// ---------------------------------------------------------------------------
// session.rs
// ---------------------------------------------------------------------------

export type SessionSource = "claude_code" | "codex";
export type LiveStatus = "busy" | "idle" | "unknown";

export interface OpenTool {
  id: string;
  name: string;
  summary: string;
  at: Timestamp;
}

export interface Touch {
  path: string;
  at: Timestamp;
}

export interface Collision {
  other: SessionId;
  other_label: string;
  path: string;
}

export type VerifyKind = "build" | "lint" | "typecheck" | "coverage" | "test";

export interface VerifyRun {
  kind: VerifyKind;
  command: string;
  at: Timestamp;
  ok?: boolean | null;
  tool_use_id?: string;
}

export interface Claim {
  text: string;
  at: Timestamp;
  kind: VerifyKind;
  evidence: string;
  contradicted?: boolean;
}

export interface FileCoverage {
  path: string;
  changed_covered: number;
  changed_missed: number;
}

export interface SignalRun {
  command: string;
  started_at: Timestamp;
  secs: number;
  exit: number | null;
  tail: string;
  coverage?: FileCoverage[];
}

export interface Session {
  id: SessionId;
  title: string | null;
  name: string | null;
  last_prompt: string | null;
  cwd: string;
  repo_root: string | null;
  git_branch: string | null;
  pid: number | null;
  alive: boolean;
  live_status: LiveStatus | null;
  version: string | null;
  started_at: Timestamp;
  last_event_at: Timestamp;
  status_since: Timestamp | null;
  turns: number;
  tool_calls: number;
  tokens_in: number;
  tokens_out: number;
  last_activity: string | null;
  touched_files: string[];
  base_sha: string | null;
  files_changed: number;
  insertions: number;
  deletions: number;
  error: string | null;
  transcript_path: string;
  reviewed: boolean;
  open_tools: OpenTool[];
  snoozed_until: Timestamp | null;
  collisions: Collision[];
  loop_signal: string | null;
  recent_touches: Touch[];
  tmux_target: string | null;
  recent_tools: string[];
  limit_hit_at: Timestamp | null;
  limit_resets: string | null;
  verify_runs: VerifyRun[];
  claims: Claim[];
  source: SessionSource;
}

// ---------------------------------------------------------------------------
// attention.rs
// ---------------------------------------------------------------------------

export type AttentionReason =
  | "idle"
  | "running"
  | "stalled"
  | "needs_review"
  | "rate_limited"
  | "failed"
  | "awaiting_input"
  | "awaiting_permission";

export interface AttentionItem {
  session_id: SessionId;
  reason: AttentionReason;
  score: number;
  detail: string;
}

// ---------------------------------------------------------------------------
// change.rs
// ---------------------------------------------------------------------------

export type RiskFlag =
  | "auth"
  | "secrets"
  | "migration"
  | "money"
  | "concurrency"
  | "error_handling"
  | "network_io"
  | "dependency"
  | "ci_config"
  | "infra"
  | "deleted_test"
  | "public_api"
  | "unsafe_code"
  | "large_deletion"
  | "noise";

export type RiskLevel = "noise" | "low" | "medium" | "high";
export type FileStatus = "added" | "modified" | "deleted" | "renamed";

export interface Hunk {
  anchor: string;
  header: string;
  lines: string[];
  insertions: number;
  deletions: number;
  flags: RiskFlag[];
  score: number;
  reviewed: boolean;
}

export interface FileChange {
  path: string;
  old_path: string | null;
  status: FileStatus;
  insertions: number;
  deletions: number;
  hunks: Hunk[];
  flags: RiskFlag[];
  score: number;
  truncated: boolean;
}

export interface Change {
  files: FileChange[];
  insertions: number;
  deletions: number;
  error: string | null;
}

// ---------------------------------------------------------------------------
// transcript.rs
// ---------------------------------------------------------------------------

export type NoticeLevel = "info" | "warn" | "error";

export type EventKind =
  | { t: "init"; model: string; cwd: string; tool_count: number }
  | { t: "user_prompt"; text: string }
  | { t: "assistant_text"; text: string }
  | { t: "thinking"; text: string }
  | { t: "tool_use"; tool_use_id: string; name: string; summary: string }
  | { t: "tool_result"; tool_use_id: string; is_error: boolean; preview: string }
  | {
      t: "result";
      cost_usd: number;
      num_turns: number;
      terminal_reason: string;
      is_error: boolean;
      text: string;
    }
  | { t: "notice"; level: NoticeLevel; message: string };

export interface TranscriptEvent {
  session_id: SessionId;
  seq: number;
  ts: Timestamp;
  kind: EventKind;
}

// ---------------------------------------------------------------------------
// health.rs
// ---------------------------------------------------------------------------

export type Alert =
  | { kind: "unknown_event_type"; event_type: string; count: number }
  | { kind: "malformed_lines"; count: number }
  | { kind: "version_changed"; from: string; to: string }
  | { kind: "history_skipped"; session_id: string; skipped_bytes: number };

export interface Health {
  last_scan: Timestamp | null;
  scans: number;
  sessions_known: number;
  sessions_live: number;
  transcripts_found: number;
  lines_seen: number;
  lines_parsed: number;
  lines_ignored: number;
  lines_barren: number;
  lines_unknown: number;
  lines_malformed: number;
  unknown_types: [string, number][];
  versions_seen: string[];
  current_version: string | null;
  history_skipped_bytes: number;
  max_transcript_bytes: number;
  alerts: Alert[];
  codex_present?: boolean;
  codex_threads?: number;
  codex_error?: string | null;
  codex_unknown?: [string, number][];
}

// ---------------------------------------------------------------------------
// review.rs
// ---------------------------------------------------------------------------

export interface DebtFile {
  path: string;
  session_id: string;
  unread_hunks: number;
  score: number;
}

export interface ReviewDebt {
  repo: string;
  sessions: number;
  sessions_unread: number;
  hunks_total: number;
  hunks_read: number;
  files_touched: number;
  worst_files: DebtFile[];
  unread_insertions: number;
}

export interface Reference {
  path: string;
  line: number;
  text: string;
  symbol: string;
  is_test: boolean;
}

export interface BlastRadius {
  session_id: string;
  path: string;
  symbols: string[];
  references: Reference[];
  truncated: boolean;
}

// ---------------------------------------------------------------------------
// usage.rs
// ---------------------------------------------------------------------------

/** Tokens split the way they are **priced**. `R-J21`. */
export interface TokenSplit {
  fresh_in: number;
  cache_read: number;
  cache_write_5m: number;
  cache_write_1h: number;
  out: number;
}

export interface ModelBurn {
  /** The id the transcript wrote, unprettified — it is the price-table key. */
  model: string;
  fast: boolean;
  tokens: TokenSplit;
  /** `null` means **no published rate**, which is not zero dollars. */
  cost_usd: number | null;
  messages: number;
}

export interface DayBurn {
  day: string;
  tokens_in: number;
  tokens_out: number;
  sessions: number;
  by_model: ModelBurn[];
  cost_usd: number;
}

export interface RepoBurn {
  repo: string;
  tokens_in: number;
  tokens_out: number;
  sessions: number;
}

export interface SessionBurn {
  session_id: string;
  repo: string;
  tokens_in: number;
  tokens_out: number;
}

export interface LimitHit {
  at: Timestamp;
  session_id: string;
  resets: string | null;
  window_tokens_out: number;
}

export interface UsageReport {
  days: DayBurn[];
  repos: RepoBurn[];
  sessions: SessionBurn[];
  window_tokens_in: number;
  window_tokens_out: number;
  limit_hits: LimitHit[];
  est_window_limit_out: number | null;
  models: ModelBurn[];
  cost_usd_total: number;
  cost_usd_today: number;
  /** Models with no rate: their tokens are counted, their dollars are not. */
  unpriced_models: string[];
  rates_as_of: string;
  generated_at: Timestamp | null;
  files_scanned: number;
  files_skipped: number;
}

// ---------------------------------------------------------------------------
// insight.rs
// ---------------------------------------------------------------------------

export type SearchSource = "transcript" | "history" | "prompt";

export interface SearchHit {
  source: SearchSource;
  session_id: string;
  line: number;
  timestamp: Timestamp | null;
  role: string;
  preview: string;
}

export interface SearchResults {
  hits: SearchHit[];
  truncated: boolean;
  files_scanned: number;
  files_truncated: number;
}

export interface SessionDigest {
  session_id: string;
  title: string | null;
  repo: string;
  turns: number;
  tool_calls: number;
  files_touched: string[];
  errors: number;
  tokens_in: number;
  tokens_out: number;
}

export interface DayDigest {
  day: string;
  sessions: SessionDigest[];
  files_scanned: number;
}

export interface RecurringFailure {
  normalized: string;
  example: string;
  sessions: string[];
  count: number;
}

export interface PromptCluster {
  representative: string;
  count: number;
  first_used: Timestamp;
  last_used: Timestamp;
  projects: string[];
}

export interface DayCount {
  day: string;
  count: number;
}

export interface ProjectCount {
  project: string;
  prompts: number;
  sessions: number;
  has_transcripts: boolean;
}

export interface Analytics {
  prompts_per_day: DayCount[];
  sessions_per_day: DayCount[];
  projects: ProjectCount[];
  /** Always 24 entries, midnight-first. */
  hour_histogram: number[];
}

export interface SubagentNode {
  agent_id: string;
  lines: number;
  first_ts: Timestamp | null;
  last_ts: Timestamp | null;
  last_activity: string | null;
  tokens_out: number;
}

export interface DecisionCandidate {
  line: number;
  timestamp: Timestamp | null;
  pattern: string;
  text: string;
}

export interface FileSession {
  session_id: string;
  label: string;
  prompt: string | null;
  at: Timestamp | null;
  matched: string;
}

// ---------------------------------------------------------------------------
// docs.rs
// ---------------------------------------------------------------------------

export type DocKind =
  | "readme"
  | "changelog"
  | "adr"
  | "spec"
  | "plan"
  | "note"
  | "generated"
  | "instruction"
  | "unknown";

export type GcAction = "archive" | "merge" | "review";

export interface DocInfo {
  path: string;
  kind: DocKind;
  evidence: string;
  title: string | null;
  status: string | null;
  updated: string | null;
  created_epoch: number | null;
  edited_epoch: number | null;
  dates_from_git: boolean;
  bytes: number;
  links_out: string[];
  links_in: number;
  orphan: boolean;
}

export interface StaleDoc {
  doc: string;
  referenced_path: string;
  doc_edited_epoch: number;
  path_edited_epoch: number;
  commits_since: number;
}

export interface GcCandidate {
  path: string;
  action: GcAction;
  evidence: string;
}

export interface PlanItem {
  text: string;
  checked: boolean;
  paths: string[];
  verifiable: boolean;
  claimed_unevidenced: boolean;
}

export interface PlanCheck {
  doc: string;
  checked: number;
  unchecked: number;
  items: PlanItem[];
}

export interface InstructionFile {
  path: string;
  bytes: number;
  edited_epoch: number | null;
  distinct_lines: number;
}

export interface InstructionPair {
  a: string;
  b: string;
  shared_lines: number;
  shared_ratio: number;
  only_in_a_total: number;
  only_in_b_total: number;
  only_in_a: string[];
  only_in_b: string[];
}

export interface InstructionDrift {
  files: InstructionFile[];
  pairs: InstructionPair[];
}

export interface DocInventory {
  docs: DocInfo[];
  stale: StaleDoc[];
  gc: GcCandidate[];
  plans: PlanCheck[];
  instruction_drift: InstructionDrift | null;
  truncated: boolean;
  generated_at: Timestamp | null;
}

// ---------------------------------------------------------------------------
// wire.rs — the payload types
// ---------------------------------------------------------------------------

export interface DirEntry {
  name: string;
  is_dir: boolean;
}

export interface CommitInfo {
  sha: string;
  short: string;
  author: string;
  epoch: number;
  summary: string;
  refs: string[];
  parents: string[];
  touches_session: boolean;
}

export interface CommitDetail {
  author: string;
  committer: string;
  epoch: number;
  commit_epoch: number;
  parents: string[];
  refs: string[];
  message: string;
  branches: string[];
}

export interface StatusEntry {
  path: string;
  staged: boolean;
  unstaged: boolean;
  /** Raw porcelain `XY`: `M `, ` M`, `??`, `A `, `!!` for ignored. */
  state: string;
  conflicted: boolean;
}

export interface BlameLine {
  sha: string;
  author: string;
  epoch: number;
  summary: string;
}

export interface BranchInfo {
  name: string;
  sha: string;
  current: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
  epoch: number;
}

export interface TagInfo {
  name: string;
  sha: string;
  epoch: number;
}

export interface RemoteInfo {
  name: string;
  url: string;
}

export interface RefsInfo {
  head: string | null;
  head_sha: string;
  branches: BranchInfo[];
  tags: TagInfo[];
  remotes: RemoteInfo[];
  fetch_epoch: number | null;
  remote_branches: BranchInfo[];
}

export interface ReflogEntry {
  sha: string;
  selector: string;
  summary: string;
}

export interface WorktreeInfo {
  path: string;
  sha: string;
  branch: string | null;
}

export interface StashInfo {
  index: number;
  message: string;
  epoch: number;
}

export interface SubmoduleInfo {
  path: string;
  sha: string;
  state: string;
  note: string;
}

/** What Claude Code has been *told* — memory it saved, skills it can run. */
export type KitKind = "memory" | "skill";
export type KitScope = "user" | "plugin" | "project";

export interface KitEntry {
  kind: KitKind;
  scope: KitScope;
  name: string;
  description: string;
  /** For a memory, the project it belongs to. Decoded, and lossy — a label. */
  project?: string | null;
  path: string;
  bytes: number;
  modified?: number | null;
}

export interface KitDoc {
  path: string;
  body: string;
  truncated: boolean;
}

export interface ContentMatch {
  path: string;
  line: number;
  text: string;
}

export interface Note {
  id: string;
  /** Markdown. Empty is legal and means a plain bookmark. */
  body: string;
  created: number;
  updated: number;
  session_id?: SessionId | null;
  seq?: number | null;
  repo?: string | null;
}

export type ResolveSide = "ours" | "theirs" | "mine";

export interface DaemonIdentity {
  machine_id?: string | null;
  host?: string | null;
  claude_home: string;
  pid: number;
  version: string;
  ssh_target?: string | null;
}

// ---------------------------------------------------------------------------
// wire.rs — ClientMsg
// ---------------------------------------------------------------------------

export type ClientMsg =
  | { cmd: "subscribe" }
  | { cmd: "set_hunk_reviewed"; session_id: SessionId; anchor: string; reviewed: boolean }
  | { cmd: "review_all"; session_id: SessionId }
  | { cmd: "refresh_change"; session_id: SessionId }
  | { cmd: "fetch_events"; session_id: SessionId; since: number }
  | { cmd: "forget_session"; session_id: SessionId }
  | { cmd: "launch_terminal"; dir: string; worktree: boolean }
  | { cmd: "rescan" }
  | { cmd: "fetch_health" }
  | { cmd: "snooze"; session_id: SessionId; minutes: number }
  | { cmd: "fetch_review_debt"; repo: string }
  | { cmd: "fetch_blast_radius"; session_id: SessionId; path: string }
  | { cmd: "focus_terminal"; session_id: SessionId }
  | { cmd: "list_dir"; session_id: SessionId; path: string }
  | { cmd: "fetch_file"; session_id: SessionId; path: string }
  | { cmd: "list_tree"; session_id: SessionId }
  | { cmd: "search_content"; session_id: SessionId; query: string }
  | {
      cmd: "git_log";
      session_id: SessionId;
      skip: number;
      limit: number;
      rev?: string | null;
      grep?: string | null;
      author?: string | null;
      path?: string | null;
      pickaxe?: string | null;
    }
  | { cmd: "git_show"; session_id: SessionId; sha: string; context?: number | null; ignore_ws?: boolean | null }
  | { cmd: "git_status"; session_id: SessionId }
  // -- the write family (R-D19). Present on the wire; this client sends only
  //    what the panes expose, and the Code pane exposes nothing.
  | { cmd: "git_stage"; session_id: SessionId; paths: string[] }
  | { cmd: "git_unstage"; session_id: SessionId; paths: string[] }
  | { cmd: "git_discard"; session_id: SessionId; paths: string[] }
  | { cmd: "git_commit"; session_id: SessionId; message: string; amend: boolean; session_trailer: boolean }
  | { cmd: "git_branch_create"; session_id: SessionId; name: string; switch_to: boolean }
  | { cmd: "git_switch"; session_id: SessionId; name: string }
  | { cmd: "git_stash_push"; session_id: SessionId; message: string; include_untracked: boolean }
  | { cmd: "git_stash_pop"; session_id: SessionId; index: number }
  | { cmd: "git_stash_drop"; session_id: SessionId; index: number }
  | { cmd: "git_fetch"; session_id: SessionId }
  | { cmd: "fetch_kit" }
  | { cmd: "fetch_kit_doc"; path: string }
  | { cmd: "note_list" }
  | {
      cmd: "note_save";
      id: string;
      body: string;
      session_id?: SessionId | null;
      seq?: number | null;
      repo?: string | null;
    }
  | { cmd: "note_delete"; id: string }
  | { cmd: "git_resolve"; session_id: SessionId; path: string; side: ResolveSide }
  | { cmd: "git_diff_file"; session_id: SessionId; path: string; context?: number | null; ignore_ws?: boolean | null }
  | { cmd: "git_blame"; session_id: SessionId; path: string; rev?: string | null }
  | { cmd: "git_refs"; session_id: SessionId }
  | { cmd: "git_stashes"; session_id: SessionId }
  | { cmd: "git_stash_show"; session_id: SessionId; index: number; context?: number | null; ignore_ws?: boolean | null }
  | { cmd: "git_submodules"; session_id: SessionId }
  | {
      cmd: "git_diff_range";
      session_id: SessionId;
      from: string;
      to: string;
      context?: number | null;
      ignore_ws?: boolean | null;
    }
  | { cmd: "git_compare"; session_id: SessionId; branch: string }
  | { cmd: "git_reflog"; session_id: SessionId }
  | { cmd: "git_worktrees"; session_id: SessionId }
  | { cmd: "git_conflict_file"; session_id: SessionId; path: string }
  | { cmd: "git_file_at_rev"; session_id: SessionId; sha: string; path: string }
  | { cmd: "fetch_usage" }
  | { cmd: "set_signal_command"; repo: string; command: string }
  | { cmd: "run_signal"; session_id: SessionId }
  | { cmd: "fetch_signal"; repo: string }
  | { cmd: "insight_search"; query: string }
  | { cmd: "fetch_digest"; day: string }
  | { cmd: "fetch_recurring" }
  | { cmd: "fetch_prompt_library" }
  | { cmd: "fetch_analytics" }
  | { cmd: "fetch_subagents"; session_id: SessionId }
  | { cmd: "fetch_decisions"; session_id: SessionId }
  | { cmd: "fetch_file_sessions"; path: string }
  | { cmd: "fetch_doc_scan"; repo: string };

// ---------------------------------------------------------------------------
// wire.rs — ServerMsg
// ---------------------------------------------------------------------------

export type ServerMsg =
  | { ev: "snapshot"; sessions: Session[]; queue: AttentionItem[]; daemon?: DaemonIdentity | null }
  | { ev: "session_updated"; session: Session }
  | { ev: "session_removed"; session_id: SessionId }
  | { ev: "events"; events: TranscriptEvent[] }
  | { ev: "queue"; queue: AttentionItem[] }
  | { ev: "change_updated"; session_id: SessionId; change: Change }
  | { ev: "health"; health: Health }
  | { ev: "review_debt"; debt: ReviewDebt }
  | { ev: "blast_radius"; radius: BlastRadius }
  | { ev: "dir_listing"; session_id: SessionId; path: string; entries: DirEntry[] }
  | { ev: "file_content"; session_id: SessionId; path: string; content: string; truncated: boolean }
  | { ev: "tree_listing"; session_id: SessionId; paths: string[]; truncated: boolean }
  | {
      ev: "content_matches";
      session_id: SessionId;
      query: string;
      matches: ContentMatch[];
      truncated: boolean;
    }
  | {
      ev: "git_commits";
      session_id: SessionId;
      skip: number;
      commits: CommitInfo[];
      done: boolean;
      rev?: string | null;
      grep?: string | null;
      author?: string | null;
      path?: string | null;
      pickaxe?: string | null;
    }
  | {
      ev: "git_commit_diff";
      session_id: SessionId;
      sha: string;
      files: FileChange[];
      detail?: CommitDetail | null;
      context?: number | null;
      ignore_ws?: boolean | null;
    }
  | { ev: "git_local_changes"; session_id: SessionId; entries: StatusEntry[] }
  | {
      ev: "git_file_diff";
      session_id: SessionId;
      path: string;
      files: FileChange[];
      context?: number | null;
      ignore_ws?: boolean | null;
    }
  | {
      ev: "git_annotation";
      session_id: SessionId;
      path: string;
      lines: BlameLine[];
      truncated: boolean;
      rev?: string | null;
    }
  | { ev: "git_refs_info"; session_id: SessionId; info: RefsInfo }
  | {
      ev: "git_fetched";
      session_id: SessionId;
      updates: string[];
      upstream: string | null;
      ahead: number;
      behind: number;
    }
  | { ev: "kit"; entries: KitEntry[] }
  | { ev: "kit_doc"; doc: KitDoc }
  | { ev: "notes"; notes: Note[] }
  | { ev: "git_stash_list"; session_id: SessionId; stashes: StashInfo[] }
  | {
      ev: "git_stash_diff";
      session_id: SessionId;
      index: number;
      files: FileChange[];
      context?: number | null;
      ignore_ws?: boolean | null;
    }
  | { ev: "git_submodule_list"; session_id: SessionId; submodules: SubmoduleInfo[] }
  | {
      ev: "git_range_diff";
      session_id: SessionId;
      from: string;
      to: string;
      files: FileChange[];
      context?: number | null;
      ignore_ws?: boolean | null;
    }
  | {
      ev: "git_file_at_rev_content";
      session_id: SessionId;
      sha: string;
      path: string;
      content: string;
      truncated: boolean;
    }
  | { ev: "git_reflog_list"; session_id: SessionId; entries: ReflogEntry[] }
  | { ev: "git_worktree_list"; session_id: SessionId; worktrees: WorktreeInfo[] }
  | {
      ev: "git_conflict_stages";
      session_id: SessionId;
      path: string;
      base: string;
      ours: string;
      theirs: string;
      truncated: boolean;
    }
  | { ev: "usage_stats"; report: UsageReport }
  | { ev: "signal_status"; repo: string; command: string | null; running: boolean; last: SignalRun | null }
  | { ev: "insight_search_results"; query: string; results: SearchResults }
  | { ev: "day_digest_report"; day: string; digest: DayDigest }
  | { ev: "recurring_failures"; failures: RecurringFailure[] }
  | { ev: "prompt_library"; clusters: PromptCluster[] }
  | { ev: "analytics_report"; analytics: Analytics }
  | { ev: "subagent_tree_report"; session_id: SessionId; nodes: SubagentNode[] }
  | { ev: "decision_report"; session_id: SessionId; candidates: DecisionCandidate[] }
  | { ev: "file_sessions"; path: string; entries: FileSession[] }
  | { ev: "doc_report"; repo: string; inventory: DocInventory }
  | { ev: "error"; message: string };

// ---------------------------------------------------------------------------
// Derived helpers — the Rust `impl` blocks this client actually needs.
// ---------------------------------------------------------------------------

/** `AttentionReason::needs_human`. The queue's whole point. */
export function needsHuman(reason: AttentionReason): boolean {
  switch (reason) {
    case "awaiting_permission":
    case "awaiting_input":
    case "failed":
    case "rate_limited":
    case "needs_review":
    case "stalled":
      return true;
    default:
      return false;
  }
}

/** `AttentionReason::label`. Upper-case ones are the ones that want you. */
export function reasonLabel(reason: AttentionReason): string {
  const map: Record<AttentionReason, string> = {
    awaiting_permission: "APPROVE",
    awaiting_input: "WAITING",
    failed: "FAILED",
    rate_limited: "LIMIT",
    needs_review: "REVIEW",
    stalled: "STALLED",
    running: "running",
    idle: "idle",
  };
  return map[reason];
}

/** `RiskLevel::from_score`. */
export function riskFromScore(score: number): RiskLevel {
  if (score < 0) return "noise";
  if (score <= 29) return "low";
  if (score <= 69) return "medium";
  return "high";
}

/** `Session::label` — the fallback chain, in order. */
export function sessionLabel(s: Session): string {
  return s.title || s.last_prompt || s.name || `session ${s.id.slice(0, 8)}`;
}

/**
 * `Session::unverified` — it edited files and nothing it ran ever reported a
 * result. Deliberately "no completed check", not "no passing check": a session
 * whose tests failed has been verified, and badly.
 */
export function unverified(s: Session): boolean {
  return s.touched_files.length > 0 && !s.verify_runs.some((r) => r.ok !== null);
}

/** `Session::repo_name`. */
export function repoName(s: Session): string {
  const p = s.repo_root || s.cwd;
  const parts = p.split("/");
  return parts[parts.length - 1] || p;
}

/**
 * `Session::awaiting_permission` — a live, idle session with an unanswered
 * tool call is sitting on a permission prompt, not waiting for a new
 * instruction. `R-B4`, and the distinction the registry cannot make.
 */
export function awaitingPermission(s: Session): OpenTool | null {
  if (s.alive && s.live_status === "idle" && s.open_tools.length > 0) {
    return s.open_tools[s.open_tools.length - 1];
  }
  return null;
}

/** `DaemonIdentity::is_same_machine_as`. Unknown on either side is *not* a match. */
export function isSameMachine(d: DaemonIdentity | null, local: string | null): boolean {
  if (!d?.machine_id || !local) return false;
  return d.machine_id === local;
}

/** `attention::fmt_dur`. */
export function fmtDur(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m${String(secs % 60).padStart(2, "0")}s`;
  return `${Math.floor(secs / 3600)}h${String(Math.floor((secs % 3600) / 60)).padStart(2, "0")}m`;
}

/** `health::human_bytes`. */
export function humanBytes(n: number): string {
  const K = 1024;
  if (n < K) return `${n} B`;
  if (n < K * K) return `${Math.round(n / K)} KB`;
  if (n < K * K * K) return `${(n / (K * K)).toFixed(1)} MB`;
  return `${(n / (K * K * K)).toFixed(1)} GB`;
}

/** The searchable text of one turn. `R-B36` — mirrors `app.rs::event_text`. */
export function eventText(kind: EventKind): string {
  switch (kind.t) {
    case "user_prompt":
    case "assistant_text":
    case "thinking":
      return kind.text;
    case "tool_use":
      return `${kind.name} ${kind.summary}`;
    case "tool_result":
      return kind.preview;
    case "result":
      return kind.text;
    case "notice":
      return kind.message;
    case "init":
      return `${kind.model} ${kind.cwd}`;
    default:
      return "";
  }
}

/** `EventKind::label`. */
export function eventLabel(kind: EventKind): string {
  const map: Record<EventKind["t"], string> = {
    init: "init",
    user_prompt: "prompt",
    assistant_text: "agent",
    thinking: "think",
    tool_use: "tool",
    tool_result: "result",
    result: "done",
    notice: "notice",
  };
  return map[kind.t] ?? kind.t;
}
