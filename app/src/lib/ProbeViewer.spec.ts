// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// The viewer's DRAG path is untestable here — it converts client pixels through
// getBoundingClientRect, which jsdom reports as 0x0. Its CAMERA path is not:
// projection runs off the SIZE constant alone, so every probe lands at a real
// viewport coordinate in jsdom and a camera move is directly observable as the
// marker moving. That is what these cover.
import { describe, expect, test } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ProbeViewer from "$lib/ProbeViewer.svelte";

const noop = () => {};
const SIZE = 520; // must match the component's own viewport constant
const CENTRE = SIZE / 2;

/** Two probes well off the origin, so "did the camera re-target" is a large,
 * unambiguous movement rather than a rounding difference. */
const PROBES: [number, number, number][] = [
  [2e10, 0, 0],
  [0, 2e10, 0],
];
const RANGES = [74798935350, 74798935350];

function mount(selected: number | null = null) {
  const { container } = render(ProbeViewer, {
    probes: PROBES,
    ranges: RANGES,
    selected,
    formationId: 0,
    onselect: noop,
    onmove: noop,
    oncommit: noop,
  });
  return container;
}

/** A probe's grab square, by its 1-based label. */
const marker = (c: Element, n: number) =>
  c.querySelector(`[aria-label="probe ${n}"]`) as SVGRectElement;

/** The centre of a grab square, in viewport units. */
const at = (r: SVGRectElement) => ({
  x: Number(r.getAttribute("x")) + Number(r.getAttribute("width")) / 2,
  y: Number(r.getAttribute("y")) + Number(r.getAttribute("height")) / 2,
});

describe("double-click camera shortcuts", () => {
  test("double-clicking a probe puts the camera on it", async () => {
    // The camera opens framing the whole formation, so probe 1 starts off to
    // one side. Focusing it must bring it to the middle of the view.
    const c = mount();
    const before = at(marker(c, 1));
    expect(Math.hypot(before.x - CENTRE, before.y - CENTRE)).toBeGreaterThan(20);

    await fireEvent.dblClick(marker(c, 1));

    const after = at(marker(c, 1));
    expect(after.x).toBeCloseTo(CENTRE, 0);
    expect(after.y).toBeCloseTo(CENTRE, 0);
  });

  test("double-clicking the centre marker returns the camera to the formation centre", async () => {
    const c = mount();
    const centre = () => c.querySelector('[aria-label="formation centre"]') as SVGCircleElement;
    expect(centre()).not.toBeNull();

    await fireEvent.dblClick(marker(c, 1)); // camera is now on the probe
    expect(Number(centre().getAttribute("cx"))).not.toBeCloseTo(CENTRE, 0);

    await fireEvent.dblClick(centre());
    expect(Number(centre().getAttribute("cx"))).toBeCloseTo(CENTRE, 1);
    expect(Number(centre().getAttribute("cy"))).toBeCloseTo(CENTRE, 1);
  });

  test("double-clicking empty space flips between the top and side views", async () => {
    // Side view looks along Z: the probe on +Y is above the middle of the
    // view. Top view looks down Y, so it projects onto the centre instead —
    // near it rather than exactly on it, because the pitch stops just short of
    // 90 to keep the camera basis from degenerating.
    const c = mount();
    const bg = c.querySelector(".bg") as SVGRectElement;

    expect(at(marker(c, 2)).y).toBeLessThan(CENTRE - 20); // opens on the side view

    await fireEvent.dblClick(bg);
    const top = at(marker(c, 2));
    expect(Math.hypot(top.x - CENTRE, top.y - CENTRE)).toBeLessThan(2);

    await fireEvent.dblClick(bg);
    expect(at(marker(c, 2)).y).toBeLessThan(CENTRE - 20); // and back
  });

  test("the gizmo's arrows leave the selected probe clear", async () => {
    // The regression this cost a round trip to find. Each arrow used to be ONE
    // line through the probe, with a 12 px round-capped transparent grab
    // stroke over it — so selecting a probe buried it under three grab lines,
    // and the second click of a double click landed on an arrow instead of the
    // probe. `dblclick` needs both clicks on the same element, so focusing a
    // selected probe was simply impossible.
    //
    // jsdom dispatches events straight at an element and never hit-tests, so
    // it cannot reproduce that directly. The geometry it comes from is
    // checkable: no grab stroke may reach into the probe's own grab square.
    const c = mount(0);
    const probe = at(marker(c, 1));
    const grabs = [...c.querySelectorAll("line.grab")];
    expect(grabs.length).toBeGreaterThan(0);

    for (const g of grabs) {
      // Segments radiate outward from the gap, so the nearer endpoint is the
      // closest the stroke ever comes to the probe.
      const d = Math.min(
        ...([["x1", "y1"], ["x2", "y2"]] as const).map(([xa, ya]) =>
          Math.hypot(Number(g.getAttribute(xa)) - probe.x, Number(g.getAttribute(ya)) - probe.y),
        ),
      );
      expect(d).toBeGreaterThanOrEqual(11); // half the probe's 22 px grab square
    }
  });

  test("double-clicking a probe does not also flip the view", async () => {
    // The three shortcuts share one event: the background's handler has to
    // ignore a double click that a probe or the centre marker already meant
    // something by, or focusing a probe would spin the camera as well.
    const c = mount();
    await fireEvent.dblClick(marker(c, 1));

    // Focused on the probe at +X, still in the side view: the +Y probe stays
    // above the middle of the view. A flip to top would put it level with it.
    expect(at(marker(c, 2)).y).toBeLessThan(CENTRE);
  });
});
