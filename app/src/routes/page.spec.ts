// Component test (vitest + jsdom).
//
// The app shell. It owns four things nothing below it can see: which SLOT a
// chosen file is opened into, whether Save is reachable, which view tabs exist
// for what is currently open, and what the History popover is describing. Each
// is a rule about the whole application state rather than about any one panel,
// so this is the only place it can be pinned.
//
// Named `page.spec.ts`, not `+page.spec.ts`: SvelteKit claims the `+page.`
// prefix for route files and would try to treat a `+page.spec.ts` as one.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import Page from "./+page.svelte";
import { calls } from "$lib/test/setup";
import { subject } from "$lib/subject.svelte";
import { names } from "$lib/names.svelte";
import { accountsStore } from "$lib/accounts.svelte";
import { toasts } from "$lib/ui/toasts.svelte";
import type { AccountRoster, OpenOutcome, Profile, TreeNodeData } from "$lib/api";

// The shell sets the OS window title from the SUBJECT. `vi.hoisted` so the spy
// exists before the hoisted `vi.mock` factory closes over it — one test asserts
// the title stops following the view tab, which is half of fault (b).
const { setTitle } = vi.hoisted(() => ({ setTitle: vi.fn(() => Promise.resolve()) }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setTitle }),
}));

// The native file picker. `picked` is what the next "Open file…" returns — the
// only way to reach an ACCOUNT file, which the sidebar deliberately does not
// list as a row (it lists characters; see Sidebar.spec.ts).
let picked: string | null = null;
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => Promise.resolve(picked),
  ask: () => Promise.resolve(true),
  message: () => Promise.resolve(),
  confirm: () => Promise.resolve(true),
}));

const tree: TreeNodeData = {
  label: "root", kind: "dict", display: "{}", path: [],
  editable: false, edit_text: null, removable: false, in_shared: false, children: [],
};

const opened = (file: string, readOnly = false): OpenOutcome => ({
  status: "opened",
  path: `/eve/${file}`,
  file_name: file,
  fidelity: readOnly ? { state: "read_only", reason: "unsupported opcode" } : { state: "editable" },
  tree,
});

const file = (file_name: string, kind: "char" | "user" | "other", id: number | null) => ({
  path: `/eve/${file_name}`, file_name, kind, id, size: 1024, modified_unix: 0,
});

const profile = (...files: ReturnType<typeof file>[]): Profile => ({
  install: "tq", server: "Tranquility", profile: "default", dir: "/eve", files,
});

/**
 * Mount the shell and wait for every call it fires on mount to have LANDED.
 *
 * Waiting for all of them still matters, and the store resets in `afterEach`
 * now cover the leak this used to guard against on its own: the preferences,
 * roster, names and subject stores are all module-level rune state shared by
 * every test in this file.
 */
async function mount(profiles: Profile[], roster: AccountRoster = { accounts: [], unassigned: [] }) {
  calls.stub("discover_profiles", profiles);
  calls.stub("account_roster", roster);
  calls.stub("preferences", { layout: { clutter: [], visible: [], detail: false, targets: 4, effects: 2 } });
  calls.stub("settings_preset_list", []);
  // An unstubbed command resolves to `undefined`, which is not a shape the real
  // backend can return — stub the ones the shell and its views fire on mount
  // rather than teaching them to defend against an impossible reply.
  calls.stub("list_file_backups", []);
  // `columns` was never a field of OverviewColumns and `windows` was missing —
  // the tab list reads the latter, so the lie surfaced as an unhandled throw.
  calls.stub("overview_columns", {
    tabs: [],
    windows: [],
    presets: [],
    appearance: { background: { enabled: [], order: [] }, flag: { enabled: [], order: [] }, colors: [], bools: [], defaulted: false },
  });
  // Both the app menu's proposal count and the Accounts view read this.
  calls.stub("launcher_proposals", []);
  // This file now mounts the Layout and Probes views — Phase 2 made every tab
  // reachable and defaulted the view away from Raw, so switching tabs actually
  // renders them. Neither defends against `undefined`, and neither should: an
  // unstubbed command resolving to `undefined` is not a shape the backend can
  // return. Left unstubbed these throw ASYNCHRONOUSLY, which vitest reports as
  // an unhandled error and exits 1 on — with every test still green, which is
  // exactly how it got past a local run and was caught by CI.
  //
  // Guarded, so a test that stubbed its own layout before mounting keeps it.
  if (!calls.stubbed("window_layout")) {
    calls.stub("window_layout", { reference_w: 0, reference_h: 0, windows: [], stacks: [] });
  }
  calls.stub("probe_formations", { formations: [], selected: null });
  render(Page);
  await waitFor(() => {
    expect(calls.of("discover_profiles").length).toBeGreaterThan(0);
    expect(calls.of("preferences").length).toBeGreaterThan(0);
  });
  // One more turn of the microtask queue, so the awaits inside those loaders
  // have assigned before the test body runs.
  await Promise.resolve();
}

