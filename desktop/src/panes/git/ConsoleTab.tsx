/**
 * What the window asked git, and what git said — per session, in order,
 * refusals verbatim. `R-D29`.
 *
 * It exists because writes fail loudly and a red banner is a poor place to
 * keep git's own words. The rows are the wire commands this window sent,
 * not the argv the daemon ran; the header says so.
 */

import { useEffect, useRef } from "react";
import { Copy, Trash2 } from "lucide-react";
import { useStore } from "@/store";
import { Dim, Empty, IconButton, Mono } from "@/ui/primitives";
import { consoleText } from "@/lib/gitConsole";
import { copyText } from "@/lib/gitActions";
import { cn } from "@/lib/cn";

const NONE: never[] = [];

export function ConsoleTab({ id }: { id: string }) {
  const rows = useStore((s) => s.git[id]?.console ?? NONE);
  const patchGit = useStore((s) => s.patchGit);
  const endRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    endRef.current?.scrollIntoView?.({ block: "end" });
  }, [rows.length]);
  const t = (ms: number) => {
    const d = new Date(ms);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
  };
  return (
    <div className="flex h-full min-h-0 w-full flex-col">
      <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
        <span className="text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">Console</span>
        <Dim className="truncate text-2xs">the commands this window sent, and what came back — git's refusals in its own words</Dim>
        <div className="ml-auto flex items-center gap-0.5">
          <IconButton title="copy the console as text" disabled={rows.length === 0} onClick={() => copyText(consoleText(rows))}>
            <Copy size={12} />
          </IconButton>
          <IconButton title="clear" disabled={rows.length === 0} onClick={() => patchGit(id, { console: [] })}>
            <Trash2 size={12} />
          </IconButton>
        </div>
      </div>
      {rows.length === 0 ? (
        <Empty hint="every git question this window asks lands here as it is asked">nothing yet</Empty>
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full border-collapse text-xs">
            <tbody>
              {rows.map((r, i) => (
                <tr key={`${r.at}:${i}`} className="border-b border-[var(--border)] align-top">
                  <td className="w-px py-0.5 pr-2 pl-2 whitespace-nowrap">
                    <Dim className="text-2xs tabular-nums">{t(r.at)}</Dim>
                  </td>
                  <td className="w-px py-0.5 pr-3 whitespace-nowrap">
                    <Mono className="text-2xs text-[var(--text)]">{r.cmd}</Mono>
                    {r.args && <Mono className="ml-1 text-2xs text-[var(--dim)]">{r.args}</Mono>}
                  </td>
                  <td className="py-0.5 pr-2">
                    {r.error ? (
                      <Mono
                        className="text-2xs whitespace-pre-wrap text-[var(--red)]"
                        title="landed on the latest command still waiting — the wire's error carries no address"
                      >
                        {r.error}
                      </Mono>
                    ) : r.result !== null ? (
                      <span className={cn("text-2xs whitespace-pre-wrap text-[var(--dim)]")}>{r.result}</span>
                    ) : (
                      <Dim className="text-2xs">…</Dim>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div ref={endRef} />
        </div>
      )}
    </div>
  );
}
