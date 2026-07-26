// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching search.test.ts.
import {
  canvasScale, toCanvas, toData, openWindows, resizeRect, stackUnits,
  NO_FILTER, filterIsActive, windowMatches, visibleIds, drawnWindowCount,
  snapLines, movingEdges, snapDelta,
} from "./layout.ts";
import type { WindowRect } from "./api.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

check("scale maps reference width onto the container", canvasScale(2560, 1280) === 0.5);
check("scale is 1 when the reference has no width", canvasScale(0, 1280) === 1);

// Absolute direction check: toCanvas multiplies by scale (a round-trip test
// alone can't tell a correct pair from a consistently-swapped one).
check("toCanvas scales data px up to canvas px", toCanvas(2560, 0.5) === 1280);
check("toData scales canvas px back down to data px", toData(1280, 0.5) === 2560);

// The drag round-trip: a data value converted to canvas px and back is itself.
for (const scale of [0.5, 0.37, 1, 2]) {
  for (const v of [0, 1, 16, 424, 2559]) {
    check(
      `round-trip v=${v} scale=${scale}`,
      toData(toCanvas(v, scale), scale) === v,
    );
  }
}

const win = (id: string, open: boolean, renderable: boolean, stack: WindowRect["stack"] = null): WindowRect => ({
  id,
  label: id,
  name: null,
  open,
  renderable,
  resolution_matches: true,
  geom: renderable
    ? {
        x: 0, y: 0, w: 1, h: 1, screen_w: 2560, screen_h: 1440,
        x_path: [], y_path: [], w_path: [], h_path: [],
        screen_w_path: [], screen_h_path: [],
      }
    : null,
  flags: [],
  stack,
});

const wins = [win("a", true, true), win("b", false, true), win("c", true, false)];
const open = openWindows(wins);
check("open filter keeps only open AND renderable windows", open.length === 1);
check("open filter keeps the right window", open[0].id === "a");

// --- resizeRect: drag one corner, opposite corner stays anchored ------------
{
  const orig = { x: 100, y: 100, w: 200, h: 100 }; // right=300, bottom=200

  // BR: only w/h grow; top-left (100,100) anchored (today's behavior).
  const br = resizeRect(orig, "br", 40, 20);
  check("br keeps top-left anchored", br.x === 100 && br.y === 100);
  check("br grows w,h by the delta", br.w === 240 && br.h === 120);

  // TL: x/y move; bottom-right (300,200) stays fixed.
  const tl = resizeRect(orig, "tl", 40, 20);
  check("tl moves x,y by the delta", tl.x === 140 && tl.y === 120);
  check("tl keeps bottom-right fixed", tl.x + tl.w === 300 && tl.y + tl.h === 200);

  // TR: right/top move; bottom-left (100,200) stays fixed.
  const tr = resizeRect(orig, "tr", 40, 20);
  check("tr keeps bottom-left fixed", tr.x === 100 && tr.y + tr.h === 200);
  check("tr grows w, shrinks h", tr.w === 240 && tr.h === 80);

  // BL: left/bottom move; top-right (300,100) stays fixed.
  const bl = resizeRect(orig, "bl", 40, 20);
  check("bl keeps top-right fixed", bl.x + bl.w === 300 && bl.y === 100);
  check("bl shrinks w, grows h", bl.w === 160 && bl.h === 120);

  // Clamp: a delta larger than the size floors size at 0 and pins the dragged
  // corner to the anchor — it cannot cross it.
  const crossed = resizeRect(orig, "tl", 999, 999);
  check("clamp floors w,h at 0", crossed.w === 0 && crossed.h === 0);
  check("clamp pins the corner to the anchor", crossed.x === 300 && crossed.y === 200);

  // The other clamp path: a right/bottom edge shrunk past its own size floors
  // at 0 (Math.max), with x/y anchored.
  const floored = resizeRect(orig, "br", -999, -999);
  check("br floors w,h at 0 on negative overshoot", floored.w === 0 && floored.h === 0);
  check("br keeps x,y anchored when floored", floored.x === 100 && floored.y === 100);
}

