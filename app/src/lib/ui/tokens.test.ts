// Guard tests for the Phase 1 token system — docs/ui-redesign/01-tokens-and-primitives.md §7.1.
//
// These read source text rather than mounting anything, exactly as
// detail.test.ts:470-486 already guards DetailParts' `pointer-events: none`.
// They are the durable half of Phase 1: the tree arrived at 45 hex literals,
// 8 radii and 10 font sizes because nothing looked, and it will get back there
// over the next ten features unless something does.
//
// Written with `expect(offenders).toEqual([])` rather than `check()` from
// $lib/test/check.ts, which §7.1 nominated. A boolean tells you a guard failed;
// these have to tell you *where*, across 45 sites in 25 files. The failure
// output is the migration worklist, and that is the whole point of writing them
// before the migration rather than after.
import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test } from "vitest";

const SRC = resolve(import.meta.dirname, "../..");

/**
 * Comments blanked out, with newlines and columns preserved.
 *
 * Prose is not chrome: a comment explaining why `.mini` was retired, or naming
 * the hex a token replaced, must not itself trip the guard that enforces the
 * removal. Only whole-line `//` comments are blanked, never a trailing one — a
 * URL in real code contains `//` and truncating from there could hide an
 * offender.
 */
const stripComments = (t: string): string =>
  t
    .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "))
    .replace(/<!--[\s\S]*?-->/g, (m) => m.replace(/[^\n]/g, " "))
    .split(/\r?\n/)
    .map((l) => (/^\s*\/\//.test(l) ? "" : l))
    .join("\n");

type Line = { path: string; n: number; text: string; style: boolean };

/** Every .svelte and .css line in app/src, tagged with whether it is inside a <style> block. */
const lines: Line[] = readdirSync(SRC, { recursive: true, encoding: "utf8" })
  .map((p) => p.replace(/\\/g, "/"))
  .filter((p) => (p.endsWith(".svelte") || p.endsWith(".css")) && !/\.(spec|test)\./.test(p))
  .sort()
  .flatMap((path) => {
    const css = path.endsWith(".css");
    let open = css;
    return stripComments(readFileSync(resolve(SRC, path), "utf8"))
      .split(/\r?\n/)
      .map((text, i) => {
        if (!css && /<style[^>]*>/.test(text)) open = true;
        const style = open;
        if (!css && /<\/style>/.test(text)) open = false;
        return { path, n: i + 1, text, style };
      });
  });

const styleLines = lines.filter((l) => l.style);
const at = (l: Line, note = ""): string => `${l.path}:${l.n}${note ? ` ${note}` : ""} — ${l.text.trim()}`;

/**
 * Declaration values for `prop`, per file, keeping only those `ok()` rejects.
 * An explicit property whitelist is what makes the naive `name:` regex safe —
 * a selector like `.row:hover` cannot collide with it.
 */
const offending = (prop: string, ok: (v: string) => boolean): Record<string, string[]> => {
  const re = new RegExp(`(?<![-\\w])(?:${prop})\\s*:\\s*([^;{}]*)`, "g");
  const out: Record<string, string[]> = {};
  for (const l of styleLines) {
    for (const m of l.text.matchAll(re)) {
      const v = m[1].trim();
      if (v && !ok(v)) (out[l.path] ??= []).push(v);
    }
  }
  return out;
};

/** `actual` must equal `allow` exactly — so a guard catches an addition as well as a leftover. */
const beyond = (actual: Record<string, string[]>, allow: Record<string, string[]>): string[] => {
  const paths = [...new Set([...Object.keys(actual), ...Object.keys(allow)])].sort();
  return paths.flatMap((p) => {
    const got = [...(actual[p] ?? [])].sort();
    const want = [...(allow[p] ?? [])].sort();
    return JSON.stringify(got) === JSON.stringify(want) ? [] : [`${p} — got [${got}], allowed [${want}]`];
  });
};

// --- 1. no hardcoded hex ---------------------------------------------------
// Every colour lives in the :root block. The two data sites are EVE's data, not
// the app's chrome (§4 rule 3): a stored overview colour is not a design token.
test("no-hardcoded-hex", () => {
  const root = (() => {
    const ls = lines.filter((l) => l.path === "app.css");
    const start = ls.findIndex((l) => /^:root\s*\{/.test(l.text));
    let depth = 0;
    for (let i = start; i < ls.length; i++) {
      depth += (ls[i].text.match(/\{/g) ?? []).length - (ls[i].text.match(/\}/g) ?? []).length;
      if (depth === 0) return { start: start + 1, end: i + 1 };
    }
    return { start: 0, end: 0 };
  })();

  const allowed = (l: Line): boolean =>
    (l.path === "app.css" && l.n >= root.start && l.n <= root.end) ||
    l.text.includes("UNSET_HEX");

  const offenders = lines
    .filter((l) => /#[0-9a-fA-F]{3,8}(?![0-9a-zA-Z_-])/.test(l.text) && !allowed(l))
    .map((l) => at(l));
  expect(offenders).toEqual([]);
});

// --- 2. no rgba() literals -------------------------------------------------
// Forces the veil tokens in HTML and fill-opacity/stroke-opacity in SVG, which
// keeps every hue inside the block guard 1 watches. Scoped to <style>, so
// ProbeViewer's runtime `rgb(${…})` formatting of EVE colour data is untouched.
test("no-rgba-literals", () => {
  const offenders = styleLines.filter((l) => /\brgba?\(/.test(l.text)).map((l) => at(l));
  expect(offenders).toEqual([]);
});

// --- 3. no undefined tokens ------------------------------------------------
// The one that would have caught --line and --panel, referenced four times in
// AccountsView and declared nowhere, so every card and chip border in that view
// silently fell back to #3333 — a colour no other view uses.
test("no-undefined-tokens", () => {
  const declared = new Set<string>();
  for (const l of lines) for (const m of l.text.matchAll(/(--[\w-]+)\s*:/g)) declared.add(m[1]);

  const offenders = lines
    .flatMap((l) =>
      [...l.text.matchAll(/var\(\s*(--[\w-]+)/g)]
        .filter((m) => !declared.has(m[1]))
        .map((m) => at(l, m[1])),
    );
  expect(offenders).toEqual([]);
});

// --- 4. one type scale -----------------------------------------------------
// `em` compounds: 0.85em resolves to three different pixel sizes depending on
// which block it lands in (§2.2), which is the mechanical source of the "text
// sizes don't match" complaint.
//
// The allowlist is three lines, not §4.3's seven. Those are genuine drawings of
// EVE's screen, where --t-caption's 12px would overflow the rectangles the
// labels name. §4.3 also listed ChatSplit's four 10px lines as canvas-scale,
// but ChatSplit renders inside WindowPanel — the side panel, not the canvas —
// so it is chrome and takes the scale like everything else. §4.2's own table
// converts all four to --t-caption, which is the half of the spec that is
// right; the two sections disagreed and this is the resolution.
const CANVAS_TYPE: Record<string, string[]> = {
  "lib/DetailParts.svelte": ["9px"],
  "lib/LayoutView.svelte": ["11px", "11px"],
};
test("type-scale", () => {
  const ok = (v: string): boolean => /^var\(--t-(caption|body|ui|title|head)\)$/.test(v) || v === "inherit";
  expect(beyond(offending("font-size", ok), CANVAS_TYPE)).toEqual([]);
});

// --- 5. three radii --------------------------------------------------------
// The three 50% sites are circles, not corners: two round HUD parts and the
// canvas anchor dot.
const CIRCLES: Record<string, string[]> = {
  "lib/DetailParts.svelte": ["50%", "50%"],
  "lib/LayoutView.svelte": ["50%"],
};
test("radius-scale", () => {
  const ok = (v: string): boolean => /^var\(--r-(sm|md|pill)\)$/.test(v) || v === "0" || v === "inherit";
  expect(beyond(offending("border-radius", ok), CIRCLES)).toEqual([]);
});

// --- 6. one spacing scale --------------------------------------------------
// Raw pixel values are allowed in two narrow places and nowhere else.
//
// The primitives may use 1px, 2px and -1px: the scale is a 4px base, but a
// dense tool needs sub-step padding on the small button and the chip, and the
// underline tab needs -1px to lap its border over the strip's.
//
// The two canvas files may use any px, for the same reason §4.3 exempts their
// font sizes. They are drawings of EVE's screen at arbitrary scale, and their
// padding and margin are GEOMETRY rather than spacing: `.anchor-dot`'s -5px is
// half a 9px dot plus its border, straddling the corner it marks, and the rect
// labels' 1px 3px inset sits inside an 11px-labelled rectangle. Rounding either
// onto a 4px scale would move the drawing, not tidy it.
//
// Inventing a --s0 to spell any of it would put a half-step in reach of all 25
// views, which is how 55 distinct padding values happened the first time.
test("space-scale", () => {
  const prop = "(?:padding|margin)(?:-(?:top|right|bottom|left|inline|block)(?:-(?:start|end))?)?|(?:row-|column-)?gap";
  const primitive = /^lib\/ui\//;
  const canvas = /^lib\/(DetailParts|LayoutView)\.svelte$/;
  const offenders = styleLines.flatMap((l) => {
    const part = (p: string): boolean =>
      /^var\(--s[1-6]\)$/.test(p) ||
      p === "0" ||
      p === "auto" ||
      p === "inherit" ||
      /^-?[\d.]+(%|fr)$/.test(p) ||
      (primitive.test(l.path) && (p === "1px" || p === "2px" || p === "-1px")) ||
      (canvas.test(l.path) && /^-?\d+px$/.test(p));
    return [...l.text.matchAll(new RegExp(`(?<![-\\w])(?:${prop})\\s*:\\s*([^;{}]*)`, "g"))]
      .map((m) => m[1].trim())
      .filter((v) => v && !v.split(/\s+/).every(part))
      .map((v) => at(l, v));
  });
  expect(offenders).toEqual([]);
});

// --- 7. one opacity value --------------------------------------------------
// opacity is retired as a hierarchy device (§3.2): it was carrying rank at ten
// different values, stacked on top of three dim greys, and got pushed past
// legibility. Rank now comes from size, weight and position. What survives
// modulates a *drawing* — a drag ghost, a resize handle, SVG depth cues in the
// probe viewer — never the legibility of text.
const GRAPHICS_OPACITY: Record<string, string[]> = {
  "lib/LayoutView.svelte": ["0.45", "0.6"],
  "lib/ProbeViewer.svelte": ["0.6", "0.75", "0.75", "0.75", "0.7", "0.3"],
  // The fade-out keyframes, moved out of app.css. An animation from 1 to 0 is
  // not a hierarchy claim, and this is the only animation in the app.
  "lib/ui/Toast.svelte": ["1", "0"],
};
test("one-opacity", () => {
  const ok = (v: string): boolean => v === "var(--o-disabled)";
  expect(beyond(offending("opacity", ok), GRAPHICS_OPACITY)).toEqual([]);
});

// --- 8. .mini is gone ------------------------------------------------------
// `.mini { opacity: 0 }`, revealed only by `.row:hover .mini`, left four buttons
// permanently invisible but still clickable, because they sit outside any `.row`.
// `.mini-visible` is the workaround that was written twice rather than the trap
// being removed. Button variant="ghost" retires both.
test("mini-is-gone", () => {
  const offenders = lines.filter((l) => /\bmini(?:-visible)?\b/.test(l.text)).map((l) => at(l));
  expect(offenders).toEqual([]);
});
