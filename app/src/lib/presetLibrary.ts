// Pure helpers for the preset library — no runes, so this is node --test-able.
// The stateful half (the loaded list itself) lives in presetLibrary.svelte.ts,
// which re-exports these for its callers.
import type { Aspect, PresetInfo } from "./api.ts";

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
