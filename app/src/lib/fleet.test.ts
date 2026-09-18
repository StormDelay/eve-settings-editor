// Pure-module tests. See test/README.md.
import { check } from "./test/check.ts";
import { BROADCASTS, SHOW_OWN, listenField } from "./fleet.ts";

check("sixteen types in EVE's row order", BROADCASTS.length === 16 && BROADCASTS[0].type === "HealArmor" && BROADCASTS[15].type === "Location");
check("a listen field is the type token verbatim", listenField("HealArmor") === "listen_HealArmor");
check("the top checkbox has its own field", SHOW_OWN.field === "listen_show_own");
// R1: sentence case — one leading capital, nothing capitalised after it.
check(
  "labels are sentence case",
  [SHOW_OWN.label, ...BROADCASTS.map((b) => b.label)].every(
    (l) => l[0] === l[0].toUpperCase() && l.slice(1).split(" ").every((w) => w === "" || w[0] === w[0].toLowerCase()),
  ),
);
check("no label says Broadcast:", BROADCASTS.every((b) => !b.label.startsWith("Broadcast")));
