// The canvas detail layer: what each rectangle looks like INSIDE. Pure — no
// DOM, no Svelte — unit-tested in detail.test.ts, rendered by DetailParts.svelte.
//
// Decoration only. Nothing here may reach hudRects, snapLines, or any drag:
// that separation is why this is its own module and not more of layout.ts.

import type { ChatPanel, NeocomBar, OverviewColumns, Stack } from "./api";
import { HUD_NOMINAL, type DrawUnit } from "./layout.ts";

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
 * ponytail: what is left here is INVENTED. Correcting each is a one-line edit,
 * and the upgrade path is a measuring pass like the 2026-07-30 one — take a
 * native screenshot, profile it, then move the corrected value OUT of this
 * object into a named constant citing format-notes.md.
 *
 * That pass has already emptied most of this object. The ship HUD, the fighter
 * panel and the overview chrome all used to live here; every one of them turned
 * out to be materially wrong (rectangles that are really circles, a rack row
 * that is really staggered, chrome bands ~60% too short), which is the argument
 * for keeping the two kinds of number strictly apart.
 *
 * The distinction matters. HUD_NOMINAL's invented 686x250 drew the ship HUD
 * 195px off its real position for three releases.
 */
export const DETAIL_NOMINAL = {
  /** Width for an overview column whose width key is absent. This one is not a
   *  screenshot away: it is EVE's own built-in default, which the file does not
   *  record and the client never shows as a number. */
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
 * The centre IS the element's anchor point horizontally: box-relative x 148,
 * against `SHIP_ANCHOR_LEFT = 148` derived independently in 2026-07-28's
 * colour-isolation pass. Two methods, same pixel. Vertically it sits at 88,
 * dead centre of the 176-tall box — because the CAPACITOR IS THE ELEMENT'S FULL
 * HEIGHT: `rim` reaches 88 either way, so the disc exactly fills the box.
 *
 * `rim` is the outer edge of the dark rim arc, visible in a 5x crop above where
 * the gauge ticks start. `gauge` is the outer edge of the tick band itself. A
 * full-circle brightness sweep peaks at r 80-81, between the two — that is the
 * rim's bright middle, not its edge, and taking it for the edge is what put the
 * element's top 8px too low on the first attempt.
 *
 * The gauge band covers the TOP HALF ONLY: the shield/armour/hull arcs sweep
 * 9 o'clock through 12 to 3, and the bottom half is the dark speed dial. That
 * asymmetry is most of the shape's signature, so it is drawn as a half annulus
 * rather than approximated with a full ring.
 */
const CAP = { cx: 148, cy: 88, rim: 88, gauge: 74, inner: 42, core: 27 };

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
const SLOTS = { firstX: 247, pitchX: 51, diameter: 44, rowTop: 20, rowPitch: 44, cols: 8, rowOffset: 25 };

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
    { x: 0, top: 40, rows: 4 },
    { x: 30, top: 56, rows: 3 },
  ],
};

