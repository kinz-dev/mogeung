/**
 * Add, name, switch and forget daemons without a restart. `R-I7`.
 *
 * Before this the address came from `?url=` or from whatever localStorage
 * happened to hold, so moving between a laptop's daemon and a dev box's meant
 * editing a query string — the store's `setUrl` had done the hard part for
 * months with nothing calling it.
 *
 * Switching drops the whole board on purpose. Session ids do not survive the
 * move, and a queue from the machine you just left is worse than an empty one:
 * it looks like current information about work that is not in front of you.
 */

import { useState } from "react";
import { Check, Plus, Trash2 } from "lucide-react";
import { useStore } from "@/store";
import { Dialog } from "@/ui/Dialog";
import { Button, Dim, Input, Mono, Row } from "@/ui/primitives";
import {
  defaultName,
  loadConnections,
  saveConnections,
  withCurrent,
  type Connection,
} from "@/lib/connections";

export function ConnectionsWindow() {
  const open = useStore((s) => s.showConnections);
  const url = useStore((s) => s.url);
  const daemon = useStore((s) => s.daemon);
  const conn = useStore((s) => s.conn);
  const setUrl = useStore((s) => s.setUrl);
  const [list, setList] = useState<Connection[]>(() => withCurrent(loadConnections(), url));
  const [draftUrl, setDraftUrl] = useState("");
  const [draftName, setDraftName] = useState("");

  if (!open) return null;

  const close = () => useStore.setState({ showConnections: false });

  const write = (next: Connection[]) => {
    setList(next);
    // Written on change rather than on close: a dialog dismissed with Escape
    // must not lose the daemon you just added.
    saveConnections(next);
  };

  const add = () => {
    const u = draftUrl.trim();
    if (!u) return;
    write([...list, { name: draftName.trim() || defaultName(u), url: u }]);
    setDraftUrl("");
    setDraftName("");
  };

  return (
    <Dialog
      title="Connections"
      subtitle="which daemon this window is watching"
      onClose={close}
    >
      <div className="min-w-[28rem]">
        {list.map((c, i) => {
          const current = c.url === url;
          return (
            <Row
              key={c.url}
              selected={current}
              onClick={() => {
                if (current) return;
                setUrl(c.url);
                close();
              }}
              className="flex items-center gap-2 border-b border-[var(--border)] px-2 py-1"
            >
              <span className="w-3 shrink-0">
                {current && <Check size={11} className="text-[var(--green)]" />}
              </span>
              <div className="min-w-0 flex-1">
                <Input
                  value={c.name}
                  ariaLabel={`name for ${c.url}`}
                  onChange={(v) =>
                    write(list.map((x, j) => (j === i ? { ...x, name: v } : x)))
                  }
                />
                <Mono className="mt-0.5 block truncate text-2xs text-[var(--dim)]">{c.url}</Mono>
                {current && (
                  <Dim className="block text-2xs">
                    {conn === "open"
                      ? daemon
                        ? `connected — ${daemon.host ?? "unknown host"} · ${daemon.version}`
                        : "connected"
                      : conn}
                  </Dim>
                )}
              </div>
              <Button
                variant="outline"
                title={
                  current
                    ? "this is the daemon you are watching — switch away before forgetting it"
                    : "forget this daemon"
                }
                disabled={current}
                onClick={(e) => {
                  e.stopPropagation();
                  write(list.filter((_, j) => j !== i));
                }}
              >
                <Trash2 size={11} />
              </Button>
            </Row>
          );
        })}

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
          The egui window can also browse the LAN for daemons. That needs multicast, which
          a webview cannot do — it would have to move into the Tauri half, and has not.
        </Dim>
      </div>
    </Dialog>
  );
}
