// Pure geometry helpers for the layout canvas. No DOM, no Svelte — unit-tested
// in layout.test.ts.
import type { WindowLayout, Stack, WindowRect, Hud } from "./api";
import { isClutter, inEnv, nameOf, type ClutterOverrides, type Env } from "./windowLabels.ts";

/** Re-exported so callers that already import from layout.ts get it here.
 * It is DECLARED in windowLabels.ts, beside the mapping it belongs to —
 * layout.ts imports from windowLabels.ts and not the reverse. */
export type { Env };

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
 * rectangle but represents each of its visible tabs, a free unit ordinarily
 * exactly one — except the folded Inventory unit `linkInventory` produces,
 * whose `tabs` carries both merged ids, so it counts 2. The counter reports
 * windows, not rectangles — "showing 3 of 68 windows". */
export function drawnWindowCount(units: DrawUnit[]): number {
  return units.reduce((n, u) => n + Math.max(u.tabs.length, 1), 0);
}

/**
 * Fold the two docked Inventory copies into one drawn rectangle.
 *
 * Inventory is the ONLY window family EVE splits per context: the character
 * file carries `InventoryStation`, `InventoryStructure` and `InventorySpace` as
 * three separate ids with three separate geometries in the otherwise-flat
 * `windowSizesAndPositions_1`. On a real character they have drifted apart
 * (624,260 623x450 vs 136,285 880x619), so the docked view would paint two
 * rectangles 488px apart for what the player thinks of as one window.
 *
 * In `docked` the two copies collapse to one drawn unit whose `fanTargets`
 * carry both window ids — already "every window a coherent move must repeat
 * the rect onto", so the existing commit path moves both from one drag with no
 * new drag code.
 *
 * **The fan follows renderability, not openness.** `stackUnits` only makes
 * units from OPEN windows, so sourcing the pair from `units` silently drops a
 * closed copy and leaves it behind — the exact drift `stackUnits` already
 * guards against for closed stack members (see `fanTargets` there). Whichever
 * copy is drawn, the fan reaches both, because the docked view is telling the
 * player these are one window.
 *
 * A copy that belongs to a stack is excluded from the fan: its stack already
 * owns its geometry, and writing to it from here would pull it out of place.
 *
 * `all` is deliberately left untouched: three independent rectangles, exactly
 * as today. That IS the escape hatch for a player who wants the station and
 * structure inventories in different places, so there is no toggle to build.
 *
 * A post-pass rather than a parameter on stackUnits: it keeps the grouping and
 * the fold separately testable, and it cannot affect the unfiltered denominator
 * `LayoutView` computes for "showing N of M".
 */
const DOCKED_INVENTORY = ["InventoryStation", "InventoryStructure"];

export function linkInventory(units: DrawUnit[], env: Env, windows: WindowRect[]): DrawUnit[] {
  if (env !== "docked") return units;
  const isCopy = (u: DrawUnit) => !u.stack && DOCKED_INVENTORY.includes(u.key);
  const drawn = units.filter(isCopy);
  if (drawn.length === 0) return units;
  // Both copies, open or closed — a closed one left out of the fan drifts away
  // from the one that moved. Stacked copies are their stack's business.
  const fanTargets = DOCKED_INVENTORY
    .map((id) => windows.find((w) => w.id === id && w.renderable && w.stack === null))
    .filter((w): w is WindowRect => !!w);
  // Station anchors when both are drawn, so the rect stays where the station
  // copy was; otherwise whichever one is drawn anchors.
  const anchor = drawn.find((u) => u.key === "InventoryStation") ?? drawn[0];
  // tabs, not just fanTargets: every selection consumer (the panel row, the
  // canvas highlight, resize handles, arrow-key nudge) keys off anchor.id or
  // tabs, so a station-only tabs array would leave the structure row selectable
  // in the panel but inert on the canvas.
  const linked = { ...anchor, tabs: drawn.flatMap((u) => u.tabs), fanTargets };
  return units.filter((u) => !isCopy(u) || u === anchor).map((u) => (u === anchor ? linked : u));
}

