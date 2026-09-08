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
import { Dialog } from "@/ui/Dialog";
import { Button, Dim, Input, Loading, Row } from "@/ui/primitives";
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

  if (!open) return null;

  const close = () => useStore.setState({ showConnections: false });

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
