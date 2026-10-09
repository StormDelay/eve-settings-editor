// Component test (vitest + jsdom).
//
// OverviewView is the shell around the three overview sub-tabs. What it owns
// alone is the gating: overview columns live in the ACCOUNT file, so a
// character with no account paired can see nothing here and must be told why
// rather than shown an empty editor. It also owns tab selection, and the rule
// that a reload keeps the selected tab only when that tab still exists.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/svelte";
import OverviewView from "$lib/OverviewView.svelte";
import { calls } from "$lib/test/setup";
import { toasts } from "$lib/ui/toasts.svelte";
import type { Appearance, OverviewColumns, OverviewTab } from "$lib/api";

const tab = (index: number, name: string): OverviewTab => ({
  index,
  name,
  pieces: [{ text: name }],
  editable: true,
  color: null,
  // A real tab always names a preset; the sub-tabs read it and an empty string
  // is not the same thing as "no preset" to them.
  preset: "PvP",
  inherits: false,
  columns: [{ name: "NAME", label: "Name", visible: true, width: 120 }],
});

const appearance: Appearance = {
  background: { enabled: [], order: [] },
  flag: { enabled: [], order: [] },
  colors: [],
  flag_colors: [],
  palette: [],
  bools: [],
  defaulted: false,
};

const columns = (...tabs: OverviewTab[]): OverviewColumns => ({
  tabs,
  windows: [],
  presets: [{ name: "PvP", groups: [], filtered_states: [], always_shown_states: [] }],
  appearance,
});

const inWindows = (tabs: OverviewTab[], windows: { index: number; tab_indices: number[] }[]): OverviewColumns =>
  ({ ...columns(...tabs), windows });

/** A row of the tab list, found by the name it shows. Scoped to the list: a tab
 *  and a preset can share a name, and the Filters sub-tab renders both. */
const list = () => document.querySelector(".tablist") as HTMLElement;
const row = (name: string) => within(list()).getByText(name).closest(".row") as HTMLElement;
const findRow = (name: string) => waitFor(() => row(name));
/** Open the inline rename editor on the nth row and hand back its input. */
async function renameEditor(nth = 0, label = "Piece 1 text") {
  await fireEvent.click(screen.getAllByRole("button", { name: "More actions" })[nth]);
  await fireEvent.click(screen.getByRole("menuitem", { name: "Rename tab…" }));
  return screen.getByLabelText(label) as HTMLInputElement;
}

function mount(over: Record<string, unknown> = {}) {
  const spies = {
    onLoadCharacter: vi.fn(), onUserDirty: vi.fn(), onCharDirty: vi.fn(),
    onWindowAdded: vi.fn(), onShowAccounts: vi.fn(),
  };
  render(OverviewView, {
    userOpen: true,
    userId: 5,
    charId: 9,
    charOpen: true,
    characters: [9],
    refreshToken: 1,
    ...spies,
    ...over,
  } as never);
  return spies;
}

