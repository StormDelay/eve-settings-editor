// Component test (vitest + jsdom).
//
// The ONE control that selects an overview tab, replacing a grouped <select>
// and a chip row that only appeared when the selected tab's window held two or
// more tabs. What it owns alone is the shape of the list — grouping, the Other
// group, the ungrouped windowless case, the truthful rendering of a tab's
// colour and weight, and which backend operation a drop turns into.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import OverviewTabList from "$lib/OverviewTabList.svelte";
import type { OverviewColumns } from "$lib/api";

const appearance = {
  background: { enabled: [], order: [] },
  flag: { enabled: [], order: [] },
  colors: [] as [number, [number, number, number, number]][],
  bools: [] as [string, boolean][],
  defaulted: false,
};

const t = (index: number, name: string) => ({
  index,
  name,
  preset: "All",
  inherits: false,
  columns: [],
});

// Two windows and an orphan: Overview 2 holds exactly one tab, which is the
// case that used to have no reorder affordance at all.
const data: OverviewColumns = {
  tabs: [t(0, "main"), t(1, "Mining"), t(2, "Travel"), t(3, "loose")],
  windows: [
    { index: 0, tab_indices: [0, 1] },
    { index: 1, tab_indices: [2] },
  ],
  presets: [],
  appearance,
};

const windowless: OverviewColumns = { ...data, tabs: [t(0, "main"), t(1, "Mining")], windows: [] };

function mount(over: Partial<Record<string, unknown>> = {}) {
  const spies = {
    onSelect: vi.fn(),
    onCreateTab: vi.fn(),
    onAddWindow: vi.fn(),
    onRemoveWindow: vi.fn(),
    onDeleteTab: vi.fn(),
    onRenameTab: vi.fn(),
    onReorder: vi.fn(),
    onMove: vi.fn(),
    onSetUpWindowMapping: vi.fn(),
  };
  render(OverviewTabList, { data, tabIndex: 0, ...spies, ...over } as never);
  return spies;
}

/** A row, found by the name it shows. */
const row = (name: string) => screen.getByText(name).closest('[role="option"]') as HTMLElement;

describe("the shape of the list", () => {
  test("tabs are grouped by window, in tab_indices order, under Overview {n+1}", () => {
    mount();
    expect(screen.getByText("Overview 1")).toBeTruthy();
    expect(screen.getByText("Overview 2")).toBeTruthy();

    const w1 = screen.getByText("Overview 1").closest("div")!.parentElement!;
    // Read the label buttons, not the whole row: the row also carries a drag
    // grip and a "⋯". main before Mining — the stored strip order IS the
    // in-game order, so the list renders it rather than sorting it.
    const shown = [...w1.querySelectorAll('[role="option"] button.label')].map((b) => b.textContent?.trim());
    expect(shown.slice(0, 2)).toEqual(["main", "Mining"]);
  });

  test("a tab in no window falls into Other rather than vanishing", () => {
    mount();
    expect(screen.getByText("Other")).toBeTruthy();
    expect(row("loose")).toBeTruthy();
  });

  test("a windowless account is one ungrouped list, with the explanation", () => {
    mount({ data: windowless });
    expect(screen.queryByText("Overview 1")).toBeNull();
    expect(screen.queryByText("Other")).toBeNull();
    expect(screen.getByText(/EVE spreads them/i)).toBeTruthy();
    expect(row("main")).toBeTruthy();
  });

  // The one place in the app a tab looks the way it looks in game. An <option>
  // could not carry this, which is why the <select> was the picker that went.
  test("a tab's real colour and bold reach the DOM", () => {
    mount({
      data: { ...data, tabs: [t(0, "<color=0xFFFF6F75><b>main</b></color>"), t(1, "Mining"), t(2, "Travel"), t(3, "loose")] },
    });
    const style = (screen.getByText("main") as HTMLElement).style;
    // #FF6F75, alpha last the way CSS wants it — jsdom reports it resolved.
    expect(style.color).toBe("rgb(255, 111, 117)");
    expect(style.fontWeight).toBe("700");
  });

  test("the selected tab is the selected row", () => {
    mount({ tabIndex: 2 });
    expect(row("Travel").getAttribute("aria-selected")).toBe("true");
    expect(row("main").getAttribute("aria-selected")).toBe("false");
  });

  test("clicking a row selects it", async () => {
    const { onSelect } = mount();
    await fireEvent.click(screen.getByRole("button", { name: "Mining" }));
    expect(onSelect).toHaveBeenCalledWith(1);
  });
});

