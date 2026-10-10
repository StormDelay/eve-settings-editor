// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import { labelFor, groupFor, GROUP_ORDER, defaultFor } from "./keybinds.ts";
import defaults from "./data/command-defaults.json" with { type: "json" };
import names from "./data/command-names.json" with { type: "json" };

import { check, eq } from "./test/check.ts";

check("resolves a client-provided label", labelFor("CmdActivateHighPowerSlot1") === "Activate High Power Slot 1");
check("resolves a fleet broadcast label", labelFor("CmdFleetBroadcast_HealArmor") === "Broadcast: Need Armor");
// Once hand-corrected against the live client (2026-07-27); the client's own
// label data now gives the same answers, which is what vouches for it.
check("the client names it Local Locations", labelFor("ToggleCurrentSystemLocationWnd") === "Local Locations");
check("portrait labels are 1-based, not 0-based", labelFor("CmdPickPortrait0") === "Pick Portrait 1");
check("the last portrait is 4", labelFor("CmdPickPortrait3") === "Pick Portrait 4");
check("an unknown command de-camelcases", labelFor("CmdSomeFutureThing") === "Some Future Thing");
check("an unknown Open command de-camelcases", labelFor("OpenFutureWindow") === "Open Future Window");

check("modules group", groupFor("CmdActivateHighPowerSlot1") === "Modules");
check("groups are the in-game tabs", groupFor("OpenFitting") === "Window");
check("the client's catch-all tab is General", groupFor("CmdZoomIn") === "General");
check("unknown falls back to General", groupFor("CmdSomeFutureThing") === "General");
check("the Agency has its in-game name", labelFor("OpenAgencyNew") === "The Agency");
check("every group used is in GROUP_ORDER", GROUP_ORDER.includes(groupFor("CmdActivateHighPowerSlot1")));

// Spot checks against the live client's keybinding screen.
check("F1 activates high slot 1", eq(defaultFor("CmdActivateHighPowerSlot1"), [112]));
check("Alt+F1 activates mid slot 1", eq(defaultFor("CmdActivateMediumPowerSlot1"), [18, 112]));
check("Ctrl+R reloads", eq(defaultFor("CmdReloadAmmo"), [17, 82]));
check("modifiers come in EVE's stored order", (Object.values(defaults) as (number[] | null)[]).every(
  (k) => !k || eq(k.filter((c) => [17, 18, 16].includes(c)), [17, 18, 16].filter((c) => k.includes(c)))));
check("a command EVE ships unbound is null", defaultFor("CmdExitStation") === null);
check("a command the table lacks is undefined", defaultFor("CmdSomeFutureThing") === undefined);
check("locked commands are not listed", defaultFor("OnEsc") === undefined);

// Both tables are generated, so a bad regen can silently shrink them while every
// probe above still passes. They cover the same commands — every rebindable one
// the client defines (292 when generated); a client update can move that, so
// check the count against the generator's output rather than deleting this.
check("names and defaults cover the same commands",
  eq(Object.keys(names).sort(), Object.keys(defaults).sort()));
check("the catalog is not truncated", Object.keys(names).length >= 250);
check("every catalog entry has a group in GROUP_ORDER",
  (Object.values(names) as { group: string }[]).every((e) => GROUP_ORDER.includes(e.group)));

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
