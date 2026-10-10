// Command labels, groups and factory defaults for the keybinding editor, all
// generated from the EVE client's own code by tools/gen-commands.py: the labels
// and tabs the in-game keybinding screen shows, and the bindings it ships.
import names from "./data/command-names.json" with { type: "json" };
import defaults from "./data/command-defaults.json" with { type: "json" };
import vkLabels from "./data/vk-labels.json" with { type: "json" };

type NameEntry = { label?: string; group: string };
const NAMES = names as Record<string, NameEntry>;
const DEFAULTS = defaults as Record<string, number[] | null>;

/** The in-game keybinding screen's tabs, in its order (eveCommands'
 *  `CATEGORIES`). Anything unlisted sorts last. */
export const GROUP_ORDER = [
  "Window",
  "Combat",
  "General",
  "Navigation",
  "Modules",
  "Drones",
  "Fighters",
  "Character Creation",
  "Character Movement",
];

/** "CmdActivateHighPowerSlot1" -> "Activate High Power Slot 1". A command the
 *  catalog does not know (a client update added it) degrades to a readable
 *  de-camelcased name rather than a blank row. */
export function labelFor(command: string): string {
  return NAMES[command]?.label ?? decamel(command);
}

export function groupFor(command: string): string {
  return NAMES[command]?.group ?? "General";
}

/** EVE's factory binding: the keys, `null` for a command EVE ships unbound,
 *  or `undefined` for one the table does not know (a client update added it).
 *  Not in any settings file — "Reset to default" writes `customCmds: {}`,
 *  because `customCmds` only ever holds overrides — so the table is generated
 *  from the client's own code by tools/gen-commands.py. */
export function defaultFor(command: string): number[] | null | undefined {
  return DEFAULTS[command];
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
