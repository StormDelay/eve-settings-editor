// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching layout.test.ts.
import { DETAIL_NOMINAL, shipHudParts, fighterParts } from "./detail.ts";
import { HUD_NOMINAL } from "./layout.ts";

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

console.log("detail.test.ts ok");