export interface FurnitureRect {
  kind: "neocom" | "shipui" | "fighter" | "badge" | "target";
  label: string;
  /** Data px, like WindowRect.geom — the canvas scales it with toCanvas. */
  x: number;
  y: number;
  w: number;
  h: number;
  drag: "none" | "x" | "xy";
}

// EVE stores anchors but never sizes, and never says what an anchor is relative
// to, so all of this began as assumption. Two live sessions settled the
// conventions and 2026-07-28 settled the sizes, off three native 2560x1440
// screenshots measured against the settings file that produced them.

/**
 * How far the ship HUD extends LEFT of its anchor.
 *
 * The anchor is the capacitor wheel's centre — NOT the element's centre. This is
 * the correction that matters: two shots of one character at one offset, flying
 * a battleship and a frigate, share a pixel-identical left edge (490) and differ
 * only on the right (1133 vs 896). The element grows rightward from a fixed left
 * edge, so it is strongly asymmetric about its anchor: 148px left, 495px right.
 *
 * Isolating the capacitor wheel by colour put its centre at x=638.5, against
 * `2560/2 + (-642) = 638` from the file — a half-pixel match. That also explains
 * the 2026-07-27 result that writing 0.0 drew the HUD "dead centre": it is the
 * capacitor that centres, not the box.
 */
export const SHIP_ANCHOR_LEFT = 148;

/**
 * Gap between the top of the screen and the HUD when it is top-aligned.
 *
 * MEASURED 2026-07-30. Was 28, which was never measured directly: it came from
 * assuming a 160px element whose rack block sat 4px in. The rack's position is
 * the only thing that measurement pinned, and it does not, on its own, fix the
 * element's own top edge.
 *
 * The element's top edge is the capacitor's outer rim, which reaches 88px above
 * the capacitor's centre — visible in a 5x crop as the dark rim arc plus its
 * highlight segment, above where the gauge ticks start. Top-aligned the centre
 * is at y 100, so the element begins at 12.
 *
 * The pair below is checked against the rack in BOTH alignments and lands
 * exactly, which is what makes it trustworthy: top-aligned the element runs
 * 12..188 with rack row 1 at `12 + 20 = 32`; bottom-aligned it runs
 * `1440 - 12 - 176 = 1252`..1428 with rack row 1 at 1272. Both match measured
 * rack positions to the pixel. See docs/format-notes.md, "Ship HUD internals".
 */
export const SHIP_TOP_MARGIN = 12;

/**
 * Gap between the HUD and the bottom of the screen when it is bottom-aligned.
 * MEASURED 2026-07-28 and UNCHANGED by the 2026-07-30 re-measure — it is the one
 * figure of the three that was right.
 *
 * What did change is the conclusion drawn from it. The margins are equal, not
 * asymmetric: the earlier note reasoned "12 below, so the 28 above is not a
 * mirror", when in fact the 28 was the wrong number and the element is
 * vertically symmetric about its capacitor after all.
 */
export const SHIP_BOTTOM_MARGIN = 12;

