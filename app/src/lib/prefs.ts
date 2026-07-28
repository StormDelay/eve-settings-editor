// Pure helpers for the editor preferences — no runes, so this is
// node --test-able. The stateful half (the loaded preferences and the writes)
// lives in prefs.svelte.ts, which re-exports these for its callers.
import type { LayoutPrefs } from "./api.ts";

/** Overrides naming a window the given document actually has. */
export const countIn = (stored: LayoutPrefs, ids: ReadonlySet<string>): number =>
  stored.clutter.filter((id) => ids.has(id)).length +
  stored.visible.filter((id) => ids.has(id)).length;

/** The stored lists with every in-scope id removed, the rest untouched. */
export const withoutIn = (stored: LayoutPrefs, ids: ReadonlySet<string>): LayoutPrefs => ({
  clutter: stored.clutter.filter((id) => !ids.has(id)),
  visible: stored.visible.filter((id) => !ids.has(id)),
});
