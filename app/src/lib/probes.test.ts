// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import {
  M_PER_AU,
  DEFAULT_RANGE_M,
  toUnit,
  fromUnit,
  toSpherical,
  toCartesian,
  cardinals,
  horizonRing,
  NORTH_AZ_DEG,
  EAST_SIGN,
  cubeFormation,
  formatUnit,
  cameraBasis,
  projectPoint,
  silhouette,
  fitDistance,
  focal,
  FOV_DEG,
  SIDE_VIEW,
  TOP_VIEW,
  pointerRay,
  axisScreen,
  axisDrag,
  planeHit,
  dragPosition,
  scenePos,
  type Camera,
  type HandleDrag,
  type Vec3,
} from "./probes.ts";

import { check } from "./test/check.ts";

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

// --- compass ---------------------------------------------------------------
// The in-game measurement (2026-08-04), restated as assertions: launch three
// probes on +X, two on +Y and one on −Z, and the tactical overlay puts them
// west, up and south. Everything below fails if a constant or the trig flips.
const axis = (label: string) => cardinals().find((c) => c.label === label)!.v;
check("north is +Z", (() => {
  const v = axis("N");
  return near(v[0], 0) && near(v[1], 0) && near(v[2], 1);
})());
check("west is +X — the three-probe cluster the overlay put west", (() => {
  const v = axis("W");
  return near(v[0], 1) && near(v[1], 0) && near(v[2], 0);
})());
check("south is -Z — the lone probe the overlay put south", (() => {
  const v = axis("S");
  return near(v[0], 0) && near(v[1], 0) && near(v[2], -1);
})());
check("east is -X", (() => {
  const v = axis("E");
  return near(v[0], -1) && near(v[1], 0) && near(v[2], 0);
})());
check("north sits at NORTH_AZ_DEG in the table's own azimuth",
  near(toSpherical(axis("N")).az, NORTH_AZ_DEG));
check("east is 90 degrees from north, on EAST_SIGN's side", (() => {
  const az = toSpherical(axis("E")).az; // atan2 range is (-180, 180]
  return near(((az - NORTH_AZ_DEG) * EAST_SIGN + 360) % 360, 90);
})());
check("the four cardinals are unit, horizontal and 90 degrees apart", (() => {
  const c = cardinals();
  if (c.length !== 4) return false;
  return c.every(({ v }, i) => {
    const nxt = c[(i + 1) % 4].v;
    // Perpendicular neighbours + unit length is the whole invariant.
    return near(Math.hypot(v[0], v[1], v[2]), 1) && near(v[1], 0)
      && near(v[0] * nxt[0] + v[2] * nxt[2], 0);
  });
})());
check("the horizon ring is horizontal and at the asked radius", (() => {
  const pts = horizonRing(2500, 16);
  return pts.length === 16
    && pts.every((p) => near(p[1], 0) && near(Math.hypot(p[0], p[2]), 2500, 1e-9));
})());

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

