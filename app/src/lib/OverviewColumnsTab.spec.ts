// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// The width field was gated on the open character's *id*, using it as a proxy
// for "a character document is open". A preset holds column widths but has no
// character id, so the proxy is wrong — this pins the real condition.
import { describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/svelte";
import OverviewColumnsTab from "$lib/OverviewColumnsTab.svelte";
import type { OverviewColumns } from "$lib/api";

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
  appearance: {
    background: { enabled: [], order: [] },
    flag: { enabled: [], order: [] },
    colors: [],
    bools: [],
    defaulted: false,
  },
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
