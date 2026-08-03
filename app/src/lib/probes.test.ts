// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching keybinds.test.ts.
import {
  M_PER_AU,
  DEFAULT_RANGE_M,
  toUnit,
  fromUnit,
  toSpherical,
  toCartesian,
  cubeFormation,
  formatUnit,
  paneScale,
  project,
  DRIFTER,
  drifterHole,
} from "./probes.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

const near = (a: number, b: number, eps = 1e-6) => Math.abs(a - b) < eps;

// The one number the whole feature is anchored on: every corpus probe entry
// carries 74798935350 m, which must read as exactly 0.5 AU.
check("0.5 AU is the corpus range", near(toUnit(DEFAULT_RANGE_M, "au"), 0.5));
check("the default range is the corpus value", DEFAULT_RANGE_M === 74798935350);
check("AU round-trips", near(fromUnit(toUnit(1234567890, "au"), "au"), 1234567890, 1e-3));
check("km is metres over 1000", toUnit(2500, "km") === 2.5);
check("km round-trips exactly", fromUnit(2.5, "km") === 2500);
check("one AU is EVE's own value", M_PER_AU === 149597870700);

// EVE's axes: X and Z are the horizontal plane, Y is up.
check("a probe on +X is azimuth 0, elevation 0", (() => {
  const s = toSpherical([100, 0, 0]);
  return near(s.r, 100) && near(s.az, 0) && near(s.el, 0);
})());
check("a probe on +Z is azimuth 90", near(toSpherical([0, 0, 100]).az, 90));
check("a probe straight up is elevation 90", near(toSpherical([0, 100, 0]).el, 90));
check("a probe straight down is elevation -90", near(toSpherical([0, -100, 0]).el, -90));

check("cartesian round-trips through spherical", (() => {
  const p: [number, number, number] = [-1199120384, -115136512, -415997952];
  const back = toCartesian(toSpherical(p));
  return p.every((v, i) => near(back[i], v, 1e-3));
})());

// r == 0 leaves the angles undefined. Reporting 0/0 rather than NaN is what
// keeps a zeroed row's fields editable instead of poisoning every derived value.
check("a probe at the centre reports finite angles", (() => {
  const s = toSpherical([0, 0, 0]);
  return s.r === 0 && Number.isFinite(s.az) && Number.isFinite(s.el);
})());

check("a cube formation has eight distinct corners", (() => {
  const c = cubeFormation(74798935350);
  const distinct = new Set(c.map((p) => p.join(",")));
  return c.length === 8 && distinct.size === 8;
})());
check("every cube corner is the same distance from the centre", (() => {
  const c = cubeFormation(74798935350);
  const rs = c.map((p) => toSpherical(p).r);
  return rs.every((r) => near(r, rs[0], 1e-3)) && rs[0] > 0;
})());

// Display text must not round a coordinate to something that reads as zero:
// one metre is 6.7e-12 AU, so a fixed 2-decimal AU display collapses a probe
// 10 000 km out to "0.00".
check("a small distance still shows a value in AU", formatUnit(1e7, "au") !== "0");
check("km formatting is readable", formatUnit(1e7, "km") === "10000");

// Top-down is X/Z, side is X/Y. Getting these the wrong way round draws a
// plausible picture of the wrong formation, which no type check would catch.
check("top-down drops Y", (() => {
  const [a, b] = project([1, 2, 3], "top");
  return a === 1 && b === 3;
})());
check("side drops Z", (() => {
  const [a, b] = project([1, 2, 3], "side");
  return a === 1 && b === 2;
})());

check("the scale fits the widest probe plus its range", (() => {
  // A probe 100 units out with a range of 10 needs 110 of half-extent to show
  // its whole sphere, so a 200px pane fits 110 into 100px.
  const s = paneScale([[100, 0, 0]], 10, 200);
  return near(s, 110 / 100, 1e-9);
})());
check("an all-centre formation still yields a finite scale", (() => {
  const s = paneScale([[0, 0, 0]], 0, 200);
  return Number.isFinite(s) && s > 0;
})());

check("the drifter hole sits 89 km out", near(Math.hypot(...drifterHole()), 89_000, 1e-6));
check("the drifter hole is below the warp-in", drifterHole()[1] < 0);
check("the jump sphere is 16 km across", DRIFTER.jumpRange === 16_000);
