// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// The viewer's DRAG path is untestable here — it converts client pixels through
// getBoundingClientRect, which jsdom reports as 0x0. Its CAMERA path is not:
// projection runs off the SIZE constant alone, so every probe lands at a real
// viewport coordinate in jsdom and a camera move is directly observable as the
// marker moving. That is what these cover.
import { describe, expect, test, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ProbeViewer from "$lib/ProbeViewer.svelte";

const noop = () => {};
const SIZE = 520; // must match the component's own viewport constant
const CENTRE = SIZE / 2;

/** Two probes well off the origin, so "did the camera re-target" is a large,
 * unambiguous movement rather than a rounding difference. They differ in Z as
 * well as laterally, so in the opening side view they sit at genuinely
 * different depths — which is what makes the cube-shape check below mean
 * something. */
const PROBES: [number, number, number][] = [
  [2e10, 0, 1e10],
  [0, 2e10, 0],
];
const RANGES = [74798935350, 74798935350];

function mount(selected: number | null = null, probes = PROBES) {
  const { container } = render(ProbeViewer, {
    probes,
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

/** Double-click an element the way a browser produces one: two press/release
 * pairs on it, then the `dblclick`.
 *
 * The presses are not decoration. Grabbing a cube face or the background takes
 * pointer capture on the svg, and a captured pointer retargets the click that
 * follows to the capture element — so the viewer reads a double click from
 * what was PRESSED, not from the event's own target. Firing a bare `dblclick`
 * tests a path the browser never takes, and hid exactly this bug.
 */
async function doubleClick(el: Element) {
  for (let i = 0; i < 2; i++) {
    await fireEvent.pointerDown(el, { button: 0, pointerId: 1 });
    await fireEvent.pointerUp(el, { button: 0, pointerId: 1 });
  }
  await fireEvent.dblClick(el);
}

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

    await doubleClick(marker(c, 1));

    const after = at(marker(c, 1));
    expect(after.x).toBeCloseTo(CENTRE, 0);
    expect(after.y).toBeCloseTo(CENTRE, 0);
  });

  test("double-clicking the centre marker returns the camera to the formation centre", async () => {
    const c = mount();
    const centre = () => c.querySelector('[aria-label="formation centre"]') as SVGCircleElement;
    expect(centre()).not.toBeNull();

    await doubleClick(marker(c, 1)); // camera is now on the probe
    expect(Number(centre().getAttribute("cx"))).not.toBeCloseTo(CENTRE, 0);

    await doubleClick(centre());
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

    await doubleClick(bg);
    const top = at(marker(c, 2));
    expect(Math.hypot(top.x - CENTRE, top.y - CENTRE)).toBeLessThan(2);

    await doubleClick(bg);
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
    // Probe 1's OWN arrows. Every probe carries a set now, so an unqualified
    // query would measure a neighbour's arrows against this probe's centre.
    const grabs = [...c.querySelectorAll('line.grab[aria-label^="drag probe 1 "]')];
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
    await doubleClick(marker(c, 1));

    // Focused on the probe at +X, still in the side view: the +Y probe stays
    // above the middle of the view. A flip to top would put it level with it.
    expect(at(marker(c, 2)).y).toBeLessThan(CENTRE);
  });

  test("a cube's up face is the world up face, seen only from above", async () => {
    // The cubes are objects in the scene with a FIXED world orientation: the
    // +Y face is the up face from every camera, and moving the camera just
    // shows the scene from somewhere else. The light is world-fixed and comes
    // from above, so the up face is the brightest of the six — which makes
    // brightness a direct read on whether it is the face being shown.
    //
    // No magic numbers: the two cameras are compared against each other.
    const brightest = (c: Element) =>
      Math.max(
        ...[...c.querySelectorAll("polygon.probe-face")].map((p) => {
          const [r, g, b] = p.getAttribute("fill")!.match(/\d+/g)!.map(Number);
          return r + g + b;
        }),
      );

    const above = mount();
    await doubleClick(above.querySelector(".bg") as SVGRectElement); // side -> top
    const fromAbove = brightest(above);

    const below = mount();
    const svg = below.querySelector("svg") as SVGSVGElement;
    // UP, to swing the camera under the formation: a downward drag brings the
    // near side down with the pointer, which lifts the camera over the top.
    await fireEvent.pointerDown(svg, { button: 0, pointerId: 1, clientX: 0, clientY: 0 });
    await fireEvent.pointerMove(svg, { button: 0, pointerId: 1, clientX: 0, clientY: -400 });
    await fireEvent.pointerUp(svg, { button: 0, pointerId: 1, clientX: 0, clientY: -400 });
    const fromBelow = brightest(below);

    expect(fromAbove).toBeGreaterThan(fromBelow);
  });

  test("dragging right turns the scene right", async () => {
    // Which way the scene follows the pointer has been flipped once in each
    // direction by eye. It is a one-character change and nothing else in the
    // suite would notice, so it is pinned here: drag right, and a probe on the
    // near side of the formation travels right, like spinning a globe.
    // A probe on +Z: the near side of the formation in the opening side view.
    // It has to be the near side and not, say, +X — a probe out on the axis
    // the camera swings around travels toward the middle whichever way you
    // turn, so it cannot tell the two directions apart.
    const c = mount(null, [[0, 0, 2e10], [0, 2e10, 0]]);
    const svg = c.querySelector("svg") as SVGSVGElement;
    const before = at(marker(c, 1));

    await fireEvent.pointerDown(svg, { button: 0, pointerId: 1, clientX: 0, clientY: 0 });
    await fireEvent.pointerMove(svg, { button: 0, pointerId: 1, clientX: 40, clientY: 0 });
    await fireEvent.pointerUp(svg, { button: 0, pointerId: 1, clientX: 40, clientY: 0 });

    expect(at(marker(c, 1)).x).toBeGreaterThan(before.x);
  });

  test("dragging down turns the scene down", async () => {
    // The vertical drag has to agree with the horizontal one or the view feels
    // nothing like the client's. It did not: this ran inverted, pushing the
    // near side up while a rightward drag carried it right.
    const c = mount(null, [[0, 0, 2e10], [0, 2e10, 0]]);
    const svg = c.querySelector("svg") as SVGSVGElement;
    const before = at(marker(c, 1));

    await fireEvent.pointerDown(svg, { button: 0, pointerId: 1, clientX: 0, clientY: 0 });
    await fireEvent.pointerMove(svg, { button: 0, pointerId: 1, clientX: 0, clientY: 40 });
    await fireEvent.pointerUp(svg, { button: 0, pointerId: 1, clientX: 0, clientY: 40 });

    expect(at(marker(c, 1)).y).toBeGreaterThan(before.y);
  });

  test("every probe carries its own handles, not just the selected one", async () => {
    // Selection-gated handles were the original design and they made the gizmo
    // useless for finding a probe: you had to have already picked the one you
    // wanted before there was anything to drag.
    const c = mount(0);
    for (const n of [1, 2]) {
      // Six arrow halves at most — fewer when an axis points near enough at
      // the camera that its arrows would be stubs on top of the cube, which is
      // the same edge-on case a drag refuses.
      expect(c.querySelectorAll(`line.grab[aria-label^="drag probe ${n} "]`).length)
        .toBeGreaterThanOrEqual(4);
      // Three faces, always: the plane handles are the cube itself.
      expect(c.querySelectorAll(`polygon[aria-label="drag probe ${n} in plane"]`).length).toBe(3);
    }
  });
});

