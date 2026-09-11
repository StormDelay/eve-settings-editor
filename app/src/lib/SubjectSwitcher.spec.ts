// Two lists of the same characters in two different orders, inside one app, is
// exactly the class of inconsistency this redesign exists to remove. So the
// first test here mounts the switcher AND the sidebar against one fixture and
// asserts they come out in the same sequence — it is the assertion that fails
// if either surface reintroduces a grouping level.
//
// Both read `subject.characters`, so today it is true by construction. The test
// is what keeps it that way.
import { expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import SubjectSwitcher from "$lib/SubjectSwitcher.svelte";
import type { Ctx } from "$lib/commands";
import Sidebar from "$lib/Sidebar.svelte";
import { calls } from "$lib/test/setup";
import { rescanProfiles, subject } from "$lib/subject.svelte";
import type { AccountRoster, Profile } from "$lib/api";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => Promise.resolve(null),
  ask: () => Promise.resolve(true),
  message: () => Promise.resolve(),
  confirm: () => Promise.resolve(true),
}));

const DIR = "/eve/settings_Default";
const file = (file_name: string, id: number | null) => ({
  path: `${DIR}/${file_name}`, file_name, kind: "char" as const, id, size: 1024, modified_unix: 0,
});

const profile = (...files: ReturnType<typeof file>[]): Profile => ({
  install: "eve", server: "tranquility", profile: "Default", dir: DIR, files,
});

const ROSTER: AccountRoster = {
  accounts: [
    { user_id: 140, alias: "stormdelay2", characters: [950, 951] },
    { user_id: 141, alias: "stormdelayghost", characters: [970] },
  ],
  unassigned: [980],
};

const NAMES = {
  950: { name: "Baguette Commander", category: "character" },
  951: { name: "Clea Otsada", category: "character" },
  970: { name: "De l'Opera", category: "character" },
  980: { name: "Fourth Pilot", category: "character" },
};

async function seed() {
  calls.stub("discover_profiles", [
    profile(file("core_char_950.dat", 950), file("core_char_951.dat", 951),
            file("core_char_970.dat", 970), file("core_char_980.dat", 980)),
  ]);
  calls.stub("account_roster", ROSTER);
  calls.stub("resolve_character_names", NAMES);
  calls.stub("settings_preset_list", []);
  await rescanProfiles();
}

// `props:` explicitly — `anchor` is also a Svelte MOUNT option, so a bare props
// object with that key is read as component options instead.
const noop = () => {};
const SWITCHER_PROPS = {
  anchor: document.body,
  onclose: noop,
  onOpen: noop,
  onOpenPreset: noop,
  onGoto: noop,
  // The palette half: the switcher runs commands as well as opening files now,
  // so it takes the registry's action context.
  ctx: {
    goto: noop,
    pickFile: noop,
    save: noop,
    discard: noop,
    showHistory: noop,
    showAccounts: noop,
    showBatch: noop,
    showAbout: noop,
    showShortcuts: noop,
    openPalette: noop,
    findInView: noop,
  } satisfies Ctx,
};

function mountSwitcher() {
  render(SubjectSwitcher, { props: SWITCHER_PROPS });
}

/**
 * Character rows inside a container, in render order.
 *
 * The row's own TEXT nodes only: the account chip is an element child of the
 * same label button, and including it would compare presentation rather than
 * order — which is what this file is about.
 */
const nameOf = (li: Element): string => {
  const btn = li.querySelector("button.label");
  if (!btn) return "";
  return Array.from(btn.childNodes)
    .filter((n) => n.nodeType === Node.TEXT_NODE)
    .map((n) => n.textContent ?? "")
    .join("")
    .trim();
};

const rowsIn = (root: HTMLElement) =>
  within(root)
    .getAllByRole("listitem")
    .map(nameOf)
    .filter((t) => /Commander|Otsada|Opera|Pilot|core_char/.test(t));

test("the switcher's order is the sidebar's order", async () => {
  await seed();
  mountSwitcher();
  await waitFor(() => expect(screen.getByRole("dialog", { name: "Find a character" })).toBeTruthy());
  const inSwitcher = rowsIn(screen.getByRole("dialog", { name: "Find a character" }));

  render(Sidebar, {
    onOpen: () => {},
    onPickFile: () => {},
    onCollapse: () => {},
    onOpenPreset: () => {},
    onShowAccounts: () => {},
  });
  const inSidebar = rowsIn(document.querySelector("aside.sidebar") as HTMLElement);

  expect(inSwitcher).toEqual(inSidebar);
  // Alphabetical by resolved name, which is how a name is found.
  expect(inSwitcher).toEqual(["Baguette Commander", "Clea Otsada", "De l'Opera", "Fourth Pilot"]);
});

/**
 * The flat list's answer to "switch to my other character on this account",
 * which is the one thing grouping genuinely did here. A filter beats a grouping
 * at it because it is temporary: it costs nothing to every other opening, which
 * is looking for one name.
 */
test("typing an account alias filters to that account's characters, still in name order", async () => {
  await seed();
  mountSwitcher();
  const box = await screen.findByRole("dialog", { name: "Find a character" });
  await fireEvent.input(within(box).getByRole("searchbox"), { target: { value: "stormdelay2" } });
  await waitFor(() => expect(rowsIn(box)).toEqual(["Baguette Commander", "Clea Otsada"]));
});

test("typing a character name filters to it", async () => {
  await seed();
  mountSwitcher();
  const box = await screen.findByRole("dialog", { name: "Find a character" });
  await fireEvent.input(within(box).getByRole("searchbox"), { target: { value: "otsada" } });
  await waitFor(() => expect(rowsIn(box)).toEqual(["Clea Otsada"]));
});

// A Chip is a CONFIRMED pairing and nothing else, on every surface.
test("a paired row carries its account chip and an unpaired one carries none", async () => {
  await seed();
  mountSwitcher();
  const box = await screen.findByRole("dialog", { name: "Find a character" });
  expect(within(box).getAllByText("stormdelay2")).toHaveLength(2);
  expect(within(box).getAllByText("stormdelayghost")).toHaveLength(1);
});

// The switcher is the seed of Phase 5's palette, so jump-to-view is here from
// the start rather than in a second component that would have to be merged.
test("the Go to… section lists the six views with their disabled reasons", async () => {
  await seed();
  mountSwitcher();
  const box = await screen.findByRole("dialog", { name: "Find a character" });
  expect(within(box).getByText("Go to…")).toBeTruthy();
  for (const v of ["Layout", "Overview", "Autofill", "Keybinds", "Probes", "Raw"]) {
    expect(within(box).getByRole("button", { name: v })).toBeTruthy();
  }
  // Nothing is open in this fixture, so five of the six say why not.
  expect(within(box).getByRole("button", { name: "Layout" }).getAttribute("title")).toMatch(/open a character/i);
});
