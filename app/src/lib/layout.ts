// Pure geometry helpers for the layout canvas. No DOM, no Svelte — unit-tested
// in layout.test.ts.
import type { WindowLayout, Stack, WindowRect, Hud } from "./api";
import { isClutter, nameOf, type ClutterOverrides } from "./windowLabels.ts";

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
 *
 * `visible`, when given, is the set of window ids the filter admits. It narrows
 * which units are drawn and which tabs a stack shows, but NOT the unit's anchor
 * or its `fanTargets`: the anchor is where the rectangle's geometry comes from
 * and what a drag starts at, and `fanTargets` is what a drag writes to, so
 * filtering either would move the stack or strand its hidden members.
 */
export function stackUnits(layout: WindowLayout, visible: Set<string> | null = null): DrawUnit[] {
  const drawn = openWindows(layout.windows);
  const byId = new Map(drawn.map((w) => [w.id, w]));
  const renderableById = new Map(layout.windows.filter((w) => w.renderable).map((w) => [w.id, w]));
  const units: DrawUnit[] = [];
  const claimed = new Set<string>();
  const shown = (id: string) => visible === null || visible.has(id);

  for (const s of layout.stacks) {
    const tabs = s.members
      .map((id) => byId.get(id))
      .filter((w): w is WindowRect => !!w && shown(w.id));
    // A stack with no open members has nothing to show — hide it, and claim its
    // (possibly open) container so it doesn't fall through as a lone window.
    // Under a filter this also covers "no member matched".
    if (tabs.length === 0) {
      claimed.add(s.container_id);
      for (const id of s.members) claimed.add(id);
      continue;
    }
    const anchor = byId.get(s.anchor_id);
    if (!anchor) continue; // anchor not open/renderable — skip the stack
    // The container itself is not a tab unless it is also a member.
    for (const w of tabs) claimed.add(w.id);
    claimed.add(s.container_id);
    for (const id of s.members) claimed.add(id);
    const fanIds = new Set<string>([s.anchor_id, s.container_id, ...s.members]);
    const fanTargets = [...fanIds].map((id) => renderableById.get(id)).filter((w): w is WindowRect => !!w);
    units.push({ key: s.container_id, anchor, stack: s, tabs, fanTargets });
  }
  for (const w of drawn) {
    if (claimed.has(w.id) || !shown(w.id)) continue;
    units.push({ key: w.id, anchor: w, stack: null, tabs: [w], fanTargets: [w] });
  }
  return units;
}

/** How many windows a set of draw units actually paints: a stack unit draws one
 * rectangle but represents each of its visible tabs, a free unit exactly one.
 * The counter reports windows, not rectangles — "showing 3 of 68 windows". */
export function drawnWindowCount(units: DrawUnit[]): number {
  return units.reduce((n, u) => n + (u.stack ? u.tabs.length : 1), 0);
}

export interface FurnitureRect {
  kind: "neocom" | "shipui" | "fighter" | "badge";
  label: string;
  /** Data px, like WindowRect.geom — the canvas scales it with toCanvas. */
  x: number;
  y: number;
  w: number;
  h: number;
  drag: "none" | "x" | "xy";
}

// EVE stores anchors but never sizes, and never says what an anchor is relative
// to, so all of this began as assumption. The 2026-07-27 live session settled
// the conventions: the ship offset is centre-relative (and was already written
// that way), the fighter UI's x is its left edge, its y is wrong by a constant.
// ponytail: the nominal SIZES below are still invented — measure them off one
// native-resolution screenshot, see HUD_NOMINAL.
/**
 * Drawn sizes for the screen furniture, in data px. Still INVENTED, and two
 * live sessions failed to measure the ship HUD: it has **no hard edge in-game**
 * — the module racks fade out with no frame and nothing to butt against another
 * window — so there is no crisp boundary to align or photograph. A native crop
 * came out 751x258 including margin, which makes 686 a plausible underestimate
 * on width and 250 about right on height, but that is not a measurement.
 *
 * Low stakes, which is why it stays invented. The width CANCELS out of a drag
 * (see shipOffsetFromX), so nothing we write to a file depends on it. What it
 * does affect is how the element is DRAWN, and therefore where 0.17.0's edge
 * snapping puts a window that snaps against the ship HUD — a fuzzy target
 * anyway, given the thing has no real edge.
 *
 * If a number is ever wanted: the HUD's centre is `reference_w / 2 + offset`
 * exactly (confirmed), so one edge measured against that centre gives the width
 * without needing to find both.
 */
