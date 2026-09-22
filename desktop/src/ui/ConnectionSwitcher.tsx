/**
 * Switch daemons from the top bar, and see which ones are alive. `R-I20`.
 *
 * Asked 2026-09-22: *"can we make a drop down on the UI for the saved remote
 * host? The drop down should show the status as well if the remote daemon is
 * alive or not."* The Connections panel is where a connection is made and
 * edited; this is where you move between the ones you have.
 *
 * **Status is asked when the menu opens, not polled.** The shell checks each
 * saved daemon's `/api/health` with that entry's own token (`connections_status`
 * — the token never leaves the `0600` file to do it), so each row says *up*,
 * *refused* (answering, token turned away), *down* or *unknown*. A daemon also
 * heard on the LAN (`R-I17`) adds its counts, which the beacon already carries.
 * A browser tab has no shell to ask, and says so.
 *
 * Choosing a row is the same switch the panel's *connect* is — `setUrl` with
 * the composed address, which drops the board and refills it.
 */

import { useEffect, useState } from "react";
import * as Menu from "@radix-ui/react-dropdown-menu";
import { ChevronDown, Settings2 } from "lucide-react";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { interactive, surface } from "@/ui/styles";
import {
  connectionsStatus,
  defaultName,
  dialledUrl,
  loadConnections,
  sameDaemon,
  withCurrent,
  type Connection,
  type Liveness,
} from "@/lib/connections";
import { hostPortOf, lanPeers, matchesSaved, type FoundDaemon } from "@/lib/lan";

type Checked = Record<string, Liveness> | "checking" | "browser";

const DOT: Record<Liveness["state"] | "checking", string> = {
  up: "var(--green)",
  refused: "var(--amber)",
  down: "var(--red)",
  unknown: "var(--dim)",
  checking: "var(--dim)",
};

function words(l: Liveness | undefined, checked: Checked): string {
  if (checked === "browser") return "status: only the desktop window can check";
  if (checked === "checking" || !l) return "checking…";
  switch (l.state) {
    case "up":
      return `up${l.host ? ` — ${l.host}` : ""}${l.version ? ` · ${l.version}` : ""}`;
    case "refused":
      return l.detail ?? "answering, and the token was refused";
    case "down":
      return "down — nothing answered";
    case "unknown":
      return l.detail ?? "unknown";
  }
}

export function ConnectionSwitcher() {
  const url = useStore((s) => s.url);
  const conn = useStore((s) => s.conn);
  const setUrl = useStore((s) => s.setUrl);
  const [list, setList] = useState<Connection[]>([]);
  const [checked, setChecked] = useState<Checked>("checking");
  const [heard, setHeard] = useState<FoundDaemon[]>([]);
  const [open, setOpen] = useState(false);

  // The trigger's name has to be right before the menu has ever opened, so
  // the list is read whenever the address changes, and again on every open.
  useEffect(() => {
    let alive = true;
    void loadConnections().then((l) => {
      if (alive) setList(withCurrent(l, url));
    });
    return () => {
      alive = false;
    };
  }, [url]);

  // Asked each time the menu opens, from an effect rather than from Radix's
  // callback — side effects belong in effects, and a stale answer from the
  // last opening is dropped rather than painted over the new one.
  useEffect(() => {
    if (!open) return;
    let alive = true;
    void loadConnections().then((l) => {
      if (alive) setList(withCurrent(l, url));
    });
    setChecked("checking");
    void connectionsStatus().then((all) => {
      if (!alive) return;
      if (all === null) {
        setChecked("browser");
        return;
      }
      const byId: Record<string, Liveness> = {};
      for (const l of all) byId[l.id] = l;
      setChecked(byId);
    });
    void lanPeers().then((a) => {
      if (alive) setHeard("peers" in a ? a.peers : []);
    });
    return () => {
      alive = false;
    };
  }, [open, url]);

  const current = list.find((c) => sameDaemon(c.url, url));
  const label = current?.name || (url ? defaultName(url) : "no daemon");

  return (
    <Menu.Root modal={false} open={open} onOpenChange={setOpen}>
      <Menu.Trigger asChild>
        <button
          type="button"
          aria-label="switch daemon"
          title={`watching ${label} — switch to another saved daemon`}
          className={cn(
            "inline-flex h-5 shrink-0 items-center gap-1 rounded-sm border border-transparent px-1.5 text-xs whitespace-nowrap",
            interactive,
            "hover:border-[var(--border)] data-[state=open]:bg-[var(--state-focus)]",
          )}
        >
          <span className="max-w-40 truncate">{label}</span>
          <ChevronDown size={11} className="opacity-70" />
        </button>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Content
          align="start"
          sideOffset={4}
          collisionPadding={8}
          className={cn(surface.overlay, "z-50 max-h-[28rem] min-w-80 overflow-y-auto p-1")}
        >
          <Menu.Label className="px-2 pt-1.5 pb-1 text-xs tracking-wider text-[var(--dim)] uppercase">
            switch to
          </Menu.Label>
          {list.length === 0 && (
            <div className="px-2 py-1.5 text-sm text-[var(--dim)]">no saved daemons yet</div>
          )}
          {list.map((c) => {
            const isCurrent = sameDaemon(c.url, url);
            const live = typeof checked === "object" ? checked[c.id] : undefined;
            const state =
              typeof checked === "object" ? (live?.state ?? "unknown") : checked === "browser" ? "unknown" : "checking";
            const beacon = heard.find((d) => !d.gone && matchesSaved(c, d));
            return (
              <Menu.Item
                key={c.id}
                data-connection={c.name}
                data-liveness={state}
                onSelect={() => {
                  if (!isCurrent) setUrl(dialledUrl(c));
                }}
                className="flex cursor-default items-start gap-2 rounded-sm px-2 py-1.5 outline-none select-none data-[highlighted]:bg-[var(--state-focus)]"
              >
                <span
                  aria-hidden
                  className={cn("mt-1.5 h-2 w-2 shrink-0 rounded-full", state === "checking" && "animate-pulse")}
                  style={{ background: DOT[state] }}
                />
                <span className="min-w-0 flex-1">
                  <span className="flex items-baseline gap-2">
                    <span className={cn("truncate text-sm", isCurrent && "font-medium text-[var(--text-strong)]")}>
                      {c.name || defaultName(c.url)}
                    </span>
                    <span className="shrink-0 font-mono text-xs text-[var(--dim)]">{hostPortOf(c.url) ?? ""}</span>
                    {isCurrent && (
                      <span className="shrink-0 text-xs text-[var(--blue)]">
                        {conn === "open" ? "watching" : conn}
                      </span>
                    )}
                  </span>
                  <span className="block truncate text-xs text-[var(--dim)]">
                    {words(live, checked)}
                    {beacon ? ` · ${beacon.live} live · ${beacon.waiting} need you` : ""}
                  </span>
                </span>
              </Menu.Item>
            );
          })}
          <Menu.Separator className="my-1 h-px bg-[var(--border)]" />
          <Menu.Item
            onSelect={() => useStore.setState({ showConnections: true })}
            className="flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-none select-none data-[highlighted]:bg-[var(--state-focus)]"
          >
            <Settings2 size={12} className="text-[var(--dim)]" />
            manage connections…
          </Menu.Item>
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  );
}
