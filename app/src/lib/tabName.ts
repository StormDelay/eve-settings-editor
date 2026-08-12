// Overview tab names are markup-bearing strings. EVE renders a small tag set in
// them, and overview packs are the main reason they matter: `overview_pack.rs`
// writes a tab's `name` and nothing else, so a pack colours its tabs by
// embedding markup in the name rather than by setting the tab's `color` key.
//
// Real names from the corpus (testdata/dumps, 134 account files):
//
//   "<color=0xFFFFFFFF>  *  </color>"
//   "<color=0xFFFF6F75>   <b>main</b>   </color>"
//   "<b> Exit! </b>"
//   "  main  "
//
// `<color=0xAARRGGBB>` and `<b>` are the only tags that occur on a tab name
// anywhere in that corpus. `<fontsize=N>` occurs too, but only on bracket
// labels' pre/post strings, so it is deliberately not handled here.
//
// Pure — no Svelte, no Tauri — so it unit-tests alongside groups.ts.

export interface TabName {
  /** `AARRGGBB`, uppercase, or null for an uncoloured name. */
  color: string | null;
  bold: boolean;
  /** The name itself, tags removed and spacing kept verbatim. */
  text: string;
}

/** EVE's in-game colour picker: a 3x8 hue wheel at 15-degree steps, as `RRGGBB`. */
export const EVE_PALETTE: string[] = [
  "ff4040", "ff6f40", "ff9f40", "ffcf40", "ffff40", "cfff40", "9fff40", "6fff40",
  "40ff40", "40ff6f", "40ff9f", "40ffcf", "40ffff", "40cfff", "409fff", "406fff",
  "4040ff", "6f40ff", "9f40ff", "cf40ff", "ff40ff", "ff40cf", "ff409f", "ff406f",
];

const COLOR_SPAN = /^<color=0x([0-9a-fA-F]{8})>([\s\S]*)<\/color>$/;

/**
 * Split a stored name into the parts the editor exposes.
 *
 * Names padded with spaces (`"  main  "`, `"  3  "`) are how a tab is widened
 * in game, so `text` keeps its spacing exactly — dropping it would silently
 * resize the user's overview.
 *
 * Anything that does not fit `[colour][bold]text` — nested spans, two colours,
 * a tag this doesn't know — comes back as plain text carrying the raw string.
 * The Rename box still edits it, the swatch just shows no colour, and nothing
 * rewrites it until the user explicitly picks a colour or toggles bold.
 */
export function parseTabName(raw: string): TabName {
  const plain: TabName = { color: null, bold: false, text: raw };
  const span = COLOR_SPAN.exec(raw);
  const color = span ? span[1].toUpperCase() : null;
  let inner = span ? span[2] : raw;

  // `<b>` sits INSIDE the colour span but need not wrap all of it: the corpus
  // has `<color=…>   <b>main</b>   </color>`, padding outside the bold. So the
  // tags are stripped wherever they are rather than matched as a wrapper.
  const bold = inner.includes("<b>") && inner.includes("</b>");
  if (bold) inner = inner.split("<b>").join("").split("</b>").join("");

  if (inner.includes("<") || inner.includes(">")) return plain;
  return { color, bold, text: inner };
}

/**
 * The inverse, up to tag nesting: `<color=…>   <b>main</b>   </color>` re-emits
 * as `<color=…><b>   main   </b></color>`. Same rendering, different bytes —
 * acceptable because a name is only rewritten when the user changes something,
 * and re-applying this is stable (see tabName.test.ts).
 */
export function formatTabName(n: TabName): string {
  let out = n.bold ? `<b>${n.text}</b>` : n.text;
  if (n.color) out = `<color=0x${n.color}>${out}</color>`;
  return out;
}

/** The readable text alone — for `<option>`s, which can't carry the markup. */
export function plainTabName(raw: string): string {
  return parseTabName(raw).text;
}

/** `AARRGGBB` as a CSS colour, alpha last the way CSS wants it. */
export function cssColor(color: string): string {
  return `#${color.slice(2)}${color.slice(0, 2)}`;
}
