// The overview column rules the frontend still needs on its own. The rules
// themselves — widths, cuts, fades, text measured by EVE's engine — live in
// app/src-tauri/src/overview_fit.rs, which the width preview and the MCP tool
// both read; see docs/format-notes.md, "Overview column rendering".

export const OV = {
  /** COLUMNMINSIZE: a stored width below this is drawn at this. */
  minWidth: 24,
} as const;

const FIXED: Record<string, number> = { ICON: 22 };
const DEFAULTS: Record<string, number> = { NAME: 112, TYPE: 112, VELOCITY: 58, ANGULARVELOCITY: 58 };

/**
 * The width EVE draws a column at, for the layout canvas's column bands.
 * MIRRORS `overview_fit::effective_width` minus its header-fit growth (the
 * canvas measures no text); keep the two tables in step.
 */
export function effectiveWidth(name: string, stored: number | null): number {
  if (name in FIXED) return FIXED[name];
  if (stored !== null) return Math.max(OV.minWidth, stored);
  return Math.max(OV.minWidth, DEFAULTS[name] ?? 80);
}

/** Where a label's fade begins, as a 0..1 stop along its visible width: fully
 *  opaque before it, transparent at the clip edge. */
export function fadeStart(clipW: number, fadeW: number): number {
  return clipW > 0 ? Math.max(0, (clipW - fadeW) / clipW) : 0;
}
