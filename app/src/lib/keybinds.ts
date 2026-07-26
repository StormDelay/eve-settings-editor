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

export const MOD_CTRL = 17;
export const MOD_ALT = 18;
export const MOD_SHIFT = 16;
/** Canonical order — the one EVE writes, verified over 4,765 real bindings. */
const MODIFIERS = [MOD_CTRL, MOD_ALT, MOD_SHIFT];
const MOD_LABEL: Record<number, string> = { [MOD_CTRL]: "Ctrl", [MOD_ALT]: "Alt", [MOD_SHIFT]: "Shift" };

/** Windows virtual-key codes EVE can store. Serves both display and capture
 *  validation: a code absent here is rejected rather than written blind. */
export const VK_LABELS: Record<number, string> = {
  8: "Backspace", 9: "Tab", 13: "Enter", 19: "Pause", 20: "Caps Lock", 27: "Esc",
  32: "Space", 33: "Page Up", 34: "Page Down", 35: "End", 36: "Home",
  37: "Left", 38: "Up", 39: "Right", 40: "Down",
  45: "Insert", 46: "Delete",
  48: "0", 49: "1", 50: "2", 51: "3", 52: "4", 53: "5", 54: "6", 55: "7", 56: "8", 57: "9",
  65: "A", 66: "B", 67: "C", 68: "D", 69: "E", 70: "F", 71: "G", 72: "H", 73: "I",
  74: "J", 75: "K", 76: "L", 77: "M", 78: "N", 79: "O", 80: "P", 81: "Q", 82: "R",
  83: "S", 84: "T", 85: "U", 86: "V", 87: "W", 88: "X", 89: "Y", 90: "Z",
  96: "Num 0", 97: "Num 1", 98: "Num 2", 99: "Num 3", 100: "Num 4", 101: "Num 5",
  102: "Num 6", 103: "Num 7", 104: "Num 8", 105: "Num 9",
  106: "Num *", 107: "Num +", 109: "Num -", 110: "Num .", 111: "Num /",
  112: "F1", 113: "F2", 114: "F3", 115: "F4", 116: "F5", 117: "F6",
  118: "F7", 119: "F8", 120: "F9", 121: "F10", 122: "F11", 123: "F12",
  144: "Num Lock", 145: "Scroll Lock",
  186: ";", 187: "=", 188: ",", 189: "-", 190: ".", 191: "/", 192: "`",
  219: "[", 220: "\\", 221: "]", 222: "'",
};

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