export const HUD_NOMINAL = {
  shipui: { w: 686, h: 250 },
  fighter: { w: 400, h: 120 },
  badge: { w: 32, h: 32 },
};

const byName = (hud: Hud, name: string) => hud.entries.find((e) => e.name === name);

/** A field's number: its value, else its default; null when not writable at all. */
export function hudNum(hud: Hud, name: string): number | null {
  const e = byName(hud, name);
  if (!e || e.set.how === "unavailable") return null;
  const n = parseFloat(e.value ?? e.default);
  return Number.isFinite(n) ? n : null;
}

export function hudFlag(hud: Hud, name: string): boolean {
  const e = byName(hud, name);
  if (!e || e.set.how === "unavailable") return false;
  return (e.value ?? e.default) === "true";
}

/** Stored offset for a ship-HUD rect at data-px `x`. Inverse of hudRects'
 * ship-HUD placement below — the two must be corrected together.
 *
 * CONFIRMED in-game 2026-07-27: the offset is centre-relative, it anchors the
 * HUD's OWN centre, and negative is leftward. Writing 0.0 drew the HUD dead
 * centre (which also rules out a left-edge origin); dragging it left made the
 * client write -642.0. See docs/format-notes.md § "HUD anchors".
 *
 * This reads HUD_NOMINAL.shipui.w, but the width CANCELS: hudRects draws at
 * `centre + offset - w/2` and this adds the same `w/2` back, so the offset
 * written for a given on-screen centre does not depend on it. Pinned by a test
 * in layout.test.ts, because the opposite is an easy thing to assume. */
export function shipOffsetFromX(x: number, referenceW: number): number {
  return Math.round(x + HUD_NOMINAL.shipui.w / 2 - referenceW / 2);
}

/**
 * Stored (x, y) for a fighter/badge rect at data-px `x, y`. Inverse of
 * hudRects' fighter/badge placement below.
 *
 * `x` is CONFIRMED in-game 2026-07-27 for the fighter UI: it is the panel's
 * left edge in absolute screen pixels, origin at the screen's left. Dragged to
 * a measurable mid-screen position the client stored 839 against 838 measured,
 * and 0 is the leftmost value a drag can produce.
 *
 * `y` is CONFIRMED too, and is the panel's top edge with the origin at the
 * screen's top — i.e. exactly what this already did. Session A appeared to show
 * it wrong by ~234px; that reading was mistaken twice over. The corner drag it
 * came from CLAMPS, and the fighter panel's *visible* extent changes with
 * whether the fighter-ability grid is drawn: with no fighters only the squad row
 * shows, and that row sits ~150px below the panel's anchor, so the panel looked
 * displaced when only its lower half was on screen.
 *
 * Settled 2026-07-28 by writing y = 497 — the exact top edge of A1's D-Scan
 * window — and photographing the two together: with fighters up, the ability
 * grid's top lines up with D-Scan's top. The client then wrote (839, 497) back
 * unchanged. The anchor is where the ability grid starts whether or not it is
 * drawn, which is why it is stable.
 */
export function hudPointFromRect(kind: FurnitureRect["kind"], x: number, y: number): { x: number; y: number } {
  return { x: Math.round(x), y: Math.round(y) };
}

/**
 * The screen furniture the canvas draws, in data px. Order is fixed
 * (neocom, ship HUD, fighter, badge) so the canvas paints the bar first and
 * tests can index. An element whose values aren't writable is omitted rather
 * than drawn at a guessed position.
 */