// --- stackUnits: group open windows into draw units -------------------------
{
  const layout = {
    reference_w: 2560, reference_h: 1440,
    stacks: [{ container_id: "C", container_label: "C", anchor_id: "C", members: ["m1", "m2"] }],
    windows: [
      win("C", true, true, { container_id: "C", role: "container" }),
      win("m1", true, true, { container_id: "C", role: "member" }),
      win("m2", false, true, { container_id: "C", role: "member" }), // closed member: excluded from tabs
      win("free", true, true, null),
    ],
  };
  const units = stackUnits(layout as any);
  check("stackUnits produces one stack + one free unit", units.length === 2);
  const stackUnit = units.find((u) => u.stack)!;
  check("stack unit anchors on the container", stackUnit.anchor.id === "C");
  check("stack tabs are open members only, in tab order", stackUnit.tabs.map((t) => t.id).join(",") === "m1");
  const freeUnit = units.find((u) => !u.stack)!;
  check("free window is its own unit", freeUnit.key === "free" && freeUnit.tabs.length === 1);

  // fanTargets: a coherent move must repeat onto every renderable member,
  // open or closed — a closed member left out of the fan would drift out of
  // the stack (the live "jumping" bug). tabs stay display-only (open members).
  const fanIds = stackUnit.fanTargets.map((w) => w.id);
  check("fanTargets include the closed member m2 (no drift)", fanIds.includes("m2"));
  check("fanTargets include the open member m1", fanIds.includes("m1"));
  check("fanTargets include the anchor/container C", fanIds.includes("C"));
}

// --- stackUnits: a stack whose anchor (container) is closed is dropped -----
{
  const layout = {
    reference_w: 2560, reference_h: 1440,
    stacks: [{ container_id: "C", container_label: "C", anchor_id: "C", members: ["m1", "m2"] }],
    windows: [
      win("C", false, true, { container_id: "C", role: "container" }), // anchor closed
      win("m1", true, true, { container_id: "C", role: "member" }), // independently open+renderable
      win("m2", false, true, { container_id: "C", role: "member" }),
    ],
  };
  const units = stackUnits(layout as any);
  check("a closed-anchor stack does not appear as a stack unit", !units.some((u) => u.stack));
  check("its open+renderable member appears as its own free unit", units.length === 1 && units[0].key === "m1");
}

// --- stackUnits: a stack with no open members is not drawn -----------------
{
  const layout = {
    reference_w: 2560, reference_h: 1440,
    stacks: [{ container_id: "C", container_label: "C", anchor_id: "C", members: ["m1", "m2"] }],
    windows: [
      win("C", true, true, { container_id: "C", role: "container" }), // container OPEN...
      win("m1", false, true, { container_id: "C", role: "member" }), // ...but all members closed
      win("m2", false, true, { container_id: "C", role: "member" }),
      win("free", true, true, null),
    ],
  };
  const units = stackUnits(layout as any);
  check("a stack with no open members is not drawn", !units.some((u) => u.stack));
  check("its open container does not fall through as a plain window", !units.some((u) => u.key === "C"));
  check("unrelated free windows still draw", units.some((u) => u.key === "free"));
}

