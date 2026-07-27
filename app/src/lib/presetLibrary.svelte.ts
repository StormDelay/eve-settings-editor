// The preset library: the editor's own folder of saved settings, loaded on
// demand and refreshed from whatever the backend returns after each mutation.
// Nothing here touches an EVE settings file — see app/src-tauri/src/presets.rs.
//
// Split from presetLibrary.ts: this half uses runes ($state), which only the
// Svelte/Vite compiler understands, so it can't be loaded by plain
// `node --test` — the pure helpers live there instead and are re-exported here
// for callers that want everything from one module.
import { api } from "./api";
import type { PresetInfo } from "./api";

export { aspectLabel, summarise } from "./presetLibrary";

let presets = $state<PresetInfo[]>([]);

export const allPresets = (): PresetInfo[] => presets;

/** Replace the library with what the backend just returned. Every mutating
 * command answers with the fresh list, so there is one refresh path. */
export function setPresets(next: PresetInfo[]): void {
  presets = next;
}

/** A failure leaves the list alone: the library is a convenience, and the
 * editor must open without it. */
export async function loadPresets(): Promise<void> {
  presets = await api.settingsPresetList().catch(() => presets);
}
