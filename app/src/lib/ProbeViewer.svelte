<script lang="ts">
  // The formation in 3D, modelled on the client's own probe view (spec §4).
  //
  // The scene is SVG elements rather than a canvas or a 3D library, so every
  // probe and (in the gizmo) every handle hit-tests itself — no raycaster, no
  // picking pass. SVG paints in document order and has no z-buffer, so
  // everything drawn goes through one depth sort.
  import {
    cameraBasis, projectPoint, silhouette, fitDistance, worldPerPixel,
    axisScreen, dragPosition,
    PITCH_LIMIT, SIDE_VIEW, TOP_VIEW, type Camera, type HandleDrag, type Vec3,
  } from "./probes";

  let { probes, ranges, formationId, selected, onselect, onmove, oncommit }: {
    probes: Vec3[];
    ranges: number[];
    /** The formation on show. The re-fit key: a different formation is a
     * different subject and gets framed, a retyped number is not. */
    formationId: number | null;
    selected: number | null;
    onselect: (i: number | null) => void;
    onmove: (i: number, p: Vec3) => void;
    oncommit: () => void;
  } = $props();

  const SIZE = 520; // px, square viewport

  /** The client's Alt view: each probe as a vector from the formation centre.
   * A checkbox rather than a held key — a modifier the camera also wants is a
   * bad trade in a window you type numbers into (spec §3). */
  let vectors = $state(false);

  let cam = $state<Camera>({ ...SIDE_VIEW, dist: 1, target: [0, 0, 0] });
  const basis = $derived(cameraBasis(cam));

  /** Frame the probes — NOT their range spheres. Also the opening view, so it
   * starts where the two panes it replaces used to.
   *
   * The spheres are context, and framing both is impossible when they differ by
   * orders of magnitude: "on grid" is ±10 000 km at 0.5 AU range, so framing the
   * spheres shrinks the formation to 0.03 px. The trade is that at this distance
   * the eye is inside every sphere and no circles draw (`silhouette` returns
   * null) until the user wheels out — absent context beats an invisible subject. */
  function fit() {
    cam = { ...cam, target: [0, 0, 0], dist: fitDistance(probes) };
  }
  // Re-fit when the formation being shown changes, and only then: editing a
  // number must not move the camera, and neither must a drag — `onmove` mutates
  // a probe's position, and re-fitting mid-drag would move the camera out from
  // under the pointer.
  let lastFitId: number | null | undefined = undefined;
  $effect(() => {
    if (formationId !== lastFitId) {
      lastFitId = formationId;
      fit();
    }
  });

  /** Every probe projected, with its silhouette, sorted back to front. */
  const drawn = $derived(
    probes
      .map((p, i) => {
        const s = projectPoint(p, basis, SIZE);
        return s === null ? null : { i, s, r: silhouette(s.dist, ranges[i] ?? 0, SIZE) };
      })
      .filter((d) => d !== null)
      .sort((a, b) => b.s.depth - a.s.depth),
  );

  /** The three axis stubs, so a free camera's orientation is readable in the
   * picture — the fixed panes carried it in their captions (spec §4.5). */
  const AXES: { v: Vec3; label: string }[] = [
    { v: [1, 0, 0], label: "X" },
    { v: [0, 1, 0], label: "Y" },
    { v: [0, 0, 1], label: "Z" },
  ];
  const axisMarks = $derived.by(() => {
    const o = projectPoint([0, 0, 0], basis, SIZE);
    if (!o) return [];
    // 60 px long whatever the zoom.
    const len = worldPerPixel(o.depth, SIZE) * 60;
    return AXES.map(({ v, label }) => {
      const e = projectPoint([v[0] * len, v[1] * len, v[2] * len], basis, SIZE);
      return e === null ? null : { o, e, label };
    }).filter((a) => a !== null);
  });

  // --- gizmo ---------------------------------------------------------------
  // Handles are sized in SCREEN pixels: the formation spread and the range
  // spheres differ by more than an order of magnitude in real data, so a
  // world-sized gizmo would be a speck at one zoom and fill the view at
  // another (spec §4.6).
  const ARM_PX = 46;   // arrow half-length
  const PLANE_PX = 18; // plane-handle side, offset from the probe by the same

  const UNIT: Vec3[] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
  const AXIS_CLASS = ["gx", "gy", "gz"];
  /** The two axes each plane handle spans, and the axis it locks. */
  const PLANES: { a: 0 | 1 | 2; b: 0 | 1 | 2; lock: 0 | 1 | 2 }[] = [
    { a: 0, b: 1, lock: 2 },
    { a: 1, b: 2, lock: 0 },
    { a: 2, b: 0, lock: 1 },
  ];

  const step = (p: Vec3, axis: Vec3, k: number): Vec3 =>
    [p[0] + axis[0] * k, p[1] + axis[1] * k, p[2] + axis[2] * k];

  /** The selected probe's handles, in viewport pixels. `null` when nothing is
   * selected or the probe does not project. */
  const gizmo = $derived.by(() => {
    if (selected === null || !probes[selected]) return null;
    const p0 = probes[selected];
    const c = projectPoint(p0, basis, SIZE);
    if (!c) return null;
    const w = worldPerPixel(c.depth, SIZE);
    const arms = UNIT.map((axis, i) => {
      const pos = projectPoint(step(p0, axis, w * ARM_PX), basis, SIZE);
      const neg = projectPoint(step(p0, axis, -w * ARM_PX), basis, SIZE);
      return pos && neg ? { i, pos, neg, cls: AXIS_CLASS[i] } : null;
    }).filter((a) => a !== null);
    const quads = PLANES.map(({ a, b, lock }) => {
      const o = w * PLANE_PX;
      const corners = [
        step(step(p0, UNIT[a], o), UNIT[b], o),
        step(step(p0, UNIT[a], o * 2), UNIT[b], o),
        step(step(p0, UNIT[a], o * 2), UNIT[b], o * 2),
        step(step(p0, UNIT[a], o), UNIT[b], o * 2),
      ].map((q) => projectPoint(q, basis, SIZE));
      if (corners.some((q) => q === null)) return null;
      return {
        lock,
        cls: AXIS_CLASS[lock],
        points: corners.map((q) => `${q!.x},${q!.y}`).join(" "),
      };
    }).filter((q) => q !== null);
    return { arms, quads };
  });

  /** A handle drag in progress. The maths lives in `probes.ts`; this file keeps
   * only the event plumbing. */
  let handleDrag = $state<HandleDrag | null>(null);

  /** Pointer position in viewport units. The SVG is square and scales with its
   * box, so client pixels convert by the box's own width. */
  function local(e: PointerEvent): { x: number; y: number } {
    const box = svgEl!.getBoundingClientRect();
    const k = SIZE / (box.width || SIZE);
    return { x: (e.clientX - box.left) * k, y: (e.clientY - box.top) * k };
  }

  function startAxis(e: PointerEvent, comp: 0 | 1 | 2) {
    if (e.button !== 0 || selected === null) return;
    e.stopPropagation();
    const p0 = probes[selected];
    const a = axisScreen(p0, UNIT[comp], basis, SIZE);
    // Edge-on: the arrow is invisible and the scale diverges, so there is
    // nothing to drag.
    if (!a) return;
    const l = local(e);
    handleDrag = { kind: "axis", i: selected, comp, p0, sx: l.x, sy: l.y, a };
    svgEl?.setPointerCapture(e.pointerId);
  }

  function startPlane(e: PointerEvent, lock: 0 | 1 | 2) {
    if (e.button !== 0 || selected === null) return;
    e.stopPropagation();
    handleDrag = { kind: "plane", i: selected, lock, p0: probes[selected] };
    svgEl?.setPointerCapture(e.pointerId);
  }

  /** Move the dragged probe. Returns nothing — it calls `onmove`, which writes
   * one probe and leaves every other coordinate in the formation untouched. */
  function dragTo(e: PointerEvent) {
    if (!handleDrag) return;
    const l = local(e);
    const next = dragPosition(handleDrag, l.x, l.y, basis, SIZE);
    if (next) onmove(handleDrag.i, next); // null: the plane went edge-on this frame
  }

  // --- camera controls -----------------------------------------------------
  // Left-drag orbits, right-drag pans, wheel zooms — the client's own bindings.

  let svgEl = $state<SVGSVGElement | undefined>();
  /** The current camera drag: which button started it, the last pointer
   * position (for the per-frame delta), where it started, and whether it has
   * travelled far enough to count as a drag rather than a click. */
  let camDrag = $state<
    { button: number; x: number; y: number; ox: number; oy: number; moved: boolean } | null
  >(null);
  /** How far a press may wander and still deselect. A click is rarely still. */
  const CLICK_SLOP = 4; // px

  function onBackgroundDown(e: PointerEvent) {
    if (e.button !== 0 && e.button !== 2) return;
    // Every probe marker's and gizmo handle's own handler stops propagation on
    // its way here, so any left press that reaches this one has already landed
    // on empty space — no target check needed. (A future handle added to this
    // view must keep that contract, or it starts a camera drag through it.)
    // The deselect happens on pointerUP and only if nothing moved: orbiting is
    // how you get at a gizmo arrow pointing at the camera, so a left-drag must
    // not drop the selection it exists to serve.
    camDrag = { button: e.button, x: e.clientX, y: e.clientY,
                ox: e.clientX, oy: e.clientY, moved: false };
    svgEl?.setPointerCapture(e.pointerId);
  }

  function onMove(e: PointerEvent) {
    if (handleDrag) {
      dragTo(e);
      return;
    }
    if (!camDrag) return;
    const dx = e.clientX - camDrag.x;
    const dy = e.clientY - camDrag.y;
    const moved = camDrag.moved ||
      Math.abs(e.clientX - camDrag.ox) + Math.abs(e.clientY - camDrag.oy) > CLICK_SLOP;
    camDrag = { ...camDrag, x: e.clientX, y: e.clientY, moved };
    if (camDrag.button === 0) {
      cam = {
        ...cam,
        yaw: cam.yaw + dx * 0.4,
        pitch: Math.max(-PITCH_LIMIT, Math.min(PITCH_LIMIT, cam.pitch - dy * 0.4)),
      };
    } else {
      // Pan in the camera's own plane, scaled so the scene tracks the pointer
      // at any zoom.
      const k = worldPerPixel(cam.dist, SIZE);
      const t = cam.target;
      cam = {
        ...cam,
        target: [
          t[0] - (basis.right[0] * dx - basis.up[0] * dy) * k,
          t[1] - (basis.right[1] * dx - basis.up[1] * dy) * k,
          t[2] - (basis.right[2] * dx - basis.up[2] * dy) * k,
        ],
      };
    }
  }

  function onUp(e: PointerEvent) {
    // The file is written once, at the end of the drag — the same rule the
    // table's fields follow on blur.
    if (handleDrag) oncommit();
    // A left click on empty space — press and release without travelling — is
    // the deselect. A handle drag or a marker press never sets `camDrag`, so
    // neither can reach this.
    else if (camDrag?.button === 0 && !camDrag.moved) onselect(null);
    handleDrag = null;
    camDrag = null;
    svgEl?.releasePointerCapture(e.pointerId);
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    // Mid-drag the pixels-per-metre captured at pointerdown is what the drag
    // converts through; zooming now would leave it stale for the rest of it.
    if (handleDrag) return;
    // Exponential, so one wheel step feels the same at every scale — and the
    // scales here span orders of magnitude.
    cam = { ...cam, dist: cam.dist * Math.exp(Math.sign(e.deltaY) * 0.15) };
  }
