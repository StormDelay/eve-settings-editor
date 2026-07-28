// Run: npm test (node --test). No framework, no @types/node — a throw is a
// failing exit code, which is all a runner needs.
import { countIn, withoutIn } from "./prefs.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

// Counting is about the document you are looking at, not the preferences file.
{
  const stored = { clutter: ["market", "chatchannel_corp"], visible: ["overview"] };
  const open = new Set(["market", "overview", "somethingElse"]);
  check("counts only overrides naming a window this document has", countIn(stored, open) === 2);
  check(
    "a document sharing no windows with the overrides counts none",
    countIn(stored, new Set(["unrelated"])) === 0,
  );
}

// The data-loss half: clearing must not touch another character's overrides.
{
  const stored = { clutter: ["market", "chatchannel_corp"], visible: ["overview"] };
  const next = withoutIn(stored, new Set(["market", "overview"]));
  check("clearing drops the in-scope clutter override", !next.clutter.includes("market"));
  check("clearing drops the in-scope visible override", !next.visible.includes("overview"));
  check(
    "clearing KEEPS an override for a window this document does not have",
    next.clutter.includes("chatchannel_corp"),
  );
}

console.log("prefs.test.ts: all checks passed");
