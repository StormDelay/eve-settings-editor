// Pure geometry helpers for the layout canvas. No DOM, no Svelte — unit-tested
// in layout.test.ts.
import type { WindowLayout, Stack, WindowRect } from "./api";

/** Canvas px per data px. 1 when the reference has no width (empty file). */
export function canvasScale(referenceWidth: number, containerWidth: number): number {
  return referenceWidth > 0 ? containerWidth / referenceWidth : 1;
}

/** Data px -> canvas px. */
export function toCanvas(dataPx: number, scale: number): number {
  return dataPx * scale;
}

/** Canvas px -> data px, rounded to the integer the wire format stores. */
export function toData(canvasPx: number, scale: number): number {
  return scale > 0 ? Math.round(canvasPx / scale) : 0;
}

/** Windows the canvas draws: open and with valid geometry. */
export function openWindows(windows: WindowRect[]): WindowRect[] {
  return windows.filter((w) => w.open && w.renderable);
}

export type Corner = "tl" | "tr" | "bl" | "br";

/**
 * Resize a rect by dragging one corner by (dx, dy) data px. The opposite
 * corner is the fixed anchor. Size floors at 0 (matching the canvas's existing
 * resize) and the dragged corner is pinned so it can't cross the anchor.
 */
export function resizeRect(
  orig: { x: number; y: number; w: number; h: number },
  corner: Corner,
  dx: number,
  dy: number,
): { x: number; y: number; w: number; h: number } {
  const left = corner === "tl" || corner === "bl";
  const top = corner === "tl" || corner === "tr";
  let { x, y, w, h } = orig;
  if (left) {
    const anchorR = orig.x + orig.w; // right edge stays fixed
    x = Math.min(orig.x + dx, anchorR);
    w = anchorR - x;
  } else {
    w = Math.max(0, orig.w + dx); // left edge fixed, right edge moves
  }
  if (top) {
    const anchorB = orig.y + orig.h; // bottom edge stays fixed
    y = Math.min(orig.y + dy, anchorB);
    h = anchorB - y;
  } else {
    h = Math.max(0, orig.h + dy);
  }
  return { x, y, w, h };
}

export interface DrawUnit {
  key: string;
  anchor: WindowRect;
  stack: Stack | null;
  tabs: WindowRect[];
  /** Every renderable window a coherent move must repeat the rect onto: the
   * anchor, all renderable members (open AND closed — a closed member left
   * behind would drift out of the stack), and the container. Deduped. */
  fanTargets: WindowRect[];
}

/**
 * Group the open, renderable windows into draw units: one per stack (drawn at
 * the stack's anchor, with its open members as tabs in preferred order) and one
 * per non-stacked window. A stack with no open members — or whose anchor is not
 * open/renderable — is not drawn (nothing to show).
 */
export function stackUnits(layout: WindowLayout): DrawUnit[] {
  const drawn = openWindows(layout.windows);
  const byId = new Map(drawn.map((w) => [w.id, w]));
  const renderableById = new Map(layout.windows.filter((w) => w.renderable).map((w) => [w.id, w]));
  const units: DrawUnit[] = [];
  const claimed = new Set<string>();

  for (const s of layout.stacks) {
    const tabs = s.members.map((id) => byId.get(id)).filter((w): w is WindowRect => !!w);
    // A stack with no open members has nothing to show — hide it, and claim its
    // (possibly open) container so it doesn't fall through as a lone window.
    if (tabs.length === 0) {
      claimed.add(s.container_id);
      continue;
    }
    const anchor = byId.get(s.anchor_id);
    if (!anchor) continue; // anchor not open/renderable — skip the stack
    // The container itself is not a tab unless it is also a member.
    for (const w of tabs) claimed.add(w.id);
    claimed.add(s.container_id);
    const fanIds = new Set<string>([s.anchor_id, s.container_id, ...s.members]);
    const fanTargets = [...fanIds].map((id) => renderableById.get(id)).filter((w): w is WindowRect => !!w);
    units.push({ key: s.container_id, anchor, stack: s, tabs, fanTargets });
  }
  for (const w of drawn) {
    if (claimed.has(w.id)) continue;
    units.push({ key: w.id, anchor: w, stack: null, tabs: [w], fanTargets: [w] });
  }
  return units;
}
