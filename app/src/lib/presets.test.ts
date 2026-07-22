// Run: npm test (node --test). Throw-based checks, no framework.
import { isDefaultKey, accountFormat, defaultsForFormat, mergePresetOptions, forkName, findDefault, type DefaultsBundle } from "./presets.ts";

const check = (name: string, ok: boolean) => { if (!ok) throw new Error(`FAIL: ${name}`); console.log(`  ok - ${name}`); };
const eq = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

const bundle: DefaultsBundle = {
  modern: [
    { key: "DefaultPreset_639443", name: "All", groups: [25, 26], filteredStates: [], alwaysShownStates: [] },
    { key: "DefaultPreset_639442", name: "Mining", groups: [462], filteredStates: [1], alwaysShownStates: [] },
  ],
  legacy: [ { key: "defaultall", name: "All", groups: [25], filteredStates: [], alwaysShownStates: [] } ],
};

check("isDefaultKey modern", isDefaultKey("DefaultPreset_639443"));
check("isDefaultKey legacy", isDefaultKey("defaultpvp"));
check("isDefaultKey user is false", !isDefaultKey("my pvp"));
check("isDefaultKey rejects a user preset that merely starts with 'default'", !isDefaultKey("Default Faves"));
check("isDefaultKey ignores Object.prototype keys (no 'in' prototype leak)", !isDefaultKey("constructor") && !isDefaultKey("__proto__"));

check("accountFormat modern from a DefaultPreset ref", accountFormat(["DefaultPreset_639452", "x"]) === "modern");
check("accountFormat legacy from a default* ref", accountFormat(["defaultall"]) === "legacy");
check("accountFormat defaults to modern with no default refs", accountFormat(["my pvp"]) === "modern");
check("accountFormat ignores a non-default 'Default Faves' ref", accountFormat(["Default Faves"]) === "modern");

check("defaultsForFormat picks the set", defaultsForFormat(bundle, "legacy")[0].key === "defaultall");

// merge: stored user presets + bundled defaults, deduped, split into defaults/user.
const merged = mergePresetOptions(["My PvP", "DefaultPreset_639443"], defaultsForFormat(bundle, "modern"));
check("merged defaults include both bundled defaults", eq(merged.defaults.sort(), ["DefaultPreset_639442", "DefaultPreset_639443"]));
check("merged user excludes the materialized default", eq(merged.user, ["My PvP"]));

check("forkName unique base", forkName("All", ["All copy"]) === "All copy 2");
check("forkName free base", forkName("All", []) === "All copy");

check("findDefault returns the profile", findDefault(defaultsForFormat(bundle, "modern"), "DefaultPreset_639442")!.groups[0] === 462);
check("findDefault miss", findDefault(defaultsForFormat(bundle, "modern"), "nope") === undefined);

console.log("presets: all checks passed");
