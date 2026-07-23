import bundle from "./data/overview-states.json" with { type: "json" };

const STATES: Record<string, string> = bundle.states;

/** Human label for a state id, or null when EVE stores the id but never shows
 *  it (id 68) or the bundle predates it. Callers render `#<id>` for null. */
export function stateLabel(id: number): string | null {
  return STATES[String(id)] ?? null;
}

/** States offered on a preset's Exceptions list — the 22 pilot states plus the
 *  two Wreck states. Excludes 68, which the client never renders. */
export const EXCEPTION_STATES: number[] = bundle.exceptionStates;

/** EVE's Exceptions tab offers exactly three choices per state. The two stored
 *  lists are disjoint on real files, and this tri-state is what keeps them so. */
export type Exception = "show" | "hide" | "always";

export function exceptionOf(filtered: number[], alwaysShown: number[], id: number): Exception {
  if (filtered.includes(id)) return "hide";
  if (alwaysShown.includes(id)) return "always";
  return "show";
}

export function applyException(
  filtered: number[], alwaysShown: number[], id: number, choice: Exception,
): { filtered: number[]; alwaysShown: number[] } {
  const f = filtered.filter((n) => n !== id);
  const a = alwaysShown.filter((n) => n !== id);
  if (choice === "hide") f.push(id);
  if (choice === "always") a.push(id);
  return { filtered: f, alwaysShown: a };
}

const clamp = (n: number) => Math.max(0, Math.min(255, Math.round(n * 255)));

/** EVE stores colours as 0..1 floats; <input type="color"> speaks #rrggbb. */
export function rgbaToHex(rgba: [number, number, number, number]): string {
  const [r, g, b] = rgba;
  return "#" + [r, g, b].map((c) => clamp(c).toString(16).padStart(2, "0")).join("");
}

/** Alpha is not exposed in the UI — every observed entry is 1.0 — so the
 *  caller passes the stored alpha through unchanged rather than resetting it. */
export function hexToRgba(hex: string, alpha: number): [number, number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255, alpha];
}

/** Move one entry of a priority order. Length is invariant, so an id the client
 *  never renders (68) rides along instead of being dropped. */
export function moveInOrder(order: number[], from: number, to: number): number[] {
  const next = [...order];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved);
  return next;
}

/** EVE's built-in background colour for a state the file stores no override
 *  for, or null when we have no sample for it (the table lives in client
 *  script, so it is harvested from EVE's own screen — see the bundle's
 *  `_defaultColorsNote`). Display only: never written to a file. */
export function defaultColor(id: number): string | null {
  return (bundle.defaultColors as Record<string, string>)[String(id)] ?? null;
}

export const DEFAULT_BACKGROUND_ORDER: number[] = bundle.defaultBackgroundOrder;
export const DEFAULT_BACKGROUND_STATES: number[] = bundle.defaultBackgroundStates;
export const DEFAULT_FLAG_ORDER: number[] = bundle.defaultFlagOrder;
export const DEFAULT_FLAG_STATES: number[] = bundle.defaultFlagStates;
