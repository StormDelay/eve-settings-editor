// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import { countIn, withoutIn } from "./prefs.svelte.ts";

import { check } from "./test/check.ts";

// Counting is about the document you are looking at, not the preferences file.
{
  const stored = { clutter: ["market", "chatchannel_corp"], visible: ["overview"], detail: false, targets: 4, effects: 2 };
  const open = new Set(["market", "overview", "somethingElse"]);
  check("counts only overrides naming a window this document has", countIn(stored, open) === 2);
  check(
    "a document sharing no windows with the overrides counts none",
    countIn(stored, new Set(["unrelated"])) === 0,
  );
}

// The data-loss half: clearing must not touch another character's overrides.
{
  const stored = { clutter: ["market", "chatchannel_corp"], visible: ["overview"], detail: false, targets: 4, effects: 2 };
  const next = withoutIn(stored, new Set(["market", "overview"]));
  check("clearing drops the in-scope clutter override", !next.clutter.includes("market"));
  check("clearing drops the in-scope visible override", !next.visible.includes("overview"));
  check(
    "clearing KEEPS an override for a window this document does not have",
    next.clutter.includes("chatchannel_corp"),
  );
}

// The view-only fields on LayoutPrefs must survive the helpers that rebuild
// the object. Both `withoutIn` and `setClutterOverride` construct a fresh
// literal; without a spread, clearing overrides would silently turn Detail off
// or reset a count the user had chosen.
{
  const stored = { clutter: ["a", "b"], visible: ["c"], detail: true, targets: 7, effects: 5 };
  const out = withoutIn(stored, new Set(["a"]));
  check("withoutIn drops only the in-scope ids", out.clutter.join(",") === "b");
  check("withoutIn preserves the detail flag", out.detail === true);
  check("withoutIn preserves the target count", out.targets === 7);
  check("withoutIn preserves the effect count", out.effects === 5);
}