const sidebar = () => document.querySelector("aside.sidebar") as HTMLElement;

/** Click the sidebar row for a settings file. Scoped to the sidebar: with
 *  nothing open, the launch empty state lists the same characters. */
async function openFile(name: string) {
  const row = await within(sidebar()).findByText(name);
  await fireEvent.click(row);
}

/** Open the ☰ menu and click one of its items. */
async function menu(name: RegExp) {
  await fireEvent.click(screen.getByRole("button", { name: "Menu" }));
  await fireEvent.click(await screen.findByRole("menuitem", { name }));
}

const save = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;

describe("routing a file to its slot", () => {
  // `core_user_<id>.dat` is the account file; everything else is the char slot,
  // which is the generic editing one. The sidebar only lists character files,
  // so both fixtures carry one.
  const both = profile(
    file("core_char_950.dat", "char", 950),
    file("core_user_140.dat", "user", 140),
  );

  test("a character file opens into the char slot", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount([both]);
    await openFile("core_char_950.dat");
    await waitFor(() => {
      expect(calls.of("open_file")[0].args).toMatchObject({
        slot: "char", path: "/eve/core_char_950.dat",
      });
    });
  });

  test("an account file picked from the dialog opens into the user slot", async () => {
    calls.stub("open_file", opened("core_user_140.dat"));
    picked = "/eve/core_user_140.dat";
    await mount([both]);
    await fireEvent.click(within(sidebar()).getByRole("button", { name: /open file/i }));
    await waitFor(() => {
      expect(calls.of("open_file")[0].args).toMatchObject({
        slot: "user", path: "/eve/core_user_140.dat",
      });
    });
    picked = null;
  });

  // Anything EVE did not name goes to the char slot, which is the generic
  // editing one — not to `user` and not to an error.
  test("a non-standard file opens into the char slot", async () => {
    calls.stub("open_file", opened("prefs.ini"));
    picked = "/eve/prefs.ini";
    await mount([both]);
    await fireEvent.click(within(sidebar()).getByRole("button", { name: /open file/i }));
    await waitFor(() => {
      expect(calls.of("open_file")[0].args).toMatchObject({
        slot: "char", path: "/eve/prefs.ini",
      });
    });
    picked = null;
  });
});

