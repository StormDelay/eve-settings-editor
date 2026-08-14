// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import { expect, test } from "vitest";

import { countIn, detailOn, setDetail, withoutIn } from "./prefs.svelte.ts";
import type { Preferences } from "./api.ts";

import { calls } from "$lib/test/setup";
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

// The ordering guarantee `persist` exists for, and the one thing no backend test
// can cover: `set_preferences` takes a whole snapshot with no sequence number,
// so the file holds whatever write COMPLETES last. Two rapid toggles fired
// independently would race, and a slower first command would leave the file one
// step behind the UI. `writeQueue` chaining each write after the previous
// settles is all that stops it.
test("two rapid toggles leave the file matching the UI, even when the first write is the slower one", async () => {
  // Stands in for the preferences file: last write to COMPLETE wins.
  let file: Preferences | undefined;
  // Call #1 finishes after call #2 would have. Unchained, the file ends up
  // holding the first toggle's state while the screen shows the second's.
  const delays = [30, 0];
  let n = 0;
  calls.stub("set_preferences", (args: Record<string, unknown> | undefined) => {
    const ms = delays[n++] ?? 0;
    return new Promise<void>((resolve) =>
      setTimeout(() => {
        file = args?.prefs as Preferences;
        resolve();
      }, ms),
    );
  });

  setDetail(true);
  setDetail(false);
  await new Promise((r) => setTimeout(r, 100));

  const sent = calls.of("set_preferences").map((c) => (c.args?.prefs as Preferences).layout.detail);
  expect(sent).toEqual([true, false]);
  expect(detailOn()).toBe(false);
  expect(file?.layout.detail).toBe(detailOn());
});