describe("gating on the account file", () => {
  // The columns live in the account file. A character with none paired is not
  // an error state — it is the normal state before pairing — so it gets the
  // route out rather than an empty editor.
  test("an unpaired character is offered the pairing flow", async () => {
    const { onShowAccounts } = mount({ userOpen: false, charId: 9 });
    const button = await screen.findByRole("button", { name: /pair/i });
    await fireEvent.click(button);
    expect(onShowAccounts).toHaveBeenCalled();
    calls.never("overview_columns");
  });

  test("with no file open at all, nothing is read", async () => {
    mount({ userOpen: false, charId: null });
    expect(screen.getByText(/open a character or an account file/i)).toBeTruthy();
    calls.never("overview_columns");
  });

  test("an open account file is read once", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP")));
    mount();
    await waitFor(() => expect(calls.of("overview_columns").length).toBe(1));
  });

  test("a backend failure is shown, not swallowed", async () => {
    calls.stub("overview_columns", () => { throw { code: "no_document", message: "no account file open" }; });
    mount();
    expect(await screen.findByText(/no account file open/i)).toBeTruthy();
  });

  test("an account with no tabs says so", async () => {
    calls.stub("overview_columns", columns());
    mount();
    expect(await screen.findByText(/no overview tabs/i)).toBeTruthy();
  });

  test("an account with no tabs can create its first one", async () => {
    calls.stub("overview_columns", columns());
    calls.stub("tab_create", columns(tab(0, "Main")));
    const { onUserDirty } = mount();
    await screen.findByText(/no overview tabs/i);
    await fireEvent.click(screen.getByRole("button", { name: "+ Tab" }));
    const box = screen.getByLabelText("Tab name") as HTMLInputElement;
    await fireEvent.input(box, { target: { value: "Main" } });
    await fireEvent.keyDown(box, { key: "Enter" });
    await findRow("Main");
    expect(calls.of("tab_create")[0].args).toMatchObject({ windowIdx: 0, name: "Main", fromTab: null });
    expect(onUserDirty).toHaveBeenCalled();
  });

  // No container at all is not the zero-tab case: nothing can write to it until
  // one is minted, so the view offers that instead of a dead tab editor.
  test("a file with no overview settings can create them", async () => {
    calls.stub("overview_columns", { ...columns(), presets: [], no_container: true });
    calls.stub("overview_create", { ...columns(), presets: [] });
    const { onUserDirty } = mount();
    await screen.findByText(/no overview settings/i);
    expect(screen.queryByRole("button", { name: "Overview actions" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Create overview settings" }));
    expect(calls.of("overview_create").length).toBe(1);
    expect(await screen.findByText(/no overview tabs/i)).toBeTruthy();
    expect(screen.queryByText(/no overview settings/i)).toBeNull();
    expect(onUserDirty).toHaveBeenCalled();
  });

  test("a failed create says so and leaves the file clean", async () => {
    calls.stub("overview_columns", { ...columns(), no_container: true });
    calls.stub("overview_create", () => { throw { code: "no_overview", message: "This file has no overview settings." }; });
    const { onUserDirty } = mount();
    await fireEvent.click(await screen.findByRole("button", { name: "Create overview settings" }));
    expect(await screen.findByText(/weren't created/i)).toBeTruthy();
    expect(onUserDirty).not.toHaveBeenCalled();
  });
});

// The <select> that used to answer `getByLabelText("Tab")` is gone: the tab
// list is the one control that selects a tab. The queries below moved onto it;
// what they assert did not.
describe("tab selection", () => {
  test("the first tab is selected on load", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP"), tab(1, "Mining")));
    mount();
    await findRow("PvP");
    expect(row("PvP").querySelector(".label")?.getAttribute("aria-current")).toBe("true");
  });

  test("every tab is offered", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP"), tab(1, "Mining")));
    mount();
    await findRow("PvP");
    expect(row("PvP")).toBeTruthy();
    expect(row("Mining")).toBeTruthy();
  });

  test("nothing selects a tab twice — the Tab select is gone", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP"), tab(1, "Mining")));
    mount();
    await findRow("PvP");
    expect(screen.queryByLabelText("Tab")).toBeNull();
  });

  // Switching between two account files leaves userOpen/userId unchanged, so
  // refreshToken is what has to drive the reload — otherwise the second file
  // shows the first one's overview.
  test("a refreshToken bump re-reads the file", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP")));
    const { rerender } = render(OverviewView, {
      userOpen: true, userId: 5, charId: 9, charOpen: true, characters: [9], refreshToken: 1,
      onLoadCharacter: vi.fn(), onUserDirty: vi.fn(), onCharDirty: vi.fn(),
      onWindowAdded: vi.fn(), onShowAccounts: vi.fn(),
    } as never);
    await waitFor(() => expect(calls.of("overview_columns").length).toBe(1));

    await rerender({ refreshToken: 2 } as never);
    await waitFor(() => expect(calls.of("overview_columns").length).toBe(2));
  });
});

