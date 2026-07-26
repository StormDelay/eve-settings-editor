// Run: npm test (node --test). Throw-based checks, no framework.
import { labelFor, groupFor, GROUP_ORDER, defaultFor } from "./keybinds.ts";
import names from "./data/command-names.json" with { type: "json" };

const check = (name: string, ok: boolean) => { if (!ok) throw new Error(`FAIL: ${name}`); console.log(`  ok - ${name}`); };

check("resolves a client-provided label", labelFor("CmdActivateHighPowerSlot1") === "Activate High Power Slot 1");
check("resolves a fleet broadcast label", labelFor("CmdFleetBroadcast_HealArmor") === "Broadcast: Need Armor");
check("hand-corrected label is used", labelFor("ToggleCurrentSystemLocationWnd") === "Toggle Current System Location Window");
check("an unknown command de-camelcases", labelFor("CmdSomeFutureThing") === "Some Future Thing");
check("an unknown Open command de-camelcases", labelFor("OpenFutureWindow") === "Open Future Window");

check("modules group", groupFor("CmdActivateHighPowerSlot1") === "Modules");
check("overload beats modules", groupFor("CmdOverloadHighPowerRack") === "Overload");
check("windows group", groupFor("OpenFitting") === "Windows");
check("unknown falls back to Misc", groupFor("CmdSomeFutureThing") === "Misc");
check("every group used is in GROUP_ORDER", GROUP_ORDER.includes(groupFor("CmdActivateHighPowerSlot1")));

check("defaults are empty until captured", defaultFor("CmdActivateHighPowerSlot1") === null);

// The catalog is generated, so a bad regen or merge can silently shrink it and
// every probe above still passes as long as its handful of keys survive. 101 is
// the corpus-measured command count (docs/settings-field-reference.md §5.3); a
// legitimate change to it means EVE added or removed commands, so update this
// number deliberately rather than deleting the check.
check("catalog carries all 101 commands", Object.keys(names).length === 101);
check("every catalog entry has a label and a group",
  Object.values(names).every((e) => typeof e.label === "string" && e.label !== ""
    && typeof e.group === "string" && e.group !== ""));

import { keysToLabel, eventToKeys, MOD_CTRL, MOD_ALT, MOD_SHIFT } from "./keybinds.ts";

check("formats a bare key", keysToLabel([81]) === "Q");
check("formats a modified key", keysToLabel([17, 81]) === "Ctrl+Q");
check("formats the canonical three-modifier order", keysToLabel([17, 18, 16, 68]) === "Ctrl+Alt+Shift+D");
check("formats unbound", keysToLabel(null) === "unbound");
check("formats a function key", keysToLabel([112]) === "F1");
check("an unknown code shows its number", keysToLabel([250]) === "VK250");

// Minimal KeyboardEvent stand-in — node has no DOM.
const ev = (o: Partial<KeyboardEvent>) => o as KeyboardEvent;

check("captures a bare key", JSON.stringify(eventToKeys(ev({ keyCode: 81 }))) === JSON.stringify([81]));
check(
  "captures modifiers in canonical order",
  JSON.stringify(eventToKeys(ev({ keyCode: 68, ctrlKey: true, altKey: true, shiftKey: true }))) ===
    JSON.stringify([MOD_CTRL, MOD_ALT, MOD_SHIFT, 68]),
);
check("a modifier-only press is not a binding", eventToKeys(ev({ keyCode: 17, ctrlKey: true })) === null);
check("an unknown key code is rejected", eventToKeys(ev({ keyCode: 250 })) === null);
check("the modifier constants match EVE's codes", MOD_CTRL === 17 && MOD_ALT === 18 && MOD_SHIFT === 16);
