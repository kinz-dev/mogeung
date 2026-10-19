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
 *
 * **Four sections in four colours since `R-J130`**, in a window of its own
 * (`R-J128`): the team in blue, its policy in purple, its sends in green and
 * its seats in teal, each a card with a header strip, its labels in one
 * column and its controls on one edge; the list of teams down the left with
 * the chosen one marked; and the verdict — what is wrong, or unsaved, or
 * saved — in a footer that stays while the form scrolls. Asked 2026-10-11:
 * *"make it visual component more visible so it can easily see the diff
 * section (by color, border, alignment, etc)"*. The two layers keep their
 * order: the team's guidance and mode come before any seat's.
 */

import { useEffect, useRef, useState } from "react";
import {
  Armchair,
  Ban,
  Check,
  CircleCheck,
  Download,
  Eraser,
  FileJson,
  FolderOpen,
  Pause,
  Play,
  Plus,
  Radio,
  Save,
  Shield,
  Square,
  Trash2,
  TriangleAlert,
  Upload,
  Users,
  X,
  Moon,
} from "lucide-react";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { chooseFile, chooseFolder, chooseSavePath, exportText, isTauri } from "@/lib/tauri";
import {
  countersLine,
  firstProblem,
  effortsFor,
  freeSeatName,
  listedModel,
  modelLine,
  modelsFor,
  newSeat,
  newTeam,
  permissionFor,
  seatSignals,
  sendLine,
  sendsFor,
  signalsLine,
  teamSummary,
  withModel,
} from "@/lib/teams";
import { Dialog } from "@/ui/Dialog";
import { Callout, Field, Section } from "@/ui/Form";
import { Button, Checkbox, Chip, Dim, IconButton, Input, Mono, Segmented } from "@/ui/primitives";
import { SourceMark } from "@/ui/SourceMark";
import { interactive } from "@/ui/styles";
import {
  PERMISSION_LABEL,
  PERMISSION_MODES,
  sourceLabel,
  type CliModels,
  type Effort,
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
  "w-full resize-y rounded-sm border border-[var(--border)] bg-[var(--bg)] p-2 font-mono text-xs leading-snug outline-none hover:border-[var(--border-hover)] focus-visible:border-[var(--ring)]";
const SELECT =
  "h-6 rounded-sm border border-[var(--border)] bg-[var(--bg)] px-1 font-mono text-xs outline-none hover:border-[var(--border-hover)] focus-visible:border-[var(--ring)]";

/** A rule — a wake, a watchdog — on a line of its own, with an edge. */
const RULE = "flex w-full flex-wrap items-center gap-1.5 rounded-sm border border-[var(--border)] bg-[var(--bg)] px-2 py-1 text-xs";

/** A whole number, typed as text: the daemon refuses what does not parse,
 *  and a box that eats a keystroke is worse than one that shows it. */
function NumberField({ label, value, onChange, title }: { label: string; value: number; onChange: (n: number) => void; title?: string }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  return (
    <span className="flex items-center gap-1.5" title={title}>
      <Dim className="text-xs">{label}</Dim>
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

/** Chosen from the list when the model typed is not on it. */
const ANOTHER = "\u0000another";

/**
 * A seat's model: the daemon's list for its CLI as a picker, with the CLI's
 * default first and *another model…* last, which opens the box for any name
 * the CLI accepts. A CLI the daemon has no list for gets the box alone, as
 * before. `R-P16`.
 */
function ModelPicker({
  seat,
  index,
  models,
  onChange,
}: {
  seat: Seat;
  index: number;
  models: CliModels | undefined;
  onChange: (model: string | undefined) => void;
}) {
  const listed = models?.models ?? [];
  const onList = !!seat.model && listed.some((m) => m.id === seat.model);
  // Typing is a choice the picker remembers, so choosing *another model…*
  // opens an empty box rather than snapping back to the default.
  const [typing, setTyping] = useState(!!seat.model && !onList);
  const showBox = listed.length === 0 || typing || (!!seat.model && !onList);
  const box = (
    <Input
      value={seat.model ?? ""}
      onChange={(v) => onChange(v.trim() ? v : undefined)}
      ariaLabel={`seat ${index + 1} model`}
      mono
      placeholder={listed.length === 0 ? "the CLI's default" : "as the CLI spells it"}
      className="w-48"
    />
  );
  if (listed.length === 0) return box;
  const value = showBox ? ANOTHER : (seat.model ?? "");
  return (
    <span className="flex flex-wrap items-center gap-1.5">
      <select
        aria-label={`seat ${index + 1} model choice`}
        className={cn(SELECT, "max-w-64")}
        value={value}
        onChange={(e) => {
          const v = e.target.value;
          if (v === ANOTHER) {
            setTyping(true);
            return;
          }
          setTyping(false);
          onChange(v || undefined);
        }}
      >
        <option value="">the CLI's default</option>
        {listed.map((m) => (
          <option key={m.id} value={m.id}>
            {m.label && m.label !== m.id ? `${m.label} · ${m.id}` : m.id}
          </option>
        ))}
        <option value={ANOTHER}>another model…</option>
      </select>
      {showBox && box}
    </span>
  );
}

/**
 * A seat's thinking level: the CLI's default, then each level its CLI — or
 * its listed model — takes. A CLI that takes none says so instead of showing
 * a control that cannot do anything; opencode's stay grey until a model is
 * named, since it applies a level to a model it was told. `R-P16`.
 */
function EffortPicker({
  seat,
  models,
  onChange,
}: {
  seat: Seat;
  models: CliModels | undefined;
  onChange: (effort: Effort | undefined) => void;
}) {
  const catalogue = models ? [models] : [];
  const levels = effortsFor(catalogue, seat);
  if (levels.length === 0) {
    return <Dim className="text-xs">{sourceLabel(seat.cli)} takes no thinking level at launch — it runs at its own default</Dim>;
  }
  const needsModel = seat.cli === "opencode" && !seat.model?.trim();
  const fallback = listedModel(catalogue, seat)?.default_effort;
  return (
    <Segmented<Effort | "default">
      size="md"
      value={seat.effort ?? "default"}
      onChange={(v) => onChange(v === "default" ? undefined : v)}
      options={[
        {
          value: "default",
          label: fallback ? `default (${fallback})` : "default",
          title: "whatever the CLI or the model runs at when not told",
        },
        ...levels.map((e) => ({
          value: e,
          label: e,
          disabled: needsModel,
          title: needsModel
            ? "name the model first — opencode applies a thinking level only to a model it is told"
            : `start it at ${e} thinking`,
        })),
      ]}
    />
  );
}

export function TeamsWindow() {
  const open = useStore((s) => s.showTeams);
  const teams = useStore((s) => s.teams);
  const allowed = useStore((s) => s.teamsAllowed);
  const launches = useStore((s) => s.seatLaunches);
  const seatModels = useStore((s) => s.seatModels);
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
  const booted = useStore((s) => s.teamBooted);
  // Where the file's schema goes once the daemon answers (`R-P20`): the
  // path the picker gave, or none for the shell's downloads folder. Set by
  // the click, taken by the answer.
  const schemaTarget = useRef<{ path: string | undefined } | null>(null);
  const schemaIn = useStore((s) => s.teamSchema);

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

  // The daemon answered with the team file's schema (`R-P20`): written
  // where the click said, through the shell — a browser tab downloads it.
  // Taken from the store like a draft is; an answer no click here asked for
  // is dropped rather than saved somewhere surprising.
  useEffect(() => {
    if (!schemaIn) return;
    useStore.setState({ teamSchema: null });
    const target = schemaTarget.current;
    schemaTarget.current = null;
    if (target) void saveSchema(schemaIn, target.path);
  }, [schemaIn]);

  // The daemon answered a Boot of the team on screen (`R-P14`): the window
  // has done its job and closes, whatever the boot found to do. The notice
  // carries the words. A boot the daemon refused never answers this way, so
  // the window stays open with the refusal in a toast.
  useEffect(() => {
    if (!open || !booted || !draft || booted.team_id !== draft.id) return;
    useStore.setState({ teamBooted: null, showTeams: false });
  }, [booted, open, draft]);

  const dirty = draft !== null && JSON.stringify(draft) !== JSON.stringify(base);
  // A wind-down in progress for the team on screen (`R-P15`): one Wind down
  // is enough, and Stop now reads as the cut-short it is.
  const winding =
    draft !== null && (allSends.find((t) => t.team_id === draft.id)?.wind_down?.seats.some((x) => x.status === "asked") ?? false);

  // Which seats a Boot would start. `R-P14`: a seat is running while its
  // tmux session stands, which the daemon reads every scan and sends as the
  // record's `alive` (`R-J134`) — the answer Boot itself uses, so the title
  // and the outcome agree. A record with `alive: false` is a seat that
  // exited, by itself or closed by hand, and the daemon starts it again.
  const seatRunning = (seatName: string): boolean =>
    draft !== null && launches.some((l) => l.team_id === draft.id && l.seat === seatName && l.alive);
  const toStart = draft ? draft.seats.filter((x) => !seatRunning(x.name)).map((x) => x.name) : [];
  const alreadyUp = draft !== null && draft.seats.length > 0 && toStart.length === 0;

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
  const downloadSchema = () => {
    void chooseSavePath(SCHEMA_FILE, "json").then((picked) => {
      // Declined: nothing. No picker (a browser tab, or a shell without the
      // plugin): the answer is downloaded, or saved where the shell saves.
      if (picked === null) return;
      schemaTarget.current = { path: picked };
      send({ cmd: "team_schema" });
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
  // A model or CLI change goes through `withModel`, which drops a level the
  // new one cannot take (`R-P16`).
  const changeSeat = (i: number, p: Partial<Pick<Seat, "cli" | "model">>) =>
    setDraft((d) => (d ? { ...d, seats: d.seats.map((s, j) => (j === i ? withModel(seatModels, s, p) : s)) } : d));
  const removeSeat = (i: number) => setDraft((d) => (d ? { ...d, seats: d.seats.filter((_, j) => j !== i) } : d));
  const addSeat = () => setDraft((d) => (d ? { ...d, seats: [...d.seats, newSeat(freeSeatName(d))] } : d));
  const patchWake = (i: number, p: Partial<Wake>) => patchPolicyList("wakes", i, p);
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

  // Records split by the daemon's `alive` (`R-J134`): a seat that exited
  // keeps its record until Boot starts it again or Stop now forgets it, and
  // the list says which is which rather than calling both *running*.
  const running = (t: Team) => launches.filter((l) => l.team_id === t.id && l.alive);
  const exited = (t: Team) => launches.filter((l) => l.team_id === t.id && !l.alive);

  return (
    <Dialog
      title="Teams"
      subtitle="a team of seats the daemon starts and owns — kept under ~/.mogeung/teams, mirrored as markdown"
      onClose={close}
      wide
      detached={{ name: "teams", size: teamsSize, onResized: (size) => setPrefs({ teamsSize: size }) }}
    >
      <div className="flex h-full min-h-[32rem] min-w-[56rem]">
        {/* The list. Newest first, as the daemon sends them. */}
        <aside className="flex w-56 shrink-0 flex-col border-r border-[var(--border)] bg-[var(--bg-panel)]">
          <div className="flex h-8 shrink-0 items-center gap-2 border-b border-[var(--border)] bg-[var(--bg-faint)] px-3">
            <Users size={14} className="shrink-0 text-[var(--blue)]" />
            <span className="text-xs font-semibold tracking-wider text-[var(--blue)] uppercase">Teams</span>
            <Dim className="ml-auto text-2xs">{teams.length}</Dim>
          </div>
          <div className="flex shrink-0 gap-1 border-b border-[var(--border)] p-2">
            <Button size="sm" variant="outline" className="flex-1" onClick={startNew} title="a new team, saved when you press Save">
              <Plus size={12} /> New team
            </Button>
            {/* A file in. `R-P12`: mogeung's own JSON, or an OpenRig rig.yaml
                read as other people's configuration; it arrives as a draft
                with notes, and nothing is stored until Save. */}
            <Button
              size="sm"
              variant="outline"
              className="flex-1"
              onClick={importFile}
              disabled={!allowed}
              title={allowed ? "import a team definition — mogeung's .json, or an OpenRig rig.yaml; it arrives as a draft to review" : (refusal ?? "")}
            >
              <Upload size={12} /> Import…
            </Button>
          </div>
          <div className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto p-2">
            {teams.length === 0 && draft === null && (
              <Dim className="px-1 py-2 text-xs leading-snug">No teams yet. A team is a named set of seats with one policy.</Dim>
            )}
            {draft?.id === "" && (
              <div className="rounded-sm border-l-2 border-[var(--amber)] bg-[var(--selection-bg)] px-2 py-1 text-sm text-[var(--text-strong)]">
                {draft.name.trim() || "new team"}
                <Dim className="block text-2xs">not saved yet</Dim>
              </div>
            )}
            {teams.map((t) => {
              const picked = draft?.id === t.id;
              const live = running(t).length;
              const gone = exited(t).length;
              return (
                <button
                  key={t.id}
                  type="button"
                  onClick={() => pick(t)}
                  className={cn(
                    interactive,
                    "rounded-sm border-l-2 px-2 py-1 text-left text-sm",
                    picked ? "border-[var(--blue)] bg-[var(--selection-bg)] text-[var(--text-strong)]" : "border-transparent",
                  )}
                >
                  {t.name}
                  <Dim className="block text-2xs">
                    {teamSummary(t)}
                    {live > 0 && <span className="text-[var(--green)]"> · {live} running</span>}
                    {gone > 0 && <span className="text-[var(--amber)]"> · {gone} exited</span>}
                  </Dim>
                </button>
              );
            })}
          </div>
          {/* The spec of the file Import… reads and Export… writes (`R-P20`):
              a JSON Schema the daemon derives from its own types. Not gated —
              nothing of yours is in it. */}
          <div className="flex shrink-0 border-t border-[var(--border)] p-2">
            <Button
              size="sm"
              variant="ghost"
              className="flex-1"
              onClick={downloadSchema}
              title="save the JSON Schema a team file follows — what Export… writes and Import… reads, generated from the daemon's own types"
            >
              <FileJson size={12} /> Team file schema…
            </Button>
          </div>
        </aside>

        {/* The editor. */}

        {draft === null ? (
          <div className="flex flex-1 items-center justify-center">
            <Dim className="text-sm">Pick a team, or make a new one.</Dim>
          </div>
        ) : (
          <div className="flex min-w-0 flex-1 flex-col">
            <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3">
              <Section
                title="Team"
                tone="blue"
                icon={<Users size={14} />}
                hint="its name, its repository, and the guidance and mode every seat starts with"
                actions={
                  <>
                    {/* Boot and stop. `R-P4`. Boot starts what is *saved*: a
                        dirty draft is grey with that reason, so a click never
                        starts a definition you are still typing. */}
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!allowed || draft.id === "" || dirty || draft.seats.length === 0 || alreadyUp}
                      title={
                        !allowed
                          ? (refusal ?? "")
                          : draft.id === ""
                            ? "save the team first"
                            : dirty
                              ? "save your changes first — Boot starts what is saved"
                              : draft.seats.length === 0
                                ? "a team with no seats boots nothing"
                                : alreadyUp
                                  ? "every seat is running"
                                  : toStart.length === draft.seats.length
                                    ? "start every seat, each in a tmux session of its own"
                                    : `start ${toStart.join(", ")} — the rest are running`
                      }
                      onClick={() => send({ cmd: "team_boot", id: draft.id })}
                    >
                      <Play size={12} /> Boot team
                    </Button>
                    {/* Two ways down (`R-P15`). Wind down asks every seat
                        for a note and stops each as it answers; Stop now
                        ends them at once, and cuts a wind-down short. */}
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!allowed || running(draft).length === 0 || winding}
                      title={
                        !allowed
                          ? (refusal ?? "")
                          : winding
                            ? "winding down — every seat has been asked for its note"
                            : running(draft).length > 0
                              ? `ask every seat for a note of where it got to, then stop each as its note arrives or its turn ends — ${draft.policy.wind_down_minutes} min at most; the next handover reads the notes`
                              : exited(draft).length > 0
                                ? "no seat of this team is running — Stop now forgets the exited ones"
                                : "no seat of this team is running"
                      }
                      onClick={() => send({ cmd: "team_wind_down", id: draft.id })}
                    >
                      <Moon size={12} /> Wind down
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={!allowed || !launches.some((l) => l.team_id === draft.id)}
                      title={
                        !allowed
                          ? (refusal ?? "")
                          : running(draft).length > 0
                            ? winding
                              ? "cut the wind-down short — the seats still asked are stopped without a note"
                              : "end every seat's process at once, no note asked — the worktrees stay"
                            : exited(draft).length > 0
                              ? "nothing is running — forget the exited seats' records"
                              : "no seat of this team is running"
                      }
                      onClick={() => send({ cmd: "team_stop", id: draft.id })}
                    >
                      <Square size={12} /> Stop now
                    </Button>
                  </>
                }
              >
                <Field label="name">
                  <Input value={draft.name} onChange={(v) => patch({ name: v })} ariaLabel="team name" autoFocus={draft.id === ""} className="max-w-md" />
                </Field>
                <Field label="repository" hint="an absolute path; worktrees are made from it">
                  <div className="flex w-full max-w-2xl items-center gap-1">
                    <Input value={draft.repo} onChange={(v) => patch({ repo: v })} ariaLabel="repository" mono placeholder="/absolute/path" className="flex-1" />
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
                </Field>
                <Field label="team guidance" hint="every seat gets this, before its own" align="start">
                  <textarea
                    aria-label="team guidance"
                    className={TEXTAREA}
                    rows={3}
                    spellCheck={false}
                    value={draft.guidance}
                    onChange={(e) => patch({ guidance: e.target.value })}
                  />
                </Field>
                <Field label="permission" hint="the team's default; a seat may override it">
                  <Segmented
                    size="md"
                    value={draft.permission}
                    onChange={(v) => patch({ permission: v })}
                    options={PERMISSION_MODES.map((m) => ({ value: m, label: PERMISSION_LABEL[m] }))}
                  />
                </Field>
              </Section>

              {/* Policy, per team. */}
              <Section title="Policy" tone="purple" icon={<Shield size={14} />} hint="declared text, read here before it ever fires">
                <Field label="budget" hint="what the policy may do on its own">
                  <NumberField
                    label="hop budget"
                    value={draft.policy.hop_budget}
                    onChange={(n) => patchPolicy({ hop_budget: n })}
                    title="agent-to-agent deliveries in a row with no human between; 0 = every message through you"
                  />
                  <NumberField
                    label="sends per hour"
                    value={draft.policy.hourly_cap}
                    onChange={(n) => patchPolicy({ hourly_cap: n })}
                    title="automatic sends an hour, across the team"
                  />
                  <Checkbox
                    checked={draft.policy.readiness_press}
                    onChange={(v) => patchPolicy({ readiness_press: v })}
                    label="readiness press"
                    title="press Enter for the one screen that asks for it (ADR-0044) — off until R-P11's week of captures"
                  />
                </Field>
                <Field label="handover" hint="what a seat that restarts is told">
                  <Segmented
                    size="md"
                    value={draft.policy.handover}
                    onChange={(v) => patchPolicy({ handover: v })}
                    options={HANDOVERS.map((h) => ({ value: h.value, label: h.label, title: h.title }))}
                  />
                </Field>
                {/* `R-P15`: the one message every seat is asked on a wind-down,
                    and how long it has to answer. Declared text, like a wake. */}
                <Field label="wind down" hint="what every seat is asked when you wind the team down; {note} is the note command" align="start">
                  <div className="flex w-full flex-col gap-1.5">
                    <textarea
                      aria-label="wind-down message"
                      className={TEXTAREA}
                      rows={3}
                      spellCheck={false}
                      value={draft.policy.wind_down}
                      onChange={(e) => patchPolicy({ wind_down: e.target.value })}
                    />
                    <NumberField
                      label="minutes to answer"
                      value={draft.policy.wind_down_minutes}
                      onChange={(n) => patchPolicy({ wind_down_minutes: n })}
                      title="a seat still silent after this long is stopped anyway, without a note"
                    />
                  </div>
                </Field>
                <Field label="wakes" hint="one seat's event types a line into another" align="start">
                  <div className="flex w-full flex-col gap-1">
                    {draft.policy.wakes.map((w, i) => (
                      <div key={i} className={RULE}>
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
                    <div>
                      <Button
                        size="sm"
                        variant="outline"
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
                  </div>
                </Field>
                <Field label="watchdogs" hint="a seat silent too long is asked" align="start">
                  <div className="flex w-full flex-col gap-1">
                    {draft.policy.watchdogs.map((d, i) => (
                      <div key={i} className={RULE}>
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
                    <div>
                      <Button
                        size="sm"
                        variant="outline"
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
                </Field>
              </Section>

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
                    <Section
                      title="Sends"
                      tone="green"
                      icon={<Radio size={14} />}
                      hint={`live · ${mine ? countersLine(mine) : "nothing yet"}`}
                      testId="sends"
                      actions={
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
                      }
                    >
                      {held.length > 0 && (
                        <Callout tone="amber" icon={<TriangleAlert size={14} />} testId="held-sends">
                          <div className="font-medium">held for you · the seat it is for ranks as needing you</div>
                          {held.map((e) => (
                            <div key={e.id} className="mt-1 flex flex-wrap items-center gap-1.5">
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
                        </Callout>
                      )}
                      {recent.length > 0 && (
                        <div className="rounded-sm border border-[var(--border)] bg-[var(--bg)] px-2 py-1" data-testid="send-log">
                          <Dim className="block text-2xs font-semibold tracking-wider uppercase">log · newest first</Dim>
                          {recent.map((e) => (
                            <Dim key={e.id} className="block truncate text-xs" title={`${e.text}\n\n${e.why}${e.note ? ` — ${e.note}` : ""}`}>
                              {sendLine(e, now)}
                            </Dim>
                          ))}
                        </div>
                      )}
                      {held.length === 0 && recent.length === 0 && (
                        <Dim className="text-xs">No sends yet. A wake or a watchdog that fires shows up here, with what became of it.</Dim>
                      )}
                    </Section>
                  );
                })()}

              {/* Seats, per seat. */}
              <Section
                title="Seats"
                tone="teal"
                icon={<Armchair size={14} />}
                hint="each boots in a tmux session of its own, with the team's guidance and then its own"
                actions={
                  <Button size="sm" variant="outline" onClick={addSeat} title="add a seat with the team's mode and no guidance of its own">
                    <Plus size={12} /> seat
                  </Button>
                }
              >
                {draft.seats.length === 0 && <Dim className="text-xs">No seats yet. A team boots nothing until it has one.</Dim>}
                {draft.seats.map((seat, i) => {
                  const live = launches.find((l) => l.team_id === draft.id && l.seat === seat.name);
                  return (
                    <div
                      key={i}
                      className="shrink-0 overflow-hidden rounded-md border border-[var(--border)] bg-[var(--bg-raised)]"
                      data-testid={`seat-${i}`}
                    >
                      <div className="flex h-7 items-center gap-2 border-b border-[var(--border)] bg-[var(--bg-faint)] px-3">
                        <SourceMark source={seat.cli} />
                        <span className="text-xs font-semibold tracking-wider text-[var(--graph-4)] uppercase">seat {i + 1}</span>
                        {seat.label.trim() && <Dim className="min-w-0 truncate text-xs">{seat.label}</Dim>}
                        <span className="min-w-0 truncate font-mono text-xs text-[var(--fg-dim)]" data-testid={`seat-${i}-model`}>
                          {modelLine(seatModels, seat)}
                        </span>
                        {live && live.alive && (
                          <Chip color="var(--green)" title={`running in ${live.tmux_session}`}>
                            running
                          </Chip>
                        )}
                        {live && !live.alive && (
                          <Chip
                            color="var(--amber)"
                            title={`its tmux session ${live.tmux_session} is gone — Boot starts it again, Stop now forgets it`}
                          >
                            exited
                          </Chip>
                        )}
                        <span className="ml-auto">
                          <IconButton title="remove this seat" onClick={() => removeSeat(i)}>
                            <Trash2 size={12} />
                          </IconButton>
                        </span>
                      </div>
                      <div className="flex flex-col gap-2 p-3">
                        <div className="grid grid-cols-2 gap-x-4 gap-y-2">
                          <Field label="name" hint="lower-case; names its tmux session">
                            <Input value={seat.name} onChange={(v) => patchSeat(i, { name: v })} ariaLabel={`seat ${i + 1} name`} mono className="w-40" />
                          </Field>
                          <Field label="label" hint="how its row reads">
                            <Input value={seat.label} onChange={(v) => patchSeat(i, { label: v })} ariaLabel={`seat ${i + 1} label`} placeholder={seat.name} className="w-40" />
                          </Field>
                        </div>
                        <Field label="cli">
                          <Segmented
                            size="md"
                            value={seat.cli}
                            onChange={(v) => changeSeat(i, { cli: v })}
                            options={CLIS.map((c) => ({
                              value: c,
                              title: `start ${sourceLabel(c)} in this seat`,
                              label: (
                                <span className="flex items-center gap-1.5">
                                  <SourceMark source={c} />
                                  {sourceLabel(c)}
                                </span>
                              ),
                            }))}
                          />
                        </Field>
                        <Field label="model" hint={modelsFor(seatModels, seat.cli)?.source ?? "blank for the CLI's default"}>
                          <ModelPicker
                            key={seat.cli}
                            seat={seat}
                            index={i}
                            models={modelsFor(seatModels, seat.cli)}
                            onChange={(model) => changeSeat(i, { model })}
                          />
                        </Field>
                        <Field label="thinking" hint="how hard the model thinks">
                          <EffortPicker seat={seat} models={modelsFor(seatModels, seat.cli)} onChange={(effort) => patchSeat(i, { effort })} />
                        </Field>
                        <div className="grid grid-cols-2 gap-x-4 gap-y-2">
                          <Field label="works in">
                            <Segmented
                              size="md"
                              value={seat.works_in}
                              onChange={(v) => patchSeat(i, { works_in: v })}
                              options={[
                                { value: "worktree", label: `fresh worktree wt/${seat.name || "seat"}`, title: "made at boot from the team's repository and left behind at stop; the daemon may write there" },
                                { value: "checkout", label: "the team's checkout", title: "the repository itself; nothing of mogeung's is written there" },
                              ]}
                            />
                          </Field>
                        </div>
                        <Field label="permission" hint="the team's, or this seat's own">
                          <Segmented<PermissionMode | "team">
                            size="md"
                            value={seat.permission ?? "team"}
                            onChange={(v) => patchSeat(i, { permission: v === "team" ? undefined : v })}
                            options={[
                              { value: "team", label: `team default (${PERMISSION_LABEL[draft.permission]})`, title: "inherit the team's mode" },
                              ...PERMISSION_MODES.map((m) => ({ value: m, label: PERMISSION_LABEL[m] })),
                            ]}
                          />
                        </Field>
                        <Field label="guidance" hint="added after the team's" align="start">
                          <textarea
                            aria-label={`seat ${i + 1} guidance`}
                            className={TEXTAREA}
                            rows={2}
                            spellCheck={false}
                            value={seat.guidance}
                            onChange={(e) => patchSeat(i, { guidance: e.target.value })}
                          />
                        </Field>
                        <Field label="home">
                          <Checkbox
                            checked={seat.own_home}
                            onChange={(v) => patchSeat(i, { own_home: v })}
                            label="a home of its own"
                            title="a blank seat under ~/.mogeung: its own login, none of your skills, plugins, memory or MCP servers (ADR-0043 clause 5)"
                          />
                          <Dim className="text-xs">
                            starts as <Mono>{PERMISSION_LABEL[permissionFor(draft, seat)]}</Mono> · signals: {seatSignals(seat)}
                          </Dim>
                        </Field>
                        {/* What the daemon says about this seat right now: its
                            tmux session while running, else the one click that
                            removes a worktree a stop left behind — refused by the
                            daemon when it is dirty. */}
                        {live ? (
                          <div className="rounded-sm border border-[var(--border)] bg-[var(--bg)] px-2 py-1" data-testid={`seat-${i}-status`}>
                            <Dim className="block text-xs">
                              {live.alive ? "running in" : "exited — was in"} <Mono>{live.tmux_session}</Mono> · {live.dir}
                            </Dim>
                            {/* What its hooks have said (`R-P5`): the kind, the
                                count, the last word and the context fill — or
                                that this is a plain seat, so silence here means
                                nothing. */}
                            <Dim className="block text-xs" title={live.signals.last_detail ?? undefined}>
                              {signalsLine(live.signals, Date.now() / 1000)}
                            </Dim>
                            {/* Its own send log (`R-P6`): the last few sends to
                                or from this seat, newest first. */}
                            {sendsFor(allSends.find((t) => t.team_id === draft.id)?.log ?? [], seat.name)
                              .slice(-3)
                              .reverse()
                              .map((e) => (
                                <Dim key={e.id} className="block truncate text-xs" title={e.text}>
                                  {sendLine(e, Date.now() / 1000)}
                                </Dim>
                              ))}
                          </div>
                        ) : seat.works_in === "worktree" && draft.id !== "" ? (
                          <div className="flex items-center gap-2" data-testid={`seat-${i}-status`}>
                            <Dim className="text-xs">stopped</Dim>
                            <Button
                              size="sm"
                              variant="outline"
                              disabled={!allowed}
                              title="remove this seat's worktree, when it is clean — the daemon refuses a dirty one and says what is in it; the branch stays"
                              onClick={() => send({ cmd: "seat_worktree_remove", team_id: draft.id, seat: seat.name })}
                            >
                              <Eraser size={12} /> remove worktree
                            </Button>
                          </div>
                        ) : null}
                      </div>
                    </div>
                  );
                })}
              </Section>

              {importNotes.length > 0 && (
                <Callout tone="amber" icon={<TriangleAlert size={14} />} testId="import-notes">
                  <div className="font-medium">imported as a draft — review, then Save</div>
                  {importNotes.map((n, i) => (
                    <div key={i} className="mt-0.5">
                      · {n}
                    </div>
                  ))}
                </Callout>
              )}
            </div>

            {/* The verdict row, in a footer that stays while the form scrolls. */}
            <div className="flex shrink-0 items-center gap-2 border-t border-[var(--border)] bg-[var(--bg-panel)] px-3 py-2">
              {saveWhy ? (
                <span className="flex min-w-0 items-center gap-1.5 text-xs text-[var(--amber)]">
                  <TriangleAlert size={13} className="shrink-0" />
                  <span className="min-w-0 truncate">{saveWhy}</span>
                </span>
              ) : dirty ? (
                <span className="flex items-center gap-1.5 text-xs text-[var(--amber)]">
                  <span className="h-2 w-2 shrink-0 rounded-full bg-[var(--amber)]" />
                  <span>unsaved changes</span>
                </span>
              ) : draft.id ? (
                <span className="flex items-center gap-1.5 text-xs text-[var(--green)]">
                  <CircleCheck size={13} className="shrink-0" />
                  <span>saved</span>
                </span>
              ) : null}
              <span className="ml-auto flex items-center gap-1">
                {draft.id !== "" && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => exportFile(draft)}
                    disabled={!allowed || dirty}
                    title={!allowed ? (refusal ?? "") : dirty ? "save first — the export is what the daemon holds" : "write the saved definition as a .json file, where you choose"}
                  >
                    <Download size={12} /> Export…
                  </Button>
                )}
                {draft.id !== "" && (
                  <Button
                    size="sm"
                    variant={confirmDelete ? "solid" : "ghost"}
                    onClick={remove}
                    disabled={!allowed || running(draft).length > 0}
                    title={
                      !allowed
                        ? (refusal ?? undefined)
                        : running(draft).length > 0
                          ? `stop the team first — still running: ${running(draft)
                              .map((l) => l.seat)
                              .join(", ")}`
                          : "delete this team: its definition and mirror file, its seat records and the files made for its seats — worktrees stay"
                    }
                  >
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

/** The name the team file's schema is saved under. `R-P20`. */
export const SCHEMA_FILE = "mogeung-team.schema.json";

/**
 * Write the schema the daemon answered where the click said. `R-P20`.
 *
 * Through the shell when there is one — `exportText` takes the picked path,
 * or chooses the downloads folder when there was no picker — and by the
 * anchor trick in a browser tab, which has no shell to write through, the
 * way the transcript's export does. Either way the notice says where it
 * went, because a save you cannot find is a save that did not happen.
 */
async function saveSchema(schema: Record<string, unknown>, path: string | undefined): Promise<void> {
  const text = `${JSON.stringify(schema, null, 2)}\n`;
  const { pushNotice, pushError } = useStore.getState();
  if (!isTauri()) {
    const url = URL.createObjectURL(new Blob([text], { type: "application/schema+json" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = SCHEMA_FILE;
    a.click();
    URL.revokeObjectURL(url);
    pushNotice(`team file schema downloaded — ${SCHEMA_FILE}`);
    return;
  }
  try {
    pushNotice(`team file schema saved — ${await exportText(SCHEMA_FILE, text, path)}`);
  } catch (e) {
    pushError(`could not save the team file schema: ${String(e)}`);
  }
}
