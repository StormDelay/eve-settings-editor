// Run: npm test (node --test). Throw-based checks, no framework.
import { stateLabel, EXCEPTION_STATES, DEFAULT_BACKGROUND_ORDER } from "./states.ts";

const check = (name: string, ok: boolean) => { if (!ok) throw new Error(`FAIL: ${name}`); console.log(`  ok - ${name}`); };

check("stateLabel resolves a known id", stateLabel(51) === "Pilot is a criminal");
check("stateLabel resolves a wreck state", stateLabel(37) === "Wreck is empty");
check("stateLabel returns null for the unrendered id 68", stateLabel(68) === null);
check("stateLabel returns null for an unknown id", stateLabel(9999) === null);
check("exception vocabulary includes both wreck states", EXCEPTION_STATES.includes(36) && EXCEPTION_STATES.includes(37));
check("exception vocabulary excludes the unrendered id 68", !EXCEPTION_STATES.includes(68));
check("the default background order carries the unrendered id 68", DEFAULT_BACKGROUND_ORDER.includes(68));