// --- the shared filter predicate -------------------------------------------
{
  const market = win("market", true, true);
  const closedMarket = win("market", false, true);
  const standingChat = win("chatchannel_local", true, true);
  const privateChat = win("chatchannel_private_0ee11e4f970011ea8e789abe94f5b483", true, true);
  const closedPrivateChat = win("chatchannel_private_0ee11e4f970011ea8e789abe94f5b483", false, true);
  const bareCargo = win("ShipCargo", true, true);
  const spawnedCargo = win("ShipCargo_1033391582929", true, true);

  check("an empty filter is not active", !filterIsActive(NO_FILTER));
  check("text makes it active", filterIsActive({ ...NO_FILTER, text: "a" }));
  check("whitespace-only text does not", !filterIsActive({ ...NO_FILTER, text: "  " }));
  check("openOnly makes it active", filterIsActive({ ...NO_FILTER, openOnly: true }));
  check("hideClutter makes it active", filterIsActive({ ...NO_FILTER, hideClutter: true }));

  check("an empty filter matches everything", windowMatches(standingChat, NO_FILTER));
  check("text matches the friendly label", windowMatches(market, { ...NO_FILTER, text: "mark" }));
  check("text matches case-insensitively", windowMatches(market, { ...NO_FILTER, text: "MARK" }));
  check("text matches the raw id", windowMatches(standingChat, { ...NO_FILTER, text: "chatchannel" }));
  check("text matches the detail", windowMatches(standingChat, { ...NO_FILTER, text: "local" }));
  check("text excludes a non-match", !windowMatches(market, { ...NO_FILTER, text: "zzz" }));
  check("openOnly drops a closed window", !windowMatches(closedMarket, { ...NO_FILTER, openOnly: true }));
  check("openOnly keeps an open window", windowMatches(market, { ...NO_FILTER, openOnly: true }));

  // hideClutter is about KIND of window, not open/closed — the old
  // "hide closed chat" axis was wrong and is gone.
  check("hideClutter drops an OPEN private chat", !windowMatches(privateChat, { ...NO_FILTER, hideClutter: true }));
  check("hideClutter drops a CLOSED private chat too", !windowMatches(closedPrivateChat, { ...NO_FILTER, hideClutter: true }));
  check("hideClutter keeps an OPEN standing channel", windowMatches(standingChat, { ...NO_FILTER, hideClutter: true }));
  check("hideClutter keeps a bare parent window (ShipCargo)", windowMatches(bareCargo, { ...NO_FILTER, hideClutter: true }));
  check("hideClutter drops a spawned instance (ShipCargo_<id>)", !windowMatches(spawnedCargo, { ...NO_FILTER, hideClutter: true }));
  // A closed non-clutter window is untouched by hideClutter — only openOnly
  // reaches it.
  check("hideClutter leaves a closed non-clutter window alone", !windowMatches(closedMarket, { ...NO_FILTER, hideClutter: true, openOnly: true }));
  check("hideClutter alone keeps a closed non-clutter window", windowMatches(closedMarket, { ...NO_FILTER, hideClutter: true }));

  // openOnly already drops every closed window, which would make hideClutter
  // vacuous in this composition — so compose hideClutter with text instead,
  // where it still has something of its own to contribute: a clutter and a
  // non-clutter chat window, narrowed further by text.
  const ids = visibleIds([standingChat, privateChat, market], { ...NO_FILTER, hideClutter: true, text: "chat" });
  check("visibleIds composes hideClutter with text", ids.size === 1 && ids.has("chatchannel_local"));

  // Orphaned stack frames: a minted numeric id with no stack membership at all
  // is a dead frame (its members are gone) — structural, not curated.
  const orphanFrame = win("219", true, true, null);
  const containerFrame = win("219", true, true, { container_id: "219", role: "container" });
  const memberFrame = win("219", true, true, { container_id: "C", role: "member" });
  check("hideClutter drops an orphaned numeric stack frame", !windowMatches(orphanFrame, { ...NO_FILTER, hideClutter: true }));
  check("hideClutter keeps a numeric id that IS a stack container", windowMatches(containerFrame, { ...NO_FILTER, hideClutter: true }));
  check("hideClutter keeps a numeric id that is a stack member", windowMatches(memberFrame, { ...NO_FILTER, hideClutter: true }));
  check("a non-numeric id with no stack is unaffected by the orphan rule", windowMatches(market, { ...NO_FILTER, hideClutter: true }));
  check("without hideClutter, an orphaned numeric frame is kept", windowMatches(orphanFrame, NO_FILTER));
}

// --- stackUnits under a filter ---------------------------------------------
{
  const layout = {
    reference_w: 2560, reference_h: 1440,
    stacks: [{ container_id: "C", container_label: "C", anchor_id: "C", members: ["m1", "m2"] }],
    windows: [
      win("C", true, true, { container_id: "C", role: "container" }),
      win("m1", true, true, { container_id: "C", role: "member" }),
      win("m2", true, true, { container_id: "C", role: "member" }),
      win("free", true, true, null),
    ],
  } as any;

  // No-regression: omitting the set is exactly today's behaviour.
  const before = stackUnits(layout);
  const same = stackUnits(layout, null);
  check("a null visible set is the unfiltered result", JSON.stringify(before.map((u) => u.key)) === JSON.stringify(same.map((u) => u.key)));

  // One matching member keeps the stack alive with only that tab.
  const oneTab = stackUnits(layout, new Set(["m1"]));
  const su = oneTab.find((u) => u.stack)!;
  check("a stack with one visible member survives", su !== undefined);
  check("only the visible member is a tab", su.tabs.map((t) => t.id).join(",") === "m1");
  check("the free window is filtered out", oneTab.length === 1);

  // The anchor and the fan are NOT filtered — the anchor is the geometry
  // source and the fan is what a drag writes to.
  check("the anchor ignores the filter", su.anchor.id === "C");
  const fan = su.fanTargets.map((w) => w.id).sort().join(",");
  check("fanTargets ignore the filter", fan === "C,m1,m2");

  // A stack with no visible member disappears entirely.
  const none = stackUnits(layout, new Set(["free"]));
  check("a stack with no visible member is dropped", none.length === 1 && none[0].key === "free");
  check("an empty visible set draws nothing", stackUnits(layout, new Set()).length === 0);
}