check("the top view looks DOWN, with +Z up the screen", (() => {
  // Both halves matter, and they pull against each other. A right-handed
  // camera looking down cannot put +X right AND +Z up, and reading +Z downward
  // makes the view register as the bottom one — reported from live use. So X
  // is the one that mirrors, and the eye stays genuinely above, which is what
  // keeps the cube shading and the near/far ordering honest.
  const b = cameraBasis({ ...TOP_VIEW, dist: 1000, target: [0, 0, 0] });
  const up = projectPoint([0, 0, 100], b, SIZE); // +Z
  const near = projectPoint([0, 100, 0], b, SIZE); // +Y, toward the eye
  const far = projectPoint([0, -100, 0], b, SIZE);
  return up !== null && near !== null && far !== null &&
    up.y < SIZE / 2 && // +Z above centre, as the flat top-down pane drew it
    near.depth < far.depth; // and the camera really is above, not below
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

check("fit frames the furthest probe plus its range when given ranges", (() => {
  // A sphere of radius `reach` fits the vertical field of view exactly at
  // dist = reach / sin(fov/2).
  const d = fitDistance([[300, 0, 0]], [700]);
  return near(d, 1000 / Math.sin((FOV_DEG * Math.PI) / 360), 1e-6);
})());

check("fit without ranges frames the probes, so a tight formation stays visible", (() => {
  // "on grid" is ±10 000 km with a 0.5 AU range: framing the spheres puts the
  // camera 177e9 m out and the whole formation projects into 0.03 px. The
  // viewer omits the ranges for exactly this reason — the probes are the
  // subject, the spheres are context the user wheels out to see.
  const probes: Vec3[] = [[1e7, 0, 0], [-1e7, 0, 0]];
  const d = fitDistance(probes);
  return near(d, 1e7 / Math.sin((FOV_DEG * Math.PI) / 360), 1e-6) &&
    d < fitDistance(probes, [DEFAULT_RANGE_M, DEFAULT_RANGE_M]) / 1000;
})());

check("fit survives a formation with nothing to frame", fitDistance([[0, 0, 0]], [0]) > 0);
check("fit survives a formation at the centre with no ranges", fitDistance([[0, 0, 0]]) > 0);

// --- drag ------------------------------------------------------------------

check("an axis across the screen drags a pixel delta into metres", (() => {
  // Side view: +X is screen-right, so a rightward pointer delta is +X metres,
  // and the conversion is the pointer travel over the axis's pixels-per-metre.
  const b = cameraBasis(sideCam(1000));
  const a = axisScreen([0, 0, 0], [1, 0, 0], b, SIZE);
  if (!a) return false;
  if (!near(a.dx, 1, 1e-6) || !near(a.dy, 0, 1e-6)) return false;
  return near(axisDrag(a, 40, 0), 40 / a.pxPerM, 1e-9);
})());

check("a drag across an axis moves it nowhere", (() => {
  const b = cameraBasis(sideCam(1000));
  const a = axisScreen([0, 0, 0], [1, 0, 0], b, SIZE);
  return a !== null && near(axisDrag(a, 0, 40), 0, 1e-9);
})());

check("an axis pointing at the camera cannot be dragged", (() => {
  // Side view looks along -Z, so the Z axis is edge-on: its screen length is
  // near zero and the metres-per-pixel diverges. The arrow is invisible in
  // exactly this case, so there is nothing the user could have meant to grab.
  const b = cameraBasis(sideCam(1000));
  return axisScreen([0, 0, 0], [0, 0, 1], b, SIZE) === null;
})());

check("a plane drag hits the plane it was given", (() => {
  // The XY plane through the origin, seen face-on from the side camera.
  const b = cameraBasis(sideCam(1000));
  const hit = planeHit(SIZE / 2, SIZE / 2, b, SIZE, [0, 0, 0], [0, 0, 1]);
  return hit !== null && near(hit[0], 0, 1e-6) && near(hit[1], 0, 1e-6) && near(hit[2], 0, 1e-6);
})());

check("planeHit's locked axis carries float noise, so the caller must keep p0's", (() => {
  // THE precision guarantee (spec §4.7). The intersection maths returns the
  // locked component with float noise on it; taking that value would displace
  // the probe along an axis the user never dragged, on every drag. This pins
  // that the noise is real (hit[2] != p0[2] bit-for-bit, though still on the
  // right plane), which is why the caller must keep p0[2] instead of hit[2].
  const b = cameraBasis(sideCam(1e11));
  const p0: Vec3 = [-1199120384.7, -115136512.3, -415997952.9];
  const hit = planeHit(SIZE / 2 + 30, SIZE / 2 - 10, b, SIZE, p0, [0, 0, 1]);
  if (hit === null) return false;
  return (
    !Object.is(hit[2], p0[2]) &&
    near(hit[2], p0[2], 1e-3) &&
    hit[0] !== p0[0] &&
    hit[1] !== p0[1]
  );
})());

check("pointerRay's vertical sign points a pixel above centre toward world +Y", (() => {
  // Pin the sign nothing else in the suite exercises: projectPoint defines
  // y = size/2 - f*up-component/depth (SVG's y grows downward), so a smaller
  // screen y needs a larger up-component, i.e. more +Y. On the z=0 plane the
  // side camera faces dead-on, a pixel above centre must therefore hit a
  // greater world Y than the mirrored pixel below centre. An inverted sign
  // would move plane drags the wrong way vertically with nothing to catch it.
  const b = cameraBasis(sideCam(1000));
  const above = planeHit(SIZE / 2, SIZE / 2 - 50, b, SIZE, [0, 0, 0], [0, 0, 1]);
  const below = planeHit(SIZE / 2, SIZE / 2 + 50, b, SIZE, [0, 0, 0], [0, 0, 1]);
  return above !== null && below !== null && above[1] > below[1];
})());

check("a plane seen edge-on is not hit", (() => {
  // Normal perpendicular to the view direction: the ray never meets it.
  const b = cameraBasis(sideCam(1000));
  return planeHit(SIZE / 2, SIZE / 2, b, SIZE, [0, 0, 0], [1, 0, 0]) === null;
})());

// --- dragPosition: the precision rule (spec §4.7) ---------------------------
// The branch's most load-bearing rule, and the reason the maths lives here and
// not in the component: an axis drag writes exactly ONE component, a plane drag
// exactly two, and every untouched one is copied verbatim from the position
// captured at pointerdown. `Object.is` throughout, not a tolerance — a
// "simplification" that reads the locked value back out of the intersection
// passes any tolerance and still displaces the probe on every drag.

const DRAG_P0: Vec3 = [-1199120384.7, -115136512.3, -415997952.9];

check("an axis drag moves one component and leaves the other two bit-identical", (() => {
  const b = cameraBasis(sideCam(1e10));
  const a = axisScreen(DRAG_P0, [1, 0, 0], b, SIZE);
  if (!a) return false;
  const d: HandleDrag = { kind: "axis", i: 0, comp: 0, p0: DRAG_P0, sx: 200, sy: 200, a };
  const next = dragPosition(d, 260, 170, b, SIZE);
  return next !== null &&
    next[0] !== DRAG_P0[0] &&
    Object.is(next[1], DRAG_P0[1]) &&
    Object.is(next[2], DRAG_P0[2]);
})());

check("a plane drag moves two components and returns the locked one bit-identical", (() => {
  // The XY plane through p0, face-on from the side camera; Z is locked.
  const b = cameraBasis(sideCam(1e10));
  const d: HandleDrag = { kind: "plane", i: 0, lock: 2, p0: DRAG_P0, sx: 200, sy: 200 };
  const next = dragPosition(d, 230, 190, b, SIZE);
  return next !== null &&
    next[0] !== DRAG_P0[0] &&
    next[1] !== DRAG_P0[1] &&
    Object.is(next[2], DRAG_P0[2]);
})());

check("a plane drag with no pointer travel does not move the probe at all", (() => {
  // The regression that shipped: the plane branch returned the ray-plane
  // intersection itself, so the probe teleported UNDER THE CURSOR on the first
  // frame — and the plane quads are drawn offset from the probe, so the jump
  // was guaranteed, in the saved coordinates and not merely on screen. A drag
  // that has not travelled must be a no-op, on all three axes.
  const b = cameraBasis(sideCam(1e10));
  const grabbed: [number, number] = [200 + 18, 200 - 18]; // a quad, offset like the real ones
  const d: HandleDrag = { kind: "plane", i: 0, lock: 2, p0: DRAG_P0, sx: grabbed[0], sy: grabbed[1] };
  const next = dragPosition(d, grabbed[0], grabbed[1], b, SIZE);
  return next !== null &&
    Object.is(next[0], DRAG_P0[0]) &&
    Object.is(next[1], DRAG_P0[1]) &&
    Object.is(next[2], DRAG_P0[2]);
})());

check("a plane drag gone edge-on returns null rather than a position", (() => {
  // The only null path: an axis drag's scale is captured at pointerdown and
  // cannot diverge mid-drag, but the camera can orbit a plane to edge-on.
  const b = cameraBasis(sideCam(1000));
  const d: HandleDrag = { kind: "plane", i: 0, lock: 0, p0: [0, 0, 0], sx: SIZE / 2, sy: SIZE / 2 };
  return dragPosition(d, SIZE / 2, SIZE / 2, b, SIZE) === null;
})());

// --- scene positions --------------------------------------------------------
// A scene is authored in COMPASS bearings, which is not the azimuth convention
// `toSpherical` uses. These pin the mapping between them. Written against
// NORTH_AZ_DEG rather than hard-coded directions, so moving the knob moves the
// test with it instead of breaking it.
{
  const at = (bearing: number, elevation = 0) =>
    scenePos({ kind: "polar", km: 100, bearing, elevation });
  const [n, e, s, w] = cardinals();

  const alongs = (v: Vec3, dir: Vec3) =>
    near(v[0], dir[0] * 100_000, 1) && near(v[1], dir[1] * 100_000, 1) &&
    near(v[2], dir[2] * 100_000, 1);

  check("bearing 0 is north", alongs(at(0), n.v));
  check("bearing 90 is east", alongs(at(90), e.v));
  check("bearing 180 is south", alongs(at(180), s.v));
  check("bearing 270 is west", alongs(at(270), w.v));

  // The drifter scenes' own object: 87.5 km west, 17.45 degrees UP. The sign of
  // y is the measured fact the whole scene rests on (spec §2.2).
  const hole = scenePos({ kind: "polar", km: 87.5, bearing: 270, elevation: 17.45 });
  check("elevation raises the object", hole[1] > 0);
  check("a raised object keeps its slant range", near(Math.hypot(...hole), 87_500, 1e-3));
  check(
    "elevation is measured from the horizontal plane",
    near(hole[1], 87_500 * Math.sin((17.45 * Math.PI) / 180), 1e-3),
  );

  // Kilometres in, metres out. Getting this backwards is a factor of a million
  // and the object lands somewhere nobody will scroll to.
  check("km are scaled to metres", near(Math.hypot(...at(0)), 100_000, 1e-6));

  // The cartesian form is a pass-through: it arrives from Rust already in
  // metres, and touching it here would be a second place units could go wrong.
  const xyz = scenePos({ kind: "xyz", m: [12500, 3000, -8000] });
  check("xyz passes through untouched", xyz[0] === 12500 && xyz[1] === 3000 && xyz[2] === -8000);
}