describe("saving", () => {
  // REWRITTEN (§8.7). This asserted that Save is ABSENT with nothing open.
  // A control that appears and disappears is the class of problem this phase
  // exists to remove, and a permanently-placed disabled Save teaches where Save
  // is before the user has anything to save.
  test("nothing open still shows Save, disabled", async () => {
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    expect(screen.getByText(/open a character to begin/i)).toBeTruthy();
    expect(save().disabled).toBe(true);
  });

  // Opening a file is not an edit. Save must stay unreachable until something
  // has actually changed, or every open offers to rewrite an untouched file.
  test("Save stays unreachable on a freshly opened, unedited file", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await openFile("core_char_950.dat");
    await waitFor(() => expect(calls.of("open_file").length).toBe(1));
    expect(save().disabled).toBe(true);
  });

  // A file the codec cannot round-trip is shown read-only (spec §7). It can
  // become dirty in no way, and Save must never light up for it.
  test("Save is unreachable for a read-only document", async () => {
    calls.stub("open_file", opened("core_char_950.dat", true));
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await openFile("core_char_950.dat");
    await screen.findByText("read-only");
    expect(save().disabled).toBe(true);
  });

  /**
   * FAULT (a), pinned. This fails on master.
   *
   * `mainView` is only reset to "file" inside openFile()/openPresetPair(), and
   * neither takeover had a close control — but the worse half was that the whole
   * file bar lived in the `{:else}` branch, so entering either view with unsaved
   * edits took Save AND both unsaved badges off the screen. Ctrl+S still worked;
   * nothing on screen said there was anything to save.
   */
  test("Save survives entering Accounts with pending edits", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await openFile("core_char_950.dat");
    await waitFor(() => expect(calls.of("open_file").length).toBe(1));

    // Set through the store rather than by driving a tree edit: what is under
    // test is that the context bar survives the `mainView` branch, not that
    // mutation marks a slot dirty (runMutation's own tests cover that).
    subject.dirty.char = true;
    await waitFor(() => expect(save().disabled).toBe(false));

    await menu(/accounts/i);
    await waitFor(() => expect(screen.getByRole("heading", { name: /accounts/i })).toBeTruthy());

    expect(save().disabled).toBe(false);
    expect(screen.getByText(/1 unsaved/)).toBeTruthy();
  });

  /**
   * §2.9's bug, pinned.
   *
   * Saving a character whose overview was edited marks BOTH slots dirty, so one
   * Save writes two files — and it used to `message()` inside the loop, stacking
   * two native modals, each naming a raw filename and a backup path, each
   * needing its own dismissal. One click, two modals, to report success.
   *
   * One toast now, naming people rather than files.
   */
  test("one Save that writes both slots produces exactly one toast", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    calls.stub("save_document", { bytes_written: 1024, backup_path: "/eve/backups/x.bak" });
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await openFile("core_char_950.dat");
    await waitFor(() => expect(calls.of("open_file").length).toBe(1));

    // Both slots resolved, so the toast can name people. The filename fallback
    // is correct when a name is unknown; what is under test is that a KNOWN name
    // is used, which is what the old message never did.
    names[950] = { name: "Baguette Commander", category: "character" };
    accountsStore.roster = { accounts: [{ user_id: 140, alias: "stormdelay2", characters: [950] }], unassigned: [] };
    subject.slots.user = opened("core_user_140.dat");
    subject.dirty.char = true;
    subject.dirty.user = true;
    toasts.length = 0;

    await fireEvent.click(save());

    // Both files written...
    await waitFor(() => expect(calls.of("save_document").length).toBe(2));
    // ...and reported once.
    await waitFor(() => expect(toasts).toHaveLength(1));
    expect(toasts[0].message).toBe("Saved Baguette Commander and stormdelay2.");
    // The two facts nobody reads at save time are NOT in it; they live in
    // History, where they are wanted.
    expect(toasts[0].message).not.toMatch(/bytes|backup|\.dat/i);
  });

  test("a single-slot save names the one file it wrote", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    calls.stub("save_document", { bytes_written: 1024, backup_path: "/eve/backups/x.bak" });
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await openFile("core_char_950.dat");
    await waitFor(() => expect(calls.of("open_file").length).toBe(1));

    subject.dirty.char = true;
    toasts.length = 0;
    await fireEvent.click(save());

    await waitFor(() => expect(calls.of("save_document").length).toBe(1));
    await waitFor(() => expect(toasts).toHaveLength(1));
    expect(toasts[0].message).not.toContain(" and ");
  });

  // §5.10's one free line: a tab click leaves the takeover, so Accounts and
  // Copy settings have a way out before Phase 3 turns them into sheets.
  test("clicking a view tab leaves the Accounts takeover", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await openFile("core_char_950.dat");
    await menu(/accounts/i);
    await waitFor(() => expect(screen.getByRole("heading", { name: /accounts/i })).toBeTruthy());
    await fireEvent.click(screen.getByRole("tab", { name: "Raw" }));
    await waitFor(() => expect(screen.queryByRole("heading", { name: /accounts/i })).toBeNull());
  });
});

