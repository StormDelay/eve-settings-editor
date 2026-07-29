// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// A profile with no character file renders no header at all — its account files
// are reachable through "Open file…" instead. When EVERY profile is in that
// state the sidebar used to come up blank with nothing saying why, and the
// emptiness test lives in a derived that has to keep agreeing with the per-row
// filter in the template. These cases are what stop the two from drifting.
import { describe, expect, test } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import Sidebar from "$lib/Sidebar.svelte";
import { calls } from "$lib/test/setup";
import type { Profile } from "$lib/api";

const DIR = "C:/eve/settings_Default";

const file = (name: string, kind: "char" | "user", id: number | null) => ({
  path: `${DIR}/${name}`,
  file_name: name,
  kind,
  id,
  size: 1000,
  modified_unix: 0,
});

function mount(profiles: Profile[]) {
  calls.stub("discover_profiles", profiles);
  calls.stub("account_roster", { accounts: [], unassigned: [] });
  calls.stub("resolve_character_names", {});
  calls.stub("settings_preset_list", []);
  render(Sidebar, {
    onOpen: () => {},
    onShowAccounts: () => {},
    onShowBatch: () => {},
    onCollapse: () => {},
    onOpenPreset: () => {},
    charOpen: false,
    userOpen: false,
    openPresetName: null,
  });
}

const profile = (files: ReturnType<typeof file>[]): Profile => ({
  install: "eve",
  server: "tranquility",
  profile: "Default",
  dir: DIR,
  files,
});

describe("when nothing in the sidebar can be listed", () => {
  test("a profile holding only account files says where to go instead", async () => {
    mount([profile([file("core_user_80000001.dat", "user", 80000001)])]);
    const hint = await waitFor(() => screen.getByText(/no character files/i));
    // "Open file…" is also a button, so assert on the hint's own text.
    expect(hint.textContent).toMatch(/Open file/);
  });

  test("the hint names the filter when the filter is what hid them", async () => {
    // A non-standard name is hidden by the default toggle, so the profile draws
    // no header — but the fix here is to untick the box, not to open a file.
    mount([profile([file("core_char_90000001.dat.bak", "char", 90000001)])]);
    // Wait on the hint itself: before discover() resolves the sidebar is still
    // showing the no-profiles-at-all branch.
    await waitFor(() => expect(screen.getByText(/Untick/i)).toBeTruthy());
  });

  test("unticking the filter reveals the file and drops the hint", async () => {
    mount([profile([file("core_char_90000001.dat.bak", "char", 90000001)])]);
    await waitFor(() => expect(screen.getByText(/Untick/i)).toBeTruthy());
    const toggle = screen.getByRole("checkbox", { name: /hide non-standard/i });
    await fireEvent.click(toggle);
    await waitFor(() => expect(screen.queryByText(/Untick/i)).toBeNull());
  });

  test("a profile with a character file shows no hint at all", async () => {
    mount([profile([file("core_char_90000001.dat", "char", 90000001)])]);
    await waitFor(() => expect(screen.getByTitle("core_char_90000001.dat")).toBeTruthy());
    expect(screen.queryByText(/no character files/i)).toBeNull();
    expect(screen.queryByText(/No EVE profiles found/i)).toBeNull();
  });
});
