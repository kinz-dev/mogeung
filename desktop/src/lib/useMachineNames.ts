/**
 * Your names for followed machines, by machine id, from the store — the
 * saved connections' names as they read now, else each follow's own
 * (`machineNames` in `lib/federation.ts`). `R-J139` for the Attention list,
 * `R-J140` for the New session window.
 */

import { useMemo } from "react";
import { useStore } from "@/store";
import { machineNames } from "@/lib/federation";

export function useMachineNames(): Record<string, string> {
  const peers = useStore((s) => s.peers);
  const saved = useStore((s) => s.savedConnections);
  return useMemo(() => machineNames(peers, saved), [peers, saved]);
}
