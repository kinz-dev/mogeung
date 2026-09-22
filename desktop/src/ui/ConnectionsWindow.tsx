/**
 * Add, name, edit, order, and forget daemons without a restart. `R-I7`, `R-I16`.
 *
 * Before `R-I7` the address came from `?url=` or from whatever localStorage
 * happened to hold, so moving between a laptop's daemon and a dev box's meant
 * editing a query string — the store's `setUrl` had done the hard part for
 * months with nothing calling it.
 *
 * `R-I16` finished the CRUD that row started. The three things it adds are all
 * consequences of one change: an entry is identified by `id` rather than by its
 * URL, so the **address becomes editable** (a typo was forget-and-add before),
 * the list becomes **orderable**, and a **token** gets a field of its own
 * instead of being smuggled through the address into localStorage. The list
 * now rests in `~/.mogeung/connections.json` at `0600` — see
 * [ADR-0036](../../../docs/decisions/0036-the-connection-list-is-the-clients-and-its-file-is-the-shells.md).
 *
 * Switching drops the whole board on purpose. Session ids do not survive the
 * move, and a queue from the machine you just left is worse than an empty one:
 * it looks like current information about work that is not in front of you.
 */

import { useEffect, useState } from "react";
import { Check, ChevronDown, ChevronUp, Plus, Trash2 } from "lucide-react";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { Dialog } from "@/ui/Dialog";
import { Button, Dim, Input, Loading, Row } from "@/ui/primitives";
import {
  agoLabel,
  foundName,
  lanPeers,
  matchesSaved,
  onLanPeers,
  reachLabel,
  wsUrlOf,
  type FoundDaemon,
  type LanAnswer,
} from "@/lib/lan";
import {
  defaultName,
  dialledUrl,
  loadConnections,
  newId,
  reorder,
  saveConnections,
  storageKind,
  withCurrent,
  type Connection,
} from "@/lib/connections";

