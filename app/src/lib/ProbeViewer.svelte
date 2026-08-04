<script lang="ts">
  // The formation in 3D, modelled on the client's own probe view (spec §4).
  //
  // The scene is SVG elements rather than a canvas or a 3D library, so every
  // probe and (in the gizmo) every handle hit-tests itself — no raycaster, no
  // picking pass. SVG paints in document order and has no z-buffer, so
  // everything drawn goes through one depth sort.
  import {
    cameraBasis, projectPoint, silhouette, fitDistance, worldPerPixel,
    PITCH_LIMIT, SIDE_VIEW, TOP_VIEW, type Camera, type Vec3,
  } from "./probes";

  let { probes, ranges, selected, onselect, onmove, oncommit }: {
    probes: Vec3[];
    ranges: number[];
    selected: number | null;
    onselect: (i: number | null) => void;
    onmove: (i: number, p: Vec3) => void;
    oncommit: () => void;
  } = $props();

  const SIZE = 520; // px, square viewport

  let cam = $state<Camera>({ ...SIDE_VIEW, dist: 1, target: [0, 0, 0] });
  const basis = $derived(cameraBasis(cam));

  /** Frame the whole formation. Also the opening view, so it starts where the
   * two panes it replaces used to. */
  function fit() {
    cam = { ...cam, target: [0, 0, 0], dist: fitDistance(probes, ranges) };
  }
  // Re-fit whenever the formation being shown changes shape, but never on a
  // drag: `onmove` mutates a probe's position, and re-fitting mid-drag would
  // move the camera out from under the pointer.
  let fitKey = $derived(`${probes.length}:${ranges.join()}`);
  let lastFitKey = "";
  $effect(() => {
    if (fitKey !== lastFitKey) {
      lastFitKey = fitKey;
      fit();
    }
  });

  /** Every probe projected, with its silhouette, sorted back to front. */
  const drawn = $derived(
    probes
      .map((p, i) => {
        const s = projectPoint(p, basis, SIZE);
        return s === null ? null : { i, p, s, r: silhouette(s.dist, ranges[i] ?? 0, SIZE) };
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

  // --- camera controls -----------------------------------------------------
  // Left-drag orbits, right-drag pans, wheel zooms — the client's own bindings.

  let svgEl = $state<SVGSVGElement | undefined>();
  /** Which button started the current camera drag, or null. */
  let camDrag = $state<{ button: number; x: number; y: number } | null>(null);

  function onBackgroundDown(e: PointerEvent) {
    if (e.button !== 0 && e.button !== 2) return;
    // Every probe marker's own handler stops propagation on its way here, so
    // any left press that reaches this one has already landed on empty space
    // — no target check needed. (A future handle added to this view must keep
    // that contract: stopPropagation on its own left-button press, or it
    // deselects through it.)
    if (e.button === 0) onselect(null);
    camDrag = { button: e.button, x: e.clientX, y: e.clientY };
    svgEl?.setPointerCapture(e.pointerId);
  }

  function onMove(e: PointerEvent) {
    if (!camDrag) return;
    const dx = e.clientX - camDrag.x;
    const dy = e.clientY - camDrag.y;
    camDrag = { ...camDrag, x: e.clientX, y: e.clientY };
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
    camDrag = null;
    svgEl?.releasePointerCapture(e.pointerId);
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
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

    {#each axisMarks as a}
      <line x1={a.o.x} y1={a.o.y} x2={a.e.x} y2={a.e.y} class="axis" />
      <text x={a.e.x} y={a.e.y} class="axis-label">{a.label}</text>
    {/each}

    {#each drawn as d (d.i)}
      {#if d.r !== null}
        <circle cx={d.s.x} cy={d.s.y} r={d.r} class="range" />
      {/if}
      <rect x={d.s.x - 5} y={d.s.y - 5} width="10" height="10"
            class="probe" class:selected={selected === d.i}
            role="button" tabindex="0"
            aria-label={`probe ${d.i + 1}`}
            onpointerdown={(e) => {
              if (e.button !== 0) return;
              e.stopPropagation();
              onselect(d.i);
            }} />
    {/each}
  </svg>

  <div class="viewer-actions">
    <button onclick={() => (cam = { ...cam, ...TOP_VIEW })}>Top</button>
    <button onclick={() => (cam = { ...cam, ...SIDE_VIEW })}>Side</button>
    <button onclick={fit}>Fit</button>
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
  .axis { stroke: var(--border); stroke-width: 1; }
  .axis-label { fill: var(--fg-dim); font-size: 10px; }
  .range { fill: rgba(79, 156, 240, 0.06); stroke: rgba(79, 156, 240, 0.35); stroke-width: 1; }
  .probe { fill: var(--accent); cursor: pointer; }
  .probe.selected { fill: var(--warn); stroke: var(--fg); stroke-width: 1; }
  .viewer-actions { display: flex; gap: 4px; align-items: center; }
  .meta { opacity: 0.7; font-size: 0.85em; margin-left: 0.5rem; }
</style>
