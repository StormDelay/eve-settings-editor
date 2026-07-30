// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching layout.test.ts.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { DETAIL_NOMINAL, shipHudParts, fighterParts, neocomParts, overviewParts, chatParts, overviewIndex, windowDetail } from "./detail.ts";
import { HUD_NOMINAL, SHIP_ANCHOR_LEFT } from "./layout.ts";
import type { NeocomBar, OverviewColumns, ChatPanel, WindowRect } from "./api.ts";
import type { DrawUnit } from "./layout.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

// --- ship HUD --------------------------------------------------------------
// Every number below is MEASURED (2026-07-30, hud_battleship.png and
// hud_frigate.png at native 2560x1440) — see format-notes.md, "Ship HUD
// internals". These checks are what stop a future edit from quietly reverting
// the shape to the invented rectangles it started as.
{
  const parts = shipHudParts();
  const circles = parts.filter((p) => p.kind === "slot");
  const rings = parts.filter((p) => p.kind === "ring");
  const arc = parts.filter((p) => p.kind === "arc");
  const core = parts.filter((p) => p.kind === "core");

  // --- capacitor: four concentric pieces about box (148, 72) ---------------
  check("the capacitor has an outer rim and an inner ring", rings.length === 2);
  check("one gauge arc", arc.length === 1);
  check("one core", core.length === 1);
  const centred = (p: { x: number; y: number; w: number; h: number }) =>
    p.x + p.w / 2 === 148 && p.y + p.h / 2 === 72;
  check("every capacitor piece is centred on the anchor", [...rings, ...arc, ...core].every(centred));
  check("every capacitor piece is round", [...rings, ...arc, ...core].every((p) => p.w === p.h));
  // Measured r 80 outer, r 42 inner ring, r 27 core — strictly nested.
  const byR = [...rings, ...core].map((p) => p.w / 2).sort((a, b) => b - a);
  check("the capacitor radii are the measured 80 / 42 / 27", byR.join(",") === "80,42,27");
  // The anchor is the capacitor's centre, established twice by different
  // methods. If SHIP_ANCHOR_LEFT ever moves, this is what notices.
  check("the capacitor centre is SHIP_ANCHOR_LEFT", rings[0].x + rings[0].w / 2 === SHIP_ANCHOR_LEFT);

  // --- module racks: round buttons on a staggered grid ---------------------
  const rackTops = [4, 48, 92];
  const rack = circles.filter((p) => rackTops.includes(p.y));
  // 8 + 7 + 8: the staggered middle row sits half a pitch in, so it carries one
  // fewer and still spans the same width.
  check("8 / 7 / 8 module slots", rack.length === 23);
  check("module slots are round", rack.every((p) => p.w === 44 && p.h === 44));

  const row = (y: number) => rack.filter((p) => p.y === y).sort((a, b) => a.x - b.x);
  const [r0, r1, r2] = rackTops.map(row);
  check("rows carry 8, 7, 8", `${r0.length},${r1.length},${r2.length}` === "8,7,8");
  check("the first slot is at the measured x", r0[0].x === 247);
  check("slots step by the measured column pitch", r0[1].x - r0[0].x === 51);
  check("the outer rows share one grid", r0.map((p) => p.x).join() === r2.map((p) => p.x).join());
  // THE signature of EVE's rack: the middle row is staggered half a pitch.
  // Confirmed on two ships at one offset — +25 both times.
  check("the middle row is staggered half a pitch", r1[0].x - r0[0].x === 25);
  check("rows are on a uniform 44 pitch", rackTops[1] - rackTops[0] === 44 && rackTops[2] - rackTops[1] === 44);

  // --- ship-control cluster: two staggered columns, 4 then 3 ---------------
  const cluster = circles.filter((p) => !rackTops.includes(p.y));
  check("7 ship-control buttons", cluster.length === 7);
  check("cluster buttons are round", cluster.every((p) => p.w === 30 && p.h === 30));
  const colA = cluster.filter((p) => p.x === 0).sort((a, b) => a.y - b.y);
  const colB = cluster.filter((p) => p.x === 30).sort((a, b) => a.y - b.y);
  check("the cluster is 4 then 3", colA.length === 4 && colB.length === 3);
  check("cluster columns are on a 32 pitch", colA[1].y - colA[0].y === 32);
  check("the second cluster column is staggered half a step", colB[0].y - colA[0].y === 16);
  // It is the leftmost thing drawn, which is what makes it define the box edge.
  check("the cluster starts at the box's left edge", colA[0].x === 0);

  // The point of correcting HUD_NOMINAL.shipui.w to 650: the widest rack row
  // has to FIT the box it is drawn in. At the old 643 it did not.
  check(
    "every rack slot lies inside the measured ship HUD box",
    rack.every((p) => p.x >= 0 && p.x + p.w <= HUD_NOMINAL.shipui.w),
  );
  const rackRight = Math.max(...rack.map((p) => p.x + p.w));
  check("the widest rack row reaches the measured 648", rackRight === 648);
  // The capacitor is the one piece that overhangs: its measured centre and
  // radius put it 8px above the box top, which `overflow: hidden` clips —
  // exactly as EVE's own capacitor overhangs the rack block.
  check("the capacitor overhangs the box top", rings[0].y < 0);
}