/**
 * Opening a file is several round trips long, and a tab click during it is the
 * most recent thing the user asked for — not a stale value to overwrite.
 *
 * `openFile` used to snapshot the view BEFORE those awaits and assign it back
 * afterwards, so clicking Probes while a character was still opening was
 * silently undone a moment later and the view snapped back to Layout.
 */
test("a tab clicked while a file is still opening is not undone", async () => {
  calls.stub("open_file", opened("core_char_950.dat"));
  // Hold the load open on its first round trip, so the click lands mid-flight.
  let release!: () => void;
  const held = new Promise<void>((r) => (release = r));
  calls.stub("window_layout", async () => {
    await held;
    return { reference_w: 0, reference_h: 0, windows: [], stacks: [] };
  });

  await mount([profile(file("core_char_950.dat", "char", 950))]);
  await openFile("core_char_950.dat");
  await waitFor(() => expect(calls.of("open_file").length).toBe(1));

  // The user reaches for a tab before the open has finished.
  await fireEvent.click(screen.getByRole("tab", { name: "Probes" }));
  expect(screen.getByRole("tab", { name: "Probes" }).getAttribute("aria-selected")).toBe("true");

  release();
  // Let the rest of the open — the layout probe and the slot reconcile — run to
  // completion, which is where the snap-back used to happen.
  await waitFor(() => expect(calls.of("window_layout").length).toBeGreaterThan(0));
  await new Promise((r) => setTimeout(r, 0));
  expect(
    screen.getByRole("tab", { name: "Probes" }).getAttribute("aria-selected"),
    "the open must not snap the view back",
  ).toBe("true");
});

/** Ctrl+F must reach the window filter on Layout, which lives two components
 *  down and is bound up through LayoutView.
 *
 *  `hud` is stubbed so HudPanel renders above WindowPanel, as it does in the
 *  real app — without it the filter sits at the top of the inspector and the
 *  test proves less than it looks like it does. */
test("Ctrl+F focuses the window filter on Layout", async () => {
  calls.stub("open_file", opened("core_char_950.dat"));
  calls.stub("hud", {
    entries: [
      {
        name: "shipui_x",
        kind: "int",
        scope: "character",
        value: 10,
        set: { how: "set", path: [] },
      },
    ],
  });
  calls.stub("neocom_bar", { buttons: [], original: [] });
  calls.stub("chat_panels", []);
  calls.stub("window_layout", {
    reference_w: 1920,
    reference_h: 1080,
    windows: [
      {
        id: "w1",
        geom: null,
        flags: [],
        stack: null,
        open: true,
        resolution_matches: true,
      },
    ],
    stacks: [],
  });
  await mount([profile(file("core_char_950.dat", "char", 950))]);
  await openFile("core_char_950.dat");
  await waitFor(() => expect(screen.getByRole("tab", { name: "Layout" }).getAttribute("aria-selected")).toBe("true"));

  const box = await screen.findByLabelText("Filter windows");
  await fireEvent.keyDown(window, { key: "f", ctrlKey: true });
  await waitFor(() => expect(document.activeElement).toBe(box));
});

