// The canvas detail layer: what each rectangle looks like INSIDE. Pure — no
// DOM, no Svelte — unit-tested in detail.test.ts, rendered by DetailParts.svelte.
//
// Decoration only. Nothing here may reach hudRects, snapLines, or any drag:
// that separation is why this is its own module and not more of layout.ts.

import type { ChatPanel, NeocomBar, OverviewColumns } from "./api";
import type { DrawUnit } from "./layout";

/**
 * One drawn piece of a rectangle's internals.
 *
 * Coordinates are DATA PX RELATIVE TO THE PARENT RECT'S ORIGIN — the same unit
 * as WindowRect.geom and FurnitureRect, so the canvas reuses `toCanvas` and the
 * parts scale with it for free. They are relative because the parts render as
 * children of the rectangle's own absolutely-positioned div, which already owns
 * the position and the clipping.
 */
export interface DetailPart {
  /** `slot` and `ring` are circles, `arc` a half annulus (the capacitor's gauge
   * band), `core` a filled disc; the rest are rectangles. The renderer keys its
   * shape off this and nothing else. */
  kind: "ring" | "arc" | "core" | "slot" | "cell" | "band" | "column" | "button";
  x: number;
  y: number;
  w: number;
  h: number;
  label?: string;
}

/**
 * ponytail: every number here is INVENTED, EXCEPT `abilityCell.w` and
 * `squadCell.w` — those two are DERIVED from the measured fighter panel width
 * (`70 + 86*4 + 53 = 467` and `43 + 86*4 + 80 = 467`, see their own comments
 * below) and pinned by assertions in detail.test.ts. Correcting the invented
 * ones is a one-line edit each. They are the sizes of things drawn inside a
 * measured pitch, plus the overview chrome, which has never had a measuring
 * pass. Upgrade path: a screenshot session like the 2026-07-28 one that
 * produced the measured table below, then move each corrected value out of
 * this object and into a named constant citing format-notes.md.
 *
 * The distinction matters. HUD_NOMINAL's invented 686x250 drew the ship HUD
 * 195px off its real position for three releases.
 */
export const DETAIL_NOMINAL = {
  /**
   * Fighter ability cell. The COLUMN pitch (86) is measured; the row pitch is
   * not — 3 rows spanning y 0..178 gives ~59.
   *
   * The WIDTH is not free: at the 5-column maximum the last column starts at
   * `70 + 86 x 4 = 414`, and the measured panel is 467 wide, so 53 is what
   * reaches the right edge exactly. Anything wider draws outside the box.
   */
  abilityCell: { w: 53, h: 52 },
  abilityRowPitch: 59,
  /**
   * Fighter squadron cell, on the same measured 86 column pitch. Same
   * derivation: `43 + 86 x 4 = 387`, and `387 + 80 = 467`. That both rows land
   * on 467 from different origins is the check that the measured table and
   * HUD_NOMINAL.fighter agree.
   */
  squadCell: { w: 80, h: 70 },
  /** The EVE-menu button at the top of the neocom. It is not in
   *  neocomButtonRawData, so the button column starts below it. */
  neocomTop: 40,
  /** Overview chrome. Never measured. */
  tabStrip: 18,
  headerBand: 16,
  /** Width for an overview column whose width key is absent (= EVE's own
   *  default, which the file does not record). */
  columnWidth: 80,
};

// --- ship HUD ---------------------------------------------------------------
// MEASURED 2026-07-30 from the two native 2560x1440 shots the 2026-07-28 pass
// used (hud_battleship.png, hud_frigate.png), by row/column brightness profile
// rather than by eye — see docs/format-notes.md, "Ship HUD internals". Every
// offset is from the element's own top-left, i.e. the box HUD_NOMINAL.shipui
// describes. Two ships at one offset agree on every number below, which is what
// separates structure from one ship's fitting.

/**
 * The capacitor assembly, as concentric radii about its centre.
 *
 * The centre IS the element's anchor point: box-relative (148, 72) here, against
 * `SHIP_ANCHOR_LEFT = 148` derived independently in 2026-07-28's colour-isolation
 * pass. Two methods, same pixel.
 *
 * `outer` came from sweeping a full circle at each radius and taking the mean:
 * both ships peak sharply at r 80-81 and are back to background by r 90. (An
 * earlier read of a magnified crop put it at 86 — the sweep is what corrected
 * it, and it is why this is measured rather than eyeballed.)
 *
 * `gauge` is the shield/armour/hull tick band, and it covers the TOP HALF ONLY:
 * the arcs sweep 9 o'clock through 12 to 3, and the bottom half is the dark
 * speed dial. That asymmetry is most of the shape's signature, so it is drawn
 * as a half annulus rather than approximated with a full ring.
 */
const CAP = { cx: 148, cy: 72, outer: 80, gaugeInner: 50, innerRing: 42, core: 27 };