export function hudRects(hud: Hud, layout: WindowLayout): FurnitureRect[] {
  const out: FurnitureRect[] = [];

  const neocom = hudNum(hud, "neocom_width");
  if (neocom !== null && neocom > 0) {
    out.push({ kind: "neocom", label: "Neocom", x: 0, y: 0, w: neocom, h: layout.reference_h, drag: "none" });
  }

  // Centre-relative placement, anchoring the HUD's own centre — confirmed
  // in-game 2026-07-27, see shipOffsetFromX. `centre + offset` is the HUD's
  // centre and `- w/2` steps back to its left edge, so the drawn centre is
  // independent of whether HUD_NOMINAL's width is right. Its inverse is
  // shipOffsetFromX, below — a matched pair, correct them together.
  const offset = hudNum(hud, "ship_offset");
  if (offset !== null) {
    const { w, h } = HUD_NOMINAL.shipui;
    out.push({
      kind: "shipui",
      label: "Ship HUD",
      x: Math.round(layout.reference_w / 2 + offset - w / 2),
      y: hudFlag(hud, "ship_top") ? 0 : layout.reference_h - h,
      w,
      h,
      drag: "x",
    });
  }

  // The stored point is the rect's top-left, in absolute screen px with the
  // origin at the screen's top-left. BOTH axes confirmed in-game (2026-07-28,
  // see hudPointFromRect) — this needed no correction. Its `h` does: the panel
  // is far taller than HUD_NOMINAL.fighter says once the fighter-ability grid
  // is drawn, and the anchor is that grid's top even when it is not.
  const fx = hudNum(hud, "fighter_x");
  const fy = hudNum(hud, "fighter_y");
  if (fx !== null && fy !== null && hudFlag(hud, "fighter_detached") && hudFlag(hud, "fighter_shown")) {
    out.push({ kind: "fighter", label: "Fighter UI", x: fx, y: fy, ...HUD_NOMINAL.fighter, drag: "xy" });
  }

  const bx = hudNum(hud, "badge_x");
  const by = hudNum(hud, "badge_y");
  if (bx !== null && by !== null) {
    out.push({ kind: "badge", label: "Badge", x: bx, y: by, ...HUD_NOMINAL.badge, drag: "xy" });
  }

  return out;
}

// --- filtering -------------------------------------------------------------
// One predicate, applied to the window list AND to what the canvas draws, so
// decluttering the list declutters the picture. Folding families in the panel
// is a separate, list-only affair — it must never reach this code.

export interface WindowFilter {
  /** Free text, matched against the friendly label, the detail and the raw id. */
  text: string;
  /** Drop windows EVE has not flagged open (roughly 77% of a real file). */
  openOnly: boolean;
  /** Drop windows EVE spawns per conversation, item or dialog — clutter, not
   * windows the player placed. Applies whether open or closed: kind of
   * window is the axis, not open/closed (see isClutter). */
  hideClutter: boolean;
}

export const NO_FILTER: WindowFilter = { text: "", openOnly: false, hideClutter: false };

/** Whether the filter narrows anything — drives the "showing N of M" line. */
export function filterIsActive(f: WindowFilter): boolean {
  return f.text.trim() !== "" || f.openOnly || f.hideClutter;
}

export function windowMatches(w: WindowRect, f: WindowFilter, o?: ClutterOverrides): boolean {
  if (f.openOnly && !w.open) return false;
  if (f.hideClutter && isClutter(w.id, o)) return false;
  // A minted numeric window id exists only to be a stack container (see
  // docs/format-notes.md, "Window stacks"). One that belongs to no stack at all
  // is a dead frame whose members are gone — it paints a phantom "Window stack"
  // rectangle. Structural, so unlike the curated tables it needs no maintenance.
  if (f.hideClutter && w.stack === null && /^\d+$/.test(w.id)) return false;
  const n = nameOf(w);
  const q = f.text.trim().toLowerCase();
  if (q === "") return true;
  // Same contract search.ts documents for the tree: label, detail and the raw
  // id all match, so "market", "corpassets" and "1037014587783" all work.
  return `${n.label} ${n.detail} ${w.id}`.toLowerCase().includes(q);
}

