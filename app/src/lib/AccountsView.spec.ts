// Component test: the Accounts view's launcher-proposal rendering and the IPC it
// fires. `openPath` is null throughout so the profile scope is inert and every
// account card renders.
import { describe, expect, test } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/svelte";
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
    onClose?: () => void;
  } = {},
) {
  const roster = opts.roster ?? ROSTER;
  calls.stub("account_roster", roster);
  calls.stub("discover_profiles", opts.profiles ?? []);
  calls.stub("resolve_character_names", NAMES);
  calls.stub("launcher_proposals", proposals);
  calls.stub("confirm_pairing", roster);
  calls.stub("confirm_pairings", { roster, rejected: opts.rejected ?? [] });
  render(AccountsView, { props: { openPath: opts.openPath ?? null, onClose: opts.onClose ?? (() => {}) } });
}

/**
 * The bulk-accept button, whatever it is called at this count.
 *
 * Its label is `Accept` at one pair and `Accept all` beyond, because "all" of
 * one is the same overclaim the character count was. The tests below are about
 * the IPC payload, the pruning and the rejection copy — not about the label —
 * so they match either. The anchors matter: a ghost's own accept is named
 * "Accept Alpha" and must not be caught here.
 */
const BULK = /^accept( all)?$/i;
const bulkAccept = () => screen.getByRole("button", { name: BULK });
const bulkAcceptOrNull = () => screen.queryByRole("button", { name: BULK });

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
    const all = await waitFor(() => bulkAccept());
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
    const all = await waitFor(() => bulkAccept());
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

  test("Move to repairs the pairing to the account the launcher names", async () => {
    mount([{ char_id: 90000009, user_id: 80000001, conflict: 80000002 }]);
    const move = await waitFor(() => screen.getByRole("button", { name: /^Move to /i }));
    await fireEvent.click(move);
    await waitFor(() => expect(calls.only("confirm_pairing").args).toEqual({
      charId: 90000009,
      userId: 80000001,
    }));
  });

  test("Move to drops its proposal from the list", async () => {
    mount([{ char_id: 90000009, user_id: 80000001, conflict: 80000002 }]);
    const move = await waitFor(() => screen.getByRole("button", { name: /^Move to /i }));
    await fireEvent.click(move);
    await waitFor(() =>
      expect(screen.queryByText(/launcher log puts Zulu/i)).toBeNull(),
    );
  });

  test("Keep here drops the warning and writes nothing", async () => {
    mount([{ char_id: 90000009, user_id: 80000001, conflict: 80000002 }]);
    const keep = await waitFor(() => screen.getByRole("button", { name: /^Keep here$/i }));
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
    expect(bulkAcceptOrNull()).toBeNull();
  });

  test("after accepting everything the view does not claim the logs said nothing", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    const all = await waitFor(() => bulkAccept());
    await fireEvent.click(all);
    await waitFor(() => expect(bulkAcceptOrNull()).toBeNull());
    expect(screen.queryByText(/launcher logs say nothing/i)).toBeNull();
  });

  test("a rejected pair names the character and the account, and keeps its ghost", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }], {
      rejected: [
        { char_id: 90000001, user_id: 80000001, reason: "Account already has 3 characters" },
      ],
    });
    const all = await waitFor(() => bulkAccept());
    await fireEvent.click(all);
    await waitFor(() => screen.getByText(/Alpha wasn.t paired with Main/i));
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
    // REWRITTEN (§4.8). This used to match `/^accept all — 1 character$/i` and
    // so pinned the count's pluralisation. The count is gone: it was never an
    // answer to "which characters am I about to assign?", on the panel's most
    // consequential button. The two halves are asserted separately now — the
    // sentence names the on-screen character and not the off-screen one, and
    // the button drops "all" at exactly one pair. The `confirm_pairings`
    // payload assertion below is untouched, because the scoping rule this test
    // was really written to protect has not changed.
    const message = await waitFor(() => screen.getByText(/Your launcher log pairs Alpha\./));
    expect(message.textContent).not.toMatch(/90000002|Bravo/);
    const all = screen.getByRole("button", { name: /^accept$/i });
    await fireEvent.click(all);
    await waitFor(() =>
      expect(calls.only("confirm_pairings").args).toEqual({ pairs: [[90000001, 80000001]] }),
    );
  });

  test("proposals only for off-screen accounts still explain the empty state", async () => {
    // Nothing renders for them — no card, no ghost, no Accept all — so counting
    // them as "the logs found something" leaves a panel that says nothing at all.
    mount([{ char_id: 90000002, user_id: 80000002, conflict: null }], {
      profiles: PROFILES,
      openPath: OPEN,
    });
    await waitFor(() => screen.getByText(/launcher logs say nothing/i));
    expect(bulkAcceptOrNull()).toBeNull();
  });
});

