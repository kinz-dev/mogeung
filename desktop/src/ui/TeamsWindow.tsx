/**
 * Teams: the definitions, edited here first. `R-P3`, feature 0051, pillar P.
 *
 * *"UI come first for the team config, later I can import/export the
 * config"* (2026-10-09) — so this window is where a team is written, and the
 * file comes after (`R-P12`). A team is daemon state like a note
 * ([ADR-0015](../../../docs/decisions/0015-markdown-is-the-truth.md), and
 * [ADR-0043](../../../docs/decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md)
 * clause 8 for a definition in terms): the daemon keeps it under `~/.mogeung`,
 * mirrors it one way as markdown, and answers every change with the whole
 * set, so `teams` here is a cache of theirs and a second window agrees.
 *
 * **Two layers**, decided the same day: the team's guidance and default
 * permission mode, then each seat's own guidance added after and its mode as
 * an override. The form is laid out in that order so the shared part is
 * written once, and a seat's override reads as one.
 *
 * **Policy is declared text.** A wake and a watchdog are sentences you can
 * read here before they ever fire — fence 3 of ADR-0003's amendment. The hop
 * budget and the hourly cap carry the owner's defaults, three and sixty.
 *
 * **What is grey, and why it says so.** Save is grey with the daemon's own
 * reason while the definition could not be addressed (`lib/teams.ts` mirrors
 * `team::validate`), and grey with a different sentence when the daemon said
 * this connection may not write one — a window dialled in from another
 * machine, on a daemon started without `--allow-run` (ADR-0025's amendment,
 * clause 4). Boot and Stop are drawn and disabled: they are `R-P4`'s, and a
 * control you can see but not press says what is missing better than a gap.
 *
 * **Sends, live** (`R-P6`). Below the policy a saved team shows what the
 * policy is doing now — the chain against the hop budget, the hour against
 * the cap — with the pause switch, which acts at once through `team_pause`
 * rather than through Save (so a form left open cannot un-pause a team on
 * its way through), every send the policy is holding with *release* and
 * *drop* beside it, and the log of what was sent, to whom, why and what
 * became of it. Fence 4 of ADR-0003's amendment, on screen.
 */

import { useEffect, useState } from "react";
import { Ban, Check, Download, Eraser, FolderOpen, Pause, Play, Plus, Save, Square, Trash2, Upload, X } from "lucide-react";
import { useStore } from "@/store";
import { chooseFile, chooseFolder, chooseSavePath } from "@/lib/tauri";
import {
  countersLine,
  firstProblem,
  freeSeatName,
  newSeat,
  newTeam,
  permissionFor,
  seatSignals,
  sendLine,
  sendsFor,
  signalsLine,
  teamSummary,
} from "@/lib/teams";
import { Dialog } from "@/ui/Dialog";
import { Button, Checkbox, Dim, IconButton, Input, Mono, Segmented } from "@/ui/primitives";
import { SourceMark } from "@/ui/SourceMark";
import { interactive } from "@/ui/styles";
import {
  PERMISSION_LABEL,
  PERMISSION_MODES,
  sourceLabel,
  type Handover,
  type PermissionMode,
  type Seat,
  type SeatEvent,
  type SessionSource,
  type Team,
  type Wake,
  type Watchdog,
} from "@/wire/types";

const CLIS: readonly SessionSource[] = ["claude_code", "codex", "qwen_code", "opencode"];

const HANDOVERS: readonly { value: Handover; label: string; title: string }[] = [
  { value: "after_read", label: "after you read it", title: "composed and shown on the seat's row; delivered once you have read it" },
  { value: "automatic", label: "deliver automatically", title: "composed and delivered as an automatic send, counted against the cap" },
  { value: "none", label: "none", title: "the seat restarts cold, and the row says so" },
];

const TEXTAREA =
  "w-full resize-y rounded border border-[var(--line)] bg-[var(--bg)] p-1.5 font-mono text-2xs outline-none focus-visible:border-[var(--ring)]";
const SELECT =
  "rounded border border-[var(--line)] bg-[var(--bg)] px-1 py-0.5 font-mono text-2xs outline-none focus-visible:border-[var(--ring)]";

