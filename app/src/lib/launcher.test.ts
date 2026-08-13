// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import { proposalsByCard, acceptAllPairs } from "./launcher.ts";
import type { Proposal } from "./api.ts";
import { check, eq } from "./test/check.ts";

const plain = (char_id: number, user_id: number): Proposal => ({
  char_id,
  user_id,
  conflict: null,
});
const disputed = (char_id: number, user_id: number, conflict: number): Proposal => ({
  char_id,
  user_id,
  conflict,
});

const none: ReadonlySet<number> = new Set();

check(
  "a plain proposal is a ghost on the account the launcher names",
  eq(proposalsByCard([plain(90000001, 80000001)], none).get(80000001), {
    ghosts: [90000001],
    conflicts: [],
  }),
);

check(
  "a disputed proposal is shown on the card that currently holds the chip",
  eq(proposalsByCard([disputed(90000001, 80000001, 80000002)], none).get(80000002), {
    ghosts: [],
    conflicts: [{ charId: 90000001, target: 80000001 }],
  }),
);

check(
  "a disputed proposal puts nothing on the account the launcher names",
  proposalsByCard([disputed(90000001, 80000001, 80000002)], none).get(80000001) === undefined,
);

check(
  "several ghosts land on the same card in order",
  eq(
    proposalsByCard([plain(90000001, 80000001), plain(90000002, 80000001)], none).get(80000001)
      ?.ghosts,
    [90000001, 90000002],
  ),
);

check(
  "a dismissed character disappears from the cards",
  proposalsByCard([plain(90000001, 80000001)], new Set([90000001])).size === 0,
);

check(
  "acceptAllPairs takes every plain proposal as a char/user pair",
  eq(acceptAllPairs([plain(90000001, 80000001), plain(90000002, 80000002)], none), [
    [90000001, 80000001],
    [90000002, 80000002],
  ]),
);

check(
  "acceptAllPairs never includes a disputed proposal",
  acceptAllPairs([disputed(90000001, 80000001, 80000002)], none).length === 0,
);

check(
  "acceptAllPairs skips dismissed characters",
  acceptAllPairs([plain(90000001, 80000001)], new Set([90000001])).length === 0,
);

check(
  "a dismissed conflicting proposal clears from both cards",
  proposalsByCard([disputed(90000001, 80000001, 80000002)], new Set([90000001])).size === 0,
);

check(
  "conflicts land on a card in input order",
  eq(
    proposalsByCard([disputed(90000001, 80000001, 80000002), disputed(90000003, 80000004, 80000002)], none).get(80000002)
      ?.conflicts,
    [
      { charId: 90000001, target: 80000001 },
      { charId: 90000003, target: 80000004 },
    ],
  ),
);