// --- drawnWindowCount: windows painted, not rectangles drawn (M5) ----------
{
  // Free-window-only layout: one rectangle per window, so the count equals
  // the unit count.
  const freeOnly = {
    reference_w: 2560, reference_h: 1440,
    stacks: [],
    windows: [win("a", true, true), win("b", true, true)],
  } as any;
  check("free-only: count equals the number of open windows", drawnWindowCount(stackUnits(freeOnly)) === 2);

  // A stack contributes its open tab count, not 1 — the whole point of the
  // fix: a 3-tab stack is one rectangle but three windows.
  const stacked = {
    reference_w: 2560, reference_h: 1440,
    stacks: [{ container_id: "C", container_label: "C", anchor_id: "C", members: ["m1", "m2", "m3"] }],
    windows: [
      win("C", true, true, { container_id: "C", role: "container" }),
      win("m1", true, true, { container_id: "C", role: "member" }),
      win("m2", true, true, { container_id: "C", role: "member" }),
      win("m3", false, true, { container_id: "C", role: "member" }), // closed: not a tab
      win("free", true, true, null),
    ],
  } as any;
  const units = stackUnits(stacked);
  check("stacked: one draw unit for the stack", units.filter((u) => u.stack).length === 1);
  check("stacked: count is tabs (2) + free (1), not units (2)", drawnWindowCount(units) === 3);

  // Filtered vs. unfiltered must agree when no filter is active — the
  // regression this fix targets: a container matching the filter while no
  // member does must not be counted, but the unfiltered case must still count
  // everything stackUnits(layout, null) draws.
  const noFilterUnits = stackUnits(stacked, null);
  check(
    "filtered-with-no-filter agrees with unfiltered",
    drawnWindowCount(noFilterUnits) === drawnWindowCount(units),
  );
}

// --- hudRects: HUD/screen furniture derived from Hud + WindowLayout --------
import { hudRects, hudNum, hudFlag, shipOffsetFromX, hudPointFromRect, HUD_NOMINAL } from "./layout.ts";
import type { Hud, HudEntry, WindowLayout } from "./api.ts";

// The four account-scoped fields, by name — a literal list rather than a
// name-prefix guess, so a future field can't be silently mislabelled.
const ACCOUNT_FIELDS = new Set(["ship_top", "fighter_detached", "fighter_shown", "neocom_width"]);

const hudEntry = (name: string, value: string | null, kind: HudEntry["kind"], dflt: string, how: "set" | "unavailable" = "set"): HudEntry => ({
  name,
  kind,
  value,
  default: dflt,
  scope: ACCOUNT_FIELDS.has(name) ? "account" : "char",
  set: how === "set" ? { how: "set", path: [] } : { how: "unavailable" },
});

const fullHud = (over: Partial<Record<string, HudEntry>> = {}): Hud => {
  const base: HudEntry[] = [
    hudEntry("ship_offset", "-100", "float", "0"),
    hudEntry("fighter_x", "326", "int", "0"),
    hudEntry("fighter_y", "54", "int", "0"),
    hudEntry("badge_x", "1000", "int", "0"),
    hudEntry("badge_y", "20", "int", "0"),
    hudEntry("ship_top", "false", "bool", "false"),
    hudEntry("fighter_detached", "true", "bool", "false"),
    hudEntry("fighter_shown", "true", "bool", "false"),
    hudEntry("neocom_width", "37", "int", "37"),
  ];
  return { entries: base.map((e) => over[e.name] ?? e) };
};

const layout2560: WindowLayout = { reference_w: 2560, reference_h: 1440, windows: [], stacks: [] };

// An absent value falls back to the default; an unavailable field reads null.
check("hudNum uses the value when present", hudNum(fullHud(), "fighter_x") === 326);
check(
  "hudNum falls back to the default when the key is absent",
  hudNum(fullHud({ fighter_x: hudEntry("fighter_x", null, "int", "7") }), "fighter_x") === 7,
);
check(
  "hudNum is null when the field is unavailable",
  hudNum(fullHud({ neocom_width: hudEntry("neocom_width", null, "int", "37", "unavailable") }), "neocom_width") === null,
);
check("hudFlag reads a bool", hudFlag(fullHud(), "fighter_detached") === true);

