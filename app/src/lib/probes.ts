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
  if (v === 0) return "0";
  const decimals = u === "au" ? 6 : 3;
  return String(Number(v.toFixed(decimals)));
}

/** Which two axes a pane shows. EVE's X and Z are the horizontal plane, Y is
 * up, so "top" is a map and "side" is an elevation. */
export type Plane = "top" | "side";

/** Drop the third axis. */
export function project(p: [number, number, number], plane: Plane): [number, number] {
  return plane === "top" ? [p[0], p[2]] : [p[0], p[1]];
}

/** Data metres per pane pixel, sized so every probe's whole range sphere fits.
 *
 * Both panes take the same scale from the same call, so a distance that looks
 * longer in one is longer. */
export function paneScale(
  probes: [number, number, number][],
  range: number,
  size: number,
): number {
  const reach = Math.max(0, ...probes.flatMap((p) => p.map(Math.abs))) + Math.abs(range);
  // A formation with every probe at the centre and no range has nothing to
  // scale to; any positive number draws it as a dot at the origin.
  if (reach === 0) return 1;
  return reach / (size / 2);
}

/** Drifter wormhole k-space geometry, relative to the warp-in beacon.
 *
 * SOURCED, NOT MEASURED, and the sources disagree:
 *
 * | source                                    | warp-in to hole |
 * |-------------------------------------------|-----------------|
 * | jambeeno.com/uni                          | exactly 89 km, 14 deg outside / 26.5 deg in, 16 km jump sphere |
 * | wiki.eveuniversity.org/Wormholes          | ~80 km |
 * | patch-note summary, March 2026            | 75 km k-space side, was 88 km |
 * | randomevestuff.wordpress.com              | ~100 km, one measured at 91 km |
 *
 * Jambeeno's is taken: the only full 3D geometry and the most recent. The
 * direction of the 14 degrees is an assumption — the source says "a slight
 * downward angle" without saying from which end.
 *
 * ponytail: hardcoded k-space drifter geometry, unverified in-client. Make it a
 * measured or user-entered scenario if a second site is ever wanted. */
export const DRIFTER = {
  /** Metres from the warp-in beacon to the hole's centre. */
  distance: 89_000,
  /** Degrees below the horizontal, from the beacon. */
  elevation: -14,
  /** Metres. A hole has a 3 km radius and a 5 km jump range, so it is enterable
   * from anywhere in a 16 km-wide sphere centred on its icon. */
  jumpRange: 16_000,
} as const;

/** The hole's position, with the warp-in beacon (and the formation centre) at
 * the origin. Azimuth 0 is arbitrary: nothing orients a formation to a site. */
export function drifterHole(): [number, number, number] {
  return toCartesian({ r: DRIFTER.distance, az: 0, el: DRIFTER.elevation });
}
