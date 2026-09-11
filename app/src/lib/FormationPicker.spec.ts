// Component test: run with `npm run test:ui` (vitest + jsdom).
import { describe, expect, test, vi } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import FormationPicker from "$lib/FormationPicker.svelte";
import type { FormationSpec } from "$lib/api";
// Imported for its afterEach cleanup: without it every render stays in the
// document and the next test's queries match two copies of everything.
import "$lib/test/setup";

const ITEMS: FormationSpec[] = [
  { name: "close", probes: [[1, 0, 0], [2, 0, 0]], ranges: [74798935350, 74798935350] },
  { name: "on grid", probes: [[3, 0, 0]], ranges: [598391482800] },
  { name: "odd", probes: [[4, 0, 0], [5, 0, 0]], ranges: [74798935350, 149597870700] },
];

function open(onconfirm = vi.fn(), oncancel = vi.fn()) {
  render(FormationPicker, {
    title: "Import formations",
    items: ITEMS,
    confirmLabel: "Import",
    onconfirm,
    oncancel,
  });
  return { onconfirm, oncancel };
}

describe("FormationPicker", () => {
  test("everything starts ticked and confirm carries the count", async () => {
    open();
    for (const f of ITEMS) {
      expect((screen.getByLabelText(f.name) as HTMLInputElement).checked).toBe(true);
    }
    expect(screen.getByText("Import 3")).toBeTruthy();
  });

  test("confirm hands back the indices that are still ticked", async () => {
    const { onconfirm } = open();
    await fireEvent.click(screen.getByLabelText("on grid"));
    await fireEvent.click(screen.getByText("Import 2"));
    expect(onconfirm).toHaveBeenCalledWith([0, 2]);
  });

  test("with nothing ticked, confirm is disabled", async () => {
    open();
    // Everything is on, so the button offers the inverse.
    await fireEvent.click(screen.getByText("None"));
    expect((screen.getByText("Import 0") as HTMLButtonElement).disabled).toBe(true);
  });

  test("select all re-ticks everything after a manual untick", async () => {
    open();
    await fireEvent.click(screen.getByLabelText("close"));
    await fireEvent.click(screen.getByText("All"));
    expect(screen.getByText("Import 3")).toBeTruthy();
  });

  test("a row shows its probe count and range, and says mixed when they differ", async () => {
    open();
    expect(screen.getByText("2 probes · 0.5 AU")).toBeTruthy();
    expect(screen.getByText("1 probe · 4 AU")).toBeTruthy();
    expect(screen.getByText("2 probes · mixed")).toBeTruthy();
  });

  test("clicking the backdrop cancels", async () => {
    const { oncancel } = open();
    await fireEvent.click(screen.getByTestId("picker-backdrop"));
    expect(oncancel).toHaveBeenCalled();
  });

  test("clicking the meta text toggles the row, not just the checkbox", async () => {
    open();
    await fireEvent.click(screen.getByText("2 probes · 0.5 AU"));
    expect((screen.getByLabelText("close") as HTMLInputElement).checked).toBe(false);
  });
});