describe("selection is not collateral damage", () => {
  /** Two press/release pairs on empty space and the dblclick they produce. */
  async function clickBackground(bg: Element, times: number) {
    for (let i = 0; i < times; i++) {
      await fireEvent.pointerDown(bg, { button: 0, pointerId: 1, clientX: 5, clientY: 5 });
      await fireEvent.pointerUp(bg, { button: 0, pointerId: 1, clientX: 5, clientY: 5 });
    }
    if (times > 1) await doubleClick(bg);
  }

  function mountWithSpy() {
    const onselect = vi.fn();
    const { container } = render(ProbeViewer, {
      probes: PROBES, ranges: RANGES, selected: 0, formationId: 0,
      onselect, onmove: noop, oncommit: noop,
    });
    return { onselect, bg: container.querySelector(".bg") as SVGRectElement };
  }

  test("double-clicking empty space flips the view without dropping the selection", async () => {
    // The browser fires the first click of a double click regardless, so a
    // deselect running on pointerup would always clear the selection on the
    // way to a view flip. It waits out the double-click window instead.
    vi.useFakeTimers();
    try {
      const { onselect, bg } = mountWithSpy();
      await clickBackground(bg, 2);
      vi.advanceTimersByTime(2000);
      expect(onselect).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  test("a single click on empty space still deselects", async () => {
    // The other half of the rule: waiting out the double click must not turn
    // the plain deselect into a no-op.
    vi.useFakeTimers();
    try {
      const { onselect, bg } = mountWithSpy();
      await clickBackground(bg, 1);
      vi.advanceTimersByTime(2000);
      expect(onselect).toHaveBeenCalledWith(null);
    } finally {
      vi.useRealTimers();
    }
  });
});