/**
 * Effects applied to the ship — the row of round buff/debuff icons that is NOT
 * part of the element: it sits outside the box, below a top-aligned HUD and
 * above a bottom-aligned one.
 *
 * MEASURED 2026-07-31. `d` and `pitch` come from two native 2560x1440 shots
 * (`effect.png`, one effect; `effects.jpg`, two) which happen to place the HUD
 * identically, so they cross-check each other: ⌀36 icons on a 48 pitch, and the
 * row is CENTRED ON THE CAPACITOR — the element's own anchor, not the middle of
 * the box. The two-icon shot lands on box x 106 and 154, which this reproduces
 * exactly and which is what fixes the pitch and the centre together.
 *
 * `d` is 36 from that shot's debuff icons; the one-icon shot's buff icon reads
 * 34 and sits a pixel off what 36 predicts. That is the icon ART differing, not
 * the layout — both are centred on the same axis — so the row is modelled at
 * the larger and the test carries a 1px tolerance for the smaller.
 *
 * `gapBelow` is from the same pair: the element ends at 176 and the row's first
 * lit pixel row is 192.
 *
 * `gapAbove` is SMALLER, and the asymmetry is measured rather than sloppy. It
 * comes from three bottom-aligned shots supplied 2026-07-31, which are 2x
 * upscales of a native capture — icon diameter, UI font cap height and the
 * capacitor gauge all land on exactly 2.0, which is what licenses dividing by
 * it. All three put the row's bottom 53 upscaled px above the capacitor's first
 * lit row, and that lit row is 16 native px below the element's top edge, so
 * the gap is `53 / 2 - 16 = 10`.
 *
 * Those same shots are why the centring is trusted at high counts rather than
 * extrapolated: holding the capacitor axis fixed, their rows of 11, 10 and 11
 * icons each solve to a WHOLE number of slots, and the two icons hidden behind
 * an overlapping window on the left are exactly what makes that arithmetic
 * close. Nothing else about the row had to be assumed to get integers.
 *
 * ponytail: `pitch` is the two-icon figure. The crowded shots put it nearer 43
 * once halved — EVE tightens the row as it grows, and nothing here models that.
 * 48 therefore draws a busy row a few percent WIDE, which is the safe direction
 * for a canvas whose job is to show what a window would collide with. Upgrade
 * path: one native-resolution shot at a known count settles the rule.
 */
const SHIP_EFFECTS = { d: 36, pitch: 48, gapBelow: 16, gapAbove: 10 };

/**
 * The ship HUD's internals. The capacitor, racks and control cluster are
 * constant: the box is fixed (HUD_NOMINAL.shipui) and none of it is in any
 * settings file.
 *
 * `effects` is not constant and not in a file either — it is the view
 * preference `LayoutPrefs.effects`, because how many buffs and debuffs a pilot
 * is carrying is combat state, not configuration. `topAligned` is the stored
 * `ship_top`, which decides whether that row hangs below the element or above
 * it.
 *
 * Order matters only for painting: the capacitor's pieces are emitted outermost
 * first so the core lands on top of the dial.
 */
