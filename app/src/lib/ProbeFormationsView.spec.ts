// Component test: run with `npm run test:ui` (vitest + jsdom).
import { describe, expect, test } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import ProbeFormationsView from "$lib/ProbeFormationsView.svelte";
import { calls } from "$lib/test/setup";
import type { Formations } from "$lib/api";

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
  });
});
