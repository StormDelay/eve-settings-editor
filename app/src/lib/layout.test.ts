// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching search.test.ts.
import { canvasScale, toCanvas, toData, openWindows, resizeRect } from "./layout.ts";
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

const win = (id: string, open: boolean, renderable: boolean): WindowRect => ({
  id,
  label: id,
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
  stacks: null,
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

console.log("layout: all checks passed");
