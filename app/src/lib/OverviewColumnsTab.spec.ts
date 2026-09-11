// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// The width field was gated on the open character's *id*, using it as a proxy
// for "a character document is open". A preset holds column widths but has no
// character id, so the proxy is wrong — this pins the real condition.
import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import OverviewColumnsTab from "$lib/OverviewColumnsTab.svelte";
import { calls } from "$lib/test/setup";
import type { OverviewColumns } from "$lib/api";

const appearance = {
  background: { enabled: [], order: [] },
  flag: { enabled: [], order: [] },
  colors: [] as [number, [number, number, number, number]][],
  bools: [] as [string, boolean][],
  defaulted: false,
};

const data: OverviewColumns = {
  tabs: [
    {
      index: 0,
      name: "Default",
      preset: "All",
      inherits: false,
      columns: [{ name: "NAME", label: "Name", visible: true, width: 120 }],
    },
  ],
  windows: [{ index: 0, tab_indices: [0] }],
  presets: [],
  appearance,
};

// Three tabs across two windows, one of them unassigned — the copy panel has to
// group them the way the tab selector does. Tab 1's name carries markup, so the
// target list also pins that it is shown readable rather than raw.
const multi: OverviewColumns = {
  tabs: [
    { index: 0, name: "  main  ", preset: "All", inherits: false, columns: [{ name: "NAME", label: "Name", visible: true, width: 120 }] },
    { index: 1, name: "<color=0xFFFF6F75><b>pvp</b></color>", preset: "All", inherits: false, columns: [] },
    { index: 2, name: "loose", preset: "All", inherits: false, columns: [] },
  ],
  windows: [{ index: 0, tab_indices: [0] }, { index: 1, tab_indices: [1] }],
  presets: [],
  appearance,
};

const noop = () => {};

describe("the column width field", () => {
  test("is editable whenever a character document is open", () => {
    render(OverviewColumnsTab, {
      data,
      tabIndex: 0,
      charOpen: true,
      onChanged: noop,
      onUserDirty: noop,
      onCharDirty: noop,
    });
    expect((screen.getByRole("spinbutton") as HTMLInputElement).disabled).toBe(false);
  });

  test("is disabled when no character document is open", () => {
    render(OverviewColumnsTab, {
      data,
      tabIndex: 0,
      charOpen: false,
      onChanged: noop,
      onUserDirty: noop,
      onCharDirty: noop,
    });
    expect((screen.getByRole("spinbutton") as HTMLInputElement).disabled).toBe(true);
  });
});

const openPanel = async (charOpen = true) => {
  render(OverviewColumnsTab, { data: multi, tabIndex: 0, charOpen, onChanged: noop, onUserDirty: noop, onCharDirty: noop });
  await fireEvent.click(screen.getByTitle("Copy this tab's column settings onto other tabs"));
};
const targetBoxes = () => [...document.querySelectorAll(".copy-targets input[type='checkbox']")] as HTMLInputElement[];
const partBox = (label: string) =>
  [...document.querySelectorAll(".copy-parts label")]
    .find((l) => l.textContent?.includes(label))!
    .querySelector("input") as HTMLInputElement;

describe("copying columns to other tabs", () => {
  test("lists the other tabs, readably, and none of them ticked", async () => {
    await openPanel();
    const labels = [...document.querySelectorAll(".copy-targets label")].map((l) => l.textContent?.trim());
    // The source tab is not a target of itself.
    expect(labels).toEqual(["pvp", "loose"]);
    expect(targetBoxes().every((b) => !b.checked)).toBe(true);
    // …while all three properties start on.
    expect([partBox("Column order"), partBox("Visible columns"), partBox("Widths")].every((b) => b.checked)).toBe(true);
  });

  test("All ticks every target", async () => {
    await openPanel();
    await fireEvent.click(screen.getByText("All"));
    expect(targetBoxes().every((b) => b.checked)).toBe(true);

    await fireEvent.click(screen.getByText("None"));
    expect(targetBoxes().some((b) => b.checked)).toBe(false);
  });

  test("sends one command carrying the ticked targets and properties", async () => {
    const onUserDirty = vi.fn();
    const onCharDirty = vi.fn();
    render(OverviewColumnsTab, { data: multi, tabIndex: 0, charOpen: true, onChanged: noop, onUserDirty, onCharDirty });
    await fireEvent.click(screen.getByTitle("Copy this tab's column settings onto other tabs"));
    await fireEvent.click(screen.getByText("All"));
    await fireEvent.click(partBox("Visible columns"));

    await fireEvent.click(screen.getByText(/^Copy to 2 tabs$/));

    expect(calls.only("overview_copy_columns").args).toEqual({
      fromTab: 0, toTabs: [1, 2], order: true, visible: false, widths: true,
    });
    // Two files touched, so both slots must be dirtied or the widths are lost.
    expect(onUserDirty).toHaveBeenCalled();
    expect(onCharDirty).toHaveBeenCalled();
  });

  test("widths are off and unavailable with no character open", async () => {
    const onCharDirty = vi.fn();
    render(OverviewColumnsTab, { data: multi, tabIndex: 0, charOpen: false, onChanged: noop, onUserDirty: noop, onCharDirty });
    await fireEvent.click(screen.getByTitle("Copy this tab's column settings onto other tabs"));

    const widths = partBox("Widths");
    expect(widths.disabled).toBe(true);
    expect(widths.checked).toBe(false);

    await fireEvent.click(screen.getByText("All"));
    await fireEvent.click(screen.getByText(/^Copy to 2 tabs$/));

    expect((calls.only("overview_copy_columns").args as { widths: boolean }).widths).toBe(false);
    expect(onCharDirty).not.toHaveBeenCalled();
  });
});
