// Command labels, groups and (eventually) factory defaults for the keybinding
// editor. Labels come from EVE's own localization data via
// tools/gen-command-names.py; see docs/superpowers/specs/2026-07-26-keybindings-editor-design.md §3.
import names from "./data/command-names.json" with { type: "json" };
import defaults from "./data/command-defaults.json" with { type: "json" };
import vkLabels from "./data/vk-labels.json" with { type: "json" };

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

/** EVE's factory binding, or null. The catalog ships EMPTY, and **cannot be
 *  filled from a settings file** — confirmed in-game 2026-07-27: "Reset to
 *  default" writes `customCmds: {}`, an *empty* dict. `customCmds` only ever
 *  holds overrides, so a reset erases the table rather than spelling out the
 *  defaults, and there is nothing to capture. (The design spec's §4 plan of
 *  capturing them from a reset-to-default logout is therefore dead — do not
 *  retry it.)
 *
 *  Fill this by transcribing EVE's keybinding screen instead. Partial data is
 *  fine and is the expected way in: this returns null per command, so each
 *  entry added lights up its own Default cell and per-row reset button while
 *  every other row is unaffected. */
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

export const MOD_CTRL = 17;
export const MOD_ALT = 18;
export const MOD_SHIFT = 16;
/** Canonical order — the one EVE writes, verified over 4,765 real bindings. */
const MODIFIERS = [MOD_CTRL, MOD_ALT, MOD_SHIFT];
const MOD_LABEL: Record<number, string> = { [MOD_CTRL]: "Ctrl", [MOD_ALT]: "Alt", [MOD_SHIFT]: "Shift" };

/** Windows virtual-key codes EVE can store, from the JSON the Rust side also
 *  reads (the MCP server turns "Q" into 81 with it). Serves both display and
 *  capture validation: a code absent here is rejected rather than written
 *  blind. */
const VK_LABELS: Record<number, string> = Object.fromEntries(
  Object.entries(vkLabels as Record<string, string>).map(([k, v]) => [Number(k), v]),
);

/** [17, 81] -> "Ctrl+Q". An unknown code renders as VK<n> rather than
 *  disappearing, so a binding we cannot name is still visible. */
export function keysToLabel(keys: number[] | null): string {
  if (!keys || keys.length === 0) return "unbound";
  return keys.map((c) => MOD_LABEL[c] ?? VK_LABELS[c] ?? `VK${c}`).join("+");
}

/** A keydown into the canonical code list, or null if it is not a usable
 *  binding (a bare modifier press, or a key outside VK_LABELS).
 *
 *  ponytail: reads the deprecated `event.keyCode`, which in WebView2 IS the
 *  Windows virtual-key code EVE stores — a one-lookup mapping. If it is ever
 *  removed, the upgrade is an `event.code` -> VK table against VK_LABELS. */
export function eventToKeys(e: KeyboardEvent): number[] | null {
  const code = e.keyCode;
  if (MODIFIERS.includes(code)) return null; // still holding the modifier down
  if (!(code in VK_LABELS)) return null;
  const mods = [
    ...(e.ctrlKey ? [MOD_CTRL] : []),
    ...(e.altKey ? [MOD_ALT] : []),
    ...(e.shiftKey ? [MOD_SHIFT] : []),
  ];
  return [...mods, code];
}
