// Run: npm test (node --test; Node strips the types). Throw-based checks, no
// framework — matching layout.test.ts and search.test.ts.
import {
  associatedCharacters,
  accountOf,
  pairedFilePath,
  userSlotFor,
  charSlotFor,
  sharedWith,
} from "./overview.ts";
import type { AccountRoster, Profile } from "./api.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

const roster: AccountRoster = {
  accounts: [{ user_id: 456, alias: "Main", characters: [123, 124] }],
  unassigned: [999],
};

check(
  "associatedCharacters returns the account's characters",
  associatedCharacters(456, roster).join(",") === "123,124",
);
check(
  "associatedCharacters returns empty for an unknown user",
  associatedCharacters(789, roster).length === 0,
);

check("accountOf finds the account holding a character", accountOf(123, roster) === 456);
check("accountOf returns null for an unassigned character", accountOf(999, roster) === null);

const profiles: Profile[] = [
  {
    install: "i",
    server: "tq",
    profile: "Default",
    dir: "/eve/settings_Default",
    files: [
      {
        path: "/eve/settings_Default/core_char_123.dat",
        file_name: "core_char_123.dat",
        kind: "char",
        id: 123,
        size: 1,
        modified_unix: 1,
      },
      {
        path: "/eve/settings_Default/core_user_456.dat",
        file_name: "core_user_456.dat",
        kind: "user",
        id: 456,
        size: 1,
        modified_unix: 1,
      },
    ],
  },
];

const anchor = "/eve/settings_Default/core_char_123.dat";
check(
  "pairedFilePath finds a file by id+kind in the anchor's folder",
  pairedFilePath(profiles, anchor, 456, "user") === "/eve/settings_Default/core_user_456.dat",
);
check(
  "pairedFilePath finds itself when id+kind match the anchor",
  pairedFilePath(profiles, anchor, 123, "char") === anchor,
);
check(
  "pairedFilePath returns null when there is no match in the folder",
  pairedFilePath(profiles, anchor, 777, "user") === null,
);

// --- slot reconciliation: the other slot becomes the correct pair or is emptied ---
const userFile = "/eve/settings_Default/core_user_456.dat";

check(
  "userSlotFor loads the paired account file when opening a paired character",
  JSON.stringify(userSlotFor(anchor, 123, null, roster, profiles)) ===
    JSON.stringify({ kind: "load", path: userFile }),
);
check(
  "userSlotFor keeps the account file when it is already the right one",
  userSlotFor(anchor, 123, userFile, roster, profiles).kind === "keep",
);
check(
  "userSlotFor clears a stale account file when the character is unpaired",
  userSlotFor(anchor, 999, userFile, roster, profiles).kind === "clear",
);
check(
  "userSlotFor keeps (nothing to do) when unpaired and the user slot is empty",
  userSlotFor(anchor, 999, null, roster, profiles).kind === "keep",
);
check(
  "charSlotFor keeps a character that belongs to the opened account",
  charSlotFor(456, 123, roster).kind === "keep",
);
check(
  "charSlotFor clears a character that does not belong to the opened account",
  charSlotFor(789, 123, roster).kind === "clear",
);
check(
  "charSlotFor keeps (nothing to do) when the char slot is empty",
  charSlotFor(456, null, roster).kind === "keep",
);

check(
  "sharedWith lists the account's OTHER characters by name",
  sharedWith(456, 123, roster, (id) => (id === 124 ? "Bravo" : String(id))).join(",") === "Bravo",
);
check(
  "sharedWith returns empty when the character has no known account",
  sharedWith(null, 123, roster, String).length === 0,
);
check(
  "sharedWith excludes only the current character",
  sharedWith(456, 999, roster, (id) => (id === 123 ? "A" : id === 124 ? "B" : String(id))).join(",") === "A,B",
);

console.log("overview: all checks passed");