/**
 * Drawn sizes for the screen furniture, in data px. MEASURED 2026-07-28 except
 * `badge`, which is still nominal.
 *
 * These are not cosmetic: `LayoutView` feeds each furniture rect's `w`/`h` into
 * the snap-line set, so a box smaller than the real element makes windows snap
 * against an edge the player cannot see and overlap the part we failed to draw.
 * The previous values (shipui 686x250, fighter 400x120) were invented, and the
 * shipui box was additionally drawn centred on the anchor — putting it 195px too
 * far left while missing 152px of module rack on the right.
 *
 * `shipui` covers the widest possible rack: the battleship shot's widest row
 * already carries the maximum 8 slots, so this is a measured maximum rather
 * than an extrapolation.
 *
 * The width was 643 until 2026-07-30, when the racks were re-measured by
 * brightness profile rather than by eye for the detail layer: the buttons sit
 * on a pitch of 51 (not 50) starting at x 247 (not 245), so the 8th ends at
 * `247 + 51 x 7 + 44 = 648` — the measured right edge, not a rounded one.
 * The old 643 came from the arithmetic `245 + 50 x 8`, which mixes a pitch with
 * a button width and lands 5px short, so windows snapped just inside the real
 * HUD. See docs/format-notes.md, "Ship HUD internals".
 *
 * `fighter` covers 5 squadrons, the most a carrier can field. The shot had 4
 * (3 launched, so 3 ability columns); column pitch is 86, so the fifth adds 86
 * to the measured 381. Height does not change with squadron count.
 *
 * Both HEIGHTS were re-measured 2026-07-30 and both grew:
 *
 * - `shipui` 160 -> 176. The element is exactly as tall as its capacitor, whose
 *   outer rim reaches 88px either side of a centre sitting 88 from the top. The
 *   old 160 was inferred from the rack block rather than measured, and pairs
 *   with the old 28px top margin — both were wrong together, which is why the
 *   rack still landed in the right place and nothing looked broken.
 * - `fighter` 253 -> 264. The fighter CONTROL COLUMN, four buttons left of the
 *   squadron dials, is the panel's lowest element and ends at 264. It was never
 *   drawn before, so nothing had noticed it hanging out of the box.
 */
export const HUD_NOMINAL = {
  shipui: { w: 648, h: 176 },
  fighter: { w: 467, h: 264 },
  badge: { w: 32, h: 32 },
  // ONE target slot, measured 2026-07-31: the list's slot pitch is 110 across
  // and 181 down (four label rows apart, in three shots). A list is N of these
  // in a row or a column — see hudRects, which takes the count, because no file
  // records how many things a pilot locks.
  target: { w: 110, h: 181 },
};

/** How many target slots the canvas draws when nobody has said otherwise. Four
 * is a common enough lock count to be representative without filling the
 * screen; the preference behind it is `LayoutPrefs.targets`. */
export const TARGET_COUNT_DEFAULT = 4;

/** The footprint of a target list of `count` slots. Vertical stacks them down,
 * horizontal runs them across — `alignHorizontally` picks which. */
export function targetSize(count: number, horizontal: boolean): { w: number; h: number } {
  const n = Math.max(1, Math.round(count));
  const { w, h } = HUD_NOMINAL.target;
  return horizontal ? { w: w * n, h } : { w, h: h * n };
}

/**
 * How much of the screen's left edge the target list's x fraction does NOT
 * cover — the neocom's drawn width, 72px on the captured client.
 *
 * Used only as a fallback. The stored value normally carries its own
 * denominator (see targetDenominator), which is exact and survives a player
 * whose neocom is a different width; this number is what a MINTED value has to
 * assume, and it is the corpus-typical one, not a universal constant. Three
 * different margins show up across the corpus (37, 48, 72).
 */
export const TARGET_MARGIN = 72;

/**
 * The width the target list's stored x is a fraction OF.
 *
 * `targetOrigin.x` is not a fraction of the screen: it spans the width to the
 * right of the neocom, so the anchor is `(referenceW - denominator) + f *
 * denominator`. EVE writes an exact `pixels / denominator`, which means the
 * denominator can be read back out of the value itself — and that is worth
 * doing, because the margin is a property of the client that wrote it, not of
 * ours.
 *
 * Only a denominator that is UNIQUE in the plausible range is trusted. A round
 * fraction (0.5, or the 0 default) divides evenly into dozens of candidates and
 * says nothing about the margin; taking the first hit there would invent a
 * margin out of a value that never encoded one.
 */
