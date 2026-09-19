// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import { check, eq } from "./test/check.ts";
import { hexToRgb, rgbToHex, snapToPalette } from "./colour.ts";

check("floats to hex", rgbToHex([0.75, 0, 0]) === "#bf0000");
check("hex to floats", eq(hexToRgb("#010203"), [1 / 255, 2 / 255, 3 / 255]));

const PALETTE: [string, [number, number, number]][] = [
  ["red", [0.75, 0.0, 0.0]],
  ["blue", [0.2, 0.5, 1.0]],
];
// #bf0000 inverts to 0.74901…, which is not the 0.75 EVE stores; the palette's
// own floats are what the file must get.
check("a palette hex snaps to the exact floats", eq(snapToPalette("#bf0000", PALETTE), [0.75, 0.0, 0.0]));
check("an off-palette hex does not snap", snapToPalette("#010203", PALETTE) === undefined);
const RGBA: [string, [number, number, number, number]][] = [["red", [0.75, 0.0, 0.0, 1.0]]];
check("four-float palettes snap too", eq(snapToPalette("#bf0000", RGBA), [0.75, 0.0, 0.0, 1.0]));
