// Overview tab names, drawn the way EVE draws them. The markup is parsed and
// written in Rust (`crates/settings-model/src/tab_name.rs`), which hands every
// tab over as styled `pieces`; this module only turns those into CSS.
//
// Pure — no Svelte, no Tauri — so it unit-tests alongside groups.ts.

import type { OverviewTab, TabPiece } from "./api";

/** EVE's in-game colour picker: a 3x8 hue wheel at 15-degree steps, as `RRGGBB`. */
export const EVE_PALETTE: string[] = [
  "ff4040", "ff6f40", "ff9f40", "ffcf40", "ffff40", "cfff40", "9fff40", "6fff40",
  "40ff40", "40ff6f", "40ff9f", "40ffcf", "40ffff", "40cfff", "409fff", "406fff",
  "4040ff", "6f40ff", "9f40ff", "cf40ff", "ff40ff", "ff40cf", "ff409f", "ff406f",
];

/** What the name editor commits: styled pieces for Rust to write as markup,
 *  or raw markup written verbatim. */
export type TabNameEdit = { pieces: TabPiece[] } | { raw: string };

/** A tab label's font size: `EVE_MEDIUM_FONTSIZE` at the Medium client setting. */
export const TAB_FONT_PX = 14;

/** `AARRGGBB` as a CSS colour, alpha last the way CSS wants it. */
export function cssColor(color: string): string {
  return `#${color.slice(2)}${color.slice(0, 2)}`;
}

/** One piece's inline style. `base` is the tab's own colour key, which the
 *  client wraps around the whole name, so a piece's own colour wins over it.
 *  Sizes scale against the row's font, so a 16px piece is 16/14 of the row. */
export function pieceStyle(p: TabPiece, base: string | null = null): string {
  const color = p.color ?? base;
  return [
    color ? `color:${cssColor(color)}` : "",
    p.bold ? "font-weight:700" : "",
    p.italic ? "font-style:italic" : "",
    p.underline ? "text-decoration:underline" : "",
    p.size ? `font-size:${(p.size / TAB_FONT_PX).toFixed(3)}em` : "",
    p.spacing ? `letter-spacing:${p.spacing}px` : "",
    p.uppercase ? "text-transform:uppercase" : "",
  ].filter(Boolean).join(";");
}

/** The readable text alone — for headings, toasts and `<option>`s. */
export function plainTabName(tab: Pick<OverviewTab, "pieces">): string {
  return tab.pieces.filter((p) => !p.hidden).map((p) => p.text).join("");
}

/** Two piece lists the same once empty pieces are dropped and neighbours of one
 *  style merged — the normalisation `tab_name::format` applies on write. */
export function samePieces(a: TabPiece[], b: TabPiece[]): boolean {
  const norm = (ps: TabPiece[]) => {
    const out: TabPiece[] = [];
    for (const p of ps) {
      if (!p.text) continue;
      const style = JSON.stringify({ ...strip(p), text: "" });
      const last = out[out.length - 1];
      if (last && JSON.stringify({ ...strip(last), text: "" }) === style) last.text += p.text;
      else out.push({ ...strip(p) });
    }
    return JSON.stringify(out);
  };
  return norm(a) === norm(b);
}

/** A piece without its unset fields, so `{bold: false}` equals `{}`. */
function strip(p: TabPiece): TabPiece {
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(p).sort()) {
    const v = (p as unknown as Record<string, unknown>)[k];
    if (v !== undefined && v !== null && v !== false && v !== "") out[k] = v;
  }
  out.text = p.text;
  return out as unknown as TabPiece;
}
