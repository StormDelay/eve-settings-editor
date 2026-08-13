// APCA 0.1.9 (W3 / 0.98G-4g). Returns |Lc| — polarity is not information here,
// because this app is dark-only and every pairing it scores is light-on-dark.
//
// Why this is 25 lines of ours rather than the `apca-w3` package: it is a pure
// function of two hex strings with no state, no I/O and a frozen constant table.
// A dependency would cost a lockfile entry and a supply-chain surface to save
// nothing.
//
// Why APCA and not WCAG 2: WCAG 2's ratio is known to give unreliable guidance
// for dark interfaces — it ignores polarity, size and weight. It is the reason
// none of this was caught. `--fg-dim` #8a919e scored a comfortable-looking
// 5.60 : 1 and is, measured properly, below any text threshold at the ~12px it
// was used at, in all 51 places. Do not add a WCAG 2 check beside this one; it
// would pass the failures this file exists to find.
const [RCO, GCO, BCO, TRC] = [0.2126729, 0.7151522, 0.072175, 2.4];
const [NORM_BG, NORM_TXT, REV_TXT, REV_BG] = [0.56, 0.57, 0.62, 0.65];
const [BLK_THRS, BLK_CLMP, SCALE, LO_OFFSET, LO_CLIP, DELTA_Y_MIN] = [0.022, 1.414, 1.14, 0.027, 0.1, 0.0005];

/** sRGB hex -> luminance Y, with APCA's soft black clamp. */
const y = (hex: string): number => {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? [...h].map((c) => c + c).join("") : h;
  const ch = (i: number): number => (parseInt(full.slice(i * 2, i * 2 + 2), 16) / 255) ** TRC;
  const lum = ch(0) * RCO + ch(1) * GCO + ch(2) * BCO;
  return lum < BLK_THRS ? lum + (BLK_THRS - lum) ** BLK_CLMP : lum;
};

/** Lightness contrast of `text` against `bg`, as an absolute Lc. */
export const lc = (text: string, bg: string): number => {
  const [yt, yb] = [y(text), y(bg)];
  if (Math.abs(yb - yt) < DELTA_Y_MIN) return 0;
  const sapc =
    yb > yt
      ? (yb ** NORM_BG - yt ** NORM_TXT) * SCALE // dark text on a light ground
      : (yb ** REV_BG - yt ** REV_TXT) * SCALE; // light text on a dark ground — this app
  const out = yb > yt ? (sapc < LO_CLIP ? 0 : sapc - LO_OFFSET) : sapc > -LO_CLIP ? 0 : sapc + LO_OFFSET;
  return Math.abs(out) * 100;
};
