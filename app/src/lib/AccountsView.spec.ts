// Component test: the Accounts view's launcher-proposal rendering and the IPC it
// fires. `openPath` is null throughout so the profile scope is inert and every
// account card renders.
import { describe, expect, test } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import AccountsView from "$lib/AccountsView.svelte";
import { calls } from "$lib/test/setup";
import type { AccountRoster, Proposal } from "$lib/api";

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

function mount(proposals: Proposal[], roster: AccountRoster = ROSTER) {
  calls.stub("account_roster", roster);
  calls.stub("discover_profiles", []);
  calls.stub("resolve_character_names", NAMES);
  calls.stub("launcher_proposals", proposals);
  calls.stub("confirm_pairing", roster);
  calls.stub("confirm_pairings", roster);
  render(AccountsView, { openPath: null });
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
});