</script>

<div class="viewer">
  <svg bind:this={svgEl} viewBox="0 0 {SIZE} {SIZE}" width={SIZE} height={SIZE}
       role="img" aria-label="the formation in 3D"
       onpointerdown={onBackgroundDown}
       onpointermove={onMove}
       onpointerup={onUp}
       onpointercancel={onUp}
       onwheel={onWheel}
       oncontextmenu={(e) => e.preventDefault()}>
    <rect x="0" y="0" width={SIZE} height={SIZE} class="bg" />

    <defs>
      <marker id="probe-vec-head" viewBox="0 0 10 10" refX="9" refY="5"
              markerWidth="6" markerHeight="6" orient="auto-start-reverse">
        <path d="M 0 0 L 10 5 L 0 10 z" class="vec-head" />
      </marker>
    </defs>

    {#if vectors}
      {@const o = projectPoint([0, 0, 0], basis, SIZE)}
      {#if o}
        {#each drawn as d (d.i)}
          <line x1={o.x} y1={o.y} x2={d.s.x} y2={d.s.y} class="vec"
                marker-end="url(#probe-vec-head)" />
        {/each}
      {/if}
    {/if}

    {#each drawn as d (d.i)}
      {#if d.r !== null}
        <circle cx={d.s.x} cy={d.s.y} r={d.r} class="range" />
      {/if}
      <!-- tabindex="-1", like the gizmo handles below: the <svg role="img">
           makes this whole subtree presentational, so a tab stop here is one
           nothing can activate (these are pointer-only). Out of the tab order,
           but still a role the linter and the pointer contract both want. The
           numeric table is the keyboard path. -->
      <rect x={d.s.x - 5} y={d.s.y - 5} width="10" height="10"
            class="probe" class:selected={selected === d.i}
            role="button" tabindex="-1"
            aria-label={`probe ${d.i + 1}`}
            onpointerdown={(e) => {
              if (e.button !== 0) return;
              e.stopPropagation();
              onselect(d.i);
            }} />
    {/each}

    {#if gizmo && !vectors}
      <g class="gizmo">
        {#each gizmo.quads as q}
          <polygon points={q.points} class="handle {q.cls}"
                   role="button" tabindex="-1" aria-label="drag in plane"
                   onpointerdown={(e) => startPlane(e, q.lock)} />
        {/each}
        {#each gizmo.arms as a}
          <line x1={a.neg.x} y1={a.neg.y} x2={a.pos.x} y2={a.pos.y} class="arm {a.cls}" />
          <!-- The grab target is a fat transparent line over the thin visible
               one, so a 1 px arrow is still catchable with a mouse. -->
          <line x1={a.neg.x} y1={a.neg.y} x2={a.pos.x} y2={a.pos.y} class="grab"
                role="button" tabindex="-1" aria-label={`drag probe along ${"XYZ"[a.i]}`}
                onpointerdown={(e) => startAxis(e, a.i as 0 | 1 | 2)} />
          <circle cx={a.pos.x} cy={a.pos.y} r="3.5" class="tip {a.cls}" />
          <circle cx={a.neg.x} cy={a.neg.y} r="3.5" class="tip {a.cls}" />
        {/each}
      </g>
    {/if}

    <!-- Last, so it paints over the translucent range circles: this indicator
         is the whole mitigation for the top view's +Z pointing down the screen
         (spec §4.5), and it has to be legible to do that job. -->
    {#each axisMarks as a}
      <line x1={a.o.x} y1={a.o.y} x2={a.e.x} y2={a.e.y} class="axis" />
      <text x={a.e.x} y={a.e.y} class="axis-label">{a.label}</text>
    {/each}
  </svg>

  <div class="viewer-actions">
    <button onclick={() => (cam = { ...cam, ...TOP_VIEW })}>Top</button>
    <button onclick={() => (cam = { ...cam, ...SIDE_VIEW })}>Side</button>
    <button onclick={fit}>Fit</button>
    <label class="toggle">
      <input type="checkbox" bind:checked={vectors} />
      Vectors
    </label>
    <span class="meta">drag to orbit · right-drag to pan · wheel to zoom</span>
  </div>
</div>

<style>
  .viewer { display: flex; flex-direction: column; gap: 0.35rem; align-items: flex-start; }
  .viewer svg {
    border: 1px solid var(--border); border-radius: 3px;
    touch-action: none; /* or a drag scrolls the page instead of orbiting */
    cursor: grab;
  }
  .bg { fill: var(--bg-panel); }
  /* Nothing decorative hit-tests. A range circle's fill is a paint even at
     alpha 0.06, so SVG's default `visiblePainted` would hit-test it — and at
     any fitted zoom the circles blanket the markers, so a click meant for a
     probe would land on a circle, bubble to the background and deselect. */
  .axis, .axis-label, .range, .vec, .vec-head { pointer-events: none; }
  .axis { stroke: var(--border); stroke-width: 1; }
  .axis-label { fill: var(--fg-dim); font-size: 10px; }
  .range { fill: rgba(79, 156, 240, 0.06); stroke: rgba(79, 156, 240, 0.35); stroke-width: 1; }
  .probe { fill: var(--accent); cursor: pointer; }
  .probe.selected { fill: var(--warn); stroke: var(--fg); stroke-width: 1; }
  .viewer-actions { display: flex; gap: 4px; align-items: center; }
  .meta { opacity: 0.7; font-size: 0.85em; margin-left: 0.5rem; }
  .vec { stroke: var(--accent); stroke-width: 1; stroke-dasharray: 4 3; opacity: 0.7; }
  .vec-head { fill: var(--accent); stroke: none; }
  .toggle { display: flex; align-items: center; gap: 4px; font-size: 0.85em; color: var(--fg-dim); }

  /* Axis colours, the near-universal convention: X red, Y green, Z blue. */
  .gx { stroke: #e06c6c; fill: #e06c6c; }
  .gy { stroke: #7bc47b; fill: #7bc47b; }
  .gz { stroke: #6c9ce0; fill: #6c9ce0; }
  .arm { stroke-width: 1.5; pointer-events: none; }
  .tip { stroke: none; pointer-events: none; }
  .grab { stroke: transparent; stroke-width: 12; stroke-linecap: round; cursor: move; }
  .handle { fill-opacity: 0.25; stroke-width: 1; cursor: move; }
  .handle:hover { fill-opacity: 0.5; }
</style>
