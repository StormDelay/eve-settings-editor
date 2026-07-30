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
  kind: "ring" | "slot" | "cell" | "band" | "column" | "button";
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
  /** Module slot cell, drawn inside the measured 50 x ~46 pitch. */
  slot: { w: 44, h: 40 },
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
// MEASURED 2026-07-28, docs/format-notes.md "HUD anchors", internal-geometry
// table. All offsets are from the element's own top-left.

/** Capacitor ring: spans x 73..231 (diameter ~158), centred on y ~74. */
const RING = { left: 73, diameter: 158, centreY: 74 };
/** Module slots: first slot x 245, column pitch 50, 8 columns x 3 rows max. */
const SLOTS = { firstX: 245, pitchX: 50, cols: 8 };
/**
 * Row tops used VERBATIM, not as a pitch: 2 -> 50 is 48 and 50 -> 94 is 44, so
 * a single averaged pitch would be wrong on two of the three rows.
 */
const SLOT_ROW_TOPS = [2, 50, 94];

/**
 * The ship HUD's internals. Constant: the box is fixed (HUD_NOMINAL.shipui) and
 * none of this is in any settings file.
 *
 * The slot count is the MAXIMUM (8), because it is ship-dependent and nothing
 * records it — and because the box's measured width was derived from it
 * (245 + 50 x 8 = 643). Drawing fewer would under-draw the footprint the canvas
 * already reserves.
 */
export function shipHudParts(): DetailPart[] {
  const out: DetailPart[] = [
    {
      kind: "ring",
      x: RING.left,
      // The measured centre puts the ring 5px above the box top. That is not a
      // transcription slip — the element's own `overflow: hidden` clips it, the
      // same way EVE's capacitor overhangs the rack block.
      y: RING.centreY - RING.diameter / 2,
      w: RING.diameter,
      h: RING.diameter,
    },
  ];
  for (const top of SLOT_ROW_TOPS) {
    for (let c = 0; c < SLOTS.cols; c++) {
      out.push({
        kind: "slot",
        x: SLOTS.firstX + SLOTS.pitchX * c,
        y: top,
        ...DETAIL_NOMINAL.slot,
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