// --- fighter UI ------------------------------------------------------------
// MEASURED 2026-07-30 from fighter.png, native 2560x1440, anchor (329, 289) —
// see format-notes.md, "Fighter UI internals". Same story as the ship HUD:
// everything is round, and a control column was missing entirely.
{
  const parts = fighterParts();
  const squad = parts.filter((p) => p.kind === "ring");
  const round = parts.filter((p) => p.kind === "slot");
  // The ability grid and the control column are both `slot`; the control column
  // is the only thing left of the squadron dials.
  const grid = round.filter((p) => p.x >= 70);
  const ctrl = round.filter((p) => p.x < 70);

  check("a 5 x 3 ability grid", grid.length === 15);
  check("5 squadron dials", squad.length === 5);
  check("4 fighter control buttons", ctrl.length === 4);

  check("ability buttons are round and 44 across", grid.every((p) => p.w === 44 && p.h === 44));
  check("squadron dials are round and 81 across", squad.every((p) => p.w === 81 && p.h === 81));
  check("control buttons are round and 24 across", ctrl.every((p) => p.w === 24 && p.h === 24));

  // Measured: ability grid from x 70 / y 2 on an 86 column pitch and a 50 row
  // pitch; squadron dials from x 42 / y 152 on the same 86 pitch.
  const top = grid.filter((p) => p.y === 2).sort((a, b) => a.x - b.x);
  check("the ability grid starts at the measured x", top[0].x === 70);
  check("ability columns step by the measured pitch", top[1].x - top[0].x === 86);
  const gridRows = [...new Set(grid.map((p) => p.y))].sort((a, b) => a - b);
  check("ability rows are on the measured 50 pitch", gridRows.join(",") === "2,52,102");
  const sq = squad.slice().sort((a, b) => a.x - b.x);
  check("the squadron row starts at the measured x", sq[0].x === 42);
  check("squadron columns step by the measured pitch", sq[1].x - sq[0].x === 86);
  check("the squadron row sits at the measured y", sq[0].y === 152);

  const c = ctrl.slice().sort((a, b) => a.y - b.y);
  check("the control column is at the measured x", c.every((p) => p.x === 4));
  check("control buttons are on the measured 32 pitch", c[1].y - c[0].y === 32 && c[0].y === 148);

  // It is the SQUADRON row that sets the panel width — at 5 squadrons
  // `42 + 86 x 4 + 81 = 467`, exactly HUD_NOMINAL.fighter.w. The ability grid
  // stops short at 458. Measuring them separately is what corrected an earlier
  // guess that derived the ability cell width by assuming the grid set the edge.
  check(
    "the squadron row reaches the panel's right edge exactly",
    Math.max(...squad.map((p) => p.x + p.w)) === HUD_NOMINAL.fighter.w,
  );
  check(
    "the ability grid stops short of it",
    Math.max(...grid.map((p) => p.x + p.w)) === 458,
  );

  check(
    "every fighter part fits the panel's measured width",
    parts.every((p) => p.x >= 0 && p.x + p.w <= HUD_NOMINAL.fighter.w),
  );
  // The control column runs 15px BELOW the recorded panel height: measured, it
  // ends at 268 against HUD_NOMINAL.fighter.h's 253. The height has not been
  // re-measured (the 253 is 2026-07-28's), so this pins the discrepancy rather
  // than hiding it — if the height is ever corrected, this check says so.
  const bottom = Math.max(...parts.map((p) => p.y + p.h));
  check("the control column overhangs the recorded panel height", bottom === 268);
  check("everything else fits it", Math.max(...[...grid, ...squad].map((p) => p.y + p.h)) <= HUD_NOMINAL.fighter.h);
}

