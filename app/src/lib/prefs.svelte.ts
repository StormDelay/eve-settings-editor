// Editor preferences, loaded once at startup and written through on change.
// Nothing here touches an EVE settings file — see app/src-tauri/src/prefs.rs.
import { api, errMessage } from "$lib/api";
import type { Preferences } from "$lib/api";
import type { ClutterOverrides } from "$lib/windowLabels";
import { message } from "@tauri-apps/plugin-dialog";

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

export const overrideCount = () => prefs.layout.clutter.length + prefs.layout.visible.length;

/** Every write is chained after the previous one settles, rather than fired
 * independently — this is a single-user desktop app with one UI, so awaiting
 * the prior write is enough to stop two rapid toggles from resolving out of
 * order and leaving the file one step behind what's on screen. A failure
 * surfaces the usual error dialog and rolls the in-memory state back to what
 * it was before this change, per spec §4. */
let writeQueue: Promise<void> = Promise.resolve();

function persist(next: Preferences, prev: Preferences): void {
  writeQueue = writeQueue.then(() =>
    api.setPreferences($state.snapshot(next)).catch(async (e) => {
      prefs = prev;
      await message(errMessage(e), { title: "Preferences not saved", kind: "error" });
    }),
  );
}

/** Force a window into or out of the clutter set, or drop the override. The
 * two lists are kept disjoint here, which is what lets `isClutter` treat them
 * as independent. */
export function setClutterOverride(id: string, mode: "clutter" | "visible" | "default"): void {
  const prev = prefs;
  const l = prefs.layout;
  prefs = {
    ...prefs,
    layout: {
      clutter: l.clutter.filter((x) => x !== id).concat(mode === "clutter" ? [id] : []),
      visible: l.visible.filter((x) => x !== id).concat(mode === "visible" ? [id] : []),
    },
  };
  persist(prefs, prev);
}

export function clearClutterOverrides(): void {
  const prev = prefs;
  prefs = { ...prefs, layout: { clutter: [], visible: [] } };
  persist(prefs, prev);
}
