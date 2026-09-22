/**
 * Add, name, edit, order, test and forget daemons without a restart. `R-I7`,
 * `R-I16`, `R-I17`, `R-I19`.
 *
 * Before `R-I7` the address came from `?url=` or from whatever localStorage
 * happened to hold, so moving between a laptop's daemon and a dev box's meant
 * editing a query string — the store's `setUrl` had done the hard part for
 * months with nothing calling it.
 *
 * `R-I16` finished the CRUD that row started: an entry is identified by `id`
 * rather than by its URL, so the address is editable, the list orderable, and
 * a token has a field of its own instead of being smuggled through the address
 * into localStorage. The list rests in `~/.mogeung/connections.json` at `0600`
 * — [ADR-0036](../../../docs/decisions/0036-the-connection-list-is-the-clients-and-its-file-is-the-shells.md).
 *
 * `R-I19` (2026-09-22) is the shape: **a list beside a form**, asked for the
 * first day the panel held more than one real daemon — *"we need to see the
 * list of the saved remote connections clearly"*. Clicking a row *selects* it;
 * the form on the right is that connection, with **Test** (who is there, without
 * switching), **Connect**, and a token you can look at while you check it.
 * The daemons found on the LAN (`R-I17`) sit under the list, and *Add* on one
 * makes a saved entry and selects it, so the next thing in front of you is the
 * token field.
 *
 * Switching drops the whole board on purpose. Session ids do not survive the
 * move, and a queue from the machine you just left is worse than an empty one:
 * it looks like current information about work that is not in front of you.
 */

import { useEffect, useState } from "react";
import { Check, ChevronDown, ChevronUp, Eye, EyeOff, Plus, Trash2 } from "lucide-react";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { Dialog } from "@/ui/Dialog";
import { Button, Dim, IconButton, Input, Loading, Row } from "@/ui/primitives";
import {
  defaultName,
  dialledUrl,
  loadConnections,
  newId,
  probeConnection,
  reorder,
  saveConnections,
  storageKind,
  withCurrent,
  type Connection,
  type ProbeResult,
} from "@/lib/connections";
import {
  agoLabel,
  foundName,
  hostPortOf,
  lanPeers,
  matchesSaved,
  onLanPeers,
  reachLabel,
  wsUrlOf,
  type FoundDaemon,
  type LanAnswer,
} from "@/lib/lan";

/** What a row and its form call a connection that has no name yet. */
function labelOf(c: Connection): string {
  return c.name || c.url || "new connection";
}

