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

export const DEFAULT_BACKGROUND_ORDER: number[] = bundle.defaultBackgroundOrder;
export const DEFAULT_BACKGROUND_STATES: number[] = bundle.defaultBackgroundStates;
export const DEFAULT_FLAG_ORDER: number[] = bundle.defaultFlagOrder;
export const DEFAULT_FLAG_STATES: number[] = bundle.defaultFlagStates;
