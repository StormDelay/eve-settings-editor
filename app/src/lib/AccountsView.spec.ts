// Component test: the Accounts view's launcher-proposal rendering and the IPC it
// fires. `openPath` is null throughout so the profile scope is inert and every
// account card renders.
import { describe, expect, test } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import AccountsView from "$lib/AccountsView.svelte";
import { calls } from "$lib/test/setup";
import type { AccountRoster, Profile, Proposal, Rejected } from "$lib/api";

const ROSTER: AccountRoster = {
  accounts: [
    { user_id: 80000001, alias: "Main", characters: [] },
    { user_id: 80000002, alias: null, characters: [90000009] },
  ],
  unassigned: [90000001, 90000009],
};

// Shape must match `ResolvedName` in api.ts — `{ name, category }`, not `source`.
const NAMES = {
  90000001: { name: "Alpha", category: "character" },
  90000009: { name: "Zulu", category: "character" },
};

function mount(
  proposals: Proposal[],
  opts: {
    roster?: AccountRoster;
    rejected?: Rejected[];
    profiles?: Profile[];
    openPath?: string | null;
  } = {},
) {
  const roster = opts.roster ?? ROSTER;
  calls.stub("account_roster", roster);
  calls.stub("discover_profiles", opts.profiles ?? []);
  calls.stub("resolve_character_names", NAMES);
  calls.stub("launcher_proposals", proposals);
  calls.stub("confirm_pairing", roster);
  calls.stub("confirm_pairings", { roster, rejected: opts.rejected ?? [] });
  render(AccountsView, { openPath: opts.openPath ?? null });
}

