// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// The sidebar is a subject browser and nothing else. Two properties are worth
// more than the rest and both are pinned below: the list is FLAT and in
// resolved-name order, and an account chip means a CONFIRMED pairing — never a
// launcher proposal. Since 2026-08-14 the chip gives up its width before the
// character name does: the chip was `nowrap`, so a shared name prefix left four
// rows all reading "Storm Holde…". The account is on the row's tooltip too, for
// when the chip is down to a stub.
//
// The empty-state cases stay: a profile with no character file used to render
// no header at all, so when every profile was in that state the sidebar came up
// blank with nothing saying why.
import { describe, expect, test } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import Sidebar from "$lib/Sidebar.svelte";
import { calls } from "$lib/test/setup";
import { rescanProfiles, subject } from "$lib/subject.svelte";
import type { AccountRoster, Profile } from "$lib/api";

const DIR = "C:/eve/settings_Default";
const DIR2 = "C:/eve/settings_Alt";

const file = (name: string, kind: "char" | "user", id: number | null, dir = DIR) => ({
  path: `${dir}/${name}`,
  file_name: name,
  kind,
  id,
  size: 1000,
  modified_unix: 0,
});

const profile = (files: ReturnType<typeof file>[], dir = DIR, label = "Default"): Profile => ({
  install: "eve",
  server: "tranquility",
  profile: label,
  dir,
  files,
});

/**
 * The scan belongs to the shell now — the sidebar reads `subject`, and used to
 * run a second `api.discover()` of its own beside the one `+page.svelte` was
 * already making. So a sidebar test seeds the store the same way the shell
 * does, and `resetSubject()` in the shared `afterEach` unseeds it.
 */
async function mount(
  profiles: Profile[],
  roster: AccountRoster = { accounts: [], unassigned: [] },
  names: Record<number, { name: string; category: string }> = {},
) {
  calls.stub("discover_profiles", profiles);
  calls.stub("account_roster", roster);
  // Stubbed here, not by the caller: `rescanProfiles` resolves names as part of
  // the scan, so a stub set after it has already missed.
  calls.stub("resolve_character_names", names);
  calls.stub("settings_preset_list", []);
  await rescanProfiles();
  render(Sidebar, {
    onOpen: () => {},
    onPickFile: () => {},
    onCollapse: () => {},
    onOpenPreset: () => {},
    onShowAccounts: () => {},
  });
}

/** The character rows, in the order the sidebar renders them. */
const rowNames = () =>
  screen
    .getAllByRole("listitem")
    .map((li) => li.querySelector("button.label")?.textContent?.trim())
    .filter((t): t is string => !!t);

describe("when nothing in the sidebar can be listed", () => {
  test("a profile holding only account files says where to go instead", async () => {
    await mount([profile([file("core_user_80000001.dat", "user", 80000001)])]);
    const hint = await waitFor(() => screen.getByText(/no character files/i));
    // "Open file…" is also a button, so assert on the hint's own text.
    expect(hint.textContent).toMatch(/Open file/);
  });

  test("the hint names the filter when the filter is what hid them", async () => {
    // A non-standard name is hidden by the default toggle, so the profile lists
    // nothing — but the fix here is to untick the box, not to open a file.
    await mount([profile([file("core_char_90000001.dat.bak", "char", 90000001)])]);
    await waitFor(() => expect(screen.getByText(/Untick/i)).toBeTruthy());
  });

  test("unticking the filter reveals the file and drops the hint", async () => {
    await mount([profile([file("core_char_90000001.dat.bak", "char", 90000001)])]);
    await waitFor(() => expect(screen.getByText(/Untick/i)).toBeTruthy());
    const toggle = screen.getByRole("checkbox", { name: /hide non-standard/i });
    await fireEvent.click(toggle);
    await waitFor(() => expect(screen.queryByText(/Untick/i)).toBeNull());
  });

  test("a profile with a character file shows no hint at all", async () => {
    await mount([profile([file("core_char_90000001.dat", "char", 90000001)])]);
    // A regex, not the bare string: the row's tooltip is now
    // "<file name> · <size> KB" — the per-row KB moved off the row and into the
    // tooltip, beside the file name it belongs to.
    await waitFor(() => expect(screen.getByTitle(/core_char_90000001\.dat/)).toBeTruthy());
    expect(screen.queryByText(/no character files/i)).toBeNull();
    expect(screen.queryByText(/No EVE profiles found/i)).toBeNull();
  });
});