// Tab names carry EVE's markup, parsed and written in Rust (tab_name.rs). The
// row editor hands back styled pieces — sent through `tab_rename_pieces`, so
// Rust writes the tags — or raw markup, sent verbatim through `tab_rename`.
// What these pin is which command runs, with what, and when nothing does.
describe("tab name markup", () => {
  const marked: OverviewTab = {
    ...tab(0, "<color=0xFFFF6F75>   <b>main</b>   </color>"),
    pieces: [{ text: "   ", color: "FFFF6F75" }, { text: "main", color: "FFFF6F75", bold: true }, { text: "   ", color: "FFFF6F75" }],
  };

  test("the list shows the readable name, not the markup", async () => {
    calls.stub("overview_columns", columns(marked));
    mount();
    expect(await findRow("main")).toBeTruthy();
  });

  test("a piece edit goes to the backend as pieces", async () => {
    calls.stub("overview_columns", columns(marked));
    mount();
    await findRow("main");

    await renameEditor();
    await fireEvent.click(screen.getByLabelText("Piece 2 bold"));
    await fireEvent.keyDown(screen.getByLabelText("Piece 2 text"), { key: "Enter" });

    expect(calls.only("tab_rename_pieces").args).toEqual({
      tabIdx: 0,
      pieces: [{ text: "   ", color: "FFFF6F75" }, { text: "main", color: "FFFF6F75" }, { text: "   ", color: "FFFF6F75" }],
    });
    calls.never("tab_rename");
  });

  // Opening the editor on a name and committing it untouched must write
  // nothing — the pieces compare equal after the writer's normalisation.
  test("a piece edit that changes nothing is not a write", async () => {
    calls.stub("overview_columns", columns(marked));
    mount();
    await findRow("main");

    const box = await renameEditor();
    await fireEvent.keyDown(box, { key: "Enter" });
    calls.never("tab_rename_pieces");
    calls.never("tab_rename");
  });

  // A name using markup the pieces can't carry is edited raw and written
  // verbatim, and an untouched one is not written at all.
  test("raw markup goes to the backend verbatim, and only when changed", async () => {
    const raw: OverviewTab = { ...tab(0, "<uppercase>main</uppercase>"), pieces: [{ text: "main", uppercase: true }], editable: false };
    calls.stub("tab_name_parse", { pieces: [{ text: "main", uppercase: true }], editable: false });
    calls.stub("overview_columns", columns(raw));
    mount();
    await findRow("main");

    let box = await renameEditor(0, "Tab name markup");
    await fireEvent.keyDown(box, { key: "Enter" });
    calls.never("tab_rename");

    box = await renameEditor(0, "Tab name markup");
    await fireEvent.input(box, { target: { value: "<uppercase>fleet</uppercase>" } });
    await fireEvent.keyDown(box, { key: "Enter" });
    expect(calls.only("tab_rename").args).toEqual({ tabIdx: 0, name: "<uppercase>fleet</uppercase>" });
  });
});

