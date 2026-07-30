// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching search.test.ts.
import {
  canvasScale, toCanvas, toData, openWindows, resizeRect, stackUnits,
  NO_FILTER, filterIsActive, windowMatches, isOrphanFrame, visibleIds, drawnWindowCount,
  snapLines, movingEdges, snapDelta, unitAt, rectsAt, moveInOrder, dropAction, linkInventory,
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

// --- the Inventory fold -----------------------------------------------------
{
  const station = win("InventoryStation", true, true);
  const structure = win("InventoryStructure", true, true);
  const market = win("market", true, true);
  const layout = { reference_w: 2560, reference_h: 1440, windows: [station, structure, market], stacks: [] };

  const all = linkInventory(stackUnits(layout as any), "all", layout.windows);
  check("all leaves both Inventory units alone", all.filter((u) => u.key.startsWith("Inventory")).length === 2);
  check("all leaves each fanning only to itself",
    all.every((u) => !u.key.startsWith("Inventory") || u.fanTargets.length === 1));

  const docked = linkInventory(stackUnits(layout as any), "docked", layout.windows);
  const inv = docked.filter((u) => u.key.startsWith("Inventory"));
  check("docked paints one Inventory rectangle", inv.length === 1);
  check("docked keeps the station copy as the anchor", inv[0].key === "InventoryStation");
  const ids = inv[0].fanTargets.map((w) => w.id).sort();
  check("docked fans a drag onto both copies", ids.join(",") === "InventoryStation,InventoryStructure");
  check("the fold leaves unrelated units untouched", docked.some((u) => u.key === "market"));

  // tabs, not just fanTargets: every selection consumer (the panel row, the
  // canvas highlight, resize handles, the arrow-key nudge) keys off anchor.id
  // or tabs, so a station-only tabs array would leave the structure row
  // selectable in the panel but inert on the canvas (F1).
  const tabIds = inv[0].tabs.map((w) => w.id).sort();
  check("the merged unit's tabs carry both Inventory ids", tabIds.join(",") === "InventoryStation,InventoryStructure");
  const marketUnit = docked.find((u) => u.key === "market")!;
  check("a normal free unit still has exactly one tab", marketUnit.tabs.length === 1);

  // The space copy is a different id and is never folded — it is a genuinely
  // separate window with its own position, and it is not in the docked view.
  const spaceOnly = { ...layout, windows: [win("InventorySpace", true, true), market] };
  const space = linkInventory(stackUnits(spaceOnly as any), "space", spaceOnly.windows);
  check("the space copy is left alone", space.filter((u) => u.key === "InventorySpace").length === 1);

  // The other copy does not exist in the file at all — nothing to fan to, and
  // the survivor must still draw.
  const lone = { ...layout, windows: [structure, market] };
  const folded = linkInventory(stackUnits(lone as any), "docked", lone.windows);
  check("a lone structure copy still draws", folded.some((u) => u.key === "InventoryStructure"));
  check("with no second copy in the file, the survivor fans only to itself",
    folded.find((u) => u.key === "InventoryStructure")!.fanTargets.length === 1);

  // THE CLOSED-COPY BUG. stackUnits only makes units from OPEN windows, so the
  // first version of this sourced the pair from `units` and silently dropped a
  // closed copy — a drag then moved one and left the other behind, which is
  // exactly the drift stackUnits' own fanTargets already guards against for
  // closed stack members. The fan follows RENDERABILITY, not openness.
  const closedStructure = win("InventoryStructure", false, true);
  const stationOpen = { ...layout, windows: [station, closedStructure, market] };
  const so = linkInventory(stackUnits(stationOpen as any), "docked", stationOpen.windows);
  const soInv = so.filter((u) => u.key.startsWith("Inventory"));
  check("a closed structure copy still leaves one drawn rectangle", soInv.length === 1);
  check("the open station copy anchors it", soInv[0].key === "InventoryStation");
  check("a drag fans onto the CLOSED structure copy too",
    soInv[0].fanTargets.map((w) => w.id).sort().join(",") === "InventoryStation,InventoryStructure");

  // ...and the mirror: the station copy closed, the structure copy open.
  const closedStation = win("InventoryStation", false, true);
  const structureOpen = { ...layout, windows: [closedStation, structure, market] };
  const sto = linkInventory(stackUnits(structureOpen as any), "docked", structureOpen.windows);
  const stoInv = sto.filter((u) => u.key.startsWith("Inventory"));
  check("a closed station copy still leaves one drawn rectangle", stoInv.length === 1);
  check("the open structure copy anchors it when the station one is closed",
    stoInv[0].key === "InventoryStructure");
  check("a drag fans onto the CLOSED station copy too",
    stoInv[0].fanTargets.map((w) => w.id).sort().join(",") === "InventoryStation,InventoryStructure");

  // A stacked Inventory already fans to its stack; merging across stacks is
  // out of scope, so the fold declines rather than guessing. Note the stacked
  // copy's unit is keyed by its CONTAINER, so it never matches the fold's
  // `key === "InventoryStation"` test in the first place — this pins that.
  const stacked = {
    reference_w: 2560, reference_h: 1440,
    stacks: [{ container_id: "C", container_label: "C", anchor_id: "C", members: ["InventoryStation"] }],
    windows: [
      win("C", true, true, { container_id: "C", role: "container" }),
      win("InventoryStation", true, true, { container_id: "C", role: "member" }),
      win("InventoryStructure", true, true, null),
      market,
    ],
  };
  const untouched = linkInventory(stackUnits(stacked as any), "docked", stacked.windows);
  check("a stacked Inventory is not folded", untouched.some((u) => u.key === "InventoryStructure"));
  check("the stacked copy still draws as its stack", untouched.some((u) => u.key === "C" && u.stack));
  // Now that the fan is sourced from the window list rather than the units, it
  // could reach INTO a stack — which would drag the stacked copy out of place
  // on every move of the free one. The stack owns its members' geometry.
  check("the fan does not reach a stacked copy",
    untouched.find((u) => u.key === "InventoryStructure")!.fanTargets
      .every((w) => w.id !== "InventoryStation"));

  // Guard the `env !== "docked"` check itself, not just its observable effect
  // in "all" above: a mutation to `env === "all"` would still pass every check
  // above (both leave the pair unfolded) but silently fold in "space" too.
  // This state is UNREACHABLE in the running app — both Inventory ids are in
  // DOCKED_ONLY, so `windowMatches`/`inEnv` strips them out of the visible set
  // before stackUnits ever sees them under `env: "space"`. The test exists to
  // pin the guard, not to describe a path a player can hit.
  const spacePair = linkInventory(stackUnits(layout as any), "space", layout.windows);
  check("a docked pair under a space env is left unfolded (guard, not a reachable path)",
    spacePair.filter((u) => u.key.startsWith("Inventory")).length === 2);
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

  // The four hideClutter checks above already run these fixtures through
  // isOrphanFrame — windowMatches calls it — so re-asserting each one adds no
  // coverage. This pins the exported name, which the delete offer counts with.
  check("the orphan rule is exported for the delete offer to count with", isOrphanFrame(orphanFrame));

  const lobby = win("lobbyWnd", true, true);
  const dscan = win("directionalScannerWindow", true, true);

  check("the default env does not make the filter active", !filterIsActive({ ...NO_FILTER, env: "all" }));
  check("a docked env makes the filter active", filterIsActive({ ...NO_FILTER, env: "docked" }));
  check("a space env makes the filter active", filterIsActive({ ...NO_FILTER, env: "space" }));

  check("docked keeps a docked-only window", windowMatches(lobby, { ...NO_FILTER, env: "docked" }));
  check("docked drops a space-only window", !windowMatches(dscan, { ...NO_FILTER, env: "docked" }));
  check("space keeps a space-only window", windowMatches(dscan, { ...NO_FILTER, env: "space" }));
  check("space drops a docked-only window", !windowMatches(lobby, { ...NO_FILTER, env: "space" }));
  check("an unlisted window survives both envs",
    windowMatches(market, { ...NO_FILTER, env: "docked" }) && windowMatches(market, { ...NO_FILTER, env: "space" }));

  // env composes with the other dimensions rather than replacing them.
  check("env and openOnly compose", !windowMatches(closedMarket, { ...NO_FILTER, env: "docked", openOnly: true }));
  check("env and text compose", !windowMatches(lobby, { ...NO_FILTER, env: "docked", text: "zzz" }));

  check("visibleIds narrows by env",
    !visibleIds([lobby, dscan, market], { ...NO_FILTER, env: "space" }).has("lobbyWnd"));
  check("visibleIds keeps the space window and the unlisted one",
    visibleIds([lobby, dscan, market], { ...NO_FILTER, env: "space" }).size === 2);
}

// --- the filter searches the real channel name -----------------------------
{
  const named = { ...win("chatchannel_private_0ee11e4f970011ea", true, true), name: "Alliance HQ" };
  check("text matches EVE's own name", windowMatches(named, { ...NO_FILTER, text: "alliance" }));
  check("text still matches the raw id", windowMatches(named, { ...NO_FILTER, text: "chatchannel" }));
  check("text still matches the derived detail", windowMatches(named, { ...NO_FILTER, text: "private" }));
  const unnamed = win("market", true, true);
  check("an unnamed window still matches its derived label", windowMatches(unnamed, { ...NO_FILTER, text: "market" }));
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

  // A stack container that matches the filter while NONE of its members do
  // draws nothing — counting the raw window list would over-report it.
  const containerOnly = new Set(["C"]);
  const filtered = stackUnits(stacked, containerOnly);
  check("a container-only filter match draws nothing", drawnWindowCount(filtered) === 0);
}

// --- hudRects: HUD/screen furniture derived from Hud + WindowLayout --------
import { hudRects, hudNum, hudFlag, shipOffsetFromX, hudPointFromRect, SHIP_ANCHOR_LEFT } from "./layout.ts";
import type { Hud, HudEntry, WindowLayout } from "./api.ts";

// The four account-scoped fields, by name — a literal list rather than a
// name-prefix guess, so a future field can't be silently mislabelled.
const ACCOUNT_FIELDS = new Set(["ship_top", "fighter_detached", "fighter_shown", "neocom_width"]);

// `insert` is what the backend really sends for a key the file does not have —
// the shape an absent value arrives in. The helper could only build `set` and
// `unavailable`, so every "absent" case here was a `set` with a null value:
// behaviourally the same for these functions, but not the wire shape.
const hudEntry = (name: string, value: string | null, kind: HudEntry["kind"], dflt: string, how: "set" | "unavailable" | "insert" = "set"): HudEntry => ({
  name,
  kind,
  value,
  default: dflt,
  scope: ACCOUNT_FIELDS.has(name) ? "account" : "char",
  set:
    how === "set"
      ? { how: "set", path: [] }
      : how === "insert"
        ? { how: "insert", parent: [], key: { kind: "bytes_hex", v: "" } }
        : { how: "unavailable" },
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
  hudNum(fullHud({ fighter_x: hudEntry("fighter_x", null, "int", "7", "insert") }), "fighter_x") === 7,
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

  // The offset places the CAPACITOR's centre; the left edge is 148px left of
  // that (measured 2026-07-28): x = 1280 - 100 - 148.
  const ship = rects[1];
  check("the ship HUD's anchor is centre plus the offset", ship.x + 148 === 1280 - 100);
  // 12, not the 28 above it: both margins are measured now (2026-07-28, one
  // character photographed top- and bottom-aligned at one offset) and the
  // element is NOT vertically symmetric. Mirroring the top margin, which this
  // asserted before, drew the box 16px high.
  check("the ship HUD sits at the bottom by default", ship.y === 1440 - 12 - 160);
  check("the ship HUD drags on x only", ship.drag === "x");

  const fighter = rects[2];
  check("the fighter panel sits at its stored point", fighter.x === 326 && fighter.y === 54);
  check("the fighter panel drags freely", fighter.drag === "xy");

  // The badge was only ever checked for its place in the order string.
  const badge = rects[3];
  check("the badge sits at its stored point", badge.x === 1000 && badge.y === 20);
  check("the badge is the nominal 32x32", badge.w === 32 && badge.h === 32);
  check("the badge drags freely", badge.drag === "xy");
}

{
  // A width of exactly 0 is a present value, not an absent one, so it takes the
  // `> 0` branch rather than the null check — and a zero-width rect would be an
  // invisible thing the canvas still hit-tests.
  const rects = hudRects(fullHud({ neocom_width: hudEntry("neocom_width", "0", "int", "37") }), layout2560);
  check("a zero-width neocom is not drawn", !rects.some((r) => r.kind === "neocom"));
}

{
  const rects = hudRects(fullHud({ ship_top: hudEntry("ship_top", "true", "bool", "false") }), layout2560);
  // Top-aligned it clears the screen edge by the measured 28px, not by 0.
  check("ship_top anchors the HUD to the top", rects[1].y === 28);
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

// --- the conventions, against the live client ------------------------------
// The round trip above only proves hudRects and its inverse agree with EACH
// OTHER; it passes just as happily if both are wrong the same way. These pin
// them to what the client actually did on 2026-07-27 at 2560x1440, character
// 93622368. See docs/format-notes.md § "HUD anchors".
{
  const at = (offset: string) =>
    hudRects(fullHud({ ship_offset: hudEntry("ship_offset", offset, "float", "0") }), layout2560)
      .find((r) => r.kind === "shipui")!;

  // Written 0.0, the client drew the HUD "dead centre" — and 2026-07-28 showed
  // what actually centres is the CAPACITOR wheel, at SHIP_ANCHOR_LEFT from the
  // left edge, not the box. Asserting on the anchor rather than the left edge is
  // deliberate for the same reason it always was: it holds whatever
  // HUD_NOMINAL's width turns out to be, while a regression to a left-edge
  // origin still fails it.
  const centred = at("0");
  check("live: offset 0 centres the ship HUD's capacitor", centred.x + SHIP_ANCHOR_LEFT === 1280);

  // Dragged left, the client wrote -642.0 -> the capacitor sits at 638
  // (measured 638.5 off the screenshot).
  const dragged = at("-642");
  check("live: offset -642 puts the capacitor at 638", dragged.x + SHIP_ANCHOR_LEFT === 638);
  check("live: a negative offset moves the HUD left", dragged.x < centred.x);

  // The fighter UI's stored point is the panel's top-left in absolute screen
  // px. x: dragged mid-screen the client stored 839, measured at 838. y: given
  // 497 — the exact top of A1's D-Scan window — the panel's ability grid drew
  // level with D-Scan's top, and the client wrote (839, 497) straight back.
  const fighter = hudRects(
    fullHud({
      fighter_x: hudEntry("fighter_x", "839", "int", "0"),
      fighter_y: hudEntry("fighter_y", "497", "int", "0"),
    }),
    layout2560,
  ).find((r) => r.kind === "fighter")!;
  check("live: the fighter UI's stored point is its top-left", fighter.x === 839 && fighter.y === 497);
}

// The 2026-07-28 screenshot, reproduced: Storm Delay, 2560x1440, offset -642,
// top-aligned. Every number is measured, not assumed — see the plan's
// Background table. If one of these changes, a real screenshot disagreed.
{
  const hud = fullHud({
    ship_offset: hudEntry("ship_offset", "-642", "float", "0"),
    ship_top: hudEntry("ship_top", "true", "bool", "false"),
  });
  const ship = hudRects(hud, layout2560).find((f) => f.kind === "shipui")!;

  check("the ship HUD's left edge sits 148px left of the anchor", ship.x === 490);
  // 1133 until 2026-07-30, when the same screenshot was re-measured by
  // brightness profile for the detail layer: the 8-slot row's last button spans
  // 1094..1137, so the element's right edge is 1138. The old figure came from
  // `245 + 50 x 8`, which mixes a pitch with a button width — see HUD_NOMINAL.
  check("its right edge covers the widest slot row", ship.x + ship.w === 1138);
  check("its top clears the screen edge by the measured margin", ship.y === 28);
  check("its bottom edge, 160px below the top", ship.y + ship.h === 188);

  // The anchor: the capacitor wheel's centre, measured at 638.5 against 638
  // predicted. NOTE this check holds for any SHIP_ANCHOR_LEFT — it pins the
  // formula's SHAPE, not the constant. What pins 148 is the literal 490 above,
  // which is why that one is written as a number and not as an expression.
  check(
    "the anchor lands on the capacitor wheel, not the box centre",
    ship.x + SHIP_ANCHOR_LEFT === 2560 / 2 - 642,
  );
}

// shipOffsetFromX must be the exact inverse of the placement above, or a drag
// writes an offset that puts the HUD somewhere other than where it was dropped.
// ODD widths are the case that matters and the one a 2560-only loop cannot see:
// a half-pixel screen centre makes Math.round bias half-up, so rounding on both
// sides stacks instead of cancelling and every drag writes an offset 1px off.
// 2559 fails this loop if the placement is ever rounded again.
{
  for (const referenceW of [2560, 1920, 3440, 2559, 1921]) {
    const layout = { ...layout2560, reference_w: referenceW };
    for (const offset of [-642, -189, 0, 300]) {
      const hud = fullHud({
        ship_offset: hudEntry("ship_offset", String(offset), "float", "0"),
        ship_top: hudEntry("ship_top", "true", "bool", "false"),
      });
      const ship = hudRects(hud, layout).find((f) => f.kind === "shipui")!;
      check(
        `dragging to its own drawn x round-trips the offset (w=${referenceW}, ${offset})`,
        shipOffsetFromX(ship.x, referenceW) === offset,
      );
    }
  }
}

// The 2026-07-28 fighter shot: anchor (329, 289), 4 squadrons with 3 launched.
// The panel's own top-left IS the anchor — that half was already right — so this
// pins the size, and specifically that the ability grid is inside it.
{
  // fighter_detached and fighter_shown are already true in fullHud's base, which
  // is what makes hudRects emit the panel at all — only the point changes here.
  const hud = fullHud({
    fighter_x: hudEntry("fighter_x", "329", "int", "0"),
    fighter_y: hudEntry("fighter_y", "289", "int", "0"),
  });
  const f = hudRects(hud, layout2560).find((x) => x.kind === "fighter")!;

  check("the fighter panel starts at the stored anchor", f.x === 329 && f.y === 289);
  check("it is wide enough for five squadrons", f.w === 467);
  // The regression this guards: the old 120 covered the squadron row alone, so
  // windows snapped straight through the ability grid above it.
  check(
    "it is tall enough for the ability grid, not just the squadron row",
    f.h === 253,
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

// --- overrides reach the filter --------------------------------------------
{
  const market = win("market", true, true);
  const invite = win("ChatInvitation_x", true, true);
  const forced = { clutter: new Set(["market"]), visible: new Set<string>() };
  check("hideClutter drops an overridden-clutter window",
    !windowMatches(market, { ...NO_FILTER, hideClutter: true }, forced));
  check("without hideClutter the override changes nothing",
    windowMatches(market, { ...NO_FILTER }, forced));

  const freed = { clutter: new Set<string>(), visible: new Set(["ChatInvitation_x"]) };
  check("hideClutter keeps a rescued window",
    windowMatches(invite, { ...NO_FILTER, hideClutter: true }, freed));

  const ids = visibleIds([market, invite], { ...NO_FILTER, hideClutter: true }, freed);
  check("visibleIds honours the overrides", ids.has("ChatInvitation_x") && ids.has("market"));
}

// --- unitAt: topmost drawn unit under a data-px point ------------------------
{
  const rect = (x: number, y: number, w: number, h: number) => ({ x, y, w, h });
  const u = (key: string, r: { x: number; y: number; w: number; h: number }) =>
    ({ key, anchor: { id: key }, stack: null, tabs: [], fanTargets: [], rect: r }) as any;
  // Two overlapping units; `big` is drawn first, `small` second (on top).
  const big = u("big", rect(0, 0, 500, 500));
  const small = u("small", rect(100, 100, 100, 100));
  const units = [big, small];
  const rectOf = (x: any) => x.rect;

  check("unitAt returns the later-drawn unit where they overlap",
    unitAt(units, rectOf, 150, 150)?.key === "small");
  check("unitAt returns the only unit under a non-overlapping point",
    unitAt(units, rectOf, 400, 400)?.key === "big");
  check("unitAt returns null on empty canvas",
    unitAt(units, rectOf, 900, 900) === null);
  check("unitAt counts the rect edge as inside",
    unitAt(units, rectOf, 100, 100)?.key === "small");

  // --- rectsAt: everything under the point, for the right-click picker ------
  check("rectsAt returns every unit containing the point, topmost first",
    rectsAt(units, rectOf, 150, 150).map((x: any) => x.key).join(",") === "small,big");
  check("rectsAt returns just the one where they do not overlap",
    rectsAt(units, rectOf, 400, 400).map((x: any) => x.key).join(",") === "big");
  check("rectsAt returns nothing on empty canvas",
    rectsAt(units, rectOf, 900, 900).length === 0);
  check("rectsAt counts the rect edge as inside, like unitAt",
    rectsAt(units, rectOf, 100, 100).map((x: any) => x.key).join(",") === "small,big");

  // THE one that matters. Two walks now answer "what is under this point":
  // unitAt keeps its early-returning walk because it runs on every pointermove
  // of a drag, so it must not allocate. This pins the two to one ranking, so a
  // change to either cannot make the menu's "topmost" differ from the window a
  // plain click selects.
  for (const [x, y] of [[150, 150], [400, 400], [100, 100], [900, 900]]) {
    check(`unitAt agrees with rectsAt[0] at ${x},${y}`,
      (unitAt(units, rectOf, x, y) ?? null) === (rectsAt(units, rectOf, x, y)[0] ?? null));
  }

  // Furniture is the second caller, and it IS its own rect rather than
  // carrying one — which is why rectsAt is generic over the element type.
  const neocom = { kind: "neocom", label: "Neocom", x: 0, y: 0, w: 60, h: 1440 };
  const shipui = { kind: "shipui", label: "Ship HUD", x: 900, y: 1200, w: 643, h: 160 };
  const furniture = [neocom, shipui] as any[];
  check("rectsAt works on furniture, which is its own rect",
    rectsAt(furniture, (f: any) => f, 30, 700).map((f: any) => f.label).join(",") === "Neocom");
  check("rectsAt finds no furniture away from it",
    rectsAt(furniture, (f: any) => f, 500, 500).length === 0);
}

// --- moveInOrder: the full ordering reorder_stack takes ----------------------
{
  const ids = ["a", "b", "c", "d"];
  check("moveInOrder moves an id forward",
    moveInOrder(ids, "a", 2).join(",") === "b,c,a,d");
  check("moveInOrder moves an id backward",
    moveInOrder(ids, "d", 1).join(",") === "a,d,b,c");
  check("moveInOrder to the same index is unchanged",
    moveInOrder(ids, "b", 1).join(",") === "a,b,c,d");
  check("moveInOrder clamps an index past the end",
    moveInOrder(ids, "a", 99).join(",") === "b,c,d,a");
  check("moveInOrder leaves the input array alone",
    ids.join(",") === "a,b,c,d");
}

// --- dropAction: the whole canvas gesture matrix -----------------------------
{
  const rect = { x: 10, y: 20, w: 300, h: 200 };
  // Minimal DrawUnit shapes: dropAction only reads key / anchor.id / stack /
  // tabs[].id, so the fixtures carry exactly those.
  const freeUnit = (id: string) =>
    ({ key: id, anchor: { id }, stack: null, tabs: [{ id }], fanTargets: [] }) as any;
  const stackUnit = (container: string, members: string[]) =>
    ({
      key: container,
      anchor: { id: container },
      stack: { container_id: container, container_label: container, anchor_id: container, members },
      tabs: members.map((id) => ({ id })),
      fanTargets: [],
    }) as any;

  const dragged = freeUnit("w1");
  const other = freeUnit("w2");
  const stack = stackUnit("C", ["m1", "m2", "m3"]);
  const other2 = stackUnit("D", ["n1"]);
  const windowDrag = { unit: dragged, tabId: null, rect };

  // --- window drags ---
  check("a plain window drag is a move",
    dropAction(windowDrag, other, false, null).op === "move");
  check("a Shift drag over empty canvas is a move",
    dropAction(windowDrag, null, true, null).op === "move");
  check("a Shift drag onto itself is a move",
    dropAction(windowDrag, dragged, true, null).op === "move");
  {
    const a = dropAction(windowDrag, other, true, null);
    check("Shift onto a free window creates a stack", a.op === "create");
    // create_stack(m1, m2) lands the stack at m1's rect: the target stays put.
    check("the target is member 1, the dragged window member 2",
      a.op === "create" && a.first === "w2" && a.second === "w1");
  }
  {
    const a = dropAction(windowDrag, stack, true, null);
    check("Shift onto a stack joins it", a.op === "add");
    check("the dragged window joins that container",
      a.op === "add" && a.member === "w1" && a.container === "C");
  }
  check("Shift while dragging a whole stack is still a move",
    dropAction({ unit: stack, tabId: null, rect }, other, true, null).op === "move");

  // --- tab drags ---
  const tabDrag = { unit: stack, tabId: "m1", rect };
  {
    const a = dropAction(tabDrag, null, false, null);
    check("a tab dropped on empty canvas unstacks", a.op === "unstack");
    check("it lands at the drop rect",
      a.op === "unstack" && a.member === "m1" && a.rect.x === 10 && a.rect.w === 300);
  }
  {
    const a = dropAction(tabDrag, stack, false, 2);
    check("a tab dropped on its own strip reorders", a.op === "reorder");
    check("the order is the full member list, moved",
      a.op === "reorder" && a.container === "C" && a.order.join(",") === "m2,m3,m1");
  }
  check("a tab dropped on its own rect body does nothing",
    dropAction(tabDrag, stack, false, null).op === "none");
  check("a tab dropped on its own index does nothing",
    dropAction(tabDrag, stack, false, 0).op === "none");
  // hoverTabIndex past the end of the target's visible tabs (stack has 3
  // members, index 99 doesn't exist) — a stale or bogus measurement must not
  // fall through to some other member.
  check("an out-of-range hover index onto the own strip does nothing",
    dropAction(tabDrag, stack, false, 99).op === "none");
  {
    const a = dropAction(tabDrag, other2, false, null);
    check("a tab dropped on another stack moves between stacks", a.op === "unstackInto");
    check("into that container",
      a.op === "unstackInto" && a.member === "m1" && a.container === "D");
  }
  {
    // A tab dropped on another stack has no competing "move" meaning (see
    // dropAction's doc comment), so Shift must be ignored here, unlike the
    // free-window case below where it selects unstackCreate over unstack.
    const a = dropAction(tabDrag, other2, true, null);
    check("Shift onto another stack is still unstackInto, not unstackCreate", a.op === "unstackInto");
  }
  {
    const a = dropAction(tabDrag, other, true, null);
    check("Shift + a tab onto a free window creates a stack there", a.op === "unstackCreate");
    check("with the free window as member 1",
      a.op === "unstackCreate" && a.member === "m1" && a.target === "w2");
  }
  check("without Shift, a tab onto a free window just lands there",
    dropAction(tabDrag, other, false, null).op === "unstack");

  // The reorder order must come from stack.members, NOT the visible tabs: a
  // filter can hide a member, and reorder_stack rewrites the whole dict from
  // the list it is given — dropping a hidden member would lose its index.
  {
    const filtered = stackUnit("C", ["m1", "m2", "m3"]);
    filtered.tabs = [{ id: "m1" }, { id: "m3" }]; // m2 hidden by the filter
    const a = dropAction({ unit: filtered, tabId: "m1", rect }, filtered, false, 1);
    check("a reorder under a filter keeps the hidden member",
      a.op === "reorder" && a.order.join(",") === "m2,m3,m1");
  }
}

console.log("layout: all checks passed");