{
  const rects = hudRects(fullHud(), layout2560);
  const kinds = rects.map((r) => r.kind).join(",");
  check("all four elements are drawn in a stable order", kinds === "neocom,shipui,fighter,badge");

  const neocom = rects[0];
  check("the neocom is a full-height left bar", neocom.x === 0 && neocom.y === 0 && neocom.w === 37 && neocom.h === 1440);
  check("the neocom is not draggable", neocom.drag === "none");

  // Centre-relative offset: x = w/2 + offset - nominal/2 = 1280 - 100 - 343.
  const ship = rects[1];
  check("the ship HUD is centred plus the offset", ship.x === 1280 - 100 - HUD_NOMINAL.shipui.w / 2);
  check("the ship HUD sits at the bottom by default", ship.y === 1440 - HUD_NOMINAL.shipui.h);
  check("the ship HUD drags on x only", ship.drag === "x");

  const fighter = rects[2];
  check("the fighter panel sits at its stored point", fighter.x === 326 && fighter.y === 54);
  check("the fighter panel drags freely", fighter.drag === "xy");
}

{
  const rects = hudRects(fullHud({ ship_top: hudEntry("ship_top", "true", "bool", "false") }), layout2560);
  check("ship_top anchors the HUD to the top", rects[1].y === 0);
}

{
  const rects = hudRects(fullHud({ fighter_shown: hudEntry("fighter_shown", "false", "bool", "false") }), layout2560);
  check("a hidden fighter UI is not drawn", !rects.some((r) => r.kind === "fighter"));
}

{
  const rects = hudRects(fullHud({ fighter_detached: hudEntry("fighter_detached", "false", "bool", "false") }), layout2560);
  check("an attached fighter UI is not drawn", !rects.some((r) => r.kind === "fighter"));
}

{
  const rects = hudRects(
    fullHud({ neocom_width: hudEntry("neocom_width", null, "int", "37", "unavailable") }),
    layout2560,
  );
  check("no account file means no neocom bar", !rects.some((r) => r.kind === "neocom"));
}

// The drag round-trip: run hudRects to get the rect a stored value places,
// then feed that rect straight back through the matching inverse and recover
// the original stored value. This is a genuine round trip through BOTH
// functions (not a hand-computed expression that holds regardless of what
// either side does), so it fails if hudRects and its inverse ever disagree
// about the convention — e.g. one side treats a point as top-left and the
// other as centre.
{
  const hud = fullHud();
  const rects = hudRects(hud, layout2560);

  const ship = rects.find((r) => r.kind === "shipui")!;
  check(
    "shipOffsetFromX inverts hudRects' ship HUD placement",
    shipOffsetFromX(ship.x, layout2560.reference_w) === hudNum(hud, "ship_offset"),
  );

  const fighter = rects.find((r) => r.kind === "fighter")!;
  const fighterPoint = hudPointFromRect("fighter", fighter.x, fighter.y);
  check(
    "hudPointFromRect inverts hudRects' fighter placement",
    fighterPoint.x === hudNum(hud, "fighter_x") && fighterPoint.y === hudNum(hud, "fighter_y"),
  );

  const badge = rects.find((r) => r.kind === "badge")!;
  const badgePoint = hudPointFromRect("badge", badge.x, badge.y);
  check(
    "hudPointFromRect inverts hudRects' badge placement",
    badgePoint.x === hudNum(hud, "badge_x") && badgePoint.y === hudNum(hud, "badge_y"),
  );
}

// --- snapping: candidate lines ---------------------------------------------
{
  // The canvas edges are candidates even when nothing is drawn.
  const empty = snapLines([], 2560, 1440);
  check("snapLines always offers the canvas x edges", empty.x.join(",") === "0,2560");
  check("snapLines always offers the canvas y edges", empty.y.join(",") === "0,1440");

  // Each rect contributes exactly its two edges per axis.
  const lines = snapLines([{ x: 100, y: 50, w: 200, h: 80 }], 2560, 1440);
  check("a rect contributes its left and right edges", lines.x.includes(100) && lines.x.includes(300));
  check("a rect contributes its top and bottom edges", lines.y.includes(50) && lines.y.includes(130));
  check("a rect adds exactly two x candidates", lines.x.length === 4);
  check("a rect adds exactly two y candidates", lines.y.length === 4);
}