export function ConnectionsWindow() {
  const open = useStore((s) => s.showConnections);
  const url = useStore((s) => s.url);
  const daemon = useStore((s) => s.daemon);
  const conn = useStore((s) => s.conn);
  const setUrl = useStore((s) => s.setUrl);

  // A load rather than a value, since ADR-0036 — so there is a tick with no
  // rows, and an empty list rendered during it reads as a list that was lost.
  const [list, setList] = useState<Connection[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // Per selection, never persisted: looking is a moment, not a setting.
  const [showToken, setShowToken] = useState(false);
  const [probe, setProbe] = useState<{ id: string; result: ProbeResult | "testing" } | null>(null);
  // Daemons announcing themselves on this LAN (`R-I17`): the shell's table as
  // it stood when the panel opened, then every change while it is open. Held
  // here rather than in the store because nothing else renders from it, and
  // `now` ticks so *12 s ago* stays true without the shell saying so.
  const [lan, setLan] = useState<LanAnswer | null>(null);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!open) return;
    let alive = true;
    void loadConnections().then((loaded) => {
      if (!alive) return;
      const next = withCurrent(loaded, url);
      setList(next);
      // Land on the daemon you are watching, so the form is about something.
      setSelectedId((cur) => cur ?? next.find((c) => c.url === url)?.id ?? next[0]?.id ?? null);
    });
    return () => {
      alive = false;
    };
  }, [open, url]);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    let unlisten: (() => void) | null = null;
    void lanPeers().then((answer) => {
      if (alive) setLan(answer);
    });
    void onLanPeers((peers) => {
      if (alive) setLan({ peers });
    }).then((u) => {
      if (alive) unlisten = u;
      else u();
    });
    const tick = setInterval(() => setNow(Date.now()), 5000);
    return () => {
      alive = false;
      unlisten?.();
      clearInterval(tick);
    };
  }, [open]);

  if (!open) return null;

  const close = () => useStore.setState({ showConnections: false });

  const write = (next: Connection[]) => {
    setList(next);
    // Written on change rather than on close: a dialog dismissed with Escape
    // must not lose the daemon you just added.
    void saveConnections(next);
  };

  const select = (id: string) => {
    setSelectedId(id);
    setShowToken(false);
  };

  const patch = (id: string, field: keyof Connection, value: string) => {
    if (!list) return;
    write(
      list.map((x) =>
        x.id === id ? { ...x, [field]: value || undefined, ...(field === "url" ? { url: value } : {}) } : x,
      ),
    );
  };

  const addBlank = () => {
    if (!list) return;
    const c: Connection = { id: newId(), name: "", url: "" };
    write([...list, c]);
    select(c.id);
  };

  /** A found daemon becomes a saved entry, selected — the token field is next. */
  const addFound = (d: FoundDaemon) => {
    if (!list) return;
    const u = wsUrlOf(d);
    const c: Connection = { id: newId(), name: d.host ?? defaultName(u), url: u };
    write([...list, c]);
    select(c.id);
  };

  const forget = (id: string) => {
    if (!list) return;
    const next = list.filter((x) => x.id !== id);
    write(next);
    if (selectedId === id) select(next.find((c) => c.url === url)?.id ?? next[0]?.id ?? "");
  };

  const test = (c: Connection) => {
    setProbe({ id: c.id, result: "testing" });
    void probeConnection(dialledUrl(c)).then((result) => setProbe({ id: c.id, result }));
  };

  const heard: FoundDaemon[] = lan && "peers" in lan ? lan.peers : [];
  // This machine's own daemons stay off the list. The panel is for daemons
  // you could go to, and asked for on the first day it was seen: *"we should
  // remove the local machine"*. The shell still holds them — hearing yourself
  // is how you know the listener works — and the probe still shows them.
  const others = heard.filter((d) => !d.mine);
  /** Is this saved connection the daemon being heard right now? */
  const heardNow = (c: Connection) => heard.some((d) => !d.gone && matchesSaved(c, d));

  const selected = list?.find((c) => c.id === selectedId) ?? null;

  return (
    <Dialog title="Connections" subtitle="which daemon this window is watching" onClose={close} wide>
      <div className="flex min-h-[22rem] gap-3">
        {/* The list. Clicking a row selects it; Connect is on the right. */}
        <div className="flex w-60 shrink-0 flex-col border-r border-[var(--border)] pr-3">
          <Dim className="mb-1 block text-2xs">saved</Dim>
          {list === null ? (
            <Loading what="connections" />
          ) : (
            <div role="listbox" aria-label="saved connections">
              {list.map((c) => {
                const current = c.url === url;
                return (
                  <Row
                    key={c.id}
                    selected={c.id === selectedId}
                    onClick={() => select(c.id)}
                    data-current={current || undefined}
                    className="flex items-center gap-2"
                  >
                    <span className="w-3 shrink-0">
                      {current && <Check size={11} className="text-[var(--green)]" />}
                    </span>
                    {heardNow(c) && (
                      <span
                        data-live
                        title="being heard on your LAN right now"
                        className="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--green)]"
                      />
                    )}
                    <span className="min-w-0 flex-1 truncate">{labelOf(c)}</span>
                    <Dim className="shrink-0 font-mono text-2xs">{hostPortOf(c.url) ?? ""}</Dim>
                  </Row>
                );
              })}
            </div>
          )}
          <Button variant="outline" className="mt-1 self-start" onClick={addBlank} title="a connection you will type in">
            <Plus size={11} /> new
          </Button>

          {/*
            Found on your LAN. `R-I17`: every daemon announces itself by default
            (ADR-0039), the shell hears it, and this is the list. **Finding is
            never connecting** — Add makes a saved entry and selects it, a hand
            types the token, and Connect is the switch that already exists.
          */}
          <div className="mt-3 border-t border-[var(--border)] pt-2">
            <Dim className="mb-1 block text-2xs">found on your LAN — nothing connects on its own</Dim>
            {lan === null ? null : "browser" in lan ? (
              <Dim className="block text-2xs">
                Not listening for beacons here: only the desktop window can, and it lists the
                daemons found on your LAN.
              </Dim>
            ) : "error" in lan ? (
              <Dim className="block text-2xs text-[var(--amber)]" data-lan-error>
                {lan.error}
              </Dim>
            ) : others.length === 0 ? (
              <Dim className="block text-2xs">
                nothing else heard yet — a daemon on this network announces itself within 15 s
              </Dim>
            ) : (
              others.map((d) => {
                const saved = list?.find((c) => matchesSaved(c, d));
                return (
                  <div
                    key={`${d.machine}\u0001${d.home}`}
                    data-found={foundName(d)}
                    data-gone={d.gone || undefined}
                    className={cn("flex items-center gap-2 py-1", d.gone && "opacity-50")}
                  >
                    <span
                      title={d.gone ? "gone — not heard for a minute" : "heard"}
                      className={cn(
                        "h-2 w-2 shrink-0 rounded-full",
                        d.gone ? "bg-[var(--dim)]" : "bg-[var(--green)]",
                      )}
                    />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-xs">
                        {foundName(d)}
                        <Dim className="ml-2 font-mono text-2xs">
                          {d.addr}:{d.port}
                        </Dim>
                      </div>
                      <Dim className="block truncate text-2xs">
                        {d.live} live · {d.waiting} need you ·{" "}
                        {d.gone
                          ? `gone — last heard ${agoLabel(d.last_seen_ms, now)}`
                          : agoLabel(d.last_seen_ms, now)}{" "}
                        · {reachLabel(d)}
                      </Dim>
                    </div>
                    {saved ? (
                      <Button variant="ghost" title={`saved as ${saved.name || saved.url}`} onClick={() => select(saved.id)}>
                        <Dim className="text-2xs">saved as {labelOf(saved)}</Dim>
                      </Button>
                    ) : (
                      <Button
                        variant="outline"
                        title="save this daemon — you supply the token, then connect"
                        onClick={() => addFound(d)}
                      >
                        <Plus size={11} /> add
                      </Button>
                    )}
                  </div>
                );
              })
            )}
          </div>
        </div>

        {/* The form: the selected connection, and what you can do to it. */}
        <div className="min-w-0 flex-1">
          {selected === null ? (
            <Dim className="block text-2xs">pick a connection on the left, or add one</Dim>
          ) : (
            <ConnectionForm
              key={selected.id}
              c={selected}
              list={list ?? []}
              current={selected.url === url}
              connState={conn}
              daemonLine={
                daemon ? `${daemon.host ?? "unknown host"} · ${daemon.version}` : null
              }
              showToken={showToken}
              setShowToken={setShowToken}
              probe={probe?.id === selected.id ? probe.result : null}
              onPatch={(field, value) => patch(selected.id, field, value)}
              onConnect={() => {
                // Composed here, so the token reaches the socket without
                // ever being part of what this panel displays.
                setUrl(dialledUrl(selected));
                close();
              }}
              onTest={() => test(selected)}
              onMove={(dir) => {
                const i = (list ?? []).findIndex((x) => x.id === selected.id);
                if (i >= 0) write(reorder(list ?? [], i, i + dir));
              }}
              onForget={() => forget(selected.id)}
            />
          )}
        </div>
      </div>

      <Dim className="mt-3 block text-2xs">
        Switching clears the board: session ids do not survive the move, and a queue from the
        machine you just left is worse than an empty one. A daemon on another host needs to be
        reachable — a beacon row says how; `ssh -L 7717:localhost:7717 host` is the way in to
        one bound to its own machine — and local-versus-remote is then decided by the daemon&apos;s
        identity rather than by the address you dialled (`R-I5`).
      </Dim>
      <Dim className="mt-1 block text-2xs">
        {storageKind() === "file" ? (
          <>
            Kept in <code>~/.mogeung/connections.json</code> on this machine, readable only by you
            — not on the daemon&apos;s machine, and not synced anywhere.
          </>
        ) : (
          <>
            <strong>This is a browser tab, so the list is in this browser&apos;s storage</strong>{" "}
            rather than in <code>~/.mogeung/connections.json</code>. A token typed here is not
            protected by the file&apos;s permissions — use the desktop window for anything you mean
            to keep.
          </>
        )}
      </Dim>
    </Dialog>
  );
}

