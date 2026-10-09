// Component test: run with `npm run test:ui` (vitest + jsdom). jsdom has no
// canvas, so this pins what the preview ASKS for and SAYS; the layout rules are
// overview_fit.rs's tests.
import { expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import OverviewWidthPreview from "$lib/OverviewWidthPreview.svelte";
import { calls } from "$lib/test/setup";
import type { Fit, OverviewTab } from "$lib/api";

const tab: OverviewTab = {
  index: 3, name: "Main", preset: "All", inherits: false, pieces: [{ text: "Main" }], editable: true, color: null,
  columns: [{ name: "VELOCITY", label: "Velocity", visible: true, width: 45 }],
};
const props = { tab, charOpen: false, onWidth: () => {} };
const fit: Fit = {
  font_size: "Tiny", ui_scale: 1.25, use_small_text: false, sorted_by: "DISTANCE", row_width: null,
  total_width: 45, rows: [{ VELOCITY: "12,345" }], columns: [],
};
const fitRequests = () => calls.log.filter((c) => c.cmd === "overview_fit").map((c) => c.args!.req as Record<string, unknown>);

test("asks for the tab with EVE's pixels, then keeps the backend's sample rows", async () => {
  calls.stub("overview_fit", fit);
  render(OverviewWidthPreview, props);
  expect(await screen.findByText(/Tiny font, UI scale 125%/)).toBeTruthy();
  expect(fitRequests()[0]).toMatchObject({ tab: 3, rows: null, sorted_by: "DISTANCE", masks: true, widths: {} });
  // The defaults come back once and are the editable rows from then on.
  await vi.waitFor(() => expect(fitRequests()[1]?.rows).toEqual([{ VELOCITY: "12,345" }]));
});

test("shows why there is no preview instead of guessing", async () => {
  calls.stub("overview_fit", () => { throw { code: "fonts", message: "EVE's fonts couldn't be read, so nothing can be measured" }; });
  render(OverviewWidthPreview, props);
  expect((await screen.findByText(/No preview/)).textContent).toContain("nothing can be measured");
});