// The owner's live-test finding, pinned: "which characters are being assigned is
// not very visible". A count is not an answer to that question.
describe("the bulk accept names what it will pair", () => {
  test("the sentence names the characters, and carries no count", async () => {
    mount([
      { char_id: 90000001, user_id: 80000001, conflict: null },
      { char_id: 90000009, user_id: 80000001, conflict: null },
    ]);
    const message = await waitFor(() => screen.getByText(/Your launcher log pairs/));
    expect(message.textContent).toMatch(/Alpha/);
    expect(message.textContent).toMatch(/Zulu/);
    // "2 characters" is exactly what this replaces.
    expect(message.textContent).not.toMatch(/\d/);
    expect(bulkAccept().textContent?.trim()).toBe("Accept all");
  });

  test("one proposal says Accept, not Accept all", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    await waitFor(() => screen.getByText(/Your launcher log pairs Alpha\./));
    expect(bulkAccept().textContent?.trim()).toBe("Accept");
  });

  /**
   * A proposal is a different KIND of thing from a pairing, not a weaker one.
   *
   * jsdom computes no colours, so this pins the mechanism rather than the hue —
   * a distinct state, and no opacity anywhere in the card. Dimming the one
   * element asking for a decision is precisely what was reported as invisible,
   * and it is the antipattern the token phase retired.
   */
  test("a proposed chip is a state of its own, not a dimmed confirmed one", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    const accept = await waitFor(() => screen.getByRole("button", { name: /accept Alpha/i }));
    const chip = accept.closest(".chip");
    expect(chip?.classList.contains("proposed")).toBe(true);
    expect(chip?.classList.contains("info")).toBe(true);
    for (const el of document.querySelectorAll<HTMLElement>(".chip, .chip *")) {
      expect(el.style.opacity).toBe("");
    }
  });

  // Accept writes to the store; Dismiss is session-only and undone by reopening
  // the app. Two bare glyphs at equal weight said they were the same kind of
  // action. The accessible name is unchanged, so the five tests querying
  // /accept Alpha/i keep passing untouched.
  test("the ghost's accept is a labelled button, still named Accept {character}", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    const accept = await waitFor(() => screen.getByRole("button", { name: /accept Alpha/i }));
    expect(accept.textContent?.trim()).toBe("Accept");
    expect(screen.getByRole("button", { name: /dismiss Alpha/i }).textContent?.trim()).toBe("✕");
  });
});

