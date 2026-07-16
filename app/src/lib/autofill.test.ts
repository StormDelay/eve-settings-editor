// Run: npm test  (node --test; Node strips the types). No test framework / no
// @types/node on purpose. A throw is the failing signal.
import { labelFor } from "./autofill.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

// Curated hit: a known People & Places search widget.
check(
  "curated widget gets its friendly name",
  labelFor("/addressbook/content/main/SearchPanel/Container/SingleLineEditText") ===
    "People & Places search",
);

// Curated hit via substring match (the needle appears mid-path).
check(
  "curated needle matches as a substring",
  labelFor("/inventory/content/main/quickFilter/SingleLineEditText") === "Quick Filter",
);

// Derived fallback: an UNCURATED widget must exercise derive() itself —
// strip boilerplate segments, split camelCase, title-case. (Must not match any
// curated needle, or it would never reach derive.)
check(
  "uncurated widget derives a readable label from camelCase",
  labelFor("/someWindow/content/main/mediumTimer/SingleLineEditText") === "Medium Timer",
);

// Never empty, even for a degenerate path.
check("empty-ish path never yields an empty label", labelFor("/") !== "");
check("raw-ish path with no useful segment falls back to the raw string",
  labelFor("///") === "///");

console.log("labelFor: all checks passed");