// The backend renumbers the tab table on every reorder and cross-window move —
// EVE draws a window's tabs in ascending index, so renumbering is the only way
// an order reaches the game. The tab the user was looking at therefore gets a
// NEW index, and a tabIndex left alone comes to name a different tab.
describe("the selection survives the renumbering", () => {
  test("after a reorder inside one window", async () => {
    calls.stub("overview_columns", inWindows([tab(0, "main"), tab(1, "Mining")], [{ index: 0, tab_indices: [0, 1] }]));
    // Mining moves to the front, so the backend hands back Mining AS INDEX 0.
    calls.stub("tab_reorder", inWindows([tab(0, "Mining"), tab(1, "main")], [{ index: 0, tab_indices: [0, 1] }]));
    mount();
    await findRow("Mining");

    await fireEvent.click(screen.getByRole("button", { name: "Mining" }));
    await fireEvent.dragStart(row("Mining"));
    await fireEvent.drop(row("main"));

    // The same TAB, not the same index.
    await waitFor(() => expect(row("Mining").querySelector(".label")?.getAttribute("aria-current")).toBe("true"));
  });

  test("after a move into another window", async () => {
    calls.stub("overview_columns", inWindows(
      [tab(0, "Travel"), tab(1, "main")],
      [{ index: 0, tab_indices: [1] }, { index: 1, tab_indices: [0] }],
    ));
    // main lands ahead of Travel in window 2, so the two swap indices.
    calls.stub("tab_move", inWindows(
      [tab(0, "main"), tab(1, "Travel")],
      [{ index: 0, tab_indices: [] }, { index: 1, tab_indices: [0, 1] }],
    ));
    mount();
    await findRow("main");

    await fireEvent.click(screen.getByRole("button", { name: "main" }));
    await fireEvent.dragStart(row("main"));
    await fireEvent.drop(row("Travel"));

    expect(calls.only("tab_move").args).toEqual({ tabIdx: 1, fromWindow: 0, toWindow: 1, pos: 0 });
    await waitFor(() => expect(row("main").querySelector(".label")?.getAttribute("aria-current")).toBe("true"));
  });

  test("a drop in place is not an edit", async () => {
    calls.stub("overview_columns", inWindows([tab(0, "main"), tab(1, "Mining")], [{ index: 0, tab_indices: [0, 1] }]));
    const { onUserDirty } = mount();
    await findRow("main");

    await fireEvent.dragStart(row("main"));
    await fireEvent.drop(row("main"));

    calls.never("tab_reorder");
    expect(onUserDirty).not.toHaveBeenCalled();
  });
});

// Per-tab column widths live in each CHARACTER file keyed by tab index, so a
// renumbering moves them — and cannot reach a character logged in at the time.
// Said once, at the moment it happens, and only when there are widths on screen.
describe("the width-swap warning", () => {
  const before = inWindows([tab(0, "main"), tab(1, "Mining")], [{ index: 0, tab_indices: [0, 1] }]);
  const after = inWindows([tab(0, "Mining"), tab(1, "main")], [{ index: 0, tab_indices: [0, 1] }]);

  async function reorder(charOpen: boolean) {
    toasts.splice(0, toasts.length);
    calls.stub("overview_columns", before);
    calls.stub("tab_reorder", after);
    mount({ charOpen });
    await findRow("Mining");
    await fireEvent.dragStart(row("Mining"));
    await fireEvent.drop(row("main"));
  }

  test("fires once with a character open", async () => {
    await reorder(true);
    await waitFor(() => expect(toasts.length).toBe(1));
    expect(toasts[0].message).toMatch(/moved with the tabs, for every character/i);
    expect(toasts[0].message).toMatch(/logged in at the time keeps the old order/i);
  });

  test("says nothing with no character open", async () => {
    await reorder(false);
    await waitFor(() => expect(calls.of("tab_reorder").length).toBe(1));
    expect(toasts.length).toBe(0);
  });
});

