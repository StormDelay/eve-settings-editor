// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import { parseTabName, formatTabName, plainTabName, cssColor, EVE_PALETTE, type TabName } from "./tabName.ts";

import { check, eq } from "./test/check.ts";

// Every one of these is a real name lifted from testdata/dumps.
{
  check(
    "parses a colour span",
    eq(parseTabName("<color=0xFFFFFFFF>  *  </color>"), { color: "FFFFFFFF", bold: false, text: "  *  " }),
  );
  check(
    "parses colour with bold nested inside the padding",
    eq(parseTabName("<color=0xFFFF6F75>   <b>main</b>   </color>"), { color: "FFFF6F75", bold: true, text: "   main   " }),
  );
  check(
    "parses bold with no colour",
    eq(parseTabName("<b> Exit! </b>"), { color: null, bold: true, text: " Exit! " }),
  );
  check(
    "parses an unmarked name",
    eq(parseTabName("  main  "), { color: null, bold: false, text: "  main  " }),
  );
}

check(
  "padding is kept verbatim — it is how a tab is widened in game",
  parseTabName("<color=0xFFFFE900>   3   </color>").text === "   3   ",
);

check("a lowercase colour normalises to uppercase", parseTabName("<color=0xffffba4e>x</color>").color === "FFFFBA4E");

// Anything outside `[colour][bold]text` must survive untouched rather than be
// half-understood and rewritten.
{
  const weird = "<color=0xFFFF0000>a</color><color=0xFF00FF00>b</color>";
  check("two spans fall through to raw text", eq(parseTabName(weird), { color: null, bold: false, text: weird }));

  const unknown = "<fontsize=11>big</fontsize>";
  check("an unknown tag falls through to raw text", eq(parseTabName(unknown), { color: null, bold: false, text: unknown }));
}

check("formatTabName wraps colour outside bold", formatTabName({ color: "FFFF6F75", bold: true, text: " m " }) === "<color=0xFFFF6F75><b> m </b></color>");
check("formatTabName with nothing set is the bare text", formatTabName({ color: null, bold: false, text: "main" }) === "main");

// Round-trip: the first format may re-nest the tags, but applying it again must
// change nothing — otherwise a name would drift on every edit.
{
  const names = [
    "<color=0xFFFFFFFF>  *  </color>",
    "<color=0xFFFF6F75>   <b>main</b>   </color>",
    "<b> Exit! </b>",
    "  main  ",
    "<fontsize=11>big</fontsize>",
  ];
  const once = names.map((n) => formatTabName(parseTabName(n)));
  const twice = once.map((n) => formatTabName(parseTabName(n)));
  check("format(parse(x)) is stable under repetition", eq(once, twice));
  check("the first pass keeps every name's readable text", eq(names.map(plainTabName), once.map(plainTabName)));
}

check("plainTabName strips the markup", plainTabName("<color=0xFFFF6F75>   <b>main</b>   </color>") === "   main   ");

check("EVE_PALETTE holds the 24 in-game colours", EVE_PALETTE.length === 24 && new Set(EVE_PALETTE).size === 24);
check("EVE_PALETTE entries are RRGGBB", EVE_PALETTE.every((c) => /^[0-9a-f]{6}$/.test(c)));

check("cssColor moves alpha to the end", cssColor("FFFF6F75") === "#FF6F75FF");

// A guard on the type export being usable from a component.
const sample: TabName = { color: null, bold: false, text: "x" };
check("TabName round-trips through format", formatTabName(sample) === "x");