export function targetDenominator(f: number, referenceW: number): number {
  const fallback = Math.max(1, referenceW - TARGET_MARGIN);
  if (!Number.isFinite(f) || f <= 0 || referenceW <= TARGET_MARGIN) return fallback;
  let found = 0;
  // 200px is well past every margin seen (37, 48, 72) without reaching the
  // half-width aliases a small denominator would also satisfy.
  for (let d = referenceW - 1; d >= referenceW - 200 && d > 0; d--) {
    const p = f * d;
    if (Math.abs(p - Math.round(p)) < 1e-6) {
      if (found) return fallback; // ambiguous — the value encodes no margin
      found = d;
    }
  }
  return found || fallback;
}

/**
 * Where a target list of `w` x `h` is DRAWN for an anchor at `ax, ay`.
 *
 * The anchor is the list's OUTER corner and the slots run toward the middle of
 * the screen — captured in both orientations and on both sides (2026-07-31) —
 * so the box hangs off whichever side of the anchor faces the centre, on each
 * axis independently. Its own function because the canvas needs the same rule
 * mid-drag, to flip the preview as the anchor crosses the middle rather than
 * letting it jump on drop.
 */
export function targetRect(
  ax: number, ay: number, w: number, h: number, referenceW: number, referenceH: number,
): { x: number; y: number } {
  return { x: ax > referenceW / 2 ? ax - w : ax, y: ay > referenceH / 2 ? ay - h : ay };
}

/**
 * The target list's anchor in data px, from its stored fractions.
 *
 * Rounded, unlike the ship HUD's placement: the client's own value is a whole
 * number of pixels over the denominator, so `f * d` lands a fraction of a
 * float's-worth away from the integer it means (1354.0000000000002). The ship
 * HUD leaves its half-pixel in place because its inverse subtracts the same
 * expression back off; this one's inverse re-derives the anchor through here,
 * so rounding on both sides cancels rather than accumulates.
 */
export function targetAnchor(fx: number, fy: number, referenceW: number, referenceH: number) {
  const d = targetDenominator(fx, referenceW);
  return { x: Math.round(referenceW - d + fx * d), y: Math.round(fy * referenceH) };
}

/**
 * Stored fractions for an anchor AT data px `x, y` — the exact inverse of
 * targetAnchor, and what the panel's pixel fields write.
 *
 * `fx` is the value being replaced, and is read only for its denominator: that
 * is the client's own margin, recovered from the number it wrote, and an edit
 * must not quietly re-base the value onto ours. A minted value has none to
 * recover and falls back to TARGET_MARGIN.
 */
export function targetFractionFromPoint(
  fx: number, x: number, y: number, referenceW: number, referenceH: number,
): { x: number; y: number } {
  const d = targetDenominator(fx, referenceW);
  return { x: (Math.round(x) - (referenceW - d)) / d, y: Math.round(y) / referenceH };
}

const byName = (hud: Hud, name: string) => hud.entries.find((e) => e.name === name);

/** A field's number: its value, else its default; null when not writable at all. */
export function hudNum(hud: Hud, name: string): number | null {
  const e = byName(hud, name);
  if (!e || e.set.how === "unavailable") return null;
  const n = parseFloat(e.value ?? e.default);
  return Number.isFinite(n) ? n : null;
}

/** Whether the file actually holds this field, as opposed to falling back to a
 * default. Only the target anchor cares: its default is a placeholder. */
function stored(hud: Hud, name: string): boolean {
  const e = byName(hud, name);
  return !!e && e.set.how !== "unavailable" && e.value !== null;
}

export function hudFlag(hud: Hud, name: string): boolean {
  const e = byName(hud, name);
  if (!e || e.set.how === "unavailable") return false;
  return (e.value ?? e.default) === "true";
}