describe("the view tabs", () => {
  // REWRITTEN (§8.7). This asserted all six tabs were ABSENT for a file with no
  // character id — including Raw's own button, so the user was given no
  // indication the other five views existed. Fault (c): the strip changed
  // membership and width as files loaded and pairings landed.
  test("a file with no character id still offers all six tabs, disabled with a reason", async () => {
    calls.stub("open_file", opened("prefs.ini"));
    picked = "/eve/prefs.ini";
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await fireEvent.click(within(sidebar()).getByRole("button", { name: /open file/i }));
    await waitFor(() => expect(calls.of("open_file").length).toBe(1));
    picked = null;

    for (const v of ["Layout", "Overview", "Autofill", "Keybinds", "Probes", "Raw"]) {
      expect(screen.getByRole("tab", { name: v })).toBeTruthy();
    }
    // Raw is always reachable; the other five say why they are not.
    for (const v of ["Layout", "Overview", "Autofill", "Keybinds", "Probes"]) {
      const tab = screen.getByRole("tab", { name: v });
      expect(tab.getAttribute("aria-disabled")).toBe("true");
      expect(tab.getAttribute("title")).toBeTruthy();
    }
    expect(screen.getByRole("tab", { name: "Raw" }).getAttribute("aria-disabled")).toBeNull();
  });

  // The account-scoped editors need an id to work from — a character id or an
  // open account file. A `core_char_<id>.dat` supplies one.
  test("a character file unlocks the account-scoped editors", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await openFile("core_char_950.dat");
    for (const v of ["Overview", "Autofill", "Keybinds", "Probes"]) {
      const tab = await screen.findByRole("tab", { name: v });
      await waitFor(() => expect(tab.getAttribute("aria-disabled")).toBeNull());
    }
  });

  // REWRITTEN (§8.7). Layout used to VANISH for a document with no windows.
  // It is gated on the same condition and now says so instead.
  test("Layout is disabled, not hidden, for a document with no windows", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    calls.stub("window_layout", { reference_w: 0, reference_h: 0, windows: [], stacks: [] });
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await openFile("core_char_950.dat");
    await waitFor(() => expect(calls.of("window_layout").length).toBeGreaterThan(0));
    const tab = screen.getByRole("tab", { name: "Layout" });
    await waitFor(() => expect(tab.getAttribute("aria-disabled")).toBe("true"));
    expect(tab.getAttribute("title")).toMatch(/no saved window layout/i);
  });

  /**
   * The property, not an instance of it. Tab membership and order are what
   * fault (c) was about, and a snapshot before and after an open is the only
   * assertion that keeps failing if anyone reintroduces a conditional tab.
   */
  test("tab membership and order are identical before and after a file opens", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    const names = () => screen.getAllByRole("tab").map((t) => t.textContent?.trim());
    const before = names();
    await openFile("core_char_950.dat");
    await waitFor(() => expect(calls.of("open_file").length).toBe(1));
    expect(names()).toEqual(before);
    expect(before).toEqual(["Layout", "Overview", "Autofill", "Keybinds", "Probes", "Raw"]);
  });
});

