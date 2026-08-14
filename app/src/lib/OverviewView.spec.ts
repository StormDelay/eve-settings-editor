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
const row = (name: string) => within(list()).getByText(name).closest('[role="option"]') as HTMLElement;
const findRow = (name: string) => waitFor(() => row(name));
/** Open the inline rename editor on the nth row and hand back its input. */
async function renameEditor(nth = 0) {
  await fireEvent.click(screen.getAllByRole("button", { name: "More actions" })[nth]);
  await fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
  return screen.getByLabelText("Tab name") as HTMLInputElement;
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
    expect(screen.getByText(/open a character or account file/i)).toBeTruthy();
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
});

// The <select> that used to answer `getByLabelText("Tab")` is gone: the tab
// list is the one control that selects a tab. The queries below moved onto it;
// what they assert did not.
describe("tab selection", () => {
  test("the first tab is selected on load", async () => {
    calls.stub("overview_columns", columns(tab(0, "PvP"), tab(1, "Mining")));
    mount();
    await findRow("PvP");
    expect(row("PvP").getAttribute("aria-selected")).toBe("true");
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

// Tab names carry EVE's markup (see tabName.ts). The colour swatch and the B
// button rewrite that string through `tab_rename` — there is no command of
// their own — so what these pin is the string that goes over the wire.
describe("tab name markup", () => {
  const marked = "<color=0xFFFF6F75>   <b>main</b>   </color>";

  test("the list shows the readable name, not the markup", async () => {
    calls.stub("overview_columns", columns(tab(0, marked)));
    mount();
    expect(await findRow("main")).toBeTruthy();
  });

  // Colour and bold are edited in the row's own editor, beside the text: all
  // three are one markup-bearing string in the file, so they commit as ONE
  // rename when the editor closes.
  test("picking a colour rewrites the name, keeping the text and the bold", async () => {
    calls.stub("overview_columns", columns(tab(0, marked)));
    mount();
    await findRow("main");

    const box = await renameEditor();
    await fireEvent.click(screen.getByLabelText("Tab name colour"));
    await fireEvent.click(screen.getByLabelText("#40ff40"));
    await fireEvent.keyDown(box, { key: "Enter" });

    expect(calls.only("tab_rename").args).toEqual({
      tabIdx: 0,
      // Padding survives — it is how a tab is widened in game.
      name: "<color=0xFF40FF40><b>   main   </b></color>",
    });
  });

  test("clearing the colour drops the span and nothing else", async () => {
    calls.stub("overview_columns", columns(tab(0, marked)));
    mount();
    await findRow("main");

    const box = await renameEditor();
    await fireEvent.click(screen.getByLabelText("Tab name colour"));
    await fireEvent.click(screen.getByText("No colour"));
    await fireEvent.keyDown(box, { key: "Enter" });

    expect((calls.only("tab_rename").args as { name: string }).name).toBe("<b>   main   </b>");
  });

  test("the B button toggles bold off", async () => {
    calls.stub("overview_columns", columns(tab(0, marked)));
    mount();
    await findRow("main");

    const box = await renameEditor();
    await fireEvent.click(screen.getByTitle("Bold tab name"));
    await fireEvent.keyDown(box, { key: "Enter" });

    expect((calls.only("tab_rename").args as { name: string }).name).toBe("<color=0xFFFF6F75>   main   </color>");
  });

  // A name the parser can't decompose must never be silently rewritten by the
  // act of looking at it — only an explicit colour or bold change may replace
  // it. Opening the editor on one and committing it untouched has to write
  // nothing, because it re-emits as itself.
  test("an unparseable name is left alone and shows no colour", async () => {
    const weird = "<color=0xFFFF0000>a</color><color=0xFF00FF00>b</color>";
    calls.stub("overview_columns", columns(tab(0, weird)));
    mount();
    await findRow(weird);

    calls.never("tab_rename");

    const box = await renameEditor();
    expect(box.value).toBe(weird);
    expect(screen.getByLabelText("Tab name colour").textContent?.trim()).toBe("—");
    await fireEvent.keyDown(box, { key: "Enter" });
    calls.never("tab_rename");
  });

  test("renaming keeps the colour and the typed spacing", async () => {
    calls.stub("overview_columns", columns(tab(0, marked)));
    mount();
    await findRow("main");

    // The row's editor is seeded with the readable text, padding and all.
    const box = await renameEditor();
    expect(box.value).toBe("   main   ");

    await fireEvent.input(box, { target: { value: "  fleet  " } });
    await fireEvent.keyDown(box, { key: "Enter" });

    expect((calls.only("tab_rename").args as { name: string }).name)
      .toBe("<color=0xFFFF6F75><b>  fleet  </b></color>");
  });

  // A plain name, deliberately: `formatTabName` is the inverse of
  // `parseTabName` only UP TO TAG NESTING — it re-emits
  // `<color=…>   <b>x</b>   </color>` as `<color=…><b>   x   </b></color>`,
  // same rendering, different bytes. That round trip is a real write and always
  // has been (tabName.ts says so). What must never write is a name that comes
  // back byte-identical.
  test("a rename that changes nothing is not a write", async () => {
    calls.stub("overview_columns", columns(tab(0, "main")));
    mount();
    await findRow("main");

    const box = await renameEditor();
    await fireEvent.keyDown(box, { key: "Enter" });

    calls.never("tab_rename");
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
    await waitFor(() => expect(row("Mining").getAttribute("aria-selected")).toBe("true"));
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
    await waitFor(() => expect(row("main").getAttribute("aria-selected")).toBe("true"));
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

// The shipped ceiling of §4.3.1: per-tab column widths live in the CHARACTER
// file keyed by tab index, so renumbering leaves them on the position. Said
// once, at the moment it happens, and only when there are widths on screen.
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
    expect(toasts[0].message).toMatch(/widths stay with the position/i);
  });

  test("says nothing with no character open", async () => {
    await reorder(false);
    await waitFor(() => expect(calls.of("tab_reorder").length).toBe(1));
    expect(toasts.length).toBe(0);
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
    expect(screen.getByRole("menuitem", { name: "Set up per-window tabs…" })).toBeTruthy();
  });

  test("set-up is present but disabled once the account has windows", async () => {
    await openMenu(inWindows([tab(0, "PvP")], [{ index: 0, tab_indices: [0] }]));
    const item = screen.getByRole("menuitem", { name: "Set up per-window tabs…" }) as HTMLButtonElement;
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
