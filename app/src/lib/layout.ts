// Pure geometry helpers for the layout canvas. No DOM, no Svelte — unit-tested
// in layout.test.ts.
import type { WindowRect } from "./api";

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
