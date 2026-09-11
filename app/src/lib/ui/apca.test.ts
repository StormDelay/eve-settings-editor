// The contrast floor, as a test rather than a hope —
// docs/ui-redesign/01-tokens-and-primitives.md §7.2.
//
// Token values are read out of app.css rather than restated here, so changing a
// hex without re-solving it fails a named test instead of shipping. That is the
// entire point: the old palette's most-used text colour was unreadable for 51
// uses and nothing said so.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test } from "vitest";
import { lc } from "./apca";

// --- the implementation itself ---------------------------------------------
// Frozen measurements of the *pre-Phase-1* palette, from §2.5. They are here to
// validate lc(), not the palette — that palette is gone. Both polarities are
// covered: the last three are dark-on-saturated, which is the shape §3.4 bans
// precisely because it looks confident and measures terrible.
const LEGACY: [string, string, string, number][] = [
  ["--fg-dim on --bg", "#8a919e", "#16181d", 41.8],
  ["--fg-dim on --bg-panel", "#8a919e", "#1e2128", 40.6],
  ["#666 on --bg", "#666666", "#16181d", 21.9],
  ["#888 on the canvas", "#888888", "#1b1f27", 36.7],
  ["--danger on --bg", "#e06c60", "#16181d", 41.5],
  [".kind-int on --bg", "#c8a1e8", "#16181d", 58.7],
  ["--fg on --bg", "#d5d9e0", "#16181d", 82.2],
  [".badge.editable", "#10240f", "#62b268", 50.7],
  [".badge.dirty", "#33260a", "#d9a441", 55.3],
  [".badge.read-only", "#2b100d", "#e06c60", 43.2],
];
for (const [name, text, bg, want] of LEGACY) {
  test(`lc() reproduces the measured ${name} = Lc ${want}`, () => {
    expect(lc(text, bg)).toBeCloseTo(want, 0);
  });
}

// --- the palette -----------------------------------------------------------
const ROOT = (() => {
  const css = readFileSync(resolve(import.meta.dirname, "../../app.css"), "utf8");
  const block = /:root\s*\{([\s\S]*?)\}/.exec(css)?.[1] ?? "";
  return Object.fromEntries(
    [...block.matchAll(/(--[\w-]+)\s*:\s*(#[0-9a-fA-F]{3,8})\s*;/g)].map((m) => [m[1], m[2]]),
  );
})();

const SURFACES = ["--bg", "--surface", "--surface-raised", "--surface-overlay"];

/** Each text token's floor, solved by binary search against the lightest surface it may sit on. */
const FLOORS: Record<string, number> = {
  "--text": 90,
  "--text-secondary": 78,
  "--text-muted": 65,
  "--accent": 65,
  "--danger": 65,
  "--warn": 65,
  "--ok": 65,
  "--info": 65,
  "--syntax-number": 65,
};

for (const [token, floor] of Object.entries(FLOORS)) {
  for (const surface of SURFACES) {
    test(`${token} on ${surface} meets its floor of Lc ${floor}`, () => {
      expect(ROOT[token], `${token} is not declared in :root`).toBeDefined();
      expect(ROOT[surface], `${surface} is not declared in :root`).toBeDefined();
      expect(lc(ROOT[token], ROOT[surface])).toBeGreaterThanOrEqual(floor);
    });
  }
}

// A badge is a light role tone on its matching -dim ground, never dark text on a
// saturated fill (§3.4). This is what makes that rule safe to apply everywhere:
// the replacement measures ~69 where the pattern it replaces measured 43-55.
for (const role of ["--accent", "--danger", "--warn", "--ok", "--info"]) {
  test(`${role} on ${role}-dim meets the Lc 65 floor`, () => {
    expect(lc(ROOT[role], ROOT[`${role}-dim`])).toBeGreaterThanOrEqual(65);
  });
}