// --- neocom ----------------------------------------------------------------
{
  const bar = (n: number): NeocomBar => ({
    buttons: Array.from({ length: n }, (_, i) => ({
      index: i, id: `btn${i}`, btn_type: 0, icon_path: "", children: 0,
    })),
    original: [],
  });

  // Bar 37 wide, 1440 tall: 40 reserved at the top, then 37px squares.
  const parts = neocomParts(bar(5), 37, 1440);
  check("one part per button", parts.length === 5);
  check("buttons are square, the bar's own width", parts[0].w === 37 && parts[0].h === 37);
  check("the column starts below the EVE menu cell", parts[0].y === DETAIL_NOMINAL.neocomTop);
  check("buttons stack by their own height", parts[1].y - parts[0].y === 37);
  check("buttons are labelled with their id", parts[2].label === "btn2");

  // A bar taller than the screen draws truncated, which is what EVE does.
  const short = neocomParts(bar(50), 37, 200);
  check("the column stops at the bar's height", short.length === 4);
  check(
    "no button is drawn past the bottom edge",
    short.every((p) => p.y + p.h <= 200),
  );

  // The break is `y + w > h`, so a button that exactly fills the remaining
  // space is drawn and one pixel over is not. Top cell 40 + 4 x 40 = 200.
  check("a button that exactly fills the bar is drawn", neocomParts(bar(9), 40, 200).length === 4);
  check("one pixel short drops it", neocomParts(bar(9), 40, 199).length === 3);
}

// --- overview columns ------------------------------------------------------
{
  const col = (name: string, visible: boolean, width: number | null) =>
    ({ name, label: name.toUpperCase(), visible, width });
  const cols: OverviewColumns = {
    tabs: [
      { index: 0, name: "General", preset: "p", inherits: false,
        columns: [col("icon", true, 30), col("distance", true, 90), col("name", false, 200), col("type", true, null)] },
      { index: 1, name: "Mining", preset: "p", inherits: false, columns: [col("icon", true, 30)] },
    ],
    windows: [{ index: 0, tab_indices: [0, 1] }, { index: 1, tab_indices: [] }],
    presets: [],
    appearance: { background: { enabled: [], order: [] }, flag: { enabled: [], order: [] }, colors: [], bools: [], defaulted: false },
  };

  const parts = overviewParts(cols, 0, { w: 400, h: 300 });
  const tabs = parts.filter((p) => p.kind === "cell");
  const bands = parts.filter((p) => p.kind === "column");

  check("one strip cell per tab in the window", tabs.length === 2);
  // Tabs are TEXT-WIDTH and left-packed, as EVE draws them — NOT an equal split
  // of the window, which is what this did until 2026-07-30.
  // "General" is 7 characters and "Mining" 6, so at 5.5/char + 34 padding they
  // are 72.5 and 67 — different widths, which an equal split could never give.
  check("tabs are sized by their label, not by the window", tabs[0].w === 72.5 && tabs[1].w === 67);
  check("tabs are packed left to right", tabs[0].x === 52 && tabs[1].x === 124.5);
  check("tab cells are labelled with the tab name", tabs[0].label === "General");
  // EVE keeps tabs on one line and runs out of room; it never wraps to a second
  // row. A tab that does not fit is simply not drawn.
  const cramped = overviewParts(cols, 0, { w: 130, h: 300 }).filter((p) => p.kind === "cell");
  check("a tab that does not fit is dropped", cramped.length === 1);
  check("no tab is ever drawn past the window", cramped.every((t) => t.x + t.w <= 130));

  // Only visible columns, in stored order — `name` is hidden and must be gone.
  check("hidden columns are omitted", bands.length === 3);
  check("columns keep their stored order", bands.map((b) => b.label).join(",") === "ICON,DISTANCE,TYPE");
  check("columns use their stored widths", bands[0].w === 30 && bands[1].w === 90);
  check("an absent width falls back to the nominal", bands[2].w === DETAIL_NOMINAL.columnWidth);

  // Offsets are the running sum, and the band sits below the tab strip.
  check("bands start at the running sum of widths", bands[1].x === 30 && bands[2].x === 120);
  check("bands sit below the tab strip", bands.every((b) => b.y === 30));

  // EVE shows the columns that fit on the one line and DROPS the rest — it does
  // not draw a column part-way and clip it. So an over-provisioned column set
  // reads as columns MISSING from the picture, which is what the player sees in
  // game. This replaced letting them overflow, which drew something EVE never
  // draws.
  const narrow = overviewParts(cols, 0, { w: 100, h: 300 }).filter((p) => p.kind === "column");
  // icon (30) fits in 100; distance (90) would end at 120, so it and everything
  // after it are gone. Once a column runs off the line, so does every column
  // right of it — hence stopping rather than skipping ahead to a narrower one.
  check("columns that do not fit are dropped", narrow.length === 1 && narrow[0].label === "ICON");
  check("no column is ever drawn past the window", narrow.every((b) => b.x + b.w <= 100));

  // A window with no tabs, and an index no window has.
  check("a window with no tabs draws nothing", overviewParts(cols, 1, { w: 400, h: 300 }).length === 0);
  check("an unknown window index draws nothing", overviewParts(cols, 9, { w: 400, h: 300 }).length === 0);
}

