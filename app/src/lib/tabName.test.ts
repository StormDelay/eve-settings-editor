// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
// Parsing and writing tab names is tested in Rust (tab_name.rs); these pin the
// CSS the pieces turn into.
import { cssColor, EVE_PALETTE, pieceStyle, plainTabName, samePieces } from "./tabName.ts";

import { check } from "./test/check.ts";

check("EVE_PALETTE holds the 24 in-game colours", EVE_PALETTE.length === 24 && new Set(EVE_PALETTE).size === 24);
check("EVE_PALETTE entries are RRGGBB", EVE_PALETTE.every((c) => /^[0-9a-f]{6}$/.test(c)));

check("cssColor moves alpha to the end", cssColor("FFFF6F75") === "#FF6F75FF");

check("an unstyled piece has no style", pieceStyle({ text: "x" }) === "");
check(
  "every authorable setting reaches the CSS",
  pieceStyle({ text: "x", color: "FF40FF40", bold: true, italic: true, underline: true, size: 21, spacing: 3 }) ===
    "color:#40FF40FF;font-weight:700;font-style:italic;text-decoration:underline;font-size:1.500em;letter-spacing:3px",
);
check("uppercase displays as capitals", pieceStyle({ text: "x", uppercase: true }) === "text-transform:uppercase");

// The client wraps the whole name in the tab's own colour key, so a piece's
// colour wins and an uncoloured piece takes the tab's.
check("the tab colour tints an uncoloured piece", pieceStyle({ text: "x" }, "FF7FFF1F") === "color:#7FFF1FFF");
check("a piece's own colour wins over the tab's", pieceStyle({ text: "x", color: "FFFF0000" }, "FF7FFF1F") === "color:#FF0000FF");

check(
  "plainTabName joins the drawn text and skips hidden pieces",
  plainTabName({ pieces: [{ text: "*", color: "FFA8C8E8" }, { text: "  main" }, { text: "tip", hidden: true }] }) === "*  main",
);

check("samePieces ignores unset flags", samePieces([{ text: "a", bold: false }], [{ text: "a" }]));
check("samePieces merges same-style neighbours", samePieces([{ text: "a" }, { text: "b" }, { text: "" }], [{ text: "ab" }]));
check("samePieces sees a style change", !samePieces([{ text: "a", bold: true }], [{ text: "a" }]));
check("samePieces sees a text change", !samePieces([{ text: "a" }], [{ text: "b" }]));
