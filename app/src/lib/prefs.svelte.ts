// Editor preferences, loaded once at startup and written through on change.
// Nothing here touches an EVE settings file — see app/src-tauri/src/prefs.rs.
import { api } from "$lib/api";
import type { Preferences } from "$lib/api";
import type { ClutterOverrides } from "$lib/windowLabels";

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
  void api.setPreferences($state.snapshot(prefs));
}

export function clearClutterOverrides(): void {
  prefs = { ...prefs, layout: { clutter: [], visible: [] } };
  void api.setPreferences($state.snapshot(prefs));
}
