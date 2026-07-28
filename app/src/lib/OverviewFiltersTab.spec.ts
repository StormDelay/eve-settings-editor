// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// The checklist renders 649 group checkboxes across 15 categories, 400 of them
// in `Entity` alone. Every one is a live reactive `checked` expression, so the
// backend round trip behind each tick re-evaluated all 649 for a one-bit
// change. Collapsed categories must therefore cost nothing at all — a
// `<details>` hides its children but Svelte still builds and tracks them.
import { describe, expect, test } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import OverviewFiltersTab from "$lib/OverviewFiltersTab.svelte";
import { calls } from "$lib/test/setup";
import type { OverviewColumns } from "$lib/api";

const data: OverviewColumns = {
  tabs: [{ index: 0, name: "Default", preset: "Mine", inherits: false, columns: [] }],
  windows: [{ index: 0, tab_indices: [0] }],
  // A stored preset with no groups: the checklist is editable, and nothing is
  // pre-ticked, so a row count is a clean count of what got rendered.
  presets: [{ name: "Mine", groups: [], filtered_states: [], always_shown_states: [] }],
  appearance: {
    background: { enabled: [], order: [] },
    flag: { enabled: [], order: [] },
    colors: [],
    bools: [],
    defaulted: false,
  },
};

const noop = () => {};

function mount() {
  calls.stub("sync_group_catalog", []);
  render(OverviewFiltersTab, { data, tabIndex: 0, onChanged: noop, onUserDirty: noop });
}

/// Group rows only — the Exceptions block below uses radios, not checkboxes.
const groupBoxes = () => document.querySelectorAll(".group-grid input[type='checkbox']");

const categoryNamed = (name: string): HTMLElement => {
  const s = [...document.querySelectorAll(".group-cat summary")].find((e) => e.textContent?.trim() === name);
  if (!s) throw new Error(`no category summary "${name}"`);
  return s as HTMLElement;
};

describe("the group checklist", () => {
  test("renders no checkbox rows while every category is collapsed", async () => {
    mount();
    // The categories themselves must still be listed — this is about their rows.
    await waitFor(() => expect(categoryNamed("Ship")).toBeTruthy());
    expect(groupBoxes().length).toBe(0);
  });

  test("opening one category renders that category's rows and no others", async () => {
    mount();
    await waitFor(() => expect(categoryNamed("Ship")).toBeTruthy());

    const ship = categoryNamed("Ship").closest("details") as HTMLDetailsElement;
    ship.open = true;
    await fireEvent(ship, new Event("toggle"));

    // Ship holds 50 of the catalog's 649 groups; Entity's 400 stay unrendered.
    await waitFor(() => expect(groupBoxes().length).toBe(50));
  });

  test("closing it again releases the rows", async () => {
    mount();
    await waitFor(() => expect(categoryNamed("Ship")).toBeTruthy());
    const ship = categoryNamed("Ship").closest("details") as HTMLDetailsElement;

    ship.open = true;
    await fireEvent(ship, new Event("toggle"));
    await waitFor(() => expect(groupBoxes().length).toBe(50));

    ship.open = false;
    await fireEvent(ship, new Event("toggle"));
    await waitFor(() => expect(groupBoxes().length).toBe(0));
  });
});
