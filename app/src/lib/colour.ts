// EVE stores colours as 0..1 floats; <input type="color"> speaks #rrggbb.
// The three-float helpers live here beside the one rule both colour editors
// share: a hex inverts to n/255 — #bf0000 gives 0.74901…, not the 0.75 EVE
// stores for red — so picking a palette colour off a swatch would write
// something the client never wrote. When the hex IS a palette colour's hex,
// write that entry's exact floats instead. Nothing visible changes: both
// render as the same #rrggbb.
import { hexToRgba, rgbaToHex } from "./states";

export type Rgb = [number, number, number];

/** The swatch shown for "no colour" — a mid grey, dimmed by the caller. The
 *  name whitelists this line in the hex guard (`ui/tokens.test.ts`). */
export const UNSET_HEX = "#808080";

export function rgbToHex(rgb: Rgb): string {
  return rgbaToHex([rgb[0], rgb[1], rgb[2], 1]);
}

export function hexToRgb(hex: string): Rgb {
  const [r, g, b] = hexToRgba(hex, 1);
  return [r, g, b];
}

/** The palette entry whose hex is `hex`, as its exact floats, or undefined. */
export function snapToPalette<C extends number[]>(hex: string, palette: [string, C][]): C | undefined {
  return palette.find(([, c]) => rgbaToHex([c[0], c[1], c[2], 1]) === hex)?.[1];
}