/** A whole number, typed as text: the daemon refuses what does not parse,
 *  and a box that eats a keystroke is worse than one that shows it. */
function NumberField({ label, value, onChange, title }: { label: string; value: number; onChange: (n: number) => void; title?: string }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  return (
    <span className="flex items-center gap-1" title={title}>
      <Dim className="text-2xs">{label}</Dim>
      <Input
        value={text}
        mono
        ariaLabel={label}
        className="w-16"
        onChange={(v) => {
          setText(v);
          const n = Number.parseInt(v, 10);
          if (Number.isFinite(n) && n >= 0) onChange(n);
        }}
      />
    </span>
  );
}

function SeatPicker({
  label,
  value,
  seats,
  any,
  onChange,
}: {
  label: string;
  value: string;
  seats: Seat[];
  /** Offer "any seat" as the empty choice. */
  any?: boolean;
  onChange: (name: string) => void;
}) {
  return (
    <select aria-label={label} className={SELECT} value={value} onChange={(e) => onChange(e.target.value)}>
      {any && <option value="">any seat</option>}
      {!any && !seats.some((s) => s.name === value) && <option value={value}>{value || "—"}</option>}
      {seats.map((s) => (
        <option key={s.name} value={s.name}>
          {s.name}
        </option>
      ))}
    </select>
  );
}

