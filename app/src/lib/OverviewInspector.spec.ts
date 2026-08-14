// Component test (vitest + jsdom).
//
// The pane that replaced the eight-button toolbar. What it owns alone: a Name
// field that must not rewrite a name by the act of showing it, an "In window"
// select that reports the tab's real window instead of a permanent
// instruction, and the two-sentence-plus-one explanation of the storage split
// that the toolbar's "Character (for widths)" label was trying to compress into
// three words.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import OverviewInspector from "$lib/OverviewInspector.svelte";
import type { OverviewTab, OverviewWindow } from "$lib/api";

const t = (index: number, name: string): OverviewTab => ({
  index,
  name,
  preset: "All",
  inherits: false,
  columns: [],
});

const windows: OverviewWindow[] = [
  { index: 0, tab_indices: [0] },
  { index: 1, tab_indices: [1] },
];

function mount(over: Record<string, unknown> = {}) {
  const spies = {
    onRename: vi.fn(),
    onMove: vi.fn(),
    onLoadCharacter: vi.fn(),
    onShowAccounts: vi.fn(),
  };
  render(OverviewInspector, {
    tab: t(0, "   main   "),
    windows,
    currentWindowIndex: 0,
    charId: 9,
    characters: [9, 11],
    ...spies,
    ...over,
  } as never);
  return spies;
}

const nameBox = () => document.querySelector(".inspect input") as HTMLInputElement;

describe("the Name field", () => {
  // Padding is how a tab is widened in game, so the field carries it verbatim.
  test("is seeded with the readable text, spacing and all", () => {
    mount();
    expect(nameBox().value).toBe("   main   ");
  });

  test("commits on Enter, keeping the colour and the bold", async () => {
    const { onRename } = mount({ tab: t(0, "<color=0xFFFF6F75><b>   main   </b></color>") });
    const box = nameBox();
    await fireEvent.input(box, { target: { value: "  fleet  " } });
    await fireEvent.keyDown(box, { key: "Enter" });
    expect(onRename).toHaveBeenCalledWith(0, "<color=0xFFFF6F75><b>  fleet  </b></color>");
  });

  test("commits on blur too", async () => {
    const { onRename } = mount();
    const box = nameBox();
    await fireEvent.input(box, { target: { value: "fleet" } });
    await fireEvent.blur(box);
    expect(onRename).toHaveBeenCalledWith(0, "fleet");
  });

  // A name that did not change is not an edit.
  test("committing an unchanged name writes nothing", async () => {
    const { onRename } = mount();
    await fireEvent.blur(nameBox());
    expect(onRename).not.toHaveBeenCalled();
  });

  // The field is SEEDED, not bound to a derived parse: a name the parser cannot
  // decompose must survive being looked at, focused and blurred.
  test("an unparseable name is never rewritten by being looked at", async () => {
    const weird = "<color=0xFFFF0000>a</color><color=0xFF00FF00>b</color>";
    const { onRename } = mount({ tab: t(0, weird) });
    expect(nameBox().value).toBe(weird);
    expect(screen.getByLabelText("Tab name colour").textContent?.trim()).toBe("—");

    await fireEvent.focus(nameBox());
    await fireEvent.blur(nameBox());
    expect(onRename).not.toHaveBeenCalled();
  });
});

describe("colour and bold", () => {
  const marked = "<color=0xFFFF6F75>   <b>main</b>   </color>";

  test("picking a colour keeps the text, the padding and the bold", async () => {
    const { onRename } = mount({ tab: t(0, marked) });
    await fireEvent.click(screen.getByLabelText("Tab name colour"));
    await fireEvent.click(screen.getByLabelText("#40ff40"));
    expect(onRename).toHaveBeenCalledWith(0, "<color=0xFF40FF40><b>   main   </b></color>");
  });

  test("No colour drops the span and nothing else", async () => {
    const { onRename } = mount({ tab: t(0, marked) });
    await fireEvent.click(screen.getByLabelText("Tab name colour"));
    await fireEvent.click(screen.getByText("No colour"));
    expect(onRename).toHaveBeenCalledWith(0, "<b>   main   </b>");
  });

  test("B toggles bold off, keeping the colour and the padding", async () => {
    const { onRename } = mount({ tab: t(0, marked) });
    await fireEvent.click(screen.getByTitle("Bold tab name"));
    expect(onRename).toHaveBeenCalledWith(0, "<color=0xFFFF6F75>   main   </color>");
  });
});

describe("In window", () => {
  // The direct regression test for a select that reset itself to a permanent
  // instruction on every use and therefore never reported state.
  test("shows the tab's real window, and does not reset after a change", async () => {
    const { onMove } = mount({ tab: t(1, "Travel"), currentWindowIndex: 1 });
    const sel = screen.getByLabelText("In window") as HTMLSelectElement;
    expect(sel.value).toBe("1");

    await fireEvent.change(sel, { target: { value: "0" } });
    expect(onMove).toHaveBeenCalledWith(0);
    expect(sel.value).not.toBe("");
  });

  test("is disabled, with the reason, for a tab in no window", () => {
    mount({ currentWindowIndex: null });
    const sel = screen.getByLabelText("In window") as HTMLSelectElement;
    expect(sel.disabled).toBe(true);
    expect(sel.title).toMatch(/isn't assigned to a window/i);
  });
});

describe("Widths", () => {
  test("lists the account's characters and loads the picked one", async () => {
    const { onLoadCharacter } = mount();
    const sel = screen.getByLabelText("Widths from") as HTMLSelectElement;
    expect([...sel.options].map((o) => o.value)).toContain("11");
    await fireEvent.change(sel, { target: { value: "11" } });
    expect(onLoadCharacter).toHaveBeenCalledWith(11);
  });

  test("with no characters, the field is replaced by the pairing offer", async () => {
    const { onShowAccounts } = mount({ characters: [] });
    expect(screen.queryByLabelText("Widths from")).toBeNull();
    expect(screen.getByText(/no characters associated/i)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: /pair a character/i }));
    expect(onShowAccounts).toHaveBeenCalled();
  });

  // The cheapest possible guard against the width-swap ceiling's disclosure
  // being dropped in a later tidy. The third sentence is the one that matters.
  test("the helper text carries all three sentences", () => {
    mount();
    const p = document.querySelector(".helper")?.textContent ?? "";
    expect(p).toMatch(/stored per character/i);
    expect(p).toMatch(/shared by the whole\s+account/i);
    expect(p).toMatch(/moves widths with the position, not with the tab/i);
  });
});

test("with no tab selected it says so, and writes nothing", () => {
  const { onRename } = mount({ tab: null });
  expect(screen.getByText(/select a tab to edit/i)).toBeTruthy();
  expect(onRename).not.toHaveBeenCalled();
});