describe("fault (b) — a panel that changed subject with the tab", () => {
  const paired = profile(
    file("core_char_950.dat", "char", 950),
    file("core_user_140.dat", "user", 140),
  );
  const roster: AccountRoster = {
    accounts: [{ user_id: 140, alias: "stormdelay2", characters: [950] }],
    unassigned: [],
  };

  const openPaired = async () => {
    calls.stub("open_file", (args: Record<string, unknown> | undefined) =>
      opened(String(args?.path).split("/").pop()!),
    );
    await mount([paired], roster);
    await openFile("core_char_950.dat");
    await waitFor(() => expect(subject.slots.user?.status).toBe("opened"));
  };

  const historyHeadings = async () => {
    await fireEvent.click(screen.getByRole("button", { name: /history/i }));
    const popover = await screen.findByRole("dialog", { name: "History" });
    return within(popover)
      .getAllByRole("heading")
      .map((h) => h.textContent?.trim());
  };

  /**
   * The active slot used to be derived from the VIEW, and `BackupsPanel` was
   * handed it — so switching from Overview to Autofill silently replaced the
   * character file's backup list with the account file's. The only marker was a
   * 0.85em, 0.7-opacity subtitle. Restore is destructive.
   *
   * History has no single subject any more: it asks for every open slot and
   * renders one titled group each, so the content is identical on every tab.
   */
  test("History lists the same files on every tab", async () => {
    await openPaired();
    const onOverview = await historyHeadings();
    expect(onOverview).toHaveLength(2);
    expect(onOverview.join(" ")).toMatch(/core_char_950\.dat/);
    expect(onOverview.join(" ")).toMatch(/core_user_140\.dat/);
    await fireEvent.click(screen.getByRole("button", { name: /history/i }));

    calls.log.length = 0;
    await fireEvent.click(screen.getByRole("tab", { name: "Autofill" }));
    const onAutofill = await historyHeadings();
    expect(onAutofill).toEqual(onOverview);
    await waitFor(() => {
      const slots = calls.of("list_file_backups").map((c) => c.args?.slot);
      expect(slots).toContain("char");
      expect(slots).toContain("user");
    });
  });

  /** The second head of the same bug: `setTitle` read `slots[active]`, so
   *  changing tab retitled the window from the character to the account. */
  test("the OS window title does not change with the view tab", async () => {
    await openPaired();
    await waitFor(() => expect(setTitle).toHaveBeenCalled());
    const before = setTitle.mock.calls.at(-1);
    for (const v of ["Autofill", "Keybinds", "Probes"]) {
      await fireEvent.click(screen.getByRole("tab", { name: v }));
      await Promise.resolve();
    }
    expect(setTitle.mock.calls.at(-1)).toEqual(before);
  });
});

/**
 * The sheet contract. Two of the app's eight views were not views at all — they
 * were takeovers, and entering one replaced the editor with nothing on screen
 * offering a way out.
 */