export function TeamsWindow() {
  const open = useStore((s) => s.showTeams);
  const teams = useStore((s) => s.teams);
  const allowed = useStore((s) => s.teamsAllowed);
  const launches = useStore((s) => s.seatLaunches);
  const allSends = useStore((s) => s.teamSends);
  const send = useStore((s) => s.send);
  // The window of its own this opens in (`R-J128`), the size it was left at.
  const teamsSize = useStore((s) => s.prefs.teamsSize);
  const setPrefs = useStore((s) => s.setPrefs);
  // What is being edited, and what it was when it arrived. `base` is `null`
  // for a team that does not exist yet; the two differ when there is
  // something to save.
  const [draft, setDraft] = useState<Team | null>(null);
  const [base, setBase] = useState<Team | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  // What an import had to say about the draft it made (`R-P12`); shown
  // until the draft is saved or replaced.
  const [importNotes, setImportNotes] = useState<string[]>([]);
  const incoming = useStore((s) => s.teamDraft);

  // A file came in: it becomes the draft, unsaved, with its notes. Taken
  // from the store so a second import replaces the first.
  useEffect(() => {
    if (!incoming) return;
    setDraft(structuredClone(incoming.team));
    setBase(null);
    setConfirmDelete(false);
    setImportNotes(incoming.notes);
    useStore.setState({ teamDraft: null });
  }, [incoming]);

  const dirty = draft !== null && JSON.stringify(draft) !== JSON.stringify(base);

  // Ask on open, every time: a reconnect is a new daemon's worth of teams.
  useEffect(() => {
    if (open) send({ cmd: "team_list" });
  }, [open, send]);

  // Follow the daemon's set. A new, unsaved draft is left alone; a picked team
  // that arrived changed replaces the draft unless you are mid-edit; a team
  // that vanished falls to the first one, or to nothing.
  useEffect(() => {
    if (!open) return;
    if (draft !== null && draft.id === "") return;
    const current = teams.find((t) => t.id === draft?.id);
    if (!current) {
      const first = teams[0] ?? null;
      setDraft(first ? structuredClone(first) : null);
      setBase(first ? structuredClone(first) : null);
      setConfirmDelete(false);
      return;
    }
    if (!dirty) {
      setDraft(structuredClone(current));
      setBase(structuredClone(current));
    }
    // `dirty` and `draft` are what this effect decides from, not what it
    // should rerun on: the list changing is the event.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [teams, open]);

  if (!open) return null;
  const close = () => useStore.setState({ showTeams: false });

  const pick = (t: Team) => {
    setDraft(structuredClone(t));
    setBase(structuredClone(t));
    setConfirmDelete(false);
  };
  const startNew = () => {
    setDraft(newTeam());
    setBase(null);
    setConfirmDelete(false);
    setImportNotes([]);
  };
  const importFile = () => {
    void chooseFile("A team definition to import — mogeung's .json, or an OpenRig rig.yaml", ["json", "yaml", "yml"]).then((path) => {
      if (path) send({ cmd: "team_import", path });
    });
  };
  const exportFile = (t: Team) => {
    const slug = t.name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "team";
    void chooseSavePath(`${slug}.json`, "json").then((picked) => {
      // Declined: nothing. No picker (a browser tab): the daemon writes
      // beside the mirror and says where.
      if (picked === null) return;
      send(picked ? { cmd: "team_export", id: t.id, path: picked } : { cmd: "team_export", id: t.id });
    });
  };
  const patch = (p: Partial<Team>) => setDraft((d) => (d ? { ...d, ...p } : d));
  const patchPolicy = (p: Partial<Team["policy"]>) => setDraft((d) => (d ? { ...d, policy: { ...d.policy, ...p } } : d));
  const patchSeat = (i: number, p: Partial<Seat>) =>
    setDraft((d) => (d ? { ...d, seats: d.seats.map((s, j) => (j === i ? { ...s, ...p } : s)) } : d));
  const removeSeat = (i: number) => setDraft((d) => (d ? { ...d, seats: d.seats.filter((_, j) => j !== i) } : d));
  const addSeat = () => setDraft((d) => (d ? { ...d, seats: [...d.seats, newSeat(freeSeatName(d))] } : d));
  const patchWake = (i: number, p: Partial<Wake>) =>
    patchPolicyList("wakes", i, p);
  const patchWatchdog = (i: number, p: Partial<Watchdog>) => patchPolicyList("watchdogs", i, p);
  function patchPolicyList<K extends "wakes" | "watchdogs">(key: K, i: number, p: Partial<Team["policy"][K][number]>) {
    setDraft((d) => {
      if (!d) return d;
      const list = d.policy[key].map((x, j) => (j === i ? { ...x, ...p } : x));
      return { ...d, policy: { ...d.policy, [key]: list } };
    });
  }
  const dropFrom = (key: "wakes" | "watchdogs", i: number) =>
    setDraft((d) => (d ? { ...d, policy: { ...d.policy, [key]: d.policy[key].filter((_, j) => j !== i) } } : d));

  const problem = draft ? firstProblem(draft) : null;
  const refusal = allowed
    ? null
    : "this daemon is reachable beyond loopback and was not started with --allow-run, so a team is edited from a window on that machine";
  const saveWhy = refusal ?? problem;

  const save = () => {
    if (!draft || saveWhy) return;
    setImportNotes([]);
    send({ cmd: "team_save", team: draft });
    if (draft.id === "") {
      // The daemon mints the id and answers with the set, newest first; the
      // follow effect then picks it up. Until then there is nothing to edit.
      setDraft(null);
      setBase(null);
    } else {
      setBase(structuredClone(draft));
    }
  };
  const remove = () => {
    if (!draft || draft.id === "") return;
    if (!confirmDelete) {
      setConfirmDelete(true);
      return;
    }
    send({ cmd: "team_delete", id: draft.id });
    setDraft(null);
    setBase(null);
    setConfirmDelete(false);
  };

  return (
    <Dialog
      title="Teams"
      subtitle="a team of seats the daemon starts and owns — kept under ~/.mogeung/teams, mirrored as markdown"
      onClose={close}
      wide
      detached={{ name: "teams", size: teamsSize, onResized: (size) => setPrefs({ teamsSize: size }) }}
    >
      <div className="flex min-h-[32rem] min-w-[56rem] gap-3">
        {/* The list. Newest first, as the daemon sends them. */}
        <div className="flex w-44 shrink-0 flex-col gap-1 border-r border-[var(--line)] pr-2">
          <Button size="sm" variant="outline" onClick={startNew} title="a new team, saved when you press Save">
            <Plus size={12} /> New team
          </Button>
          {/* A file in. `R-P12`: mogeung's own JSON, or an OpenRig rig.yaml
              read as other people's configuration; it arrives as a draft
              with notes, and nothing is stored until Save. */}
          <Button size="sm" variant="outline" onClick={importFile} disabled={!allowed} title={allowed ? "import a team definition — mogeung's .json, or an OpenRig rig.yaml; it arrives as a draft to review" : (refusal ?? "")}>
            <Upload size={12} /> Import…
          </Button>
          {teams.length === 0 && draft === null && (
            <Dim className="mt-2 text-2xs">No teams yet. A team is a named set of seats with one policy.</Dim>
          )}
          {draft?.id === "" && (
            <div className="rounded bg-[var(--selection-bg)] px-2 py-1 text-xs text-[var(--text-strong)]">
              {draft.name.trim() || "new team"}
              <Dim className="block text-2xs">not saved yet</Dim>
            </div>
          )}
          {teams.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => pick(t)}
              className={`${interactive} rounded px-2 py-1 text-left text-xs ${
                draft?.id === t.id ? "bg-[var(--selection-bg)] text-[var(--text-strong)]" : ""
              }`}
            >
              {t.name}
              <Dim className="block text-2xs">{teamSummary(t)}</Dim>
            </button>
          ))}
        </div>

        {/* The editor. */}
        {draft === null ? (
          <div className="flex flex-1 items-center justify-center">
            <Dim className="text-xs">Pick a team, or make a new one.</Dim>
          </div>
        ) : (
          <div className="flex min-w-0 flex-1 flex-col gap-3 overflow-y-auto pr-1">
            <div className="flex items-center gap-2">
              <Dim className="w-20 shrink-0 text-2xs">name</Dim>
              <Input value={draft.name} onChange={(v) => patch({ name: v })} ariaLabel="team name" autoFocus={draft.id === ""} className="flex-1" />
              <span className="ml-auto flex items-center gap-1">
                {/* Boot and stop. `R-P4`. Boot starts what is *saved*: a
                    dirty draft is grey with that reason, so a click never
                    starts a definition you are still typing. */}
                <Button
                  size="sm"
                  variant="outline"
                  disabled={!allowed || draft.id === "" || dirty || draft.seats.length === 0}
                  title={
                    !allowed
                      ? (refusal ?? "")
                      : draft.id === ""
                        ? "save the team first"
                        : dirty
                          ? "save your changes first — Boot starts what is saved"
                          : draft.seats.length === 0
                            ? "a team with no seats boots nothing"
                            : "start every seat that is not already running, each in a tmux session of its own"
                  }
                  onClick={() => send({ cmd: "team_boot", id: draft.id })}
                >
                  <Play size={12} /> Boot team
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={!allowed || !launches.some((l) => l.team_id === draft.id)}
                  title={
                    !allowed
                      ? (refusal ?? "")
                      : launches.some((l) => l.team_id === draft.id)
                        ? "end every seat's process — the worktrees stay"
                        : "no seat of this team is running"
                  }
                  onClick={() => send({ cmd: "team_stop", id: draft.id })}
                >
                  <Square size={12} /> Stop team
                </Button>
              </span>
            </div>

            <div className="flex items-center gap-2">
              <Dim className="w-20 shrink-0 text-2xs">repository</Dim>
              <Input value={draft.repo} onChange={(v) => patch({ repo: v })} ariaLabel="repository" mono placeholder="/absolute/path — worktrees are made from it" className="flex-1" />
              <IconButton
                title="choose the repository folder"
                onClick={() => {
                  void chooseFolder("Absolute path of the team's repository", draft.repo || undefined).then((picked) => {
                    if (picked) patch({ repo: picked });
                  });
                }}
              >
                <FolderOpen size={13} />
              </IconButton>
            </div>

            <div>
              <Dim className="mb-0.5 block text-2xs">team guidance · every seat gets this, before its own</Dim>
              <textarea
                aria-label="team guidance"
                className={TEXTAREA}
                rows={3}
                spellCheck={false}
                value={draft.guidance}
                onChange={(e) => patch({ guidance: e.target.value })}
              />
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <Dim className="text-2xs">default permission mode · a seat may override</Dim>
              <Segmented
                value={draft.permission}
                onChange={(v) => patch({ permission: v })}
                options={PERMISSION_MODES.map((m) => ({ value: m, label: PERMISSION_LABEL[m] }))}
              />
            </div>

            {/* Policy, per team. */}
            <div className="rounded border border-[var(--line)] p-2">
              <Dim className="mb-1 block text-2xs">policy · per team</Dim>
              <div className="flex flex-wrap items-center gap-3">
                <NumberField label="hop budget" value={draft.policy.hop_budget} onChange={(n) => patchPolicy({ hop_budget: n })} title="agent-to-agent deliveries in a row with no human between; 0 = every message through you" />
                <NumberField label="sends per hour" value={draft.policy.hourly_cap} onChange={(n) => patchPolicy({ hourly_cap: n })} title="automatic sends an hour, across the team" />
                <Checkbox
                  checked={draft.policy.readiness_press}
                  onChange={(v) => patchPolicy({ readiness_press: v })}
                  label="readiness press"
                  title="press Enter for the one screen that asks for it (ADR-0044) — off until R-P11's week of captures"
                />
              </div>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <Dim className="text-2xs">handover</Dim>
                <Segmented
                  value={draft.policy.handover}
                  onChange={(v) => patchPolicy({ handover: v })}
                  options={HANDOVERS.map((h) => ({ value: h.value, label: h.label, title: h.title }))}
                />
              </div>

              <div className="mt-2">
                <Dim className="block text-2xs">wakes · declared text, read here before it fires</Dim>
                {draft.policy.wakes.map((w, i) => (
                  <div key={i} className="mt-1 flex flex-wrap items-center gap-1 text-2xs">
                    <Dim>on</Dim>
                    <SeatPicker label={`wake ${i + 1} watches`} value={w.on_seat} seats={draft.seats} onChange={(v) => patchWake(i, { on_seat: v })} />
                    <Segmented<SeatEvent>
                      value={w.on}
                      onChange={(v) => patchWake(i, { on: v })}
                      options={[
                        { value: "stop", label: "Stop", title: "the seat finished a turn" },
                        { value: "exit", label: "Exit", title: "the seat's process exited" },
                      ]}
                    />
                    <Dim>→</Dim>
                    <SeatPicker label={`wake ${i + 1} addresses`} value={w.to} seats={draft.seats} onChange={(v) => patchWake(i, { to: v })} />
                    <Input value={w.text} onChange={(v) => patchWake(i, { text: v })} ariaLabel={`wake ${i + 1} text`} placeholder="what is typed, verbatim" className="min-w-[14rem] flex-1" />
                    <IconButton title="remove this wake" onClick={() => dropFrom("wakes", i)}>
                      <X size={12} />
                    </IconButton>
                  </div>
                ))}
                <Button
                  size="sm"
                  className="mt-1"
                  disabled={draft.seats.length < 2}
                  title={draft.seats.length < 2 ? "a wake needs two seats: one to watch and one to tell" : "declare a send from one seat's event to another seat"}
                  onClick={() =>
                    patchPolicy({
                      wakes: [...draft.policy.wakes, { on_seat: draft.seats[0].name, on: "stop", to: draft.seats[1].name, text: "" }],
                    })
                  }
                >
                  <Plus size={12} /> wake
                </Button>
              </div>

              <div className="mt-2">
                <Dim className="block text-2xs">watchdogs · declared text</Dim>
                {draft.policy.watchdogs.map((d, i) => (
                  <div key={i} className="mt-1 flex flex-wrap items-center gap-1 text-2xs">
                    <SeatPicker label={`watchdog ${i + 1} seat`} value={d.seat ?? ""} seats={draft.seats} any onChange={(v) => patchWatchdog(i, { seat: v || undefined })} />
                    <Dim>silent</Dim>
                    <NumberField label={`watchdog ${i + 1} minutes`} value={d.silent_minutes} onChange={(n) => patchWatchdog(i, { silent_minutes: n })} />
                    <Dim>min</Dim>
                    <Checkbox checked={d.while_busy} onChange={(v) => patchWatchdog(i, { while_busy: v })} label="while busy" title="only mid-turn; a seat idle at its prompt is waiting for a person" />
                    <Dim>→ that seat</Dim>
                    <Input value={d.text} onChange={(v) => patchWatchdog(i, { text: v })} ariaLabel={`watchdog ${i + 1} text`} placeholder='"status?"' className="min-w-[10rem] flex-1" />
                    <IconButton title="remove this watchdog" onClick={() => dropFrom("watchdogs", i)}>
                      <X size={12} />
                    </IconButton>
                  </div>
                ))}
                <Button
                  size="sm"
                  className="mt-1"
                  onClick={() =>
                    patchPolicy({
                      watchdogs: [...draft.policy.watchdogs, { silent_minutes: 15, while_busy: true, text: "status?" }],
                    })
                  }
                >
                  <Plus size={12} /> watchdog
                </Button>
              </div>
            </div>

            {/* Sends, live. `R-P6`. What the policy is doing now for a
                saved team: the counters, the pause switch (acts at once),
                what is held with release and drop, and the log. */}
            {draft.id !== "" &&
              (() => {
                const mine = allSends.find((t) => t.team_id === draft.id);
                const now = Date.now() / 1000;
                const held = mine ? mine.log.filter((e) => e.status === "held") : [];
                const recent = mine ? mine.log.filter((e) => e.status !== "held").slice(-12).reverse() : [];
                const paused = mine?.paused ?? draft.paused;
                return (
                  <div className="rounded border border-[var(--line)] p-2" data-testid="sends">
                    <div className="flex flex-wrap items-center gap-2">
                      <Dim className="text-2xs">
                        sends · live · {mine ? countersLine(mine) : "nothing yet"}
                      </Dim>
                      <span className="ml-auto">
                        <Button
                          size="sm"
                          variant={paused ? "solid" : "outline"}
                          disabled={!allowed}
                          title={
                            !allowed
                              ? (refusal ?? "")
                              : paused
                                ? "resume: automatic sends are judged and delivered again"
                                : "pause: no automatic send while set — held, not lost; typing in a pane always works"
                          }
                          onClick={() => send({ cmd: "team_pause", id: draft.id, paused: !paused })}
                        >
                          {paused ? <Play size={12} /> : <Pause size={12} />} {paused ? "Resume sends" : "Pause sends"}
                        </Button>
                      </span>
                    </div>
                    {held.length > 0 && (
                      <div className="mt-1" data-testid="held-sends">
                        <Dim className="block text-2xs text-[var(--amber)]">held for you · the seat it is for ranks as needing you</Dim>
                        {held.map((e) => (
                          <div key={e.id} className="mt-0.5 flex flex-wrap items-center gap-1 text-2xs">
                            <span className="min-w-0 flex-1 truncate" title={`${e.text}\n\n${e.why}${e.note ? ` — ${e.note}` : ""}`}>
                              {sendLine(e, now)}
                            </span>
                            <Button
                              size="sm"
                              variant="outline"
                              disabled={!allowed}
                              title="type it into the seat now — your act, so the chain starts again after it"
                              onClick={() => send({ cmd: "send_release", team_id: draft.id, id: e.id })}
                            >
                              <Check size={12} /> release
                            </Button>
                            <Button
                              size="sm"
                              disabled={!allowed}
                              title="never deliver it"
                              onClick={() => send({ cmd: "send_drop", team_id: draft.id, id: e.id })}
                            >
                              <Ban size={12} /> drop
                            </Button>
                          </div>
                        ))}
                      </div>
                    )}
                    {recent.length > 0 && (
                      <div className="mt-1" data-testid="send-log">
                        <Dim className="block text-2xs">log · newest first</Dim>
                        {recent.map((e) => (
                          <Dim key={e.id} className="block truncate text-2xs" title={`${e.text}\n\n${e.why}${e.note ? ` — ${e.note}` : ""}`}>
                            {sendLine(e, now)}
                          </Dim>
                        ))}
                      </div>
                    )}
                  </div>
                );
              })()}

            {/* Seats, per seat. */}
            <div className="rounded border border-[var(--line)] p-2">
              <div className="flex items-center gap-2">
                <Dim className="text-2xs">seats · per seat</Dim>
                <Button size="sm" onClick={addSeat} title="add a seat with the team's mode and no guidance of its own">
                  <Plus size={12} /> seat
                </Button>
              </div>
              {draft.seats.length === 0 && <Dim className="mt-1 block text-2xs">No seats yet. A team boots nothing until it has one.</Dim>}
              {draft.seats.map((seat, i) => (
                <div key={i} className="mt-2 rounded border border-[var(--line)] bg-[var(--bg-raised,transparent)] p-2" data-testid={`seat-${i}`}>
                  <div className="flex flex-wrap items-center gap-2">
                    <Dim className="text-2xs">name</Dim>
                    <Input value={seat.name} onChange={(v) => patchSeat(i, { name: v })} ariaLabel={`seat ${i + 1} name`} mono className="w-32" />
                    <Dim className="text-2xs">label</Dim>
                    <Input value={seat.label} onChange={(v) => patchSeat(i, { label: v })} ariaLabel={`seat ${i + 1} label`} placeholder={seat.name} className="w-32" />
                    <span className="ml-auto">
                      <IconButton title="remove this seat" onClick={() => removeSeat(i)}>
                        <Trash2 size={12} />
                      </IconButton>
                    </span>
                  </div>
                  <div className="mt-1 flex flex-wrap items-center gap-2">
                    <Dim className="text-2xs">cli</Dim>
                    <Segmented
                      value={seat.cli}
                      onChange={(v) => patchSeat(i, { cli: v })}
                      options={CLIS.map((c) => ({
                        value: c,
                        title: `start ${sourceLabel(c)} in this seat`,
                        label: (
                          <span className="flex items-center gap-1">
                            <SourceMark source={c} />
                            {sourceLabel(c)}
                          </span>
                        ),
                      }))}
                    />
                    <Dim className="text-2xs">model</Dim>
                    <Input
                      value={seat.model ?? ""}
                      onChange={(v) => patchSeat(i, { model: v.trim() ? v : undefined })}
                      ariaLabel={`seat ${i + 1} model`}
                      mono
                      placeholder="the CLI's default"
                      className="w-40"
                    />
                  </div>
                  <div className="mt-1 flex flex-wrap items-center gap-2">
                    <Dim className="text-2xs">works in</Dim>
                    <Segmented
                      value={seat.works_in}
                      onChange={(v) => patchSeat(i, { works_in: v })}
                      options={[
                        { value: "worktree", label: `fresh worktree wt/${seat.name || "seat"}`, title: "made at boot from the team's repository and left behind at stop; the daemon may write there" },
                        { value: "checkout", label: "the team's checkout", title: "the repository itself; nothing of mogeung's is written there" },
                      ]}
                    />
                  </div>
                  <div className="mt-1 flex flex-wrap items-center gap-2">
                    <Dim className="text-2xs">permission · override</Dim>
                    <Segmented<PermissionMode | "team">
                      value={seat.permission ?? "team"}
                      onChange={(v) => patchSeat(i, { permission: v === "team" ? undefined : v })}
                      options={[
                        { value: "team", label: `team default (${PERMISSION_LABEL[draft.permission]})`, title: "inherit the team's mode" },
                        ...PERMISSION_MODES.map((m) => ({ value: m, label: PERMISSION_LABEL[m] })),
                      ]}
                    />
                  </div>
                  <div className="mt-1">
                    <Dim className="mb-0.5 block text-2xs">this seat's guidance · added after the team's</Dim>
                    <textarea
                      aria-label={`seat ${i + 1} guidance`}
                      className={TEXTAREA}
                      rows={2}
                      spellCheck={false}
                      value={seat.guidance}
                      onChange={(e) => patchSeat(i, { guidance: e.target.value })}
                    />
                  </div>
                  <div className="mt-1 flex flex-wrap items-center gap-3">
                    <Checkbox
                      checked={seat.own_home}
                      onChange={(v) => patchSeat(i, { own_home: v })}
                      label="a home of its own"
                      title="a blank seat under ~/.mogeung: its own login, none of your skills, plugins, memory or MCP servers (ADR-0043 clause 5)"
                    />
                    <Dim className="text-2xs">
                      starts as <Mono>{PERMISSION_LABEL[permissionFor(draft, seat)]}</Mono> · signals: {seatSignals(seat)}
                    </Dim>
                  </div>
                  {/* What the daemon says about this seat right now: its
                      tmux session while running, else the one click that
                      removes a worktree a stop left behind — refused by the
                      daemon when it is dirty. */}
                  {(() => {
                    const running = launches.find((l) => l.team_id === draft.id && l.seat === seat.name);
                    if (running) {
                      return (
                        <div className="mt-1" data-testid={`seat-${i}-status`}>
                          <Dim className="text-2xs">
                            running in <Mono>{running.tmux_session}</Mono> · {running.dir}
                          </Dim>
                          {/* What its hooks have said (`R-P5`): the kind, the
                              count, the last word and the context fill — or
                              that this is a plain seat, so silence here means
                              nothing. */}
                          <Dim className="text-2xs" title={running.signals.last_detail ?? undefined}>
                            {signalsLine(running.signals, Date.now() / 1000)}
                          </Dim>
                          {/* Its own send log (`R-P6`): the last few sends to
                              or from this seat, newest first. */}
                          {sendsFor(allSends.find((t) => t.team_id === draft.id)?.log ?? [], seat.name)
                            .slice(-3)
                            .reverse()
                            .map((e) => (
                              <Dim key={e.id} className="block truncate text-2xs" title={e.text}>
                                {sendLine(e, Date.now() / 1000)}
                              </Dim>
                            ))}
                        </div>
                      );
                    }
                    if (seat.works_in === "worktree" && draft.id !== "") {
                      return (
                        <div className="mt-1 flex items-center gap-1" data-testid={`seat-${i}-status`}>
                          <Dim className="text-2xs">stopped</Dim>
                          <Button
                            size="sm"
                            disabled={!allowed}
                            title="remove this seat's worktree, when it is clean — the daemon refuses a dirty one and says what is in it; the branch stays"
                            onClick={() => send({ cmd: "seat_worktree_remove", team_id: draft.id, seat: seat.name })}
                          >
                            <Eraser size={12} /> remove worktree
                          </Button>
                        </div>
                      );
                    }
                    return null;
                  })()}
                </div>
              ))}
            </div>

            {importNotes.length > 0 && (
              <div className="rounded border border-[var(--amber)] p-2" data-testid="import-notes">
                <Dim className="block text-2xs text-[var(--amber)]">imported as a draft — review, then Save</Dim>
                {importNotes.map((n, i) => (
                  <Dim key={i} className="block text-2xs">
                    · {n}
                  </Dim>
                ))}
              </div>
            )}

            {/* The verdict row. */}
            <div className="mt-auto flex items-center gap-2 border-t border-[var(--line)] pt-2">
              {saveWhy ? (
                <Dim className="text-2xs text-[var(--amber)]">{saveWhy}</Dim>
              ) : (
                <Dim className="text-2xs">{dirty ? "unsaved changes" : draft.id ? "saved" : ""}</Dim>
              )}
              <span className="ml-auto flex items-center gap-1">
                {draft.id !== "" && (
                  <Button size="sm" variant="ghost" onClick={() => exportFile(draft)} disabled={!allowed || dirty} title={!allowed ? (refusal ?? "") : dirty ? "save first — the export is what the daemon holds" : "write the saved definition as a .json file, where you choose"}>
                    <Download size={12} /> Export…
                  </Button>
                )}
                {draft.id !== "" && (
                  <Button size="sm" variant={confirmDelete ? "solid" : "ghost"} onClick={remove} disabled={!allowed} title={allowed ? "delete this team's definition; its mirror file goes with it" : refusal ?? undefined}>
                    <Trash2 size={12} /> {confirmDelete ? "Really delete" : "Delete"}
                  </Button>
                )}
                <Button size="sm" variant="solid" onClick={save} disabled={saveWhy !== null || !dirty} title={saveWhy ?? "save the definition — the daemon validates, stamps and mirrors it"}>
                  <Save size={12} /> Save
                </Button>
              </span>
            </div>
          </div>
        )}
      </div>
    </Dialog>
  );
}