describe("removing a window that is not the last", () => {
  const three = inWindows([tab(0, "main"), tab(1, "Mining"), tab(2, "Scan")], [
    { index: 0, tab_indices: [0] }, { index: 1, tab_indices: [1] }, { index: 2, tab_indices: [2] },
  ]);
  const two = inWindows([tab(0, "main"), tab(1, "Mining"), tab(2, "Scan")], [
    { index: 0, tab_indices: [0, 1] }, { index: 1, tab_indices: [2] },
  ]);

  async function remove(windowName: string, over: Record<string, unknown>) {
    toasts.splice(0, toasts.length);
    calls.stub("overview_columns", three);
    calls.stub("overview_window_remove", two);
    mount(over);
    await findRow("Mining");
    await fireEvent.click(screen.getByRole("button", { name: `${windowName} actions` }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Remove this window" }));
    await waitFor(() => expect(calls.of("overview_window_remove").length).toBe(1));
  }

  test("names the other characters whose positions did not move", async () => {
    await remove("Overview 2", { sharedNames: ["Pilot Two"] });
    await waitFor(() => expect(toasts.length).toBe(2));
    expect(toasts[1].message).toMatch(/Overview 3 and later are now one number lower/);
    expect(toasts[1].message).toMatch(/Pilot Two still has the old window positions/);
  });

  test("says every character is off when no character file is open", async () => {
    await remove("Overview 1", { charOpen: false, sharedNames: [] });
    await waitFor(() => expect(toasts.length).toBe(2));
    expect(toasts[1].message).toMatch(/Every character on this account still has the old window positions/);
  });

  test("says nothing extra when the open character is the only one", async () => {
    await remove("Overview 2", { sharedNames: [] });
    await waitFor(() => expect(toasts.length).toBe(1));
  });

  test("says nothing extra for the last window", async () => {
    await remove("Overview 3", { sharedNames: ["Pilot Two"] });
    await waitFor(() => expect(toasts.length).toBe(1));
    expect(toasts[0].message).toMatch(/Removed Overview 3/);
  });
});

// Account-wide and rare, so they are behind a visible ⋯ rather than wedged into
// the sub-tab strip as two non-tab children of a tablist.
describe("the view menu", () => {
  async function openMenu(data = columns(tab(0, "PvP"))) {
    calls.stub("overview_columns", data);
    const spies = mount();
    await findRow("PvP");
    await fireEvent.click(screen.getByRole("button", { name: "Overview actions" }));
    return spies;
  }

  test("offers both pack commands and the window set-up", async () => {
    await openMenu();
    expect(screen.getByRole("menuitem", { name: "Import overview pack…" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Export overview pack…" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Assign tabs to windows" })).toBeTruthy();
  });

  test("set-up is present but disabled once the account has windows", async () => {
    await openMenu(inWindows([tab(0, "PvP")], [{ index: 0, tab_indices: [0] }]));
    const item = screen.getByRole("menuitem", { name: "Assign tabs to windows" }) as HTMLButtonElement;
    expect(item.disabled).toBe(true);
    expect(item.title).toMatch(/already assigns tabs to windows/i);
  });
});

/**
 * Overview reaches across both shell columns the way LayoutView reaches into
 * the second one: `display: contents` on the root, so the root stops
 * participating in layout and its child becomes a grid item of `.shell`.
 *
 * Unlike Layout it has exactly ONE child, spanning columns 2 to 4 — a tab's
 * properties are docked under the list that selects it, so there is no third
 * column. Wrap the child in a scroller and it silently stops spanning; add a
 * second and it lands in the column the shell is no longer drawing anything in.
 * jsdom computes no layout, so nothing else would fail.
 */
describe("the shell grid contract", () => {
  test("the root renders exactly one child, the work area", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP")));
    mount();
    await findRow("PvP");

    const root = document.querySelector(".overview-view") as HTMLElement;
    const kids = Array.from(root.children);
    expect(kids).toHaveLength(1);
    expect(kids[0].classList.contains("work")).toBe(true);
    expect(kids[0].classList.contains("wide")).toBe(true);
  });

  test("it still spans with no account file open", () => {
    mount({ userOpen: false, charId: null });
    const root = document.querySelector(".overview-view") as HTMLElement;
    expect(Array.from(root.children)).toHaveLength(1);
    expect(root.children[0].classList.contains("wide")).toBe(true);
  });

  // There is no properties pane. Everything a tab has is on its row, and the
  // two fields that outlived the move turned out to duplicate controls that
  // already existed elsewhere — see §13.
  test("no properties pane, and no inspector column", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP")));
    mount();
    await findRow("PvP");

    expect(document.querySelector(".side .tablist")).toBeTruthy();
    expect(document.querySelector(".inspect")).toBeNull();
    expect(document.querySelector("aside.inspector")).toBeNull();
  });
});