export function ConnectionsWindow() {
  const open = useStore((s) => s.showConnections);
  const url = useStore((s) => s.url);
  const daemon = useStore((s) => s.daemon);
  const conn = useStore((s) => s.conn);
  const setUrl = useStore((s) => s.setUrl);

  // A load rather than a value, since ADR-0036 — so there is a tick with no
  // rows, and an empty list rendered during it reads as a list that was lost.
  const [list, setList] = useState<Connection[] | null>(null);
  const [draftUrl, setDraftUrl] = useState("");
  const [draftName, setDraftName] = useState("");
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
      if (alive) setList(withCurrent(loaded, url));
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

  const heard: FoundDaemon[] = lan && "peers" in lan ? lan.peers : [];
  /** Is this saved connection the daemon being heard right now? */
  const heardNow = (c: Connection) => heard.some((d) => !d.gone && matchesSaved(c, d));

  const write = (next: Connection[]) => {
    setList(next);
    // Written on change rather than on close: a dialog dismissed with Escape
    // must not lose the daemon you just added.
    void saveConnections(next);
  };

  const patch = (i: number, field: keyof Connection, value: string) => {
    if (!list) return;
    write(
      list.map((x, j) =>
        j === i ? { ...x, [field]: value || undefined, ...(field === "url" ? { url: value } : {}) } : x,
      ),
    );
  };

  const add = () => {
    const u = draftUrl.trim();
    if (!u || !list) return;
    write([...list, { id: newId(), name: draftName.trim() || defaultName(u), url: u }]);
    setDraftUrl("");
    setDraftName("");
  };

  return (
    <Dialog
      title="Connections"
      subtitle="which daemon this window is watching"
      onClose={close}
    >
      <div className="min-w-[32rem]">
        {list === null ? (
          <Loading what="connections" />
        ) : (
          list.map((c, i) => {
            const current = c.url === url;
            return (
              <div
                key={c.id}
                className="border-b border-[var(--border)] px-2 py-1.5"
                data-current={current || undefined}
              >
                <Row
                  selected={current}
                  onClick={() => {
                    if (current) return;
                    // Composed here, so the token reaches the socket without
                    // ever being part of what this panel displays.
                    setUrl(dialledUrl(c));
                    close();
                  }}
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
                  <div className="min-w-0 flex-1">
                    <Input
                      value={c.name}
                      ariaLabel={`name for ${c.name}`}
                      onChange={(v) => patch(i, "name", v)}
                    />
                  </div>
                  <div className="flex shrink-0 items-center gap-0.5" onClick={(e) => e.stopPropagation()}>
                    <Button
                      variant="outline"
                      title="move up"
                      disabled={i === 0}
                      onClick={() => write(reorder(list, i, i - 1))}
                    >
                      <ChevronUp size={11} />
                    </Button>
                    <Button
                      variant="outline"
                      title="move down"
                      disabled={i === list.length - 1}
                      onClick={() => write(reorder(list, i, i + 1))}
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
                      onClick={() => write(list.filter((_, j) => j !== i))}
                    >
                      <Trash2 size={11} />
                    </Button>
                  </div>
                </Row>

                <div className="mt-1 grid grid-cols-[1fr_11rem] gap-1 pl-5">
                  <div>
                    <Dim className="mb-0.5 block text-2xs">address</Dim>
                    <Input
                      value={c.url}
                      mono
                      ariaLabel={`address for ${c.name}`}
                      onChange={(v) => patch(i, "url", v)}
                    />
                  </div>
                  <div>
                    <Dim className="mb-0.5 block text-2xs">token (remote only)</Dim>
                    <Input
                      value={c.token ?? ""}
                      secret
                      placeholder="none"
                      ariaLabel={`token for ${c.name}`}
                      onChange={(v) => patch(i, "token", v)}
                    />
                  </div>
                </div>

                <div className="mt-1 pl-5">
                  <Dim className="mb-0.5 block text-2xs">
                    tunnel command — recorded so it is here when you need it, never run
                  </Dim>
                  <Input
                    value={c.note ?? ""}
                    mono
                    placeholder="ssh -L 7717:localhost:7717 devbox"
                    ariaLabel={`tunnel command for ${c.name}`}
                    onChange={(v) => patch(i, "note", v)}
                  />
                </div>

                {current && (
                  <Dim className="mt-1 block pl-5 text-2xs">
                    {conn === "open"
                      ? daemon
                        ? `connected — ${daemon.host ?? "unknown host"} · ${daemon.version}`
                        : "connected"
                      : conn}
                  </Dim>
                )}
              </div>
            );
          })
        )}

        {/*
          Found on your LAN. `R-I17`: every daemon announces itself by default
          (ADR-0039), the shell hears it, and this is the list. **Finding is
          never connecting** — a row fills the form below, a hand types the
          token, and Connect on the saved row is the switch that already exists.
        */}
        <div className="mt-3 border-t border-[var(--border)] pt-2">
          <Dim className="mb-1 block text-2xs">
            found on your LAN — daemons announcing themselves; nothing connects on its own
          </Dim>
          {lan === null ? null : "browser" in lan ? (
            <Dim className="block text-2xs">
              Not listening for beacons here: only the desktop window can, and it lists the
              daemons found on your LAN.
            </Dim>
          ) : "error" in lan ? (
            <Dim className="block text-2xs text-[var(--amber)]" data-lan-error>
              {lan.error}
            </Dim>
          ) : lan.peers.length === 0 ? (
            <Dim className="block text-2xs">
              nothing heard yet — a daemon on this network announces itself within 15 s
            </Dim>
          ) : (
            lan.peers.map((d) => {
              const saved = list?.find((c) => matchesSaved(c, d));
              return (
                <div
                  key={`${d.machine}\u0001${d.home}`}
                  data-found={foundName(d)}
                  data-gone={d.gone || undefined}
                  data-mine={d.mine || undefined}
                  className={cn("flex items-center gap-2 px-2 py-1", d.gone && "opacity-50")}
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
                      {d.mine && <Dim className="ml-1 text-2xs">(this machine)</Dim>}
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
                    <Dim className="shrink-0 text-2xs">saved as {saved.name}</Dim>
                  ) : (
                    <Button
                      variant="outline"
                      title="fill the form below with this daemon — you supply the token, then connect"
                      onClick={() => {
                        setDraftUrl(wsUrlOf(d));
                        setDraftName(d.host ?? "");
                      }}
                    >
                      <Plus size={11} /> add
                    </Button>
                  )}
                </div>
              );
            })
          )}
        </div>

        <div className="mt-2 flex items-end gap-1">
          <div className="flex-1">
            <Dim className="mb-0.5 block text-2xs">address</Dim>
            <Input
              value={draftUrl}
              mono
              placeholder="ws://devbox:7717/ws"
              onChange={setDraftUrl}
              onKeyDown={(e) => e.key === "Enter" && add()}
            />
          </div>
          <div className="w-40">
            <Dim className="mb-0.5 block text-2xs">name (optional)</Dim>
            <Input
              value={draftName}
              placeholder={draftUrl ? defaultName(draftUrl) : "dev box"}
              onChange={setDraftName}
              onKeyDown={(e) => e.key === "Enter" && add()}
            />
          </div>
          <Button variant="outline" onClick={add}>
            <Plus size={11} /> add
          </Button>
        </div>

        <Dim className="mt-2 block text-2xs">
          Switching clears the board: session ids do not survive the move, and a queue from
          the machine you just left is worse than an empty one. A daemon on another host
          needs to be reachable — `ssh -L 7717:localhost:7717 host` is the usual way, and
          local-versus-remote is then decided by the daemon's identity rather than by the
          address you dialled (`R-I5`).
        </Dim>
        <Dim className="mt-1 block text-2xs">
          {storageKind() === "file" ? (
            <>
              Kept in <code>~/.mogeung/connections.json</code> on this machine, readable only
              by you — not on the daemon&apos;s machine, and not synced anywhere.
            </>
          ) : (
            <>
              <strong>This is a browser tab, so the list is in this browser&apos;s storage</strong>{" "}
              rather than in <code>~/.mogeung/connections.json</code>. A token typed here is
              not protected by the file&apos;s permissions — use the desktop window for
              anything you mean to keep.
            </>
          )}
        </Dim>
      </div>
    </Dialog>
  );
}
