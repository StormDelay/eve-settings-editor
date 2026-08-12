// The assertion helper the pure-module tests are written against.
//
// These sixteen files used to run under `node --test` with no framework at all,
// each declaring its own identical `check`. That bought one thing — no test
// dependency — and stopped buying it the moment vitest arrived for the
// component tests. What it still cost was real: `node --test` cannot compile
// Svelte runes, so any module using `$state` was unreachable from a test, and
// `prefs`/`presetLibrary` had to be split into a pure half and a rune half with
// a re-export shim between them purely to satisfy the runner.
//
// So `check` now registers a vitest test instead of throwing, and the sixteen
// files keep their shape. The condition is still evaluated eagerly, at
// collection time, exactly as it was when a throw was the failure signal: a
// false one fails its own named test, and an exception while computing one
// fails the whole file, which is what `node --test` did too.
import { expect, test } from "vitest";

export const check = (name: string, ok: boolean): void => {
  test(name, () => expect(ok).toBe(true));
};

/** Structural equality by JSON shape, for comparing arrays and plain objects. */
export const eq = (a: unknown, b: unknown): boolean =>
  JSON.stringify(a) === JSON.stringify(b);
