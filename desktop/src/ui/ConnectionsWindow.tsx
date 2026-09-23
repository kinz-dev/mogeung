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
 * **Follow** (`R-I21`, feature 0047) is the other thing a saved connection
 * can be: not *go there* but *bring its sessions here*. This machine's daemon
 * holds the connection, so it lasts with the window closed and needs no
 * switch; the form shows where each follow stands, and — per follow, off
 * until a hand turns it on — whether changes may cross to it (`R-I23`).
 *
 * Switching drops the whole board on purpose. Session ids do not survive the
 * move, and a queue from the machine you just left is worse than an empty one:
 * it looks like current information about work that is not in front of you.
 */

import { useEffect, useState } from "react";
import { Check, ChevronDown, ChevronUp, Eye, EyeOff, Plus, Trash2 } from "lucide-react";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { composeDial } from "@/lib/dial";
import { Dialog } from "@/ui/Dialog";
import { Button, Checkbox, Dim, IconButton, Input, Loading, Row } from "@/ui/primitives";
import { followMsg, followOf, peerLine, peerTrouble } from "@/lib/federation";
import type { PeerState } from "@/wire/types";
import {
  defaultName,
  dialledUrl,
  loadConnections,
  missingToken,
  newId,
  probeConnection,
  reorder,
  sameDaemon,
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
  const daemonStatus = useStore((s) => s.daemonStatus);
  const localAddr = useStore((s) => s.localAddr);
  // `null` is the daemon's word that this window may not follow — see the store.
  const peers = useStore((s) => s.peers);
  const send = useStore((s) => s.send);

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
      setSelectedId((cur) => cur ?? next.find((c) => sameDaemon(c.url, url))?.id ?? next[0]?.id ?? null);
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
    if (selectedId === id) select(next.find((c) => sameDaemon(c.url, url))?.id ?? next[0]?.id ?? "");
  };

  const test = (c: Connection) => {
    // This machine's own daemon takes its token from the shell, not the list —
    // the same composition a switch makes (`lib/dial.ts`, `R-J102`).
    const dial = composeDial(dialledUrl(c), { addr: localAddr, status: daemonStatus });
    const missing = dial === dialledUrl(c) ? missingToken(c) : null;
    if (missing) {
      setProbe({ id: c.id, result: { ok: false, reason: missing } });
      return;
    }
    setProbe({ id: c.id, result: "testing" });
    void probeConnection(dial).then((result) => setProbe({ id: c.id, result }));
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
          <Dim className="mb-1 block text-sm">saved</Dim>
          {list === null ? (
            <Loading what="connections" />
          ) : (
            <div role="listbox" aria-label="saved connections">
              {list.map((c) => {
                const current = sameDaemon(c.url, url);
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
                    <span className="min-w-0 flex-1 truncate text-base">{labelOf(c)}</span>
                    {(() => {
                      const f = followOf(c, peers);
                      return (
                        f && (
                          <span
                            data-following={f.state}
                            title={peerLine(f)}
                            className={cn(
                              "shrink-0 text-xs",
                              peerTrouble(f) ? "text-[var(--amber)]" : "text-[var(--blue)]",
                            )}
                          >
                            following
                          </span>
                        )
                      );
                    })()}
                    <Dim className="shrink-0 font-mono text-sm">{hostPortOf(c.url) ?? ""}</Dim>
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
            <Dim className="mb-1 block text-sm">found on your LAN — nothing connects on its own</Dim>
            {lan === null ? null : "browser" in lan ? (
              <Dim className="block text-sm">
                Not listening for beacons here: only the desktop window can, and it lists the
                daemons found on your LAN.
              </Dim>
            ) : "error" in lan ? (
              <Dim className="block text-sm text-[var(--amber)]" data-lan-error>
                {lan.error}
              </Dim>
            ) : others.length === 0 ? (
              <Dim className="block text-sm">
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
                      <div className="truncate text-base">
                        {foundName(d)}
                        <Dim className="ml-2 font-mono text-sm">
                          {d.addr}:{d.port}
                        </Dim>
                      </div>
                      <Dim className="block truncate text-sm">
                        {d.live} live · {d.waiting} need you ·{" "}
                        {d.gone
                          ? `gone — last heard ${agoLabel(d.last_seen_ms, now)}`
                          : agoLabel(d.last_seen_ms, now)}{" "}
                        · {reachLabel(d)}
                      </Dim>
                    </div>
                    {saved ? (
                      <Button variant="ghost" title={`saved as ${saved.name || saved.url}`} onClick={() => select(saved.id)}>
                        <Dim className="text-sm">saved as {labelOf(saved)}</Dim>
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
            <Dim className="block text-sm">pick a connection on the left, or add one</Dim>
          ) : (
            <ConnectionForm
              key={selected.id}
              c={selected}
              list={list ?? []}
              current={sameDaemon(selected.url, url)}
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
              follow={
                // A daemon from before federation has nothing to offer here.
                !daemon?.federation
                  ? null
                  : peers === null
                    ? "elsewhere"
                    : {
                        peer: followOf(selected, peers),
                        onFollow: (allow) => send(followMsg(selected, allow)),
                        onUnfollow: (id) => send({ cmd: "unfollow", id }),
                        onWrites: (id, allow) => send({ cmd: "set_peer_writes", id, allow }),
                      }
              }
            />
          )}
        </div>
      </div>

      <Dim className="mt-3 block text-sm">
        Switching clears the board: session ids do not survive the move, and a queue from the
        machine you just left is worse than an empty one. A daemon on another host needs to be
        reachable — a beacon row says how; `ssh -L 7717:localhost:7717 host` is the way in to
        one bound to its own machine — and local-versus-remote is then decided by the daemon&apos;s
        identity rather than by the address you dialled (`R-I5`).
      </Dim>
      <Dim className="mt-1 block text-sm">
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
  follow,
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
  /**
   * What following this connection looks like from here: the controls, the
   * reason there are none (`"elsewhere"` — this window is another machine's),
   * or `null` for a daemon that cannot follow at all.
   */
  follow:
    | null
    | "elsewhere"
    | {
        peer: PeerState | null;
        onFollow: (allowWrites: boolean) => void;
        onUnfollow: (id: string) => void;
        onWrites: (id: string, allow: boolean) => void;
      };
}) {
  const label = labelOf(c);
  const i = list.findIndex((x) => x.id === c.id);
  return (
    <div className="flex flex-col gap-2">
      <div>
        <Dim className="mb-0.5 block text-sm">name</Dim>
        <Input
          className="h-7 text-base"
          value={c.name}
          placeholder={c.url ? defaultName(c.url) : "dev box"}
          ariaLabel={`name for ${label}`}
          onChange={(v) => onPatch("name", v)}
        />
      </div>
      <div>
        <Dim className="mb-0.5 block text-sm">address</Dim>
        <Input
          className="h-7 text-base"
          value={c.url}
          mono
          placeholder="ws://devbox:7717/ws"
          ariaLabel={`address for ${label}`}
          onChange={(v) => onPatch("url", v)}
        />
      </div>
      <div>
        <Dim className="mb-0.5 block text-sm">token (remote only)</Dim>
        <div className="flex items-center gap-1">
          <div className="min-w-0 flex-1">
            <Input
              className="h-7 text-base"
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
        <Dim className="mb-0.5 block text-sm">
          tunnel command — recorded so it is here when you need it, never run
        </Dim>
        <Input
          className="h-7 text-base"
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
        <Dim className="block text-sm" data-probe={probe === "testing" ? "testing" : probe.ok ? "ok" : "fail"}>
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
        <Dim className="block text-sm">
          {connState === "open" ? (daemonLine ? `connected — ${daemonLine}` : "connected") : connState}
        </Dim>
      )}

      {/*
        **Follow.** `R-I21`, feature 0047. Not a switch: this machine's daemon
        opens its own connection to that one and mirrors its sessions into this
        Attention list, ranked with yours, for as long as it follows — window
        open or not. Offered on every saved entry but the one you are watching,
        which would be following itself.
      */}
      {follow === "elsewhere" ? (
        <Dim className="mt-2 block border-t border-[var(--border)] pt-2 text-sm" data-follow="elsewhere">
          Following is set up from a window on the machine that follows — this window is watching
          another machine&apos;s daemon, which only shows its own sessions to a window elsewhere.
        </Dim>
      ) : follow && !current ? (
        <div className="mt-2 flex flex-col gap-1 border-t border-[var(--border)] pt-2" data-follow>
          <Dim className="block text-sm">
            follow — this machine&apos;s daemon watches it too, and its sessions join your Attention
            list without switching
          </Dim>
          {follow.peer ? (
            <>
              <div
                data-peer-state={follow.peer.state}
                className={cn(
                  "text-sm",
                  peerTrouble(follow.peer) ? "text-[var(--amber)]" : "text-[var(--text-strong)]",
                )}
              >
                {peerLine(follow.peer)}
              </div>
              {/*
                The consent for `R-I23`, per follow and off until a hand turns
                it on (ADR-0013's 2026-09-23 amendment). It does not *grant*
                anything on the other machine: that daemon's own gates still
                decide — its `--allow-run`, and send-to-session from its own
                screen only (ADR-0003's amendment).
              */}
              <Checkbox
                checked={follow.peer.allow_writes}
                onChange={(v) => follow.peer && follow.onWrites(follow.peer.id, v)}
                label={`allow changes on ${follow.peer.host ?? label} from here`}
                title="git stage, commit, push and the rest, run configurations, forgetting a session — sent to that machine and decided by its own gates. Off, they are refused here in words."
              />
              <div className="flex items-center gap-1">
                <Button
                  variant="outline"
                  title="send the address and token above to the daemon again — how a corrected token reaches it"
                  onClick={() => follow.peer && follow.onFollow(follow.peer.allow_writes)}
                >
                  follow again
                </Button>
                <Button
                  variant="outline"
                  title="stop following — its sessions leave this Attention list"
                  onClick={() => follow.peer && follow.onUnfollow(follow.peer.id)}
                >
                  unfollow
                </Button>
              </div>
            </>
          ) : (
            <div>
              <Button
                variant="outline"
                disabled={!c.url}
                title="this machine's daemon connects to it with the token above and keeps its sessions in your Attention list"
                onClick={() => follow.onFollow(false)}
              >
                follow
              </Button>
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}
