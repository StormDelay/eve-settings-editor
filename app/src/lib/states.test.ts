// Run: npm test (node --test). Throw-based checks, no framework.
import { stateLabel, EXCEPTION_STATES, DEFAULT_BACKGROUND_ORDER, DEFAULT_BACKGROUND_STATES, DEFAULT_FLAG_STATES, exceptionOf, applyException, rgbaToHex, hexToRgba, moveInOrder, defaultColor } from "./states.ts";

const check = (name: string, ok: boolean) => { if (!ok) throw new Error(`FAIL: ${name}`); console.log(`  ok - ${name}`); };
const eq = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

check("stateLabel resolves a known id", stateLabel(51) === "Pilot is a criminal");
check("stateLabel resolves a wreck state", stateLabel(37) === "Wreck is empty");
check("stateLabel returns null for the unrendered id 68", stateLabel(68) === null);
check("stateLabel returns null for an unknown id", stateLabel(9999) === null);
check("exception vocabulary includes both wreck states", EXCEPTION_STATES.includes(36) && EXCEPTION_STATES.includes(37));
check("exception vocabulary excludes the unrendered id 68", !EXCEPTION_STATES.includes(68));
check("the default background order carries the unrendered id 68", DEFAULT_BACKGROUND_ORDER.includes(68));

check("a state in neither list shows normally", exceptionOf([9], [11], 13) === "show");
check("a state in filteredStates is hidden", exceptionOf([9], [11], 9) === "hide");
check("a state in alwaysShownStates is always shown", exceptionOf([9], [11], 11) === "always");

const toHide = applyException([], [11], 11, "hide");
check("choosing hide moves a state out of alwaysShown", eq(toHide.filtered, [11]) && eq(toHide.alwaysShown, []));

const toAlways = applyException([9], [], 9, "always");
check("choosing always moves a state out of filtered", eq(toAlways.filtered, []) && eq(toAlways.alwaysShown, [9]));

const toShow = applyException([9], [], 9, "show");
check("choosing show removes a state from both lists", eq(toShow.filtered, []) && eq(toShow.alwaysShown, []));

const others = applyException([9, 13], [11], 13, "show");
check("applying a choice leaves other states alone", eq(others.filtered, [9]) && eq(others.alwaysShown, [11]));

check("rgbaToHex converts EVE's 0..1 floats to a hex colour", rgbaToHex([1, 0.35, 0, 1]) === "#ff5900");
check("hexToRgba round-trips through rgbaToHex", rgbaToHex(hexToRgba("#ff5900", 1)) === "#ff5900");
check("hexToRgba preserves the alpha it is given", hexToRgba("#000000", 0.5)[3] === 0.5);

const moved = moveInOrder([13, 44, 9, 68], 0, 2);
check("moveInOrder reorders without dropping any id", eq(moved, [44, 9, 13, 68]) && moved.length === 4);
check("moveInOrder keeps an unrendered id in place", moveInOrder([13, 44, 68], 0, 1).includes(68));

check("defaultColor gives a harvested built-in colour", defaultColor(13) === "#bf0000");
check("defaultColor is null for the unrendered id 68", defaultColor(68) === null);
// EVE's own reset output: 66 is in the order arrays but off by default.
check("the default enabled set leaves the retribution timer off",
  !DEFAULT_BACKGROUND_STATES.includes(66) && !DEFAULT_FLAG_STATES.includes(66)
  && DEFAULT_BACKGROUND_STATES.includes(48) && DEFAULT_BACKGROUND_STATES.length === 21);
check("every rendered state has a harvested default",
  DEFAULT_BACKGROUND_ORDER.filter((id) => id !== 68).every((id) => /^#[0-9a-f]{6}$/.test(defaultColor(id) ?? "")));

console.log("states: all checks passed");