// --- snapping: which edges a drag moves -------------------------------------
{
  const r = { x: 100, y: 50, w: 200, h: 80 }; // right = 300, bottom = 130

  const move = movingEdges(r, null);
  check("a move tests both x edges", move.x.join(",") === "100,300");
  check("a move tests both y edges", move.y.join(",") === "50,130");

  // Each corner moves exactly one edge per axis: the one it is named for.
  check("tl moves left and top", movingEdges(r, "tl").x.join(",") === "100" && movingEdges(r, "tl").y.join(",") === "50");
  check("tr moves right and top", movingEdges(r, "tr").x.join(",") === "300" && movingEdges(r, "tr").y.join(",") === "50");
  check("bl moves left and bottom", movingEdges(r, "bl").x.join(",") === "100" && movingEdges(r, "bl").y.join(",") === "130");
  check("br moves right and bottom", movingEdges(r, "br").x.join(",") === "300" && movingEdges(r, "br").y.join(",") === "130");
}

// --- snapping: the search ---------------------------------------------------
{
  const lines = { x: [0, 100, 500, 2560], y: [0, 200, 1440] };

  // The correction is what CLOSES the gap: an edge at 98 with a candidate at
  // 100 corrects by +2 and lands on 100 — not 102. This is the sign test.
  const near = snapDelta({ x: [98], y: [] }, lines, 6);
  check("a near edge corrects toward the candidate", near.dx === 2);
  check("the caught candidate is reported as the guide", near.gx === 100);
  check("an axis with no moving edge does not move", near.dy === 0 && near.gy === null);

  // Outside the tolerance nothing happens at all.
  const far = snapDelta({ x: [90], y: [] }, lines, 6);
  check("an edge outside the tolerance is untouched", far.dx === 0 && far.gx === null);

  // The tolerance is inclusive at the boundary.
  const edge = snapDelta({ x: [94], y: [] }, lines, 6);
  check("an edge exactly at the tolerance still snaps", edge.dx === 6 && edge.gx === 100);

  // A rect's RIGHT edge snaps as readily as its left: the rect spans 400..502,
  // so it is the trailing edge that is 2px from the candidate at 500.
  const byRight = snapDelta({ x: [400, 502], y: [] }, lines, 6);
  check("the right edge can win the snap", byRight.dx === -2 && byRight.gx === 500);

  // Nearest wins when several candidates are in range.
  const nearest = snapDelta({ x: [102], y: [] }, { x: [100, 104], y: [] }, 6);
  check("the nearest candidate wins", nearest.dx === -2 && nearest.gx === 100);

  // Ties go to the LOWER candidate, so the result never depends on array order.
  const tie = snapDelta({ x: [102], y: [] }, { x: [104, 100], y: [] }, 6);
  check("a tie goes to the lower candidate", tie.gx === 100 && tie.dx === -2);

  // Both axes resolve independently in one call.
  const both = snapDelta({ x: [3], y: [198] }, lines, 6);
  check("both axes snap in one call", both.dx === -3 && both.dy === 2);
  check("both guides are reported", both.gx === 0 && both.gy === 200);

  // No candidates at all (an empty canvas) is a clean no-op, not a crash.
  const none = snapDelta({ x: [50], y: [50] }, { x: [], y: [] }, 6);
  check("no candidates is a no-op", none.dx === 0 && none.dy === 0 && none.gx === null && none.gy === null);
}

// --- onPointerMove's composition: resizeRect(dx + snap.dx, dy + snap.dy) ---
// This guards the composition the canvas actually performs — resizeRect and
// snapDelta are each tested pure above, but onPointerMove feeds snapDelta's
// correction back INTO resizeRect's delta rather than adding it to the
// output rect, so resizeRect's own anchor-crossing clamp still runs on the
// final numbers. Covers both corner branches: br (the "else" path, where
// resizeRect grows w) and tl (the "if (left)" path, where it moves x).
{
  const orig = { x: 100, y: 50, w: 200, h: 80 };

  const rawR = resizeRect(orig, "br", 3, 0);
  const s = snapDelta(movingEdges(rawR, "br"), { x: [300], y: [] }, 6);
  const fin = resizeRect(orig, "br", 3 + s.dx, s.dy);
  check("a br corner resize lands its moving edge on the candidate", fin.x + fin.w === 300);

  const rawL = resizeRect(orig, "tl", -3, 0);
  const s2 = snapDelta(movingEdges(rawL, "tl"), { x: [95], y: [] }, 6);
  const fin2 = resizeRect(orig, "tl", -3 + s2.dx, s2.dy);
  check("a tl corner resize lands its moving edge on the candidate", fin2.x === 95);
}

console.log("layout: all checks passed");