describe("the character list", () => {
  const three = [
    file("core_char_950.dat", "char", 950),
    file("core_char_951.dat", "char", 951),
    file("core_char_952.dat", "char", 952),
  ];

  // THE assertion that fails if anyone reintroduces a grouping level. One flat
  // list, alphabetical by resolved name, with files still showing a bare id
  // after the named ones. Account grouping was proposed and rejected: browsing
  // is this column's whole job and grouping makes a character harder to find.
  test("is flat and in resolved-name order, bare ids last", async () => {
    await mount([profile(three)], undefined, {
      950: { name: "Zed", category: "character" },
      951: { name: "Alpha", category: "character" },
    });
    await waitFor(() => expect(rowNames()).toContain("Alpha"));
    expect(rowNames()).toEqual(["Alpha", "Zed", "core_char_952.dat"]);
  });

  test("a confirmed pairing draws an account chip; an unpaired character draws none and offers Link…", async () => {
    await mount([profile([file("core_char_950.dat", "char", 950), file("core_char_951.dat", "char", 951)])], {
      accounts: [{ user_id: 140, alias: "stormdelay2", characters: [950] }],
      unassigned: [951],
    });
    await waitFor(() => expect(screen.getByText("stormdelay2")).toBeTruthy());
    // One chip across the whole list: 950 has one, 951 does not.
    expect(screen.getAllByText("stormdelay2")).toHaveLength(1);
    expect(screen.getAllByRole("button", { name: "Link…" })).toHaveLength(1);
  });

  // The chip ellipsises before the character name does, so the account is also
  // on the row itself — that is what you hover once the chip is a stub, and it
  // is why the name is allowed to win the row.
  test("the row's tooltip names the account in full", async () => {
    await mount([profile([file("core_char_950.dat", "char", 950)])], {
      accounts: [{ user_id: 140, alias: "stormdelay2", characters: [950] }],
      unassigned: [],
    });
    const paired = await waitFor(() => {
      const found = document.querySelectorAll('[title*="account stormdelay2"]');
      expect(found).toHaveLength(1);
      return found[0] as HTMLElement;
    });
    expect(paired.title).toContain("core_char_950.dat");
  });

  /**
   * §5.7.1, and the assertion that fails if anyone adds a third chip state.
   *
   * v0.34 gave a character a third pairing state — proposed, claimed by a
   * launcher log line nobody has accepted. It renders exactly as unpaired,
   * because it IS unpaired everywhere that matters: `accountOf()` still returns
   * null, the batch copy still greys its checkbox, and all four account-scoped
   * views still nag. A chip beside a character the Copy-settings view will
   * refuse would be a false statement about capability.
   *
   * The second half matters as much as the first: the sidebar must not become a
   * consumer of the launcher log scan, which reads and UTF-8-decodes every .log
   * in the launcher's directory. One surface pays for that, on demand.
   */
  test("a character the launcher proposes renders as unpaired, and no log scan is fired", async () => {
    calls.stub("launcher_proposals", [{ char_id: 951, user_id: 140, conflict: null }]);
    await mount([profile([file("core_char_951.dat", "char", 951)])], {
      accounts: [{ user_id: 140, alias: "stormdelay2", characters: [] }],
      unassigned: [951],
    });
    await waitFor(() => expect(screen.getByRole("button", { name: "Link…" })).toBeTruthy());
    expect(screen.queryByText("stormdelay2")).toBeNull();
    // Not in the tooltip either — the row must not claim a pairing that the
    // batch copy and all four account-scoped views would refuse to honour.
    expect(document.querySelector('[title*="account stormdelay2"]')).toBeNull();
    calls.never("launcher_proposals");
  });

  test("changing the profile selector replaces the rows with the other folder's", async () => {
    await mount([
      profile([file("core_char_950.dat", "char", 950)], DIR, "Default"),
      profile([file("core_char_960.dat", "char", 960, DIR2)], DIR2, "Alt"),
    ]);
    await waitFor(() => expect(rowNames()).toEqual(["core_char_950.dat"]));
    subject.selectedProfileDir = DIR2;
    await waitFor(() => expect(rowNames()).toEqual(["core_char_960.dat"]));
  });
});

// Both halves in one place: this pair is what stops the migration silently
// dropping an action on the floor. The other half — that they ARE in the app
// menu — is in AppMenu.spec.ts.
test("the five migrated actions are gone from the sidebar, and Open file… is not", async () => {
  await mount([profile([file("core_char_950.dat", "char", 950)])]);
  await waitFor(() => expect(screen.getByRole("button", { name: /open file/i })).toBeTruthy());
  for (const name of [/^accounts$/i, /copy settings/i, /^about$/i, /refresh names/i, /rescan/i]) {
    expect(screen.queryByRole("button", { name })).toBeNull();
  }
});