/**
 * Module slots. Round buttons of diameter 44 on a column pitch of 51, first
 * column at x 247, rows on a uniform 44 pitch from y 4.
 *
 * `rowOffset` is the one that makes it read as EVE: the middle row is staggered
 * half a pitch against the outer two. Confirmed independently on both ships —
 * the frigate's mid row sits at 762/813/864 against its outer rows' 737/788/839,
 * the battleship's at 813/915/966 against 788/890/941. Exactly +25 both times.
 *
 * The count is the MAXIMUM, because slot count is ship-dependent and no settings
 * file records it. The staggered row gets one fewer: sitting half a pitch in, 7
 * spans the same width the outer rows' 8 do, which is exactly how it looks.
 */
const SLOTS = { firstX: 247, pitchX: 51, diameter: 44, rowTop: 4, rowPitch: 44, cols: 8, rowOffset: 25 };

/**
 * The ship-control button cluster left of the capacitor — the part that was
 * missing entirely, leaving the left third of the box empty. It belongs to the
 * HUD: 2026-07-28 recorded it moving by exactly the HUD's drag delta, which is
 * how the 148px left extension was established in the first place.
 *
 * Two staggered columns, 4 then 3, on a 32px vertical pitch with the second
 * column half a step down — the same brick motif as the racks. Measured column A
 * at box x -2, clamped to 0: it is the leftmost thing drawn, so it defines the
 * box edge, and 2px is inside the anchor's own stated tolerance.
 */
const CLUSTER = {
  diameter: 30,
  rowPitch: 32,
  columns: [
    { x: 0, top: 24, rows: 4 },
    { x: 30, top: 40, rows: 3 },
  ],
};

/**
 * The ship HUD's internals. Constant: the box is fixed (HUD_NOMINAL.shipui) and
 * none of this is in any settings file.
 *
 * Order matters only for painting: the capacitor's pieces are emitted outermost
 * first so the core lands on top of the dial.
 */
export function shipHudParts(): DetailPart[] {
  const disc = (r: number, kind: DetailPart["kind"]): DetailPart =>
    ({ kind, x: CAP.cx - r, y: CAP.cy - r, w: r * 2, h: r * 2 });

  const out: DetailPart[] = [
    // The outer rim, then the tick band as a half annulus (its border thickness
    // is what fills r gaugeInner..outer), then the metallic inner ring, then the
    // bright core.
    disc(CAP.outer, "ring"),
    disc(CAP.outer, "arc"),
    disc(CAP.innerRing, "ring"),
    disc(CAP.core, "core"),
  ];

  for (let row = 0; row < 3; row++) {
    const staggered = row === 1;
    const cols = staggered ? SLOTS.cols - 1 : SLOTS.cols;
    for (let c = 0; c < cols; c++) {
      out.push({
        kind: "slot",
        x: SLOTS.firstX + (staggered ? SLOTS.rowOffset : 0) + SLOTS.pitchX * c,
        y: SLOTS.rowTop + SLOTS.rowPitch * row,
        w: SLOTS.diameter,
        h: SLOTS.diameter,
      });
    }
  }

  for (const { x, top, rows } of CLUSTER.columns) {
    for (let i = 0; i < rows; i++) {
      out.push({
        kind: "slot",
        x,
        y: top + CLUSTER.rowPitch * i,
        w: CLUSTER.diameter,
        h: CLUSTER.diameter,
      });
    }
  }

  return out;
}

// --- fighter UI -------------------------------------------------------------
// MEASURED 2026-07-28, same table: ability grid at x 70 / y 0, squadron row at
// x 43 / y ~178, both on an 86px column pitch, 5 columns max (a carrier's
// maximum squadron count, which is also what HUD_NOMINAL.fighter's width is).

const FIGHTER = { abilityX: 70, squadX: 43, squadY: 178, pitch: 86, cols: 5, rows: 3 };

/** The fighter panel's internals. Constant, and maxima, for the same reasons
 * as shipHudParts. */
export function fighterParts(): DetailPart[] {
  const out: DetailPart[] = [];
  for (let r = 0; r < FIGHTER.rows; r++) {
    for (let c = 0; c < FIGHTER.cols; c++) {
      out.push({
        kind: "cell",
        x: FIGHTER.abilityX + FIGHTER.pitch * c,
        y: DETAIL_NOMINAL.abilityRowPitch * r,
        ...DETAIL_NOMINAL.abilityCell,
      });
    }
  }
  for (let c = 0; c < FIGHTER.cols; c++) {
    out.push({
      kind: "cell",
      x: FIGHTER.squadX + FIGHTER.pitch * c,
      y: FIGHTER.squadY,
      ...DETAIL_NOMINAL.squadCell,
    });
  }
  return out;
}

// --- neocom -----------------------------------------------------------------

/**
 * The real buttons on the neocom, top-down. `w` is the bar's own width
 * (`neocomWidth`), which is also the cell size — EVE's neocom buttons are
 * square and fill the bar.
 *
 * Stops emitting when the next square would pass the bar's height, so a bar
 * with more buttons than fit draws truncated. That is what the client does; it
 * is not an error to report.
 */
export function neocomParts(bar: NeocomBar, w: number, h: number): DetailPart[] {
  const out: DetailPart[] = [];
  let y = DETAIL_NOMINAL.neocomTop;
  for (const b of bar.buttons) {
    if (y + w > h) break;
    out.push({ kind: "button", x: 0, y, w, h: w, label: b.id });
    y += w;
  }
  return out;
}

