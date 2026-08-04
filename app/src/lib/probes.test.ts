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
  cameraBasis,
  projectPoint,
  silhouette,
  fitDistance,
  focal,
  FOV_DEG,
  SIDE_VIEW,
  TOP_VIEW,
  type Camera,
  type Vec3,
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

// --- camera and projection -------------------------------------------------
// EVE's axes: X and Z are the horizontal plane, Y is up. The `side` camera
// (yaw 90, pitch 0) is the one that puts +X to the right and +Y up, matching
// the side pane this replaces.

const SIZE = 400;
const sideCam = (dist = 1000): Camera => ({ ...SIDE_VIEW, dist, target: [0, 0, 0] });

check("the camera basis is orthonormal", (() => {
  for (const c of [
    { yaw: 0, pitch: 0, dist: 10, target: [0, 0, 0] as Vec3 },
    { yaw: 37, pitch: -22, dist: 1e10, target: [1, 2, 3] as Vec3 },
    { yaw: 200, pitch: 80, dist: 5, target: [0, 0, 0] as Vec3 },
  ]) {
    const b = cameraBasis(c);
    const dot = (u: Vec3, v: Vec3) => u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    const unit = (u: Vec3) => near(dot(u, u), 1, 1e-9);
    if (!unit(b.right) || !unit(b.up) || !unit(b.fwd)) return false;
    if (!near(dot(b.right, b.up), 0, 1e-9)) return false;
    if (!near(dot(b.right, b.fwd), 0, 1e-9)) return false;
    if (!near(dot(b.up, b.fwd), 0, 1e-9)) return false;
  }
  return true;
})());

check("a pitch of 90 does not produce NaN", (() => {
  // The up vector degenerates when the view direction is parallel to Y, so the
  // basis clamps rather than trusting its caller to.
  const b = cameraBasis({ yaw: 0, pitch: 90, dist: 10, target: [0, 0, 0] });
  return [...b.right, ...b.up, ...b.fwd, ...b.eye].every(Number.isFinite);
})());

check("the target projects to the viewport centre", (() => {
  const p = projectPoint([0, 0, 0], cameraBasis(sideCam()), SIZE);
  return p !== null && near(p.x, SIZE / 2, 1e-6) && near(p.y, SIZE / 2, 1e-6);
})());

check("in the side view +X is to the right", (() => {
  const p = projectPoint([100, 0, 0], cameraBasis(sideCam()), SIZE);
  return p !== null && p.x > SIZE / 2 && near(p.y, SIZE / 2, 1e-6);
})());

check("in the side view +Y is above centre", (() => {
  // SVG's y grows downward, so "above" is a SMALLER y.
  const p = projectPoint([0, 100, 0], cameraBasis(sideCam()), SIZE);
  return p !== null && p.y < SIZE / 2 && near(p.x, SIZE / 2, 1e-6);
})());

check("in the top view +Z is below centre", (() => {
  // The old top-down pane drew +Z upward, the map convention. A camera above
  // the formation sees the opposite, and this is a camera (spec §4.2).
  const b = cameraBasis({ ...TOP_VIEW, dist: 1000, target: [0, 0, 0] });
  const p = projectPoint([0, 0, 100], b, SIZE);
  return p !== null && p.y > SIZE / 2;
})());

check("a point behind the eye does not project", (() => {
  // Reachable by panning, not theoretical. A projection that returned a point
  // anyway would draw it mirrored through the centre.
  const b = cameraBasis(sideCam(1000));
  return projectPoint([0, 0, 5000], b, SIZE) === null;
})());

check("the silhouette radius matches the closed form", (() => {
  // A sphere of radius R at distance d subtends a circle of projected radius
  // f*R/sqrt(d^2 - R^2).
  const r = silhouette(1000, 600, SIZE);
  const want = (focal(SIZE) * 600) / Math.sqrt(1000 * 1000 - 600 * 600);
  return r !== null && near(r, want, 1e-9);
})());

check("a sphere containing the eye has no silhouette", silhouette(500, 600, SIZE) === null);

check("fit frames the furthest probe plus its range", (() => {
  // A sphere of radius `reach` fits the vertical field of view exactly at
  // dist = reach / sin(fov/2).
  const d = fitDistance([[300, 0, 0]], [700]);
  return near(d, 1000 / Math.sin((FOV_DEG * Math.PI) / 360), 1e-6);
})());

check("fit survives a formation with nothing to frame", fitDistance([[0, 0, 0]], [0]) > 0);