export function shipHudParts(effects = 0, topAligned = true): DetailPart[] {
  const disc = (r: number, kind: DetailPart["kind"]): DetailPart =>
    ({ kind, x: CAP.cx - r, y: CAP.cy - r, w: r * 2, h: r * 2 });

  const out: DetailPart[] = [
    // The outer rim, then the tick band as a half annulus (the renderer's border
    // thickness is what fills it inward from `gauge`), then the metallic inner
    // ring, then the bright core.
    disc(CAP.rim, "ring"),
    disc(CAP.gauge, "arc"),
    disc(CAP.inner, "ring"),
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

  // The effects row, last because it is the only part drawn OUTSIDE the box —
  // negative y when the HUD is bottom-aligned. The canvas has to let the ship
  // HUD's detail layer overflow for this to be visible at all; see
  // LayoutView's `.furniture.spills`.
  const n = Math.max(0, Math.round(effects));
  if (n > 0) {
    const { d, pitch, gapBelow, gapAbove } = SHIP_EFFECTS;
    // Centred on the capacitor, so the row grows symmetrically about the
    // anchor as effects come and go — which is what the shots show it doing.
    const x0 = CAP.cx - (pitch * (n - 1) + d) / 2;
    const y = topAligned ? HUD_NOMINAL.shipui.h + gapBelow : -(gapAbove + d);
    for (let i = 0; i < n; i++) {
      out.push({ kind: "slot", x: x0 + pitch * i, y, w: d, h: d });
    }
  }

  return out;
}

// --- fighter UI -------------------------------------------------------------
// MEASURED 2026-07-30 from fighter.png (native 2560x1440, anchor (329, 289),
// 4 squadrons with 3 launched) by the same profile method as the ship HUD.
// Offsets are from the anchor, which is the panel's left edge and the ability
// grid's top. See docs/format-notes.md, "Fighter UI internals".
//
// Like the ship HUD, everything here is ROUND, and there is a control column on
// the left that was not drawn at all.

const FIGHTER = {
  /** Ability grid: ⌀44 buttons — the same size as the ship HUD's module slots —
   * on the measured 86 column pitch from x 70, rows on a 50 pitch from y 2. */
  abilityX: 70, abilityY: 0, abilityD: 44, abilityRowPitch: 50, rows: 3,
  /** Squadron gauges: large ⌀81 dials on the same 86 pitch, from x 42. These are
   * what set the panel's width — at the 5-squadron carrier maximum,
   * `42 + 86 x 4 + 81 = 467`, exactly HUD_NOMINAL.fighter.w. The ability grid
   * stops short of that at 458, which is why the two are measured separately
   * rather than one being derived from the other. */
  squadX: 42, squadY: 152, squadD: 81,
  /** Fighter control column, left of the squadron dials: 4 small ⌀24 buttons on
   * a 32 pitch from y 144. Missing entirely until 2026-07-30 — and it is the
   * panel's LOWEST element, ending at 264, which is what re-measured
   * HUD_NOMINAL.fighter.h up from 253. */
  ctrlX: 4, ctrlY: 144, ctrlD: 24, ctrlPitch: 32, ctrlRows: 4,
  /** Column pitch, shared by the ability grid and the squadron row. */
  pitch: 86,
  /** A carrier's maximum squadron count, and the maximum this can ever draw. */
  cols: 5,
};

// --- target list ------------------------------------------------------------
// MEASURED 2026-07-31 off the four native 2560x1440 shots the anchor capture
// produced (target.png, vertical.png, horizontal.png, horizontal2.png), by
// bright-pixel clustering — see docs/format-notes.md, "Target list anchor".
// Offsets are from ONE SLOT's top-left; a slot is HUD_NOMINAL.target, 110x181.
const TARGET = {
  /** The lock ring: 79px of bright pixels across, centred 58 in and 40 down.
   * Two ships at one stored value gave the same extent, and the horizontal
   * shots put it at the same offset within the slot as the vertical ones. */
  ringX: 19, ringY: 0, ringD: 79,
  /** The three label rows under it — two of name, one of distance — at y 102,
   * 115 and 128 on an 8px height. Their WIDTHS are what the measured names
   * happened to be (90/68/28 for "Caldari Police / Commissioner / 29 km"), so
   * they are drawn centred on the ring: a shorter name uses fewer rows and a
   * longer one wraps differently, and neither is in any file. */
  labelY: 102, labelPitch: 13, labelH: 8, labelW: [90, 68, 28],
};

/**
 * Effects applied to a LOCKED TARGET — the same idea as the ship's row, drawn
 * under the slot's label rows.
 *
 * MEASURED 2026-07-31 from `target_effects.jpg` (native 2560x1440): ⌀25 icons
 * on a 32 pitch with their tops at slot y 142, centred on the ring's own axis,
 * which is the axis the three label rows are already centred on. Registered off
 * the label rows rather than the ring, because the ring's bright extent
 * includes the lock brackets and the labels' 13 pitch matches TARGET.labelPitch
 * exactly — two independent things agreeing on where the slot starts.
 *
 * The row ENDS at 167, inside the 181-tall slot, so `HUD_NOMINAL.target` needs
 * no adjustment and the target list's footprint is unchanged.
 *
 * `count` is fixed, unlike the ship's: this is per-slot decoration on a list
 * whose length is already a preference, and a second spinner for how many icons
 * hang under each of N targets is more knobs than the picture is worth.
 */
const TARGET_EFFECTS = { d: 25, pitch: 32, y: 142, count: 2 };

/**
 * The target list's internals: `count` identical slots, running across when the
 * account has `alignHorizontally` set and down when it does not.
 *
 * Every slot is drawn the same, which is why this does not care WHICH end of
 * the list the anchor is at — the client fills them from the anchor toward the
 * middle of the screen, but a slot at one end looks like a slot at the other.
 */
export function targetParts(count: number, horizontal: boolean): DetailPart[] {
  const out: DetailPart[] = [];
  const slot = HUD_NOMINAL.target; // one source for the slot size, shared with hudRects
  for (let i = 0; i < Math.max(1, Math.round(count)); i++) {
    const ox = horizontal ? slot.w * i : 0;
    const oy = horizontal ? 0 : slot.h * i;
    out.push({ kind: "ring", x: ox + TARGET.ringX, y: oy + TARGET.ringY, w: TARGET.ringD, h: TARGET.ringD });
    TARGET.labelW.forEach((w, row) => {
      out.push({
        kind: "band",
        x: ox + TARGET.ringX + (TARGET.ringD - w) / 2,
        y: oy + TARGET.labelY + TARGET.labelPitch * row,
        w,
        h: TARGET.labelH,
      });
    });
    // Centred on the ring, the same rule the label rows above use.
    const ex = TARGET.ringX + TARGET.ringD / 2
      - (TARGET_EFFECTS.pitch * (TARGET_EFFECTS.count - 1) + TARGET_EFFECTS.d) / 2;
    for (let e = 0; e < TARGET_EFFECTS.count; e++) {
      out.push({
        kind: "slot",
        x: ox + ex + TARGET_EFFECTS.pitch * e,
        y: oy + TARGET_EFFECTS.y,
        w: TARGET_EFFECTS.d,
        h: TARGET_EFFECTS.d,
      });
    }
  }
  return out;
}

/** The fighter panel's internals. Constant, and maxima, for the same reasons
 * as shipHudParts: squadron count is fitting-dependent and no file records it. */
export function fighterParts(): DetailPart[] {
  const out: DetailPart[] = [];
  for (let r = 0; r < FIGHTER.rows; r++) {
    for (let c = 0; c < FIGHTER.cols; c++) {
      out.push({
        kind: "slot",
        x: FIGHTER.abilityX + FIGHTER.pitch * c,
        y: FIGHTER.abilityY + FIGHTER.abilityRowPitch * r,
        w: FIGHTER.abilityD,
        h: FIGHTER.abilityD,
      });
    }
  }
  for (let c = 0; c < FIGHTER.cols; c++) {
    out.push({
      kind: "ring",
      x: FIGHTER.squadX + FIGHTER.pitch * c,
      y: FIGHTER.squadY,
      w: FIGHTER.squadD,
      h: FIGHTER.squadD,
    });
  }
  for (let i = 0; i < FIGHTER.ctrlRows; i++) {
    out.push({
      kind: "slot",
      x: FIGHTER.ctrlX,
      y: FIGHTER.ctrlY + FIGHTER.ctrlPitch * i,
      w: FIGHTER.ctrlD,
      h: FIGHTER.ctrlD,
    });
  }
  return out;
}

// --- neocom -----------------------------------------------------------------
// MEASURED 2026-08-01 off the 2026-07-28 shots (neocom_docked.png,
// hud_battleship.png, hud_frigate.png, native 2560x1440) by row profile — see
// docs/format-notes.md, "Neocom top tiles".

/**
 * Tiles above the button column, counted in bar widths: the EVE menu and the
 * character portrait. Neither is in `neocomButtonRawData` — the catalog has no
 * id for either — and both are one bar-width SQUARE, like the buttons, so the
 * reserve scales with the bar. All three shots carry a 48px bar and put the
 * first button at 96. This was an invented flat 40 until it was measured, which
 * put every button on a default-width bar more than a button too high.
 */
const NEOCOM_TOP_TILES = 2;

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
  let y = NEOCOM_TOP_TILES * w;
  for (const b of bar.buttons) {
    if (y + w > h) break;
    out.push({ kind: "button", x: 0, y, w, h: w, label: b.id });
    y += w;
  }
  return out;
}