// --- overview ---------------------------------------------------------------

/**
 * An overview window's tab strip and the column header band of its FIRST tab.
 *
 * The first tab, because nothing in either settings file records which tab is
 * selected (`tabgroups` is chat-window state). Naming every tab in the strip is
 * what keeps that choice visible instead of silent.
 *
 * Column bands are laid out left to right from x 0 at their STORED widths, with
 * no clamping. A set wider than the window therefore runs off the edge and gets
 * clipped by the rectangle — which is the whole point: it makes an overflowing
 * overview visible without any overflow arithmetic.
 */
export function overviewParts(
  cols: OverviewColumns,
  windowIndex: number,
  rect: { w: number; h: number },
): DetailPart[] {
  const win = cols.windows.find((w) => w.index === windowIndex);
  const tabs = (win?.tab_indices ?? [])
    .map((i) => cols.tabs.find((t) => t.index === i))
    .filter((t): t is OverviewColumns["tabs"][number] => !!t);
  if (tabs.length === 0) return [];

  const out: DetailPart[] = [];
  // Equal-width tab cells. EVE sizes them by their text, but the information
  // here is which tabs the window holds, not how wide their labels render, and
  // an equal split always fits the rect it is drawn in.
  const tw = rect.w / tabs.length;
  tabs.forEach((t, i) => {
    out.push({ kind: "cell", x: i * tw, y: 0, w: tw, h: DETAIL_NOMINAL.tabStrip, label: t.name });
  });

  let x = 0;
  for (const c of tabs[0].columns) {
    if (!c.visible) continue;
    // width null = the key is absent = EVE's own default, which the file does
    // not record. The nominal is the only thing available.
    const w = c.width ?? DETAIL_NOMINAL.columnWidth;
    out.push({ kind: "column", x, y: DETAIL_NOMINAL.tabStrip, w, h: DETAIL_NOMINAL.headerBand, label: c.label });
    x += w;
  }
  return out;
}

// --- chat -------------------------------------------------------------------

/**
 * A chat window's member-list and input splits, from the stored widths.
 *
 * Either field being null means the player has never resized that split, so the
 * part is OMITTED rather than drawn at a guessed default — a split that is not
 * in the file is a split the canvas has nothing to say about.
 *
 * The input band spans the message pane only, not the full window width. That
 * is the one thing here NOT confirmed against the client (format-notes.md,
 * "Chat window splits") — the live smoke settles it, and it is a one-line
 * change either way.
 */
export function chatParts(panel: ChatPanel, rect: { w: number; h: number }): DetailPart[] {
  const out: DetailPart[] = [];
  const members = panel.userlist_width;
  if (members !== null) {
    out.push({ kind: "band", x: rect.w - members, y: 0, w: members, h: rect.h, label: "Members" });
  }
  if (panel.input_height !== null) {
    out.push({
      kind: "band",
      x: 0,
      y: rect.h - panel.input_height,
      // Clamped: a stored userlist_width can exceed the window's own width
      // (real stored data, not invented), which would otherwise go negative.
      // CSS silently drops a negative width, so the band would vanish with no
      // signal — clamp to 0 instead so it stays visible as "no room".
      w: Math.max(0, rect.w - (members ?? 0)),
      h: panel.input_height,
      label: "Input",
    });
  }
  return out;
}

// --- dispatch ---------------------------------------------------------------

/**
 * The overview window index a canvas window id names: `overview` is window 0,
 * `overview_N` is window N — the positional link `overview_tabs.rs` documents
 * on `add_overview_window` and enforces on `remove_overview_window`.
 *
 * Anchored at both ends so `overviewsettings` cannot match.
 */
export function overviewIndex(id: string): number | null {
  if (id === "overview") return 0;
  const m = /^overview_(\d+)$/.exec(id);
  return m ? Number(m[1]) : null;
}

/**
 * The detail parts for a drawn window unit, or `[]` when the unit is not a
 * family this layer knows about (which is most of them).
 *
 * For a STACK the family is resolved from the tab carrying the selection,
 * falling back to the first tab. A chat stack (`ChatWindowStack`) is the common
 * case, and the selected tab is the one the player is looking at — resolving
 * from the anchor instead would show one channel's splits while another's tab
 * is active.
 *
 * A pure function rather than a ternary in markup, so the id resolution is
 * unit-tested.
 */
export function windowDetail(
  unit: DrawUnit,
  selectedId: string | null,
  cols: OverviewColumns | null,
  chats: ChatPanel[],
  rect: { w: number; h: number },
): DetailPart[] {
  const id = unit.stack
    ? (unit.tabs.find((t) => t.id === selectedId)?.id ?? unit.tabs[0]?.id ?? unit.anchor.id)
    : unit.anchor.id;

  const ov = overviewIndex(id);
  if (ov !== null) return cols ? overviewParts(cols, ov, rect) : [];

  const panel = chats.find((c) => c.window_id === id);
  return panel ? chatParts(panel, rect) : [];
}
