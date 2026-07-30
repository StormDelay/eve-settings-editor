// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching layout.test.ts.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { DETAIL_NOMINAL, shipHudParts, fighterParts, neocomParts, overviewParts, chatParts, overviewIndex, windowDetail } from "./detail.ts";
import { HUD_NOMINAL } from "./layout.ts";
import type { NeocomBar, OverviewColumns, ChatPanel, WindowRect } from "./api.ts";
import type { DrawUnit } from "./layout.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

// --- ship HUD --------------------------------------------------------------
{
  const parts = shipHudParts();
  const ring = parts.filter((p) => p.kind === "ring");
  const slots = parts.filter((p) => p.kind === "slot");

  check("one capacitor ring", ring.length === 1);
  // Measured: the ring spans x 73..231, so it is 158 wide and its left edge is
  // at 73 — NOT centred on the box.
  check("the ring sits at the measured span", ring[0].x === 73 && ring[0].w === 158);
  check("the ring is round", ring[0].h === ring[0].w);

  check("8 columns x 3 rows of module slots", slots.length === 24);
  // Measured: first slot x 245, column pitch 50.
  const row0 = slots.filter((p) => p.y === 2).sort((a, b) => a.x - b.x);
  check("the first slot is at the measured x", row0[0].x === 245);
  check("slots step by the measured column pitch", row0[1].x - row0[0].x === 50);
  // Measured verbatim, NOT as an averaged pitch: 2 -> 50 is 48, 50 -> 94 is 44.
  const tops = [...new Set(slots.map((p) => p.y))].sort((a, b) => a - b);
  check("the three row tops are the measured ones", tops.join(",") === "2,50,94");

  // The whole point of the measured box: everything drawn inside it must fit.
  // The ring is the one exception — its measured centre puts it 5px above the
  // box top, which the rectangle's own `overflow: hidden` clips.
  check(
    "every slot lies inside the measured ship HUD box",
    slots.every((p) => p.x >= 0 && p.x + p.w <= HUD_NOMINAL.shipui.w
      && p.y >= 0 && p.y + p.h <= HUD_NOMINAL.shipui.h),
  );
}

// --- fighter UI ------------------------------------------------------------
{
  const parts = fighterParts();
  const cells = parts.filter((p) => p.kind === "cell");

  // 5 x 3 ability grid plus a 5-cell squadron row = 20.
  check("20 fighter cells", cells.length === 20);

  // 178 is the MEASURED squadron row top (format-notes.md, "HUD anchors").
  const grid = cells.filter((p) => p.y < 178);
  const squad = cells.filter((p) => p.y >= 178);
  check("15 ability cells", grid.length === 15);
  check("5 squadron cells", squad.length === 5);

  // Measured: ability grid starts at x 70, squadron row at x 43, both on an
  // 86px column pitch.
  const top = grid.filter((p) => p.y === 0).sort((a, b) => a.x - b.x);
  check("the ability grid starts at the measured x", top[0].x === 70);
  check("ability columns step by the measured pitch", top[1].x - top[0].x === 86);
  const sq = squad.sort((a, b) => a.x - b.x);
  check("the squadron row starts at the measured x", sq[0].x === 43);
  check("squadron columns step by the measured pitch", sq[1].x - sq[0].x === 86);

  check(
    "every fighter cell lies inside the measured fighter box",
    cells.every((p) => p.x >= 0 && p.x + p.w <= HUD_NOMINAL.fighter.w
      && p.y >= 0 && p.y + p.h <= HUD_NOMINAL.fighter.h),
  );
  // The cell widths are DERIVED from the measured panel width, not guessed:
  // both rows must reach its right edge exactly, from different origins.
  // If HUD_NOMINAL.fighter.w is ever corrected, these are what fail.
  const right = (ps: typeof cells) => Math.max(...ps.map((p) => p.x + p.w));
  check("the ability grid reaches the panel's right edge", right(grid) === HUD_NOMINAL.fighter.w);
  check("the squadron row reaches the panel's right edge", right(squad) === HUD_NOMINAL.fighter.w);
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
  check("tab cells split the rect width", tabs[0].w === 200 && tabs[1].x === 200);
  check("tab cells are labelled with the tab name", tabs[0].label === "General");

  // Only visible columns, in stored order — `name` is hidden and must be gone.
  check("hidden columns are omitted", bands.length === 3);
  check("columns keep their stored order", bands.map((b) => b.label).join(",") === "ICON,DISTANCE,TYPE");
  check("columns use their stored widths", bands[0].w === 30 && bands[1].w === 90);
  check("an absent width falls back to the nominal", bands[2].w === DETAIL_NOMINAL.columnWidth);

  // Offsets are the running sum, and the band sits below the tab strip.
  check("bands start at the running sum of widths", bands[1].x === 30 && bands[2].x === 120);
  check("bands sit below the tab strip", bands.every((b) => b.y === DETAIL_NOMINAL.tabStrip));

  // THE payoff: a column set wider than its window runs off the edge, and that
  // is the signal. No clamping.
  const narrow = overviewParts(cols, 0, { w: 100, h: 300 }).filter((p) => p.kind === "column");
  check("an overflowing column set runs past the rect", narrow[2].x + narrow[2].w > 100);

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