describe("sheets", () => {
  const openChar = async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await openFile("core_char_950.dat");
    await waitFor(() => expect(calls.of("open_file").length).toBe(1));
  };

  // The anti-regression for the whole phase: the editor is never unmounted, so
  // "restore the prior state" is not a feature — it is the absence of a
  // destruction.
  test("the editor stays mounted while a sheet is open", async () => {
    await openChar();
    await menu(/accounts/i);
    await waitFor(() => screen.getByRole("dialog", { name: "Accounts" }));
    for (const v of ["Layout", "Overview", "Autofill", "Keybinds", "Probes", "Raw"]) {
      expect(screen.getByRole("tab", { name: v })).toBeTruthy();
    }
    expect(screen.getByRole("button", { name: "Save" })).toBeTruthy();
  });

  test("Esc closes the sheet and returns to the editor", async () => {
    await openChar();
    await menu(/accounts/i);
    await waitFor(() => screen.getByRole("dialog", { name: "Accounts" }));
    await fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Accounts" })).toBeNull());
  });

  test("the close button closes the sheet", async () => {
    await openChar();
    await menu(/copy settings/i);
    await waitFor(() => screen.getByRole("dialog", { name: "Copy settings" }));
    await fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Copy settings" })).toBeNull());
  });

  // Fails on master, where the only way back out of a takeover re-opens the file
  // — which resets the view, the selection and the scroll position with it.
  test("the view tab survives a sheet round-trip", async () => {
    await openChar();
    await fireEvent.click(await screen.findByRole("tab", { name: "Keybinds" }));
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Keybinds" }).getAttribute("aria-selected")).toBe("true"),
    );
    await menu(/accounts/i);
    await waitFor(() => screen.getByRole("dialog", { name: "Accounts" }));
    await fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Accounts" })).toBeNull());
    expect(screen.getByRole("tab", { name: "Keybinds" }).getAttribute("aria-selected")).toBe("true");
  });

  // Opening a document is a request to be in the editor, so it closes any sheet.
  test("opening a file closes an open sheet", async () => {
    await openChar();
    await menu(/accounts/i);
    await waitFor(() => screen.getByRole("dialog", { name: "Accounts" }));
    await openFile("core_char_950.dat");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Accounts" })).toBeNull());
  });

  test("a tab click also leaves the sheet", async () => {
    await openChar();
    await menu(/accounts/i);
    await waitFor(() => screen.getByRole("dialog", { name: "Accounts" }));
    await fireEvent.click(screen.getByRole("tab", { name: "Raw" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Accounts" })).toBeNull());
  });
});

/**
 * A batch copy writes files on disk, behind the in-memory documents. Driven all
 * the way through the sheet, because the wiring between BatchView's report and
 * the shell's reaction is the whole of what is under test.
 */
describe("a batch copy that wrote the open document", () => {
  const twoChars = profile(
    file("core_char_950.dat", "char", 950),
    file("core_char_951.dat", "char", 951),
  );

  /** Open 950, open Copy settings, tick 951 and an aspect, and apply — with the
   *  backend reporting that it wrote the OPEN character file. */
  async function applyOntoOpenFile(dirty = false) {
    calls.stub("open_file", opened("core_char_950.dat"));
    calls.stub("setup_preview", {
      char_writes: [{ path: "/eve/core_char_951.dat" }],
      account_writes: [],
      excluded: [],
      source_error: null,
    });
    calls.stub("setup_apply", [
      { path: "/eve/core_char_950.dat", ok: true, backup_path: "/eve/b.bak", error: null },
    ]);
    // Both characters paired: every aspect Copy settings offers is
    // account-scoped, so an unpaired target has its checkbox disabled outright.
    await mount([twoChars], {
      accounts: [{ user_id: 140, alias: "stormdelay2", characters: [950, 951] }],
      unassigned: [],
    });
    await openFile("core_char_950.dat");
    await waitFor(() => expect(calls.of("open_file").length).toBe(1));
    // After the open, which clears the flag it sets.
    subject.dirty.char = dirty;

    await menu(/copy settings/i);
    const box = await screen.findByRole("dialog", { name: "Copy settings" });
    await fireEvent.click(within(box).getByRole("checkbox", { name: /core_char_951\.dat/ }));
    await fireEvent.click(within(box).getByRole("checkbox", { name: /window layout/i }));
    const copy = within(box).getByRole("button", { name: "Copy" });
    await waitFor(() => expect(copy.hasAttribute("disabled")).toBe(false));
    await fireEvent.click(copy);
    await waitFor(() => expect(calls.of("setup_apply").length).toBe(1));
  }

  test("a clean slot it wrote is re-read", async () => {
    await applyOntoOpenFile();
    // The same api.open + savedAt bump opening and discarding already use, so
    // every projection-based view refreshes through the token it already has.
    await waitFor(() => expect(calls.of("open_file").length).toBe(2));
    expect(calls.of("open_file")[1].args).toMatchObject({ slot: "char" });
  });

  test("a dirty slot it wrote is never re-read, and says so", async () => {
    await applyOntoOpenFile(true);
    // Re-reading would destroy unsaved edits, and no amount of warning makes a
    // silent discard acceptable. The message moves the discovery forward from
    // save time to now; Discard and Save are both already correct routes out.
    await waitFor(() => screen.getByText(/rewritten on disk by Copy settings/i));
    expect(calls.of("open_file").length).toBe(1);
  });
});

describe("the launch empty state", () => {
  test("offers the profile's characters, and clicking one opens it", async () => {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount([profile(file("core_char_950.dat", "char", 950), file("core_char_951.dat", "char", 951))]);
    const work = document.querySelector(".work") as HTMLElement;
    const rows = await within(work).findAllByText(/core_char_95[01]\.dat/);
    expect(rows).toHaveLength(2);
    await fireEvent.click(rows[0]);
    await waitFor(() => {
      expect(calls.of("open_file")[0].args).toMatchObject({ path: "/eve/core_char_950.dat" });
    });
  });

  test("with no characters it reuses the sidebar's own hint, word for word", async () => {
    await mount([profile(file("core_user_140.dat", "user", 140))]);
    const work = document.querySelector(".work") as HTMLElement;
    const inWork = await within(work).findByText(/no character files/i);
    const inSidebar = within(sidebar()).getByText(/no character files/i);
    expect(inWork.textContent).toBe(inSidebar.textContent);
  });
});

// The context bar carried TWO controls running `switcherOpen = !switcherOpen`:
// the subject button and a "Search or run a command" button beside it. Same
// panel, same toggle, and the panel anchors to the SUBJECT button — so a click
// at the far right of the bar opened a popup at the far left. One door now, and
// the shortcut is written on it.
describe("opening the subject switcher", () => {
  const panel = () => screen.queryByPlaceholderText(/search characters, presets and commands/i);

  test("the subject button opens it, and it is the only button that does", async () => {
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    expect(panel()).toBeNull();
    expect(screen.queryByRole("button", { name: /search or run a command/i })).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: /no character open/i }));
    await waitFor(() => expect(panel()).toBeTruthy());
  });

  test("so does the shortcut it advertises", async () => {
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    await waitFor(() => expect(panel()).toBeTruthy());
  });
});