export function visibleIds(windows: WindowRect[], f: WindowFilter, o?: ClutterOverrides): Set<string> {
  return new Set(windows.filter((w) => windowMatches(w, f, o)).map((w) => w.id));
}

// --- snapping ---------------------------------------------------------------
// Edge snapping, in data px. EVE has no layout grid; it snaps windows to each
// other and to the screen, so these candidates are edges, never a fixed step.

export interface Rect { x: number; y: number; w: number; h: number }

/** Candidate edge coordinates a drag can lock onto, split by axis. */
export interface SnapLines { x: number[]; y: number[] }

/**
 * Every edge worth snapping to: the four edges of each rect, plus the screen's
 * own. The caller decides what is in `rects` — what the canvas DRAWS (so the
 * filter already applies), minus the dragged unit's own windows, plus the
 * furniture. Duplicates are kept: a few hundred numbers scanned linearly per
 * pointer move is nothing next to the DOM update that move triggers.
 */
export function snapLines(rects: Rect[], referenceW: number, referenceH: number): SnapLines {
  const x = [0, referenceW];
  const y = [0, referenceH];
  for (const r of rects) {
    x.push(r.x, r.x + r.w);
    y.push(r.y, r.y + r.h);
  }
  return { x, y };
}

/**
 * The edges a drag actually moves. A move carries all four; a corner resize
 * moves only the two edges its name points at (the opposite corner is the fixed
 * anchor — see resizeRect), and snapping an edge that isn't moving would drag
 * the anchor along with it.
 */
export function movingEdges(r: Rect, corner: Corner | null): { x: number[]; y: number[] } {
  if (corner === null) return { x: [r.x, r.x + r.w], y: [r.y, r.y + r.h] };
  const left = corner === "tl" || corner === "bl";
  const top = corner === "tl" || corner === "tr";
  return { x: [left ? r.x : r.x + r.w], y: [top ? r.y : r.y + r.h] };
}

/** A correction to add to a drag's delta, plus the candidates it caught. */
export interface SnapResult { dx: number; dy: number; gx: number | null; gy: number | null }

/** Nearest candidate within `tol` wins; ties go to the lower coordinate, so the
 * outcome never depends on the order rects were collected in. */
function nearest(edges: number[], lines: number[], tol: number): { d: number; line: number | null } {
  let d = 0;
  let line: number | null = null;
  let best = Infinity;
  for (const e of edges) {
    for (const c of lines) {
      const diff = c - e;
      const dist = Math.abs(diff);
      if (dist > tol) continue;
      if (dist < best || (dist === best && line !== null && c < line)) {
        best = dist;
        d = diff;
        line = c;
      }
    }
  }
  return { d, line };
}

/**
 * Snap a drag. `moving` is the edges that move, already displaced by the raw
 * pointer delta; the returned dx/dy is the extra correction that lands them on
 * a candidate, and gx/gy the lines caught (null when nothing was in range, in
 * which case the drag passes through untouched).
 */
export function snapDelta(
  moving: { x: number[]; y: number[] },
  lines: SnapLines,
  tol: number,
): SnapResult {
  const x = nearest(moving.x, lines.x, tol);
  const y = nearest(moving.y, lines.y, tol);
  return { dx: x.d, dy: y.d, gx: x.line, gy: y.line };
}

/**
 * The topmost drawn unit whose displayed rect contains a data-px point, or
 * null for empty canvas. The canvas paints `units` in array order, so the LAST
 * match is the one on top — the one a click would hit — hence the reverse walk.
 * `rectOf` is passed in rather than read off the unit because the displayed
 * rect is the live drag preview when there is one, which only the component
 * knows.
 */
