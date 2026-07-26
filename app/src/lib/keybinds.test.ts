// Run: npm test (node --test). Throw-based checks, no framework.
import { labelFor, groupFor, GROUP_ORDER, defaultFor } from "./keybinds.ts";

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