/**
 * The inspector column is drawn only where there is something to inspect.
 *
 * Phase 2 drew it on every tab and Phase 4 kept that for a while: a column that
 * comes and goes is the same class of fault as a tab strip that changes
 * membership. What settled it was seeing it — on Autofill, Keybinds and Probes
 * it only ever said "Select something to see its properties", which is a pane
 * teaching people it is broken while taking a fifth of the window to do it.
 *
 * Layout is absent from the assertions below on purpose: it supplies its own
 * pane through `display: contents`, so the shell must NOT draw one, and
 * `LayoutView.spec.ts` owns that half.
 */
describe("which views get an inspector", () => {
  const shellAside = () => document.querySelector("aside.inspector");
  const work = () => document.querySelector(".work") as HTMLElement;

  const selected = (tab: string) => screen.getByRole("tab", { name: tab }).getAttribute("aria-selected");

  /** Open the character AND wait for the view resolution that follows it.
   *  `openFile` sets `view = resolveView(...)` after several awaits, so a tab
   *  clicked before that lands is silently undone by it. */
  async function openChar() {
    calls.stub("open_file", opened("core_char_950.dat"));
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    await openFile("core_char_950.dat");
    await waitFor(() => expect(selected("Layout")).toBe("false"));
  }

  async function on(tab: string) {
    await fireEvent.click(screen.getByRole("tab", { name: tab }));
    await waitFor(() => expect(selected(tab)).toBe("true"));
  }

  test("Raw has one, and it is the shell's", async () => {
    await openChar();
    await on("Raw");
    await waitFor(() => expect(shellAside()).toBeTruthy());
    expect(work().classList.contains("wide")).toBe(false);
  });

  for (const tab of ["Autofill", "Keybinds", "Probes", "Overview"]) {
    test(`${tab} has none, and takes the width instead`, async () => {
      await openChar();
      await on(tab);
      await waitFor(() => expect(shellAside()).toBeNull());
      // Overview draws its own `.work`, and it is wide for the same reason.
      expect(work().classList.contains("wide")).toBe(true);
      // Nor a rail, which would be 1.5rem of nothing beside a view with no use
      // for the column.
      expect(document.querySelector(".rail-right")).toBeNull();
    });
  }

  test("with no file open there is nothing to inspect either", async () => {
    await mount([profile(file("core_char_950.dat", "char", 950))]);
    expect(shellAside()).toBeNull();
    expect(work().classList.contains("wide")).toBe(true);
  });
});
