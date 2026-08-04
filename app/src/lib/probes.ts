// Pure geometry and unit helpers for the probe formation editor — no runes, so
// this is node --test-able like keybinds.ts.
//
// METRES ARE THE SOURCE OF TRUTH. Everything here converts for display only.
// One metre is 6.7e-12 AU, so a value that round-trips through a rounded AU
// string comes back displaced; the view converts a field only when the user
// actually types into it (spec §4.2).

/** EVE's own astronomical unit: 0.5 AU is exactly 74798935350 m in every
 * corpus formation, which fixes this value to the metre. */
export const M_PER_AU = 149597870700;
export const M_PER_KM = 1000;

/** 0.5 AU in metres — the range every corpus formation carries, and what a new
 * formation starts at. Mirrors `probes.rs`'s `DEFAULT_RANGE`. */
export const DEFAULT_RANGE_M = 74798935350;

/** EVE's probe scan ranges, in AU. The in-game control is a slider with these
 * fixed stops, not a free value — so the editor offers exactly these and cannot
 * write a range the client has no way to represent. */
export const RANGE_STEPS_AU = [0.25, 0.5, 1, 2, 4, 8, 16, 32, 64] as const;

/** The scan-range stops in metres, which is what the file stores. */
export const RANGE_STEPS_M = RANGE_STEPS_AU.map((au) => au * M_PER_AU);

/** A formation holds 1 to 8 probes. Mirrors `probes.rs`'s `MAX_PROBES`. */
export const MAX_PROBES = 8;

export type Unit = "au" | "km";

const scale = (u: Unit) => (u === "au" ? M_PER_AU : M_PER_KM);

export const toUnit = (metres: number, u: Unit): number => metres / scale(u);
export const fromUnit = (value: number, u: Unit): number => value * scale(u);

export interface Spherical {
  /** Metres from the formation centre. */
  r: number;
  /** Horizontal bearing in degrees, from +X towards +Z. */
  az: number;
  /** Degrees above the horizontal plane. */
  el: number;
}

const DEG = 180 / Math.PI;

/** EVE's axes: X and Z are the horizontal plane, Y is up. */
export function toSpherical([x, y, z]: [number, number, number]): Spherical {
  const r = Math.hypot(x, y, z);
  // At the centre the angles are undefined. Returning 0 rather than NaN keeps a
  // zeroed row's fields editable — NaN would propagate into every derived value
  // and into the SVG, where it silently drops the element.
  return { r, az: Math.atan2(z, x) * DEG, el: r === 0 ? 0 : Math.asin(y / r) * DEG };
}

export function toCartesian({ r, az, el }: Spherical): [number, number, number] {
  const a = az / DEG;
  const e = el / DEG;
  const horizontal = r * Math.cos(e);
  return [horizontal * Math.cos(a), r * Math.sin(e), horizontal * Math.sin(a)];
}

/** The starting arrangement for a new-from-scratch formation: eight probes on
 * the corners of a cube of half-side `range / 2`.
 *
 * ponytail: arbitrary starting cube. EVE ships default formations at several
 * range increments, but none of them are stored in the settings file (spec
 * §2.5), so there is nothing here to derive them from. Replace this if a source
 * for the client's own defaults is ever found. */
export function cubeFormation(range: number): [number, number, number][] {
  const h = range / 2;
  const out: [number, number, number][] = [];
  for (const x of [-h, h]) for (const y of [-h, h]) for (const z of [-h, h]) out.push([x, y, z]);
  return out;
}

/** Display text for a metre value. Trims trailing zeros but keeps enough
 * precision that a probe 10 000 km out never reads as "0.00" in AU. */
export function formatUnit(metres: number, u: Unit): string {
  const v = toUnit(metres, u);
  // AU needs six places or a probe 10 000 km out reads as "0.000000"; km needs
  // none, because a formation is millions of km across and the metres behind
  // the display are what actually get saved.
  return String(Number(v.toFixed(u === "au" ? 6 : 0)));
}

// --- camera, projection and drag -------------------------------------------
//
// A perspective camera written as pure functions so it stays node --test-able,
// which is also why there is no 3D library here: the scene is SVG elements, so
// picking is the browser's job and nothing needs a raycaster (spec §4.1).

export type Vec3 = [number, number, number];

/** An orbit camera. `yaw` and `pitch` are degrees; the eye sits `dist` metres
 * from `target` and always looks at it. */
export interface Camera {
  yaw: number;
  pitch: number;
  dist: number;
  target: Vec3;
}

/** The camera's orthonormal axes and its position, all in world metres. */
export interface Basis {
  right: Vec3;
  up: Vec3;
  fwd: Vec3;
  eye: Vec3;
}

/** Vertical field of view, degrees. */
export const FOV_DEG = 50;

/** Pitch never reaches ±90: the up vector degenerates when the view direction
 * is parallel to Y, and the whole basis comes back NaN. */
export const PITCH_LIMIT = 89.9;