// Add window writes the account grouping AND the char-file geometry. Miss the
// second flag and the new window's position is silently dropped on save.
test("+ Window dirties both slots and hands the new window up", async () => {
  calls.stub("overview_columns", inWindows([tab(0, "PvP")], [{ index: 0, tab_indices: [0] }]));
  calls.stub("overview_window_add", inWindows(
    [tab(0, "PvP"), tab(1, "Combat")],
    [{ index: 0, tab_indices: [0] }, { index: 1, tab_indices: [1] }],
  ));
  const { onUserDirty, onCharDirty, onWindowAdded } = mount();
  await findRow("PvP");

  await fireEvent.click(screen.getByRole("button", { name: "+ Window" }));
  const box = screen.getByLabelText("First tab name") as HTMLInputElement;
  await fireEvent.input(box, { target: { value: "Combat" } });
  await fireEvent.keyDown(box, { key: "Enter" });

  await waitFor(() => expect(onWindowAdded).toHaveBeenCalledWith("overview_1"));
  expect(onUserDirty).toHaveBeenCalled();
  expect(onCharDirty).toHaveBeenCalled();
});

/**
 * §2.8's bug, pinned by its own string.
 *
 * The confirm this replaces said "This can't be undone." It could: the delete
 * mutates the in-memory document and Discard re-reads both files from disk. The
 * genuinely comparable mutation thirty lines away in LayoutView said the
 * opposite and said it correctly — two dialogs, opposite claims, identical
 * mechanism, which teaches a user that the app's warnings are decoration.
 *
 * There is no dialog mock in this file, and that absence is half the assertion.
 */
describe("deleting a tab", () => {
  const twoTabs = () => columns(tab(0, "PvP"), tab(1, "Mining"));

  async function deleteFirstTab() {
    calls.stub("overview_columns", twoTabs());
    calls.stub("tab_delete", columns(tab(0, "Mining")));
    const spies = mount();
    await findRow("PvP");
    await fireEvent.click(screen.getAllByRole("button", { name: "More actions" })[0]);
    await fireEvent.click(screen.getByRole("menuitem", { name: "Delete tab" }));
    return spies;
  }

  test("it happens on the click, with no confirmation in the way", async () => {
    toasts.length = 0;
    await deleteFirstTab();
    await waitFor(() => expect(calls.of("tab_delete")).toHaveLength(1));
  });

  test("the toast names the tab and does NOT claim it cannot be undone", async () => {
    toasts.length = 0;
    await deleteFirstTab();
    await waitFor(() => expect(toasts.length).toBeGreaterThan(0));
    const m = toasts[toasts.length - 1].message;
    expect(m).toContain("PvP");
    expect(m).toMatch(/save to write it to disk/i);
    expect(m).not.toMatch(/can'?t be undone/i);
  });

  /** Both slots, because the backend carries the surviving tabs' per-tab column
   *  widths onto their new indices — miss the char flag and that half is
   *  silently dropped at the next save. */
  test("it marks both slots unsaved", async () => {
    const { onUserDirty, onCharDirty } = await deleteFirstTab();
    await waitFor(() => expect(onUserDirty).toHaveBeenCalled());
    expect(onCharDirty).toHaveBeenCalled();
  });

  /** A refused delete lands at the tab actions, not in a modal — and says which
   *  thing failed, which "Edit failed" never could. */
  test("a refused delete reports at the control and raises no dialog", async () => {
    calls.stub("overview_columns", twoTabs());
    calls.stub("tab_delete", () => {
      throw { code: "read_only", message: "The account file is read-only." };
    });
    mount();
    await findRow("PvP");
    await fireEvent.click(screen.getAllByRole("button", { name: "More actions" })[0]);
    await fireEvent.click(screen.getByRole("menuitem", { name: "Delete tab" }));

    const msg = await screen.findByRole("alert");
    expect(msg.textContent).toContain("That tab wasn't deleted");
    expect(msg.textContent).toContain("The account file is read-only.");
    // The bracketed machine code is relegated to `title=`, never the sentence.
    expect(msg.textContent).not.toContain("read_only");
  });
});