export function unitAt(
  units: DrawUnit[],
  rectOf: (u: DrawUnit) => Rect,
  x: number,
  y: number,
): DrawUnit | null {
  for (let i = units.length - 1; i >= 0; i--) {
    const r = rectOf(units[i]);
    if (x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h) return units[i];
  }
  return null;
}

/** `ids` with `id` moved to `toIndex` (clamped into range). This is the whole
 * ordering, because `reorder_stack` rewrites `preferredIdxInStack3[container]`
 * from the list it is given. Pure — the input array is not touched. */
export function moveInOrder(ids: string[], id: string, toIndex: number): string[] {
  const rest = ids.filter((x) => x !== id);
  const at = Math.max(0, Math.min(toIndex, rest.length));
  return [...rest.slice(0, at), id, ...rest.slice(at)];
}

/** What a canvas drop does. One variant per row of the stack-polish spec's
 * gesture table; `none` is a drop that asks for nothing. */
export type DropAction =
  | { op: "move" }
  | { op: "none" }
  | { op: "create"; first: string; second: string }
  | { op: "add"; member: string; container: string }
  | { op: "unstack"; member: string; rect: Rect }
  | { op: "unstackInto"; member: string; container: string }
  | { op: "unstackCreate"; member: string; target: string }
  | { op: "reorder"; container: string; order: string[] };

/** What is being dragged: a unit, optionally by one of its tabs, and the rect
 * it would land at (the live preview). */
export interface DragSubject {
  unit: DrawUnit;
  /** The dragged tab's window id, or null when the whole rect is being moved. */
  tabId: string | null;
  rect: Rect;
}

/**
 * Decide a drop. `target` is the unit under the pointer (`unitAt`), `shift`
 * whether Shift is down, `hoverTabIndex` which of the target's VISIBLE tabs the
 * pointer is over (null when it is not over the strip) — the one input that has
 * to be measured from the DOM.
 *
 * Shift is only what disambiguates a drag that also has a plain-move meaning:
 * a window drag always could have been a move, so stacking needs the modifier;
 * a tab dropped on another *stack* has no competing meaning and needs none.
 */
export function dropAction(
  drag: DragSubject,
  target: DrawUnit | null,
  shift: boolean,
  hoverTabIndex: number | null,
): DropAction {
  const { unit, tabId } = drag;

  if (tabId === null) {
    // Whole-rect drag. A stack can't be dragged into another stack (merging is
    // out of scope), so Shift is ignored for one.
    if (!shift || !target || target.key === unit.key || unit.stack) return { op: "move" };
    return target.stack
      ? { op: "add", member: unit.anchor.id, container: target.stack.container_id }
      // create_stack(m1, m2) puts the stack at m1's rect: the window that
      // stayed put keeps its position and becomes tab 0.
      : { op: "create", first: target.anchor.id, second: unit.anchor.id };
  }

  // Tab drag. It always leaves its stack unless it is dropped back on it.
  if (!unit.stack) return { op: "none" }; // unreachable: a tab implies a stack
  if (target && target.key === unit.key) {
    if (hoverTabIndex === null) return { op: "none" }; // over the body, not the strip
    const over = unit.tabs[hoverTabIndex]?.id;
    if (over === undefined) return { op: "none" };
    // Reorder against the FULL member list, not the visible tabs: reorder_stack
    // rewrites the container's whole index dict from what it is given, so a
    // member the filter is hiding must still be in the list.
    const members = unit.stack.members;
    const to = members.indexOf(over);
    if (to < 0) return { op: "none" };
    const order = moveInOrder(members, tabId, to);
    if (order.join(" ") === members.join(" ")) return { op: "none" };
    return { op: "reorder", container: unit.stack.container_id, order };
  }
  if (target?.stack) return { op: "unstackInto", member: tabId, container: target.stack.container_id };
  if (target && shift) return { op: "unstackCreate", member: tabId, target: target.anchor.id };
  return { op: "unstack", member: tabId, rect: drag.rect };
}