function ConnectionForm({
  c,
  list,
  current,
  connState,
  daemonLine,
  showToken,
  setShowToken,
  probe,
  onPatch,
  onConnect,
  onTest,
  onMove,
  onForget,
}: {
  c: Connection;
  list: Connection[];
  current: boolean;
  connState: string;
  daemonLine: string | null;
  showToken: boolean;
  setShowToken: (v: boolean) => void;
  probe: ProbeResult | "testing" | null;
  onPatch: (field: keyof Connection, value: string) => void;
  onConnect: () => void;
  onTest: () => void;
  onMove: (dir: -1 | 1) => void;
  onForget: () => void;
}) {
  const label = labelOf(c);
  const i = list.findIndex((x) => x.id === c.id);
  return (
    <div className="flex flex-col gap-2">
      <div>
        <Dim className="mb-0.5 block text-2xs">name</Dim>
        <Input
          value={c.name}
          placeholder={c.url ? defaultName(c.url) : "dev box"}
          ariaLabel={`name for ${label}`}
          onChange={(v) => onPatch("name", v)}
        />
      </div>
      <div>
        <Dim className="mb-0.5 block text-2xs">address</Dim>
        <Input
          value={c.url}
          mono
          placeholder="ws://devbox:7717/ws"
          ariaLabel={`address for ${label}`}
          onChange={(v) => onPatch("url", v)}
        />
      </div>
      <div>
        <Dim className="mb-0.5 block text-2xs">token (remote only)</Dim>
        <div className="flex items-center gap-1">
          <div className="min-w-0 flex-1">
            <Input
              value={c.token ?? ""}
              secret={!showToken}
              placeholder="none"
              ariaLabel={`token for ${label}`}
              onChange={(v) => onPatch("token", v)}
            />
          </div>
          {/* A look, not a setting: it re-masks when the selection changes. */}
          <IconButton
            title={showToken ? "hide the token" : "show the token while you check it"}
            active={showToken}
            onClick={() => setShowToken(!showToken)}
          >
            {showToken ? <EyeOff size={12} /> : <Eye size={12} />}
          </IconButton>
        </div>
      </div>
      <div>
        <Dim className="mb-0.5 block text-2xs">
          tunnel command — recorded so it is here when you need it, never run
        </Dim>
        <Input
          value={c.note ?? ""}
          mono
          placeholder="ssh -L 7717:localhost:7717 devbox"
          ariaLabel={`tunnel command for ${label}`}
          onChange={(v) => onPatch("note", v)}
        />
      </div>

      <div className="mt-1 flex items-center gap-1">
        <Button
          variant="solid"
          disabled={current || !c.url}
          title={current ? "this is the daemon you are watching" : "switch this window to it — the board is dropped and refilled"}
          onClick={onConnect}
        >
          connect
        </Button>
        <Button
          variant="outline"
          disabled={!c.url || probe === "testing"}
          title="ask it who it is, without switching"
          onClick={onTest}
        >
          test
        </Button>
        <span className="flex-1" />
        <Button variant="outline" title="move up" disabled={i <= 0} onClick={() => onMove(-1)}>
          <ChevronUp size={11} />
        </Button>
        <Button
          variant="outline"
          title="move down"
          disabled={i < 0 || i === list.length - 1}
          onClick={() => onMove(1)}
        >
          <ChevronDown size={11} />
        </Button>
        <Button
          variant="outline"
          title={
            current
              ? "this is the daemon you are watching — switch away before forgetting it"
              : "forget this daemon"
          }
          disabled={current}
          onClick={onForget}
        >
          <Trash2 size={11} />
        </Button>
      </div>

      {probe !== null && (
        <Dim className="block text-2xs" data-probe={probe === "testing" ? "testing" : probe.ok ? "ok" : "fail"}>
          {probe === "testing"
            ? "asking…"
            : probe.ok
              ? `answered — ${probe.host ?? "unknown host"}${probe.version ? ` · ${probe.version}` : ""}${
                  probe.queued !== null ? ` · ${probe.queued} in its queue` : ""
                }`
              : probe.reason}
        </Dim>
      )}

      {current && (
        <Dim className="block text-2xs">
          {connState === "open" ? (daemonLine ? `connected — ${daemonLine}` : "connected") : connState}
        </Dim>
      )}
    </div>
  );
}