describe("launcher proposals", () => {
  test("an undisputed proposal offers the character on the named account", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    const accept = await waitFor(() => screen.getByRole("button", { name: /accept Alpha/i }));
    await fireEvent.click(accept);
    await waitFor(() => expect(calls.only("confirm_pairing").args).toEqual({
      charId: 90000001,
      userId: 80000001,
    }));
  });

  test("accept all sends every undisputed pair in one call", async () => {
    mount([
      { char_id: 90000001, user_id: 80000001, conflict: null },
      { char_id: 90000009, user_id: 80000001, conflict: 80000002 },
    ]);
    const all = await waitFor(() => screen.getByRole("button", { name: /accept all/i }));
    await fireEvent.click(all);
    await waitFor(() =>
      expect(calls.only("confirm_pairings").args).toEqual({ pairs: [[90000001, 80000001]] }),
    );
  });

  test("accept all drops exactly the characters it sent", async () => {
    mount([
      { char_id: 90000001, user_id: 80000001, conflict: null },
      { char_id: 90000009, user_id: 80000001, conflict: 80000002 },
    ]);
    const all = await waitFor(() => screen.getByRole("button", { name: /accept all/i }));
    await fireEvent.click(all);
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /accept Alpha/i })).toBeNull(),
    );
  });

  test("a disputed character is flagged on the card that holds it, naming the target", async () => {
    mount([{ char_id: 90000009, user_id: 80000001, conflict: 80000002 }]);
    const warning = await waitFor(() => screen.getByText(/launcher log puts Zulu on Main/i));
    expect(warning).toBeTruthy();
  });

  test("move it repairs the pairing to the account the launcher names", async () => {
    mount([{ char_id: 90000009, user_id: 80000001, conflict: 80000002 }]);
    const move = await waitFor(() => screen.getByRole("button", { name: /move Zulu/i }));
    await fireEvent.click(move);
    await waitFor(() => expect(calls.only("confirm_pairing").args).toEqual({
      charId: 90000009,
      userId: 80000001,
    }));
  });

  test("move it drops its proposal from the list", async () => {
    mount([{ char_id: 90000009, user_id: 80000001, conflict: 80000002 }]);
    const move = await waitFor(() => screen.getByRole("button", { name: /move Zulu/i }));
    await fireEvent.click(move);
    await waitFor(() =>
      expect(screen.queryByText(/launcher log puts Zulu/i)).toBeNull(),
    );
  });

  test("keep mine drops the warning and writes nothing", async () => {
    mount([{ char_id: 90000009, user_id: 80000001, conflict: 80000002 }]);
    const keep = await waitFor(() => screen.getByRole("button", { name: /keep Zulu/i }));
    await fireEvent.click(keep);
    await waitFor(() =>
      expect(screen.queryByText(/launcher log puts Zulu/i)).toBeNull(),
    );
    calls.never("confirm_pairing");
    calls.never("confirm_pairings");
  });

  test("an accepted ghost leaves the list instead of reappearing in the next slot", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    const accept = await waitFor(() => screen.getByRole("button", { name: /accept Alpha/i }));
    await fireEvent.click(accept);
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /accept Alpha/i })).toBeNull(),
    );
  });

  test("with no proposals there is no accept-all button, and the view says why", async () => {
    mount([]);
    await waitFor(() => screen.getByText(/launcher logs say nothing/i));
    expect(screen.queryByRole("button", { name: /accept all/i })).toBeNull();
  });

  test("after accepting everything the view does not claim the logs said nothing", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    const all = await waitFor(() => screen.getByRole("button", { name: /accept all/i }));
    await fireEvent.click(all);
    await waitFor(() => expect(screen.queryByRole("button", { name: /accept all/i })).toBeNull());
    expect(screen.queryByText(/launcher logs say nothing/i)).toBeNull();
  });

  test("a rejected pair names the character and the account, and keeps its ghost", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }], {
      rejected: [
        { char_id: 90000001, user_id: 80000001, reason: "Account already has 3 characters" },
      ],
    });
    const all = await waitFor(() => screen.getByRole("button", { name: /accept all/i }));
    await fireEvent.click(all);
    await waitFor(() => screen.getByText(/Alpha could not join Main/i));
    // Not claimed as applied: the ghost stays, so the user can retry after
    // unpairing something on that account.
    expect(screen.queryByRole("button", { name: /accept Alpha/i })).not.toBeNull();
  });

  test("calibrating a character the launcher also proposed drops its ghost", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    calls.stub("begin_capture", undefined);
    calls.stub("resolve_capture", {
      changed_chars: [90000001],
      changed_users: [80000001],
      detected: [90000001, 80000001],
    });
    await waitFor(() => screen.getByRole("button", { name: /accept Alpha/i }));
    await fireEvent.click(screen.getByRole("button", { name: /calibrate/i }));
    await fireEvent.click(await waitFor(() => screen.getByRole("button", { name: "Done" })));
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /accept Alpha/i })).toBeNull(),
    );
  });
});

// The cards are scoped to the profile folder the open file lives in. An accept
// action that reads the unscoped proposal list writes pairings for accounts the
// user has no card for and never saw a ghost for.
describe("accept all is scoped to the visible cards", () => {
  const file = (path: string, kind: "char" | "user", id: number) => ({
    path,
    file_name: path,
    kind,
    id,
    size: 1,
    modified_unix: 1,
  });
  const OPEN = "core_char_90000001.dat";
  const PROFILES: Profile[] = [
    {
      install: "i",
      server: "tq",
      profile: "Default",
      dir: "d",
      files: [file(OPEN, "char", 90000001), file("core_user_80000001.dat", "user", 80000001)],
    },
  ];

  test("a proposal for an off-screen account is neither counted nor sent", async () => {
    mount(
      [
        { char_id: 90000001, user_id: 80000001, conflict: null },
        { char_id: 90000002, user_id: 80000002, conflict: null }, // no card in this folder
      ],
      { profiles: PROFILES, openPath: OPEN },
    );
    const all = await waitFor(() => screen.getByRole("button", { name: /accept all — 1 /i }));
    await fireEvent.click(all);
    await waitFor(() =>
      expect(calls.only("confirm_pairings").args).toEqual({ pairs: [[90000001, 80000001]] }),
    );
  });
});
