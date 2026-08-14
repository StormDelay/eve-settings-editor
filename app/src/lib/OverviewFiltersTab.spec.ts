// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// The checklist renders 649 group checkboxes across 15 categories, 400 of them
// in `Entity` alone. Every one is a live reactive `checked` expression, so the
// backend round trip behind each tick re-evaluated all 649 for a one-bit
// change. Collapsed categories must therefore cost nothing at all — a
// `<details>` hides its children but Svelte still builds and tracks them.
import { describe, expect, test, vi } from "vitest";
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

// Matched on `.cat-name`, not the whole summary: the summary also holds the
// category's All/None buttons.
const categoryNamed = (name: string): HTMLElement => {
  const s = [...document.querySelectorAll(".group-cat summary")]
    .find((e) => e.querySelector(".cat-name")?.textContent?.trim() === name);
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

describe("the group filter", () => {
  test("does not expand matches until typing pauses", async () => {
    vi.useFakeTimers();
    try {
      mount();
      await vi.advanceTimersByTimeAsync(0);
      const box = document.querySelector(".group-filter") as HTMLInputElement;

      // A query that actually matches: the catalog holds *group* names
      // ("Shuttle"), not type names, so "vexor" would match nothing and the
      // test would pass for the wrong reason.
      await fireEvent.input(box, { target: { value: "shuttle" } });
      // Mid-burst: nothing has expanded yet.
      expect(groupBoxes().length).toBe(0);

      await vi.advanceTimersByTimeAsync(200);
      expect(groupBoxes().length).toBeGreaterThan(0);
    } finally {
      vi.useRealTimers();
    }
  });

  test("clearing the filter collapses what the filter expanded", async () => {
    // Assigning `details.open` fires `toggle` just like a click does, so a
    // filter's auto-expand would otherwise write itself into the user's
    // per-category state and pin those categories open forever. Clearing the
    // box then leaves 450 rows rendered (Entity 400 + Ship 50) with an empty
    // filter — the exact cost this component exists to avoid, reachable by the
    // ordinary type-then-clear flow since any one-letter query matches Entity.
    vi.useFakeTimers();
    try {
      mount();
      await vi.advanceTimersByTimeAsync(0);
      const box = document.querySelector(".group-filter") as HTMLInputElement;

      await fireEvent.input(box, { target: { value: "shuttle" } });
      await vi.advanceTimersByTimeAsync(200);
      expect(groupBoxes().length).toBeGreaterThan(0);

      await fireEvent.input(box, { target: { value: "" } });
      await vi.advanceTimersByTimeAsync(200);
      expect(groupBoxes().length).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  test("a category the user opened by hand survives a filter round trip", async () => {
    // The other half of the same rule: only a DIVERGENCE from the filter's
    // default is the user's choice. Opening Ship with no filter is a real
    // choice and must outlive a query that comes and goes.
    vi.useFakeTimers();
    try {
      mount();
      await vi.advanceTimersByTimeAsync(0);
      const ship = categoryNamed("Ship").closest("details") as HTMLDetailsElement;
      ship.open = true;
      await fireEvent(ship, new Event("toggle"));
      expect(groupBoxes().length).toBe(50);

      const box = document.querySelector(".group-filter") as HTMLInputElement;
      await fireEvent.input(box, { target: { value: "shuttle" } });
      await vi.advanceTimersByTimeAsync(200);
      await fireEvent.input(box, { target: { value: "" } });
      await vi.advanceTimersByTimeAsync(200);

      expect(groupBoxes().length).toBe(50);
    } finally {
      vi.useRealTimers();
    }
  });
});

// The buttons are direct children of the summary now, not of a `.cat-bulk`
// wrapper: that wrapper made them trail the name inline, so they landed at a
// different x on every category. `categoryNamed` already returns the summary.
const bulkButton = (category: string, label: "All" | "None"): HTMLElement => {
  const b = [...categoryNamed(category).querySelectorAll("button")]
    .find((e) => e.textContent?.trim() === label);
  if (!b) throw new Error(`no ${label} button on "${category}"`);
  return b as HTMLElement;
};

describe("per-category select all", () => {
  test("selects the whole category in ONE backend call", async () => {
    mount();
    await waitFor(() => expect(categoryNamed("Ship")).toBeTruthy());

    await fireEvent.click(bulkButton("Ship", "All"));

    // One call, not one per group — the whole point of the bulk path.
    const sent = calls.only("preset_set_groups").args as { name: string; groups: number[] };
    expect(sent.groups.length).toBe(50);
  });

  test("deselects only that category, leaving other groups alone", async () => {
    // Seed a preset holding 25 (Frigate, in Ship) and 100 (Combat Drone, not);
    // None on Ship must drop the first and keep the second.
    calls.stub("sync_group_catalog", []);
    const seeded = { ...data, presets: [{ name: "Mine", groups: [25, 100], filtered_states: [], always_shown_states: [] }] };
    render(OverviewFiltersTab, { data: seeded, tabIndex: 0, onChanged: noop, onUserDirty: noop });
    await waitFor(() => expect(categoryNamed("Ship")).toBeTruthy());

    await fireEvent.click(bulkButton("Ship", "None"));

    const sent = calls.only("preset_set_groups").args as { name: string; groups: number[] };
    expect(sent.groups).not.toContain(25);
    expect(sent.groups).toContain(100);
  });

  test("acts on the groups the filter is showing, not the whole category", async () => {
    vi.useFakeTimers();
    try {
      mount();
      await vi.advanceTimersByTimeAsync(0);
      const box = document.querySelector(".group-filter") as HTMLInputElement;
      await fireEvent.input(box, { target: { value: "shuttle" } });
      await vi.advanceTimersByTimeAsync(200);

      await fireEvent.click(bulkButton("Ship", "All"));

      const sent = calls.only("preset_set_groups").args as { name: string; groups: number[] };
      expect(sent.groups.length).toBeGreaterThan(0);
      expect(sent.groups.length).toBeLessThan(50);
    } finally {
      vi.useRealTimers();
    }
  });

  test("does not collapse the category it just filled", async () => {
    mount();
    await waitFor(() => expect(categoryNamed("Ship")).toBeTruthy());
    const ship = categoryNamed("Ship").closest("details") as HTMLDetailsElement;
    ship.open = true;
    await fireEvent(ship, new Event("toggle"));
    expect(groupBoxes().length).toBe(50);

    // A click inside a <summary> toggles the <details> unless the handler
    // prevents it — which would shut the category as you bulk-select it.
    await fireEvent.click(bulkButton("Ship", "All"));
    expect(ship.open).toBe(true);
  });
});