// --- overview ---------------------------------------------------------------
// MEASURED 2026-07-30 off a real overview window in fighter.png (native
// 2560x1440), the same profile method as the HUD. These were invented until
// then, and both bands were roughly 60% short.
//
// Font-dependent: EVE's overview font size is configurable, so these are the
// default. That is a ceiling worth knowing, not a reason to keep guessing.

const OVERVIEW = {
  /** Tab strip: text band measured at y 12..23, strip through to the header at
   * ~34. Was an invented 18. */
  tabStrip: 30,
  /** Column header band: text at y 46..53, band ~34..60. Was an invented 16. */
  headerBand: 26,
  /** Tabs start clear of the window's leading overview-settings icon. */
  tabsX: 52,
  /** Approximate glyph advance and per-tab padding, from the measured strip:
   * "Main" spans 22px over 4 characters and the next tab starts 59px on, "Exit!"
   * 20px over 5 with the next 53 on. Approximate on purpose — the exact advance
   * is a font metric the canvas has no way to know, and this is decoration. */
  charWidth: 5.5,
  tabPad: 34,
};


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
  // Tabs are TEXT-WIDTH and left-packed, exactly as EVE draws them — not an
  // equal split of the window, which is what this did until 2026-07-30 and
  // which made a wide window look like a row of stretched cells. A tab that
  // does not fit is not drawn: EVE keeps them on one line and simply runs out
  // of room, it never wraps to a second row.
  let tx = OVERVIEW.tabsX;
  for (const t of tabs) {
    const w = t.name.length * OVERVIEW.charWidth + OVERVIEW.tabPad;
    if (tx + w > rect.w) break;
    out.push({ kind: "cell", x: tx, y: 0, w, h: OVERVIEW.tabStrip, label: t.name });
    tx += w;
  }

  let x = 0;
  for (const c of tabs[0].columns) {
    if (!c.visible) continue;
    // width null = the key is absent = EVE's own default, which the file does
    // not record. The nominal is the only thing available.
    const w = c.width ?? DETAIL_NOMINAL.columnWidth;
    // A column that does not fit WHOLE is not drawn at all — EVE shows what fits
    // on the one line and drops the rest. So a column set wider than its window
    // shows up as columns MISSING from the picture, which is exactly what the
    // player sees in game. (This replaced letting them overflow and clip: that
    // drew something EVE never draws.)
    if (x + w > rect.w) break;
    out.push({ kind: "column", x, y: OVERVIEW.tabStrip, w, h: OVERVIEW.headerBand, label: c.label });
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
 * The input band spans the FULL window width and the member list stops on top
 * of it. MEASURED 2026-08-01 off hud_battleship.png: the member splitter ends
 * on the input separator, and that separator runs the window's whole width
 * (format-notes.md, "Chat window splits"). The editor drew the input under the
 * message pane only until then — the shape this file had flagged as its one
 * unconfirmed guess, and it was the wrong one.
 */
export function chatParts(panel: ChatPanel, rect: { w: number; h: number }): DetailPart[] {
  const out: DetailPart[] = [];
  const input = panel.input_height ?? 0;
  const members = panel.userlist_width;
  if (members !== null) {
    out.push({
      kind: "band",
      x: rect.w - members,
      y: 0,
      w: members,
      // Clamped: a stored input_height can exceed the window's own height (real
      // stored data, not invented), which would otherwise go negative. CSS
      // silently drops a negative height, so the band would vanish with no
      // signal — clamp to 0 instead so it stays visible as "no room".
      h: Math.max(0, rect.h - input),
      label: "Members",
    });
  }
  if (panel.input_height !== null) {
    out.push({ kind: "band", x: 0, y: rect.h - input, w: rect.w, h: input, label: "Input" });
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

/**
 * The chat channels in a stack — what the "apply to this stack" button writes
 * to. Non-chat members are skipped: the split keys are named by concatenating
 * the window id, so minting one for `market` would leave a key EVE never reads.
 *
 * Takes only the stack: its `members` are already window ids from the same
 * projection, so there is nothing to cross-reference them against.
 */
export function chatStackTargets(stack: Stack): string[] {
  return stack.members.filter((id) => id.startsWith("chatchannel_"));
}

/**
 * What the chat history area is left with, for THIS character's window.
 *
 * Deliberately unclamped, and it can go negative. The splits are account-scoped
 * while the window geometry is character-scoped, so a value that fits one
 * character can overflow another's window — and reporting that honestly is the
 * point. Clamping would hide exactly the case worth seeing.
 *
 * An absent split subtracts nothing: the player has never resized it, so EVE's
 * own default applies and the file has no number to show.
 */
export function historyArea(
  geom: { w: number; h: number },
  panel: ChatPanel | undefined,
): { w: number; h: number } {
  return {
    w: geom.w - (panel?.userlist_width ?? 0),
    h: geom.h - (panel?.input_height ?? 0),
  };
}