describe("drag", () => {
  // The direct regression test for the old rule, which only drew the chip row
  // when the selected tab's window held more than one tab.
  test("every row is draggable, including in a group of one", () => {
    mount();
    expect(row("main").getAttribute("draggable")).toBe("true");
    expect(row("Travel").getAttribute("draggable")).toBe("true");
  });

  // There is no backend operation that un-assigns a tab from every window, so
  // an orphan has nothing to drag to.
  test("a tab in no window is not draggable", () => {
    mount();
    expect(row("loose").getAttribute("draggable")).toBeNull();
  });

  test("a drop inside a group reorders that window", async () => {
    const { onReorder, onMove } = mount();
    await fireEvent.dragStart(row("main"));
    await fireEvent.drop(row("Mining"));
    expect(onReorder).toHaveBeenCalledWith(0, [1, 0]);
    expect(onMove).not.toHaveBeenCalled();
  });

  test("a drop into another group moves the tab, with the drop index", async () => {
    const { onMove, onReorder } = mount();
    await fireEvent.dragStart(row("main"));
    await fireEvent.drop(row("Travel"));
    expect(onMove).toHaveBeenCalledWith(0, 0, 1, 0);
    expect(onReorder).not.toHaveBeenCalled();
  });
});

describe("the window menu", () => {
  // Present-and-disabled, never absent: the button this replaces appeared and
  // disappeared as the selection moved between windows.
  test("Remove this window is present but disabled on a non-last window", async () => {
    mount();
    await fireEvent.click(screen.getByRole("button", { name: "Overview 1 actions" }));
    const item = screen.getByRole("menuitem", { name: "Remove this window" }) as HTMLButtonElement;
    expect(item.disabled).toBe(true);
    expect(item.title).toMatch(/only the last overview window/i);
  });

  test("Remove this window is enabled on the last window", async () => {
    const { onRemoveWindow } = mount();
    await fireEvent.click(screen.getByRole("button", { name: "Overview 2 actions" }));
    const item = screen.getByRole("menuitem", { name: "Remove this window" }) as HTMLButtonElement;
    expect(item.disabled).toBe(false);
    await fireEvent.click(item);
    expect(onRemoveWindow).toHaveBeenCalledWith(1);
  });

  test("with one window the reason is that it is the only one", async () => {
    mount({ data: { ...data, tabs: [t(0, "main")], windows: [{ index: 0, tab_indices: [0] }] } });
    await fireEvent.click(screen.getByRole("button", { name: "Overview 1 actions" }));
    const item = screen.getByRole("menuitem", { name: "Remove this window" }) as HTMLButtonElement;
    expect(item.disabled).toBe(true);
    expect(item.title).toMatch(/only overview window/i);
  });

  test("a group's New tab creates in that window", async () => {
    const { onCreateTab } = mount();
    await fireEvent.click(screen.getByRole("button", { name: "Overview 2 actions" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "New tab in this window" }));

    const box = screen.getByLabelText("Tab name") as HTMLInputElement;
    await fireEvent.input(box, { target: { value: "Travel 2" } });
    await fireEvent.keyDown(box, { key: "Enter" });

    expect(onCreateTab).toHaveBeenCalledWith("Travel 2", 1);
  });
});