// Everything here exists because the panel is a DISMISSABLE sheet now. Before
// that, "once per mount" and "once per session" were the same thing.
describe("state that has to outlive a dismissal", () => {
  const CAPTURE_STUBS = () => {
    calls.stub("begin_capture", undefined);
    calls.stub("clear_capture", undefined);
  };

  test("a capture survives the sheet being dismissed and reopened", async () => {
    mount([]);
    CAPTURE_STUBS();
    await fireEvent.click(await waitFor(() => screen.getByRole("button", { name: /calibrate/i })));
    await waitFor(() => screen.getByRole("button", { name: "Done" }));

    cleanup(); // the sheet is dismissed
    mount([]);

    // Still in flight, and the expensive half — the backend baseline — was never
    // re-taken. A second `begin_capture` would snapshot the files as they are
    // AFTER EVE's write, and the detection would then find nothing, silently.
    await waitFor(() => expect(screen.getByRole("button", { name: "Done" })).toBeTruthy());
    expect(calls.of("begin_capture").length).toBe(1);
    // And dismissing is not an ending, so it discards nothing.
    calls.never("clear_capture");
  });

  test("Calibrate cannot re-baseline a capture already in flight", async () => {
    mount([]);
    CAPTURE_STUBS();
    const calibrate = await waitFor(() => screen.getByRole("button", { name: /calibrate/i }));
    await fireEvent.click(calibrate);
    await waitFor(() => screen.getByRole("button", { name: "Done" }));
    await fireEvent.click(calibrate);
    expect(calls.of("begin_capture").length).toBe(1);
  });

  test("Cancel discards the backend baseline", async () => {
    mount([]);
    CAPTURE_STUBS();
    await fireEvent.click(await waitFor(() => screen.getByRole("button", { name: /calibrate/i })));
    await fireEvent.click(await waitFor(() => screen.getByRole("button", { name: "Cancel" })));
    await waitFor(() => expect(calls.of("clear_capture").length).toBe(1));
    expect(screen.queryByRole("button", { name: "Done" })).toBeNull();
  });

  test("Done pairs the detected character, clears the wizard, and names the account by alias", async () => {
    mount([]);
    CAPTURE_STUBS();
    calls.stub("resolve_capture", {
      changed_chars: [90000001],
      changed_users: [80000001],
      detected: [90000001, 80000001],
    });
    await fireEvent.click(await waitFor(() => screen.getByRole("button", { name: /calibrate/i })));
    await fireEvent.click(await waitFor(() => screen.getByRole("button", { name: "Done" })));

    await waitFor(() => expect(calls.of("confirm_pairing").length).toBe(1));
    await waitFor(() => expect(calls.of("clear_capture").length).toBe(1));
    // The account is named by ALIAS. "account 80000001" is a raw `core_user`
    // number and does not tell the user which account they just paired — the
    // same reason the rejection copy names it.
    const note = await waitFor(() => screen.getByText(/ with Main\./));
    expect(note.textContent).not.toMatch(/80000001/);
    expect(screen.queryByRole("button", { name: "Done" })).toBeNull();
  });

  test("a retryable capture keeps its baseline", async () => {
    mount([]);
    CAPTURE_STUBS();
    calls.stub("resolve_capture", {
      changed_chars: [],
      changed_users: [80000001, 80000002],
      detected: null,
    });
    await fireEvent.click(await waitFor(() => screen.getByRole("button", { name: /calibrate/i })));
    await fireEvent.click(await waitFor(() => screen.getByRole("button", { name: "Done" })));

    // The message asks for a retry, and the retry diffs against this baseline.
    await waitFor(() => screen.getByText(/Several account files changed/));
    expect(screen.getByRole("button", { name: "Done" })).toBeTruthy();
    calls.never("clear_capture");
  });

  test("a dismissed ghost stays dismissed across a sheet round-trip", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    await fireEvent.click(
      await waitFor(() => screen.getByRole("button", { name: /dismiss Alpha/i })),
    );
    await waitFor(() => expect(screen.queryByRole("button", { name: /accept Alpha/i })).toBeNull());

    cleanup();
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    await waitFor(() => expect(screen.getByRole("heading", { name: "Accounts" })).toBeTruthy());

    // Session-only is right; a dismissal of the SHEET is not the end of the
    // session, and re-dismissing the same ghosts every time is the cost.
    expect(screen.queryByRole("button", { name: /accept Alpha/i })).toBeNull();
    // And the logs are parsed once per session, not once per mount.
    expect(calls.of("launcher_proposals").length).toBe(1);
  });

  test("reopening the sheet does not claim the logs said nothing", async () => {
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);
    await fireEvent.click(await waitFor(() => bulkAccept()));
    await waitFor(() => expect(bulkAcceptOrNull()).toBeNull());

    cleanup();
    mount([{ char_id: 90000001, user_id: 80000001, conflict: null }]);

    // `foundCards` is recorded at load and never pruned, precisely because
    // `proposals` empties as they are accepted. Recomputing it on a remount is
    // what makes the panel call its own logs liars.
    await waitFor(() => expect(screen.getByRole("heading", { name: "Accounts" })).toBeTruthy());
    expect(screen.queryByText(/launcher logs say nothing/i)).toBeNull();
  });
});

describe("the sheet frame", () => {
  test("is a labelled dialog with Refresh and Calibrate in its header", async () => {
    mount([]);
    const dialog = await waitFor(() => screen.getByRole("dialog", { name: "Accounts" }));
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(screen.getByRole("button", { name: "Refresh accounts" })).toBeTruthy();
    expect(screen.getByRole("button", { name: /calibrate/i })).toBeTruthy();
    // The capture panel's own role="dialog" is gone: inside a sheet that was a
    // dialog within a dialog.
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
  });

  test("closes on Escape and on the close button", async () => {
    let closed = 0;
    mount([], { onClose: () => (closed += 1) });
    await waitFor(() => screen.getByRole("dialog", { name: "Accounts" }));

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(closed).toBe(1);

    await fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(closed).toBe(2);
  });
});
