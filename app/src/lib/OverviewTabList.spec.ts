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
import type { OverviewColumns, OverviewTab, TabPiece } from "$lib/api";

const appearance = {
  background: { enabled: [], order: [] },
  flag: { enabled: [], order: [] },
  colors: [] as [number, [number, number, number, number]][],
  flag_colors: [] as [number, [number, number, number, number]][],
  palette: [] as [string, [number, number, number, number]][],
  bools: [] as [string, boolean][],
  defaulted: false,
};

const t = (index: number, name: string) => ({
  index,
  name,
  pieces: [{ text: name }],
  editable: true,
  color: null,
  preset: "All",
  inherits: false,
  columns: [],
});

/** A tab whose name parses to `pieces` — what Rust hands over for markup. */
const styled = (index: number, name: string, pieces: TabPiece[], extra: Partial<OverviewTab> = {}): OverviewTab => ({
  ...t(index, name), pieces, ...extra,
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
    // The backend's parser and writer, stood in for: the editor only previews
    // and switches modes through them.
    parseName: vi.fn(async (raw: string) => ({ pieces: [{ text: raw }], editable: true })),
    formatName: vi.fn(async (pieces: TabPiece[]) => pieces.map((p) => p.text).join("")),
  };
  render(OverviewTabList, { data, tabIndex: 0, ...spies, ...over } as never);
  return spies;
}

/** A row, found by the name it shows. */
const row = (name: string) => screen.getByText(name).closest(".row") as HTMLElement;

