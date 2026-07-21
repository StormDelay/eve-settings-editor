// Pure helpers for EVE's built-in default overview profiles. No Svelte/Tauri/JSON
// deps, so this is node --test-able; the component imports the bundle JSON and
// passes it in.

export interface DefaultProfile {
  key: string;
  name: string;
  groups: number[];
  filteredStates: number[];
  alwaysShownStates: number[];
}
export interface DefaultsBundle { modern: DefaultProfile[]; legacy: DefaultProfile[]; }

export const LEGACY_NAMES: Record<string, string> = {
  defaultall: "All", defaultpvp: "PvP", defaultmining: "Mining", defaultloot: "Loot",
  defaultdrones: "Drones", defaultwarpto: "Warp To", default: "Default",
};

// A preset key that belongs to a built-in default (read-only, forked on edit).
// Matches the known legacy names exactly rather than any "default*" prefix, so
// a user preset merely named e.g. "Default Faves" isn't swept in as built-in.
export function isDefaultKey(key: string): boolean {
  return /^DefaultPreset_\d+$/.test(key) || key.toLowerCase() in LEGACY_NAMES;
}

// The account's on-disk regime, inferred from the default profiles its tabs
// reference: modern (DefaultPreset_<id>) vs legacy (default* literals). Defaults
// to modern when no tab references a default (the offered defaults are a nicety).
export function accountFormat(tabPresets: string[]): "modern" | "legacy" {
  if (tabPresets.some((p) => /^DefaultPreset_\d+$/.test(p))) return "modern";
  if (tabPresets.some((p) => p.toLowerCase() in LEGACY_NAMES)) return "legacy";
  return "modern";
}

export function defaultsForFormat(bundle: DefaultsBundle, format: "modern" | "legacy"): DefaultProfile[] {
  return format === "legacy" ? bundle.legacy : bundle.modern;
}

// Dropdown split: all default keys (bundled ∪ any stored default) vs user keys.
// A materialized default appears once (in `defaults`, not `user`).
export function mergePresetOptions(storedNames: string[], defaults: DefaultProfile[]): { defaults: string[]; user: string[] } {
  const defaultKeys = new Set(defaults.map((d) => d.key));
  for (const n of storedNames) if (isDefaultKey(n)) defaultKeys.add(n);
  const user = storedNames.filter((n) => !isDefaultKey(n));
  return { defaults: [...defaultKeys].sort(), user };
}

// Unique "<base> copy" name given the keys already in use.
export function forkName(baseLabel: string, existingKeys: string[]): string {
  const set = new Set(existingKeys);
  const base = `${baseLabel} copy`;
  if (!set.has(base)) return base;
  let i = 2;
  while (set.has(`${base} ${i}`)) i++;
  return `${base} ${i}`;
}

export function findDefault(defaults: DefaultProfile[], key: string): DefaultProfile | undefined {
  return defaults.find((d) => d.key === key);
}
