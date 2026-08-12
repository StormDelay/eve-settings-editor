// The preset library: the editor's own folder of saved settings, loaded on
// demand and refreshed from whatever the backend returns after each mutation.
// Nothing here touches an EVE settings file — see app/src-tauri/src/presets.rs.
//
// The label helpers below used to live in a separate presetLibrary.ts, because
// this half's `$state` rune is compiler-only and `node --test` could not load
// it. One vitest suite now runs both kinds of test, so the split — and the
// re-export shim that held it together — is gone.
import { api } from "./api";
import type { Aspect, PresetInfo } from "./api";

const LABELS: Record<Aspect, string> = {
  layout: "Layout",
  overview: "Overview",
  autofill: "Autofill",
  keybinds: "Keybinds",
  probe_formations: "Probe formations",
  everything: "Everything",
};

export const aspectLabel = (a: Aspect): string => LABELS[a];

/** One line describing what a preset holds. A full preset says "Everything"
 * rather than listing the aspects it implies. */
export function summarise(p: PresetInfo): string {
  if (p.error) return "unreadable";
  if (p.full) return LABELS.everything;
  if (p.aspects.length === 0) return "empty";
  return p.aspects.map(aspectLabel).join(" · ");
}

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