// --- chat splits -----------------------------------------------------------
{
  const rect = { w: 256, h: 424 };
  const both: ChatPanel = { window_id: "chatchannel_local", userlist_width: 135, input_height: 64 };
  const parts = chatParts(both, rect);
  check("two bands when both values are stored", parts.length === 2);

  const members = parts[0];
  check("the member list is right-anchored", members.x === 256 - 135 && members.w === 135);
  check("the member list is full height", members.y === 0 && members.h === 424);

  const input = parts[1];
  check("the input is bottom-anchored", input.y === 424 - 64 && input.h === 64);
  // Drawn under the message pane only, not under the member list. NOT captured
  // in-game — the live smoke settles it.
  check("the input spans the message pane only", input.x === 0 && input.w === 256 - 135);

  // Absent means "never resized". Inventing a default would draw a split that
  // is not there.
  const widthOnly: ChatPanel = { window_id: "c", userlist_width: 135, input_height: null };
  check("no input band without a stored height", chatParts(widthOnly, rect).length === 1);
  const inputOnly: ChatPanel = { window_id: "c", userlist_width: null, input_height: 64 };
  const io = chatParts(inputOnly, rect);
  check("no member band without a stored width", io.length === 1);
  check("the input spans the full width with no member list", io[0].w === 256);
  const neither: ChatPanel = { window_id: "c", userlist_width: null, input_height: null };
  check("nothing drawn when neither is stored", chatParts(neither, rect).length === 0);

  // A stored userlist_width can exceed the window's own width (real stored
  // data, not invented) — the input band's width must clamp at 0 rather than
  // go negative, which CSS would silently drop with no visible signal.
  const overflow: ChatPanel = { window_id: "c", userlist_width: 300, input_height: 64 };
  const of = chatParts(overflow, rect);
  check("an oversized member list clamps the input band width to 0", of[1].w === 0);
}

