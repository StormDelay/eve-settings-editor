// Command labels, groups and (eventually) factory defaults for the keybinding
// editor. Labels come from EVE's own localization data via
// tools/gen-command-names.py; see docs/superpowers/specs/2026-07-26-keybindings-editor-design.md §3.
import names from "./data/command-names.json" with { type: "json" };
import defaults from "./data/command-defaults.json" with { type: "json" };

type NameEntry = { label: string; group: string };
const NAMES = names as Record<string, NameEntry>;
const DEFAULTS = defaults as Record<string, number[]>;

/** Display order for the grouped list. Anything unlisted sorts last. */
export const GROUP_ORDER = [
  "Modules",
  "Overload",
  "Drones & Fighters",
  "Targeting",
  "Navigation",
  "Fleet broadcasts",
  "Windows",
  "Misc",
];

/** "CmdActivateHighPowerSlot1" -> "Activate High Power Slot 1". A command the
 *  catalog does not know (a client update added it) degrades to a readable
 *  de-camelcased name rather than a blank row. */
export function labelFor(command: string): string {
  return NAMES[command]?.label ?? decamel(command);
}

export function groupFor(command: string): string {
  return NAMES[command]?.group ?? "Misc";
}

/** EVE's factory binding, or null. The catalog ships EMPTY: no factory defaults
 *  exist anywhere in the settings files, and an account that never opened the
 *  keybinding screen has no table at all, so they must be captured from a
 *  reset-to-default logout. Spec §4. */
export function defaultFor(command: string): number[] | null {
  return DEFAULTS[command] ?? null;
}

function decamel(command: string): string {
  return command
    .replace(/^Cmd/, "")
    .replace(/_/g, ": ")
    .replace(/(?<=[a-z0-9])(?=[A-Z])/g, " ")
    .trim();
}