/** X to the right, Y up — the old side (X/Y) pane. */
export const SIDE_VIEW = { yaw: 90, pitch: 0 };
/** Looking down from above, with +Z up the screen the way the flat top-down
 * pane drew it.
 *
 * Yaw 90 would put +X to the right instead, and that was the original choice —
 * but a right-handed camera looking down cannot have both, and reading +Z
 * downward makes the view register as the BOTTOM one however the button is
 * labelled. Between mirroring X and mirroring the axis people navigate by,
 * mirroring X is the cheaper lie: the eye stays genuinely above, so the cube
 * shading and the near/far ordering both stay honest, and the axis indicator
 * shows which way X runs. */
export const TOP_VIEW = { yaw: -90, pitch: PITCH_LIMIT };

const RAD = Math.PI / 180;

const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vec3, b: Vec3): Vec3 =>
  [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const mul = (a: Vec3, k: number): Vec3 => [a[0] * k, a[1] * k, a[2] * k];
const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const norm = (a: Vec3): Vec3 => {
  const m = Math.hypot(a[0], a[1], a[2]);
  // Only reachable if the pitch clamp is bypassed. Returning a valid axis beats
  // seeding NaN through every coordinate downstream.
  return m === 0 ? [1, 0, 0] : [a[0] / m, a[1] / m, a[2] / m];
};

/** The camera's axes and eye position. Clamps pitch itself, so no caller can
 * produce the degenerate basis. */
export function cameraBasis(c: Camera): Basis {
  const p = Math.max(-PITCH_LIMIT, Math.min(PITCH_LIMIT, c.pitch)) * RAD;
  const y = c.yaw * RAD;
  const cp = Math.cos(p);
  // Unit vector from the target toward the eye.
  const out: Vec3 = [cp * Math.cos(y), Math.sin(p), cp * Math.sin(y)];
  const fwd = mul(out, -1);
  const right = norm(cross(fwd, [0, 1, 0]));
  return { right, up: cross(right, fwd), fwd, eye: add(c.target, mul(out, c.dist)) };
}

/** Focal length in pixels for a viewport `size` px tall. */
export const focal = (size: number) => size / 2 / Math.tan((FOV_DEG * RAD) / 2);

/** World metres per screen pixel at a given camera-space depth. What makes a
 * screen-sized gizmo handle possible: its world length is this times its
 * pixel length. */
export const worldPerPixel = (depth: number, size: number) => depth / focal(size);

/** A world point in viewport pixels, or `null` when it is at or behind the eye
 * plane — reachable by panning, and a point that projected anyway would draw
 * mirrored through the centre.
 *
 * `depth` is the camera-space forward distance, for painter's-order sorting;
 * `dist` is the true distance to the eye, which is what `silhouette` needs. */
export function projectPoint(
  p: Vec3,
  b: Basis,
  size: number,
): { x: number; y: number; depth: number; dist: number } | null {
  const d = sub(p, b.eye);
  const z = dot(d, b.fwd);
  if (z <= 1e-9) return null;
  const f = focal(size);
  return {
    x: size / 2 + (f * dot(d, b.right)) / z,
    y: size / 2 - (f * dot(d, b.up)) / z, // SVG's y grows downward
    depth: z,
    dist: Math.hypot(d[0], d[1], d[2]),
  };
}

/** The projected radius of a sphere's silhouette, or `null` when the eye is
 * inside it. A sphere's silhouette is a circle from every viewpoint, so this
 * is the shape and not an approximation of it (spec §4.4).
 *
 * With eight 0.5 AU spheres, an eye inside one is the normal state at any
 * useful zoom — hence the null rather than a NaN radius. */
export function silhouette(dist: number, radius: number, size: number): number | null {
  if (!(dist > radius)) return null;
  return (focal(size) * radius) / Math.sqrt(dist * dist - radius * radius);
}

/** The camera distance that frames every probe, and — when `ranges` is given —
 * its range sphere too. A sphere of radius `reach` fits the vertical field of
 * view exactly at `reach / sin(fov/2)`.
 *
 * The viewer omits `ranges`: framing both is impossible when they differ by
 * orders of magnitude. "on grid" is ±10 000 km with a 0.5 AU range, so framing
 * the spheres projects the whole formation into 0.03 px. An invisible formation
 * is worse than absent context — the probes are the subject, and the user
 * wheels out to see coverage. */
export function fitDistance(probes: Vec3[], ranges?: number[]): number {
  const reach = Math.max(
    0,
    ...probes.map((p, i) => Math.hypot(p[0], p[1], p[2]) + Math.abs(ranges?.[i] ?? 0)),
  );
  // Every probe at the centre with no range has nothing to frame; any positive
  // distance draws it as a dot.
  if (!(reach > 0)) return 1;
  return reach / Math.sin((FOV_DEG * RAD) / 2);
}

/** Unit world direction from the eye through a viewport pixel. */
export function pointerRay(sx: number, sy: number, b: Basis, size: number): Vec3 {
  const f = focal(size);
  return norm(
    add(
      mul(b.fwd, f),
      add(mul(b.right, sx - size / 2), mul(b.up, size / 2 - sy)),
    ),
  );
}

/** An axis at `p0` seen in screen space: a unit screen direction and how many
 * pixels one metre along it covers.
 *
 * `null` when the axis points nearly at or away from the camera. The scale
 * diverges there, and the arrow is edge-on and all but invisible, so there is
 * nothing the user could have meant to grab. */
export function axisScreen(
  p0: Vec3,
  axis: Vec3,
  b: Basis,
  size: number,
): { dx: number; dy: number; pxPerM: number } | null {
  const a = projectPoint(p0, b, size);
  if (!a) return null;
  // A step worth roughly one pixel. A fixed metre step would be ~1e-10 px at
  // formation scale and lose the direction to rounding.
  const step = worldPerPixel(a.depth, size);
  const q = projectPoint(add(p0, mul(axis, step)), b, size);
  if (!q) return null;
  const dx = q.x - a.x;
  const dy = q.y - a.y;
  const len = Math.hypot(dx, dy);
  // A one-pixel step gives len ≈ 1 across the view and ≈ 0 down it; 0.15 is
  // about 8.6° off the view direction.
  if (len < 0.15) return null;
  return { dx: dx / len, dy: dy / len, pxPerM: len / step };
}

/** Metres to move along an axis for a pointer delta in pixels. */
export const axisDrag = (
  a: { dx: number; dy: number; pxPerM: number },
  px: number,
  py: number,
) => (px * a.dx + py * a.dy) / a.pxPerM;

/** Where the ray through viewport pixel (`sx`, `sy`) meets the plane through
 * `p0` with normal `n`, or `null` when the plane is edge-on or the hit is
 * behind the eye.
 *
 * The caller must keep the locked component from `p0` rather than reading it
 * back out of the result: the intersection returns it with float noise on top,
 * which would displace the probe along an axis nobody dragged (spec §4.7). */
export function planeHit(
  sx: number,
  sy: number,
  b: Basis,
  size: number,
  p0: Vec3,
  n: Vec3,
): Vec3 | null {
  const dir = pointerRay(sx, sy, b, size);
  const den = dot(dir, n);
  if (Math.abs(den) < 1e-6) return null;
  const t = dot(sub(p0, b.eye), n) / den;
  if (t <= 0) return null;
  return add(b.eye, mul(dir, t));
}

/** A gizmo drag in progress. `p0` is the probe's position at pointerdown — the
 * source for every component the drag does not own (spec §4.7). `i` is the
 * probe being dragged; `sx`/`sy` the pointerdown position in viewport pixels
 * and `a` the axis's screen scale captured with it. */
export type HandleDrag =
  | { kind: "axis"; i: number; comp: 0 | 1 | 2; p0: Vec3; sx: number; sy: number;
      a: { dx: number; dy: number; pxPerM: number } }
  | { kind: "plane"; i: number; lock: 0 | 1 | 2; p0: Vec3; sx: number; sy: number };

/** The dragged probe's new position for a pointer at (`lx`, `ly`) in viewport
 * units, or `null` when this frame has no answer (a plane gone edge-on).
 *
 * THE precision rule: an axis drag writes exactly one component, a plane drag
 * exactly two, and the untouched ones are copied verbatim from `p0`. The
 * intersection returns the locked component with float noise on top, which
 * would displace the probe along an axis nobody dragged, on every drag. */
export function dragPosition(
  d: HandleDrag,
  lx: number,
  ly: number,
  b: Basis,
  size: number,
): Vec3 | null {
  if (d.kind === "axis") {
    const next: Vec3 = [...d.p0];
    next[d.comp] = d.p0[d.comp] + axisDrag(d.a, lx - d.sx, ly - d.sy); // ONLY this component
    return next;
  }
  const n: Vec3 = [0, 0, 0];
  n[d.lock] = 1;
  // Where the grab landed on this plane, and where the pointer is now. The
  // probe moves by the DIFFERENCE between them, exactly as the axis branch
  // moves by a pointer delta.
  //
  // Returning the intersection itself would place the probe under the cursor,
  // so it would jump on the first frame by however far the handle sits from
  // the probe it belongs to — and the plane quads are deliberately offset from
  // it, so that jump is guaranteed rather than incidental.
  const from = planeHit(d.sx, d.sy, b, size, d.p0, n);
  const to = planeHit(lx, ly, b, size, d.p0, n);
  if (!from || !to) return null;
  const next: Vec3 = [
    d.p0[0] + (to[0] - from[0]),
    d.p0[1] + (to[1] - from[1]),
    d.p0[2] + (to[2] - from[2]),
  ];
  // Still copied, not left to the delta: both intersections carry their own
  // float noise on this axis, so their difference is not exactly zero.
  next[d.lock] = d.p0[d.lock]; // from p0, NOT from the intersection
  return next;
}
