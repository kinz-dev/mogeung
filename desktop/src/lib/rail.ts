/**
 * Which tools the right rail has open. `R-J33`.
 *
 * The rail showed one tool at a time until 2026-08-19 — press Search while
 * Files was open and Files went away. Asked for directly: *"can we display
 * multiple panel at the right hand side"*. So `prefs.rail` is a list now, and
 * these are the three things anything does to it.
 *
 * Pure, and separate from [`Rail`] for that reason: the rules worth pinning
 * here are about the *list* — that it never holds a tool twice, that it stays
 * in strip order however you got there, and that toggling is its own inverse —
 * and none of them need a rendered panel to say so.
 */

import { RAIL_TOOLS, type RailTool } from "@/store/prefs";

/** Add a tool, keeping strip order and never twice. */
export function openRail(open: readonly RailTool[], tool: RailTool): RailTool[] {
  if (open.includes(tool)) return [...open];
  return RAIL_TOOLS.filter((t) => t === tool || open.includes(t));
}

export function closeRail(open: readonly RailTool[], tool: RailTool): RailTool[] {
  return open.filter((t) => t !== tool);
}

/**
 * One key both ways — the rule the rail and the dock have shared since they
 * were built. What changed with the stack is only what "the other way" means:
 * the chord used to swap the open tool for this one, and now it adds this one
 * to what is already there.
 */
export function toggleRail(open: readonly RailTool[], tool: RailTool): RailTool[] {
  return open.includes(tool) ? closeRail(open, tool) : openRail(open, tool);
}
