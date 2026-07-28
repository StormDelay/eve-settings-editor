// Editor preferences, loaded once at startup and written through on change.
// Nothing here touches an EVE settings file — see app/src-tauri/src/prefs.rs.
//
// Split from prefs.ts: this half uses runes ($state), which only the
// Svelte/Vite compiler understands, so it can't be loaded by plain
// `node --test` — the pure helpers live there instead and are re-exported here
// for callers that want everything from one module.
import { api, errMessage } from "$lib/api";
import type { Preferences } from "$lib/api";
import type { ClutterOverrides } from "$lib/windowLabels";
import { countIn, withoutIn } from "$lib/prefs";
import { message } from "@tauri-apps/plugin-dialog";

export { countIn, withoutIn } from "$lib/prefs";

let prefs = $state<Preferences>({ layout: { clutter: [], visible: [] } });

/** Load once. A failure leaves the defaults in place: preferences are a
 * convenience, and the editor must open without them. */
export async function loadPrefs(): Promise<void> {
  prefs = await api.preferences().catch(() => prefs);
}

export const clutterOverrides = (): ClutterOverrides => ({
  clutter: new Set(prefs.layout.clutter),
  visible: new Set(prefs.layout.visible),
});

/**
 * How many overrides are doing something in the document on screen.
 *
 * The stored list is application-wide, but what it is DOING at any moment
 * depends on the file you have open — so a global tally sat beside "showing N
 * of M windows" claiming to describe this layout while describing every
 * character's. Scoped to the open document's windows, not to the windows
 * currently drawn, for stability: a count that moved while you typed in the
 * filter box would be worse than one that is slightly too broad.
 */
export const overrideCount = (ids: ReadonlySet<string>): number => countIn(prefs.layout, ids);

/** Every write is chained after the previous one settles, rather than fired
 * independently — this is a single-user desktop app with one UI, so awaiting
 * the prior write is enough to stop two rapid toggles from resolving out of
 * order and leaving the file one step behind what's on screen. A failure
 * surfaces the usual error dialog, matching `LayoutView`'s `commit`/`runStack`/
 * `setHud`.
 *
 * Deliberately no rollback. `prefs` is written whole, not as a delta, so if a
 * later write lands (as `loadPrefs` would after a restart, or the next
 * successful toggle) it carries the full current state and self-heals the
 * file regardless of an earlier failure. A hand-restored `prev` snapshot
 * would instead risk *reverting* a concurrent edit that queued after the one
 * that failed — WA fails and restores state0 while WB, already queued on top
 * of state1, still succeeds and writes state2 to disk; memory then holds the
 * stale state0, and the next edit builds on it, silently overwriting the good
 * on-disk state2. Per spec §4: a failed write "leaves the in-memory state
 * alone" — with no rollback there is no stale value to diverge from. */
let writeQueue: Promise<void> = Promise.resolve();

function persist(next: Preferences): void {
  writeQueue = writeQueue.then(() =>
    api.setPreferences($state.snapshot(next)).catch(async (e) => {
      await message(errMessage(e), { title: "Preferences not saved", kind: "error" });
    }),
  );
}

/** Force a window into or out of the clutter set, or drop the override. The
 * two lists are kept disjoint here, which is what lets `isClutter` treat them
 * as independent. */
export function setClutterOverride(id: string, mode: "clutter" | "visible" | "default"): void {
  const l = prefs.layout;
  prefs = {
    ...prefs,
    layout: {
      clutter: l.clutter.filter((x) => x !== id).concat(mode === "clutter" ? [id] : []),
      visible: l.visible.filter((x) => x !== id).concat(mode === "visible" ? [id] : []),
    },
  };
  persist(prefs);
}

/** Drop only the overrides naming a window the given document has. Every other
 * override is left on disk.
 *
 * Note the exact guarantee, which is narrower than it first looks: it cannot
 * delete an override for a window THIS DOCUMENT DOES NOT HAVE. It is not
 * per-character isolation, and cannot be — window ids are per-character dict
 * keys and the common ones (`overview`, `market`) repeat across characters, so
 * clearing from A still drops B's override on a window they share. Making that
 * true would mean keying the stored list by character, which is a different and
 * larger change; the list is application-wide on purpose, because marking a
 * window as clutter is a statement about the window, not about one pilot.
 *
 * Same chained write as `setClutterOverride` — only the value written changed. */
export function clearClutterOverrides(ids: ReadonlySet<string>): void {
  prefs = { ...prefs, layout: withoutIn(prefs.layout, ids) };
  persist(prefs);
}
