// Component test: run with `npm run test:ui` (vitest + jsdom).
import { describe, expect, test } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import ProbeFormationsView from "$lib/ProbeFormationsView.svelte";
import { calls } from "$lib/test/setup";
import type { Formation, Formations } from "$lib/api";

const noop = () => {};

// A coordinate with bits well below what any rounded AU display can carry.
const AWKWARD: [number, number, number] = [-1199120384.7, -115136512.3, -415997952.9];

const FORMATIONS: Formations = {
  formations: [
    {
      id: 0,
      name: "close",
      probes: [AWKWARD, [1e9, 2e9, 3e9]],
      ranges: [74798935350, 74798935350],
      range: 74798935350,
      mixed_range: false,
    },
  ],
  selected: 0,
};

async function open() {
  calls.stub("probe_formations", FORMATIONS);
  calls.stub("set_probe_formation", FORMATIONS);
  render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });
  await screen.findByDisplayValue("close");
}

/** The arguments of the last set_probe_formation call. */
const lastSet = () => {
  const c = [...calls.log].reverse().find((x) => x.cmd === "set_probe_formation");
  return c?.args as { id: number | null; name: string; probes: number[][]; range: number };
};

describe("precision", () => {
  test("an untouched coordinate is sent back to the metre", async () => {
    // One metre is 6.7e-12 AU. If a displayed, rounded AU string were the
    // source of truth, saving after editing ANY field would displace every
    // other probe in the formation — silently, and on every save.
    await open();
    const nameField = await screen.findByDisplayValue("close");
    await fireEvent.input(nameField, { target: { value: "closer" } });
    await fireEvent.blur(nameField);

    const args = lastSet();
    expect(args.name).toBe("closer");
    expect(args.probes[0]).toEqual(AWKWARD);
  });
});

describe("editing", () => {
  test("typing a distance moves the probe along its existing direction", async () => {
    await open();
    // Probe 1's distance field, doubled. Its angles must not change, so the
    // new position is the old one scaled by two.
    const dist = await screen.findByLabelText("probe 1 distance");
    const before = AWKWARD;
    const r = Math.hypot(...before);
    await fireEvent.input(dist, { target: { value: String((r * 2) / 149597870700) } });
    await fireEvent.blur(dist);

    const p = lastSet().probes[0];
    for (let i = 0; i < 3; i++) expect(p[i]).toBeCloseTo(before[i] * 2, 0);
    // The scenario the "untouched coordinate" test's comment describes: edit
    // ONE probe, and a DIFFERENT one must not move at all.
    expect(lastSet().probes[1]).toEqual([1e9, 2e9, 3e9]);
  });

  test("typing a negative range is rejected, not sent to set_probe_formation", async () => {
    // A range of zero or less is meaningless in EVE and would otherwise be
    // written straight to the user's real settings file — and it drives a
    // range-circle radius in the visualiser panes, which is invalid SVG if
    // negative (probes.ts/ProbeFormationsView.svelte, review fix round 1).
    await open();
    const range = await screen.findByLabelText("formation range");
    await fireEvent.input(range, { target: { value: "-5" } });
    await fireEvent.blur(range);

    expect(lastSet().range).toBeGreaterThan(0);
    expect(lastSet().range).toBe(74798935350); // the loaded value, untouched
  });

  test("New selects the newly minted formation even when its id fills a gap", async () => {
    // next_id fills the lowest free gap (probes.rs), so with ids {0, 2} the
    // new formation lands at id 1 — the MIDDLE of the sorted response, not
    // its end. Selecting by position would land on id 2 ("b") instead.
    const a: Formation = { id: 0, name: "a", probes: [[1, 2, 3]], ranges: [74798935350], range: 74798935350, mixed_range: false };
    const b: Formation = { id: 2, name: "b", probes: [[4, 5, 6]], ranges: [74798935350], range: 74798935350, mixed_range: false };
    const created: Formation = { id: 1, name: "New formation", probes: [[0, 0, 0]], ranges: [74798935350], range: 74798935350, mixed_range: false };
    calls.stub("probe_formations", { formations: [a, b], selected: 0 } satisfies Formations);
    calls.stub("set_probe_formation", { formations: [a, created, b], selected: 1 } satisfies Formations);
    render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });
    await screen.findByDisplayValue("a");

    await fireEvent.click(screen.getByText("New"));

    expect(await screen.findByDisplayValue("New formation")).toBeTruthy();
  });

  test("a mixed-range formation does not offer an edit that would flatten it", async () => {
    calls.stub("probe_formations", {
      formations: [{
        ...FORMATIONS.formations[0],
        ranges: [74798935350, 37399467675],
        mixed_range: true,
      }],
      selected: 0,
    } satisfies Formations);
    render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });
    const range = await screen.findByLabelText("formation range");
    expect((range as HTMLInputElement).disabled).toBe(true);
    // And it says WHICH row differs, not just that one does.
    expect(await screen.findByText(/probes 2/)).toBeTruthy();
    // The whole formation is read-only, not just the range field — the
    // editor writes one range for the whole formation, so any other edit
    // would flatten the mix just as surely as the range field would.
    const nameField = await screen.findByDisplayValue("close");
    expect((nameField as HTMLInputElement).disabled).toBe(true);
    const x = await screen.findByLabelText("probe 1 X");
    expect((x as HTMLInputElement).disabled).toBe(true);
  });

  test("Copy with uniform range creates a new formation, leaving the mixed original untouched", async () => {
    const mixed: Formation = {
      ...FORMATIONS.formations[0],
      ranges: [74798935350, 37399467675],
      mixed_range: true,
    };
    const copy: Formation = { ...mixed, id: 1, name: "close copy", ranges: [74798935350, 74798935350], mixed_range: false };
    calls.stub("probe_formations", { formations: [mixed], selected: 0 } satisfies Formations);
    calls.stub("set_probe_formation", { formations: [mixed, copy], selected: 1 } satisfies Formations);
    render(ProbeFormationsView, { userOpen: true, userId: 1, onUserDirty: noop });
    await screen.findByDisplayValue("close");

    await fireEvent.click(screen.getByText("Copy with uniform range"));

    const args = lastSet();
    expect(args.id).toBe(null); // a create, never a write onto the mixed original
    expect(await screen.findByDisplayValue("close copy")).toBeTruthy();
  });
});