/**
 * Stored offset for a ship-HUD rect whose left edge is at data-px `x`. The exact
 * inverse of hudRects' ship-HUD placement below — a matched pair, correct them
 * together, and `layout.test.ts` round-trips them at even AND odd reference
 * widths because getting this wrong writes a bad offset into a real settings
 * file. The rounding lives here and nowhere else: rounding the placement too
 * made the two biases stack on an odd width instead of cancelling.
 *
 * CONFIRMED in-game 2026-07-27 that the offset is centre-relative and negative
 * is leftward; MEASURED 2026-07-28 that what it centres is the capacitor wheel,
 * which sits `SHIP_ANCHOR_LEFT` from the element's left edge. The old version
 * used `w/2` here and claimed the width cancelled out; that was true only while
 * the drawn box was (wrongly) centred on the anchor.
 */
export function shipOffsetFromX(x: number, referenceW: number): number {
  return Math.round(x + SHIP_ANCHOR_LEFT - referenceW / 2);
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
 *
 * Sizes MEASURED 2026-07-28 from the same shot the anchor was confirmed on:
 * with 4 squadrons (3 launched) the panel spans 381x253 from the anchor, on a
 * column pitch of 86 shared by the ability grid and the squadron row. Five
 * squadrons is the carrier maximum, hence the 467 width in HUD_NOMINAL. Height
 * is independent of squadron count.
 */
export function hudPointFromRect(kind: FurnitureRect["kind"], x: number, y: number): { x: number; y: number } {
  return { x: Math.round(x), y: Math.round(y) };
}

/**
 * The screen furniture the canvas draws, in data px. Order is fixed
 * (neocom, ship HUD, fighter, badge, target list) so the canvas paints the bar
 * first and tests can index. An element whose values aren't writable is omitted
 * rather than drawn at a guessed position.
 *
 * `targetCount` is how many locked targets to draw the target list at. It is a
 * view preference rather than anything read from a file — no settings file
 * records how many things a pilot locks — and it changes the rectangle's size,
 * so windows snap against the area the list really covers at that count.
 */
export function hudRects(
  hud: Hud,
  layout: WindowLayout,
  targetCount: number = TARGET_COUNT_DEFAULT,
): FurnitureRect[] {
  const out: FurnitureRect[] = [];

  const neocom = hudNum(hud, "neocom_width");
  if (neocom !== null && neocom > 0) {
    out.push({ kind: "neocom", label: "Neocom", x: 0, y: 0, w: neocom, h: layout.reference_h, drag: "none" });
  }

  // The stored offset places the capacitor wheel's centre at
  // `reference_w/2 + offset` (measured 2026-07-28 to within half a pixel). The
  // element then extends SHIP_ANCHOR_LEFT to the left of that point and the rest
  // to the right — it is NOT centred on it. Its inverse is shipOffsetFromX.
  const offset = hudNum(hud, "ship_offset");
  if (offset !== null) {
    const { w, h } = HUD_NOMINAL.shipui;
    out.push({
      kind: "shipui",
      label: "Ship HUD",
      // Deliberately NOT rounded. On an odd `reference_w` the half-pixel centre
      // makes `Math.round` bias half-up on BOTH sides, so the two rounds stack
      // instead of cancelling and a drag writes an offset one px off what was
      // dropped. Leaving x fractional makes shipOffsetFromX an exact inverse at
      // every width, and nothing downstream needs an integer — snapLines takes
      // plain numbers and toCanvas only multiplies.
      x: layout.reference_w / 2 + offset - SHIP_ANCHOR_LEFT,
      // Both margins measured, and they differ: 28 above, 12 below. Mirroring
      // the top margin — what this did before a bottom-aligned shot existed —
      // drew the box 16px high, so windows snapped to an edge the racks
      // actually cover.
      y: hudFlag(hud, "ship_top") ? SHIP_TOP_MARGIN : layout.reference_h - SHIP_BOTTOM_MARGIN - h,
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

  // The target list. Drawn only when the fractions are actually STORED, not
  // when they fall back to a default like every other element here: EVE's own
  // starting position for the list was never captured, 87% of account files
  // have never had it dragged, and 0 would put the slot in the top-left corner
  // — a place the list has never been. Nothing is more honest than nothing.
  const tx = hudNum(hud, "target_x");
  const ty = hudNum(hud, "target_y");
  if (tx !== null && ty !== null && stored(hud, "target_x") && stored(hud, "target_y")) {
    const a = targetAnchor(tx, ty, layout.reference_w, layout.reference_h);
    const { w, h } = targetSize(targetCount, hudFlag(hud, "target_horizontal"));
    out.push({
      kind: "target",
      label: "Target list",
      // The anchor is the list's OUTER corner and the slots run toward the
      // middle of the screen — captured in both orientations and on both sides
      // (2026-07-31). So the slot hangs off whichever side of the anchor faces
      // the centre, on each axis independently.
      x: a.x > layout.reference_w / 2 ? a.x - w : a.x,
      y: a.y > layout.reference_h / 2 ? a.y - h : a.y,
      w,
      h,
      drag: "xy",
    });
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
  /** Show only the windows that exist in one environment. `all` — the default
   * — is today's mixed picture. Windows the mapping does not recognise show in
   * every environment, so this narrows rather than hides (see inEnv). */
  env: Env;
}

export const NO_FILTER: WindowFilter = { text: "", openOnly: false, hideClutter: false, env: "all" };

/** Whether the filter narrows anything — drives the "showing N of M" line. */
export function filterIsActive(f: WindowFilter): boolean {
  return f.text.trim() !== "" || f.openOnly || f.hideClutter || f.env !== "all";
}

/**
 * A minted numeric window id that belongs to no stack — a dead frame whose
 * members are gone (see docs/format-notes.md, "Window stacks"). It paints a
 * phantom "Window stack" rectangle until it is deleted.
 *
 * Shared by the `Hide clutter` filter and the delete offer on purpose: the
 * offer must remove exactly the frames the filter calls dead, and two copies
 * of this rule would eventually disagree.
 */
export function isOrphanFrame(w: WindowRect): boolean {
  return w.stack === null && /^\d+$/.test(w.id);
}

export function windowMatches(w: WindowRect, f: WindowFilter, o?: ClutterOverrides): boolean {
  if (f.openOnly && !w.open) return false;
  if (f.hideClutter && isClutter(w.id, o)) return false;
  if (f.hideClutter && isOrphanFrame(w)) return false;
  if (!inEnv(w.id, f.env)) return false;
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
    if (hits(rectOf(units[i]), x, y)) return units[i];
  }
  return null;
}

/** Point-in-rect, edges inclusive. The one copy of this test: `unitAt` and
 * `rectsAt` both rank by it, and two hand-written copies would eventually
 * disagree on an edge pixel. */
const hits = (r: Rect, x: number, y: number) =>
  x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h;

/**
 * Everything whose rect contains a data-px point, topmost first — the same
 * last-painted-wins ranking `unitAt` uses, so `rectsAt(...)[0]` is always the
 * unit a plain click would select (pinned by a test).
 *
 * This is what the canvas's right-click picker lists: a click can only ever
 * reach the top rectangle, and on a real file with hundreds of heavily
 * overlapping windows that leaves anything underneath findable only by name.
 *
 * `unitAt` is NOT rewritten as `rectsAt(...)[0]`. It runs on every pointermove
 * for the length of a drag, where returning early beats allocating an array to
 * answer a question that only needs its first element.
 *
 * Generic because it has two callers with different element types: draw units,
 * which are asked for their anchor's rect, and furniture rects, which are their
 * own rect.
 */
export function rectsAt<T>(items: T[], rectOf: (t: T) => Rect, x: number, y: number): T[] {
  const out: T[] = [];
  for (let i = items.length - 1; i >= 0; i--) {
    if (hits(rectOf(items[i]), x, y)) out.push(items[i]);
  }
  return out;
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