// Renaming happens ON the row. Started from a menu here and finished in a panel
// below, it was two places for one gesture — and it cost a permanently-visible
// Name field that did nothing whenever nobody was renaming.
describe("renaming in place", () => {
  async function startRename(nth: number) {
    await fireEvent.click(screen.getAllByRole("button", { name: "More actions" })[nth]);
    await fireEvent.click(screen.getByRole("menuitem", { name: "Rename tab…" }));
    return screen.getByLabelText("Tab name") as HTMLInputElement;
  }

  test("Rename selects the row and turns it into an editor", async () => {
    const { onSelect } = mount();
    const box = await startRename(1);
    expect(onSelect).toHaveBeenCalledWith(1);
    // The row it replaced is gone while the editor is up.
    expect(screen.queryByRole("button", { name: "Mining" })).toBeNull();
    expect(box.value).toBe("Mining");
  });

  // Padding is how a tab is widened in game, so the editor is seeded with the
  // readable text spacing and all, and sends it back verbatim.
  test("the editor carries the typed spacing both ways", async () => {
    const { onRenameTab } = mount({
      data: { ...data, tabs: [t(0, "   main   "), t(1, "Mining"), t(2, "Travel"), t(3, "loose")] },
    });
    const box = await startRename(0);
    expect(box.value).toBe("   main   ");

    await fireEvent.input(box, { target: { value: "  fleet  " } });
    await fireEvent.keyDown(box, { key: "Enter" });
    // The whole decomposed name, because the editor owns all three parts.
    expect(onRenameTab).toHaveBeenCalledWith(0, { text: "  fleet  ", color: null, bold: false });
  });

  // Colour and bold are edited beside the text and ride out on the same commit:
  // in the file they are one markup-bearing string, so splitting them across
  // two panes was splitting one property in half.
  test("the colour and the weight commit with the text, as one rename", async () => {
    const { onRenameTab } = mount({
      data: { ...data, tabs: [t(0, "main"), t(1, "Mining"), t(2, "Travel"), t(3, "loose")] },
    });
    const box = await startRename(0);

    await fireEvent.click(screen.getByLabelText("Tab name colour"));
    await fireEvent.click(screen.getByLabelText("#40ff40"));
    await fireEvent.click(screen.getByTitle("Bold tab name"));
    await fireEvent.keyDown(box, { key: "Enter" });

    expect(onRenameTab).toHaveBeenCalledWith(0, { text: "main", color: "FF40FF40", bold: true });
  });

  test("the editor opens carrying the tab's existing colour and weight", async () => {
    const { onRenameTab } = mount({
      data: { ...data, tabs: [t(0, "<color=0xFFFF6F75><b>main</b></color>"), t(1, "Mining"), t(2, "Travel"), t(3, "loose")] },
    });
    const box = await startRename(0);
    await fireEvent.keyDown(box, { key: "Enter" });
    expect(onRenameTab).toHaveBeenCalledWith(0, { text: "main", color: "FFFF6F75", bold: true });
  });

  // It is seeded from the READABLE text, never the stored markup — the view
  // composes the colour and bold back around whatever comes out.
  test("a marked-up name opens as its readable text", async () => {
    mount({
      data: { ...data, tabs: [t(0, "<color=0xFFFF6F75><b>main</b></color>"), t(1, "Mining"), t(2, "Travel"), t(3, "loose")] },
    });
    expect((await startRename(0)).value).toBe("main");
  });

  test("Escape cancels and writes nothing", async () => {
    const { onRenameTab } = mount();
    const box = await startRename(1);
    await fireEvent.input(box, { target: { value: "Ore" } });
    await fireEvent.keyDown(box, { key: "Escape" });
    expect(onRenameTab).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Mining" })).toBeTruthy();
  });

  test("leaving the editor commits, the way the box it replaces did", async () => {
    const { onRenameTab } = mount();
    const box = await startRename(1);
    await fireEvent.input(box, { target: { value: "Ore" } });
    await fireEvent.focusOut(box, { relatedTarget: null });
    expect(onRenameTab).toHaveBeenCalledWith(1, { text: "Ore", color: null, bold: false });
  });

  test("an empty name is not a rename", async () => {
    const { onRenameTab } = mount();
    const box = await startRename(1);
    await fireEvent.input(box, { target: { value: "   " } });
    await fireEvent.keyDown(box, { key: "Enter" });
    expect(onRenameTab).not.toHaveBeenCalled();
  });
});

describe("the row menu", () => {

  test("Delete tab names the row it was opened on", async () => {
    const { onDeleteTab } = mount();
    await fireEvent.click(screen.getAllByRole("button", { name: "More actions" })[1]);
    await fireEvent.click(screen.getByRole("menuitem", { name: "Delete tab" }));
    expect(onDeleteTab).toHaveBeenCalledWith(1);
  });
});

describe("the footer", () => {
  test("+ Tab creates in the selected tab's window", async () => {
    const { onCreateTab } = mount({ tabIndex: 2 });
    await fireEvent.click(screen.getByRole("button", { name: "+ Tab" }));
    const box = screen.getByLabelText("Tab name") as HTMLInputElement;
    await fireEvent.input(box, { target: { value: "Scout" } });
    await fireEvent.keyDown(box, { key: "Enter" });
    expect(onCreateTab).toHaveBeenCalledWith("Scout", 1);
  });

  test("+ Window asks for the first tab's name", async () => {
    const { onAddWindow } = mount();
    await fireEvent.click(screen.getByRole("button", { name: "+ Window" }));
    const box = screen.getByLabelText("First tab name") as HTMLInputElement;
    await fireEvent.input(box, { target: { value: "Combat" } });
    await fireEvent.keyDown(box, { key: "Enter" });
    expect(onAddWindow).toHaveBeenCalledWith("Combat");
  });

  test("Escape cancels the name entry without creating anything", async () => {
    const { onCreateTab } = mount();
    await fireEvent.click(screen.getByRole("button", { name: "+ Tab" }));
    const box = screen.getByLabelText("Tab name") as HTMLInputElement;
    await fireEvent.keyDown(box, { key: "Escape" });
    expect(screen.queryByLabelText("Tab name")).toBeNull();
    expect(onCreateTab).not.toHaveBeenCalled();
  });

  test("+ Window is disabled, with a reason, on a windowless account", () => {
    mount({ data: windowless });
    const b = screen.getByRole("button", { name: "+ Window" }) as HTMLButtonElement;
    expect(b.disabled).toBe(true);
    expect(b.title).toMatch(/doesn't assign tabs to windows/i);
  });

  test("the windowless message offers the set-up command", async () => {
    const { onSetUpWindowMapping } = mount({ data: windowless });
    await fireEvent.click(screen.getByRole("button", { name: "Assign tabs to windows" }));
    expect(onSetUpWindowMapping).toHaveBeenCalled();
  });
});