describe("the shape of the list", () => {
  test("tabs are grouped by window, in tab_indices order, under Overview {n+1}", () => {
    mount();
    expect(screen.getByText("Overview 1")).toBeTruthy();
    expect(screen.getByText("Overview 2")).toBeTruthy();

    const w1 = screen.getByText("Overview 1").closest("div")!.parentElement!;
    // Read the label buttons, not the whole row: the row also carries a drag
    // grip and a "⋯". main before Mining — the stored strip order IS the
    // in-game order, so the list renders it rather than sorting it.
    const shown = [...w1.querySelectorAll('.row button.label')].map((b) => b.textContent?.trim());
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
  test("each piece reaches the DOM in its own style", () => {
    mount({
      data: { ...data, tabs: [
        styled(0, "<color=0xFFA8C8E8>*</color> <b><i>main</i></b>", [
          { text: "*", color: "FFA8C8E8" }, { text: " " }, { text: "main", bold: true, italic: true, size: 16 },
        ]),
        t(1, "Mining"), t(2, "Travel"), t(3, "loose"),
      ] },
    });
    // #A8C8E8 — jsdom reports colours resolved.
    expect((screen.getByText("*") as HTMLElement).style.color).toBe("rgb(168, 200, 232)");
    const main = (screen.getByText("main") as HTMLElement).style;
    expect([main.fontWeight, main.fontStyle, main.fontSize]).toEqual(["700", "italic", "1.143em"]);
  });

  // The client wraps the whole name in the tab's own colour key.
  test("the tab's own colour tints the pieces that have none", () => {
    mount({ data: { ...data, tabs: [styled(0, "main", [{ text: "main" }], { color: "FF7FFF1F" }), t(1, "Mining"), t(2, "Travel"), t(3, "loose")] } });
    expect((screen.getByText("main") as HTMLElement).style.color).toBe("rgb(127, 255, 31)");
  });

  // Text EVE won't draw is not shown, and the row says so.
  test("a name that won't show fully in game is flagged", () => {
    mount({ data: { ...data, tabs: [
      styled(0, "a<hint=x>b</hint>", [{ text: "a" }, { text: "b", hidden: true }],
        { editable: false, warnings: ["text inside <hint> is not drawn on the tab"] }),
      t(1, "Mining"), t(2, "Travel"), t(3, "loose"),
    ] } });
    expect(screen.queryByText("b")).toBeNull();
    expect(screen.getByRole("img", { name: "Won't show fully in game" }).getAttribute("title")).toContain("hint");
  });

  test("the selected tab is the selected row", () => {
    mount({ tabIndex: 2 });
    expect(row("Travel").querySelector(".label")?.getAttribute("aria-current")).toBe("true");
    expect(row("main").querySelector(".label")?.getAttribute("aria-current")).toBeNull();
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

  // A row only takes a drop before itself, so the end of a window needs its
  // own target — it is the only way into an empty window by drag.
  test("dropping on a window's end zone moves the tab to its end", async () => {
    const { onMove } = mount();
    expect(screen.queryByText(/Drop here/)).toBeNull();
    await fireEvent.dragStart(row("main"));
    const zones = screen.getAllByText(/Drop here/);
    expect(zones).toHaveLength(2); // one per window, none for Other
    await fireEvent.drop(zones[1]);
    expect(onMove).toHaveBeenCalledWith(0, 0, 1, 1);
    expect(screen.queryByText(/Drop here/)).toBeNull();
  });

  test("the end zone reorders within the window too", async () => {
    const { onReorder } = mount();
    await fireEvent.dragStart(row("main"));
    await fireEvent.drop(screen.getAllByText(/Drop here/)[0]);
    expect(onReorder).toHaveBeenCalledWith(0, [1, 0]);
  });

  test("the row being dragged over is marked", async () => {
    mount();
    await fireEvent.dragStart(row("main"));
    await fireEvent.dragOver(row("Mining"));
    expect(row("Mining").classList.contains("over")).toBe(true);
    await fireEvent.drop(row("Mining"));
    expect(row("Mining").classList.contains("over")).toBe(false);
  });
});

describe("the window menu", () => {
  // Present-and-disabled, never absent: the button this replaces appeared and
  // disappeared as the selection moved between windows.
  // Any window can go now: the backend re-keys the later windows' positions.
  test("Remove this window is enabled on the first window too", async () => {
    const { onRemoveWindow } = mount();
    await fireEvent.click(screen.getByRole("button", { name: "Overview 1 actions" }));
    const item = screen.getByRole("menuitem", { name: "Remove this window" }) as HTMLButtonElement;
    expect(item.disabled).toBe(false);
    expect(item.title).toBe("");
    await fireEvent.click(item);
    expect(onRemoveWindow).toHaveBeenCalledWith(0);
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
  }
  const text = (n: number) => screen.getByLabelText(`Piece ${n} text`) as HTMLInputElement;
  const withTabs = (first: OverviewTab) => ({ data: { ...data, tabs: [first, t(1, "Mining"), t(2, "Travel"), t(3, "loose")] } });

  test("Rename selects the row and turns it into an editor", async () => {
    const { onSelect } = mount();
    await startRename(1);
    expect(onSelect).toHaveBeenCalledWith(1);
    // The row it replaced is gone while the editor is up.
    expect(screen.queryByRole("button", { name: "Mining" })).toBeNull();
    expect(text(1).value).toBe("Mining");
  });

  // Spaces inside a piece are how a tab is widened in game, so they go out
  // verbatim.
  test("typed spacing goes out verbatim", async () => {
    const { onRenameTab } = mount();
    await startRename(0);
    await fireEvent.input(text(1), { target: { value: "  fleet  " } });
    await fireEvent.keyDown(text(1), { key: "Enter" });
    expect(onRenameTab).toHaveBeenCalledWith(0, { pieces: [{ text: "  fleet  " }] });
  });

  test("every piece setting commits with the text, as one rename", async () => {
    const { onRenameTab } = mount();
    await startRename(0);
    await fireEvent.click(screen.getByLabelText("Piece 1 colour"));
    await fireEvent.click(screen.getByLabelText("#40ff40"));
    for (const what of ["bold", "italic", "underline"]) await fireEvent.click(screen.getByLabelText(`Piece 1 ${what}`));
    await fireEvent.input(screen.getByLabelText("Piece 1 size"), { target: { value: "16" } });
    await fireEvent.input(screen.getByLabelText("Piece 1 letter spacing"), { target: { value: "2" } });
    await fireEvent.keyDown(text(1), { key: "Enter" });
    expect(onRenameTab).toHaveBeenCalledWith(0, { pieces: [
      { text: "main", color: "FF40FF40", bold: true, italic: true, underline: true, size: 16, spacing: 2 },
    ] });
  });

  test("a second piece can be added, styled on its own, and removed", async () => {
    const { onRenameTab } = mount();
    await startRename(0);
    expect((screen.getByLabelText("Remove piece 1") as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(screen.getByText("+ Piece"));
    await fireEvent.input(text(2), { target: { value: " ops" } });
    await fireEvent.click(screen.getByLabelText("Piece 2 bold"));
    await fireEvent.keyDown(text(2), { key: "Enter" });
    expect(onRenameTab).toHaveBeenCalledWith(0, { pieces: [{ text: "main" }, { text: " ops", bold: true }] });
  });

  test("the editor opens on the tab's own pieces", async () => {
    const { onRenameTab } = mount(withTabs(styled(0, "<color=0xFFA8C8E8>*</color><b> main</b>", [
      { text: "*", color: "FFA8C8E8" }, { text: " main", bold: true },
    ])));
    await startRename(0);
    expect([text(1).value, text(2).value]).toEqual(["*", " main"]);
    expect(screen.getByLabelText("Piece 2 bold").getAttribute("aria-pressed")).toBe("true");
    await fireEvent.keyDown(text(1), { key: "Enter" });
    expect(onRenameTab).toHaveBeenCalledWith(0, { pieces: [{ text: "*", color: "FFA8C8E8" }, { text: " main", bold: true }] });
  });

  // A name using markup the pieces can't carry must not be rebuilt from them:
  // that would drop the markup. It opens as raw markup instead.
  test("a name the pieces can't carry opens as raw markup and commits raw", async () => {
    const { onRenameTab } = mount(withTabs(styled(0, "<uppercase>up</uppercase>", [{ text: "up", uppercase: true }], { editable: false })));
    await startRename(0);
    const box = screen.getByLabelText("Tab name markup") as HTMLInputElement;
    expect(box.value).toBe("<uppercase>up</uppercase>");
    await fireEvent.input(box, { target: { value: "<uppercase>down</uppercase>" } });
    await fireEvent.keyDown(box, { key: "Enter" });
    expect(onRenameTab).toHaveBeenCalledWith(0, { raw: "<uppercase>down</uppercase>" });
  });

  test("Edit markup switches to the markup the pieces would write", async () => {
    const { formatName } = mount();
    await startRename(0);
    await fireEvent.click(screen.getByText("Edit markup"));
    expect(formatName).toHaveBeenCalledWith([{ text: "main" }]);
    expect(((await screen.findByLabelText("Tab name markup")) as HTMLInputElement).value).toBe("main");
  });

  test("Escape cancels and writes nothing", async () => {
    const { onRenameTab } = mount();
    await startRename(1);
    await fireEvent.input(text(1), { target: { value: "Ore" } });
    await fireEvent.keyDown(text(1), { key: "Escape" });
    expect(onRenameTab).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Mining" })).toBeTruthy();
  });

  test("leaving the editor commits, the way the box it replaces did", async () => {
    const { onRenameTab } = mount();
    await startRename(1);
    await fireEvent.input(text(1), { target: { value: "Ore" } });
    await fireEvent.focusOut(text(1), { relatedTarget: null });
    expect(onRenameTab).toHaveBeenCalledWith(1, { pieces: [{ text: "Ore" }] });
  });

  test("moving focus between the editor's own controls does not commit", async () => {
    const { onRenameTab } = mount();
    await startRename(1);
    await fireEvent.focusOut(text(1), { relatedTarget: screen.getByLabelText("Piece 1 bold") });
    expect(onRenameTab).not.toHaveBeenCalled();
  });

  test("an empty name is not a rename", async () => {
    const { onRenameTab } = mount();
    await startRename(1);
    await fireEvent.input(text(1), { target: { value: "   " } });
    await fireEvent.keyDown(text(1), { key: "Enter" });
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