// --- id dispatch -----------------------------------------------------------
{
  check("the bare overview window is index 0", overviewIndex("overview") === 0);
  check("a numbered overview window is its number", overviewIndex("overview_7") === 7);
  check("the overview settings window is not an overview", overviewIndex("overviewsettings") === null);
  check("an unrelated id is not an overview", overviewIndex("market") === null);
  check("a bare trailing underscore has no digits to match", overviewIndex("overview_") === null);
  check("a non-numeric suffix does not match", overviewIndex("overview_1x") === null);
  check("a prefix that merely ends in overview_N does not match", overviewIndex("myoverview_1") === null);
  // \d+ accepts leading zeros; Number() parses them as decimal, not octal.
  check("a zero-padded number still parses", overviewIndex("overview_01") === 1);

  const w = (id: string): WindowRect => ({
    id, label: id, name: null, open: true, renderable: true,
    resolution_matches: true, geom: null, flags: [], stack: null,
  });
  const free = (id: string): DrawUnit =>
    ({ key: id, anchor: w(id), stack: null, tabs: [w(id)], fanTargets: [w(id)] });

  const rect = { w: 400, h: 300 };
  const chats: ChatPanel[] = [
    { window_id: "chatchannel_local", userlist_width: 135, input_height: 64 },
  ];
  const cols: OverviewColumns = {
    tabs: [{ index: 0, name: "General", preset: "p", inherits: false,
             columns: [{ name: "icon", label: "ICON", visible: true, width: 30 }] }],
    windows: [{ index: 0, tab_indices: [0] }],
    presets: [],
    appearance: { background: { enabled: [], order: [] }, flag: { enabled: [], order: [] }, colors: [], bools: [], defaulted: false },
  };

  check("an overview window gets column parts",
    windowDetail(free("overview"), null, cols, chats, rect).some((p) => p.kind === "column"));
  check("an overview window with no projection draws nothing",
    windowDetail(free("overview"), null, null, chats, rect).length === 0);
  check("a chat window gets its splits",
    windowDetail(free("chatchannel_local"), null, cols, chats, rect).length === 2);
  check("a chat window with no stored panel draws nothing",
    windowDetail(free("chatchannel_corp"), null, cols, chats, rect).length === 0);
  check("an unrelated window draws nothing",
    windowDetail(free("market"), null, cols, chats, rect).length === 0);

  // A stack resolves from the SELECTED tab — a chat stack is the common case,
  // and the selected tab is the one you are looking at.
  const stack: DrawUnit = {
    key: "ChatWindowStack",
    anchor: w("ChatWindowStack"),
    stack: { container_id: "ChatWindowStack", container_label: "ChatWindowStack", anchor_id: "chatchannel_corp", members: ["chatchannel_corp", "chatchannel_local"] },
    tabs: [w("chatchannel_corp"), w("chatchannel_local")],
    fanTargets: [],
  };
  check("a stack resolves from its selected tab",
    windowDetail(stack, "chatchannel_local", cols, chats, rect).length === 2);
  // With no selection, this stack's fallback (tabs[0] = chatchannel_corp) and
  // an incorrect fallback to the anchor (ChatWindowStack) both miss `chats`,
  // so this alone cannot tell the two paths apart — it still pins "a resolved
  // id with no matching panel draws nothing".
  check("a selected tab that has no panel draws nothing",
    windowDetail(stack, null, cols, chats, rect).length === 0);

  // Discriminating case: first tab IS in `chats`. Falling back to tabs[0]
  // yields the two chat bands; falling back to the anchor (ChatWindowStack2,
  // absent from chats) would yield zero — so this separates the two paths.
  const stack2: DrawUnit = {
    key: "ChatWindowStack2",
    anchor: w("ChatWindowStack2"),
    stack: { container_id: "ChatWindowStack2", container_label: "ChatWindowStack2", anchor_id: "chatchannel_local", members: ["chatchannel_local", "chatchannel_corp"] },
    tabs: [w("chatchannel_local"), w("chatchannel_corp")],
    fanTargets: [],
  };
  check("with no selection, a stack falls back to its first tab (not the anchor)",
    windowDetail(stack2, null, cols, chats, rect).length === 2);

  // A stack with no tabs at all falls back through to the anchor id, and must
  // not throw doing it.
  const emptyStack: DrawUnit = {
    key: "EmptyStack",
    anchor: w("EmptyStack"),
    stack: { container_id: "EmptyStack", container_label: "EmptyStack", anchor_id: "EmptyStack", members: [] },
    tabs: [],
    fanTargets: [],
  };
  check("a stack with no tabs falls back to the anchor without throwing",
    windowDetail(emptyStack, null, cols, chats, rect).length === 0);
}

// --- decoration-only guarantee -----------------------------------------
{
  // The ENTIRE "decoration only" guarantee of this feature rests on this one
  // CSS declaration: it is what stops detail parts from swallowing drags on
  // the rectangles they decorate. Nothing in TS/Svelte type-checking would
  // catch its removal — only this text check would, so it exists purely as a
  // tripwire for that one line.
  //
  // Scoped to the <style> block, not the whole file: the markup above it has
  // an explanatory comment that also contains the words "pointer-events:
  // none" in prose, which would let a whole-file substring search pass even
  // after the real declaration was deleted.
  const svelte = readFileSync(resolve(import.meta.dirname, "DetailParts.svelte"), "utf8");
  const style = /<style>([\s\S]*)<\/style>/.exec(svelte)?.[1] ?? "";
  check("DetailParts.svelte's <style> declares pointer-events: none", style.includes("pointer-events: none"));
  check("DetailParts.svelte's <style> never overrides it with pointer-events: auto", !style.includes("pointer-events: auto"));
}

console.log("detail.test.ts ok");
