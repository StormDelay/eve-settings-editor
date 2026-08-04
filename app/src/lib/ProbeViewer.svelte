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

  /** Side of a probe's transparent grab square, px. The cube inside it is
   * smaller; this is what the pointer actually has to hit. */
  const HIT_PX = 22;

  /** The formation centre on screen — the origin every probe is an offset from,
   * and the camera's home. `null` once a pan puts it behind the eye. */
  const origin = $derived(projectPoint([0, 0, 0], basis, SIZE));

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

  // --- probe cubes ---------------------------------------------------------
  // A flat square gives the scene no depth, and depth is what makes a probe
  // placeable by eye — so a probe draws as a shaded cube, as the client's own
  // view does. The cube is axis-aligned in WORLD space, so it turns as you
  // orbit and the shading turns with it: that motion is most of the cue.

  /** Cube width in screen px. Sized in pixels like the gizmo, so it stays
   * readable across the orders of magnitude these formations span. */
  const CUBE_PX = 15;
  /** A world-fixed light. Every face normal is a unit axis, so the lambert term
   * for a face is just ±LIGHT[k] — no dot product needed. */
  const LIGHT = [0.34, 0.86, 0.44];
  const AMBIENT = 0.34;
  const CUBE_RGB = [79, 156, 240]; // --accent
  const CUBE_SEL_RGB = [217, 164, 65]; // --warn

  /** One probe's cube as its visible faces, back-face culled — only three of
   * the six can ever face the camera — each with its own shade.
   *
   * Sized from the probe's OWN depth, so every cube covers the same number of
   * screen pixels however far away it is. Size is not the depth cue here and
   * making it one only shrinks the distant probes you still have to click:
   * what reads as depth is the perspective TURN, which scaling a cube about
   * its own centre does not touch. */
  function cubeFaces(p: Vec3, depth: number, sel: boolean) {
    const h = worldPerPixel(depth, SIZE) * (CUBE_PX / 2);
    const faces: { pts: string; fill: string }[] = [];
    for (let k = 0; k < 3; k++) {
      const u = (k + 1) % 3;
      const v = (k + 2) % 3;
      // Of the two faces on this axis, the one the eye is on the side of.
      const s = basis.eye[k] - p[k] > 0 ? 1 : -1;
      const proj = [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([a, c]) => {
        const q = [p[0], p[1], p[2]];
        q[k] += s * h;
        q[u] += a * h;
        q[v] += c * h;
        return projectPoint(q as Vec3, basis, SIZE);
      });
      if (proj.some((q) => q === null)) continue;
      // Wrap lighting, not plain lambert: a face pointing away from the light
      // would otherwise sit at flat ambient, and so would the two beside it —
      // three identical greys, which is exactly the flatness this is here to
      // fix. Wrapping keeps all six distinct from every angle.
      const shade = AMBIENT + (1 - AMBIENT) * (0.5 + 0.5 * s * LIGHT[k]);
      const rgb = sel ? CUBE_SEL_RGB : CUBE_RGB;
      faces.push({
        pts: proj.map((q) => `${q!.x},${q!.y}`).join(" "),
        fill: `rgb(${rgb.map((ch) => Math.round(ch * shade)).join(",")})`,
      });
    }
    return faces;
  }

  /** The three axis stubs, so a free camera's orientation is readable in the
   * picture — the fixed panes carried it in their captions (spec §4.5). */
  const AXES: { v: Vec3; label: string }[] = [
    { v: [1, 0, 0], label: "X" },
    { v: [0, 1, 0], label: "Y" },
    { v: [0, 0, 1], label: "Z" },
  ];
  const axisMarks = $derived.by(() => {
    const o = origin;
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
  /** Each arrow stops short of the probe by this much, leaving the middle
   * clear. Two reasons, and the second is not cosmetic: a single line through
   * the centre puts all three arrows' fat grab strokes on top of the probe, so
   * the first click of a double click selects it and the SECOND lands on an
   * arrow — no `dblclick` ever reaches the probe. Wider than the probe's own
   * 22 px grab square, so the two never overlap. */
  const GAP_PX = 14;

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
  /** One probe's handles, in viewport pixels. `null` when it does not project. */
  function gizmoFor(n: number) {
    const p0 = probes[n];
    if (!p0) return null;
    const c = projectPoint(p0, basis, SIZE);
    if (!c) return null;
    const w = worldPerPixel(c.depth, SIZE);
    const arms = UNIT.map((axis, i) => {
      // Each arrow is two segments with the probe in the gap between them,
      // never one line through it. The gap is measured in SCREEN pixels, not
      // stepped in world space: an axis tilted towards the camera foreshortens,
      // so a world-space gap collapses on screen exactly when the arrow lies
      // over the probe and the clearance matters most.
      const halves = [1, -1].map((s) => {
        const out = projectPoint(step(p0, axis, s * w * ARM_PX), basis, SIZE);
        if (!out) return null;
        const dx = out.x - c.x;
        const dy = out.y - c.y;
        const len = Math.hypot(dx, dy);
        // Foreshortened to shorter than the gap: the whole half would be a
        // stub sitting on the probe. Drop it — this is the same edge-on case
        // `axisScreen` refuses to drag.
        if (len <= GAP_PX) return null;
        return { inner: { x: c.x + (dx / len) * GAP_PX, y: c.y + (dy / len) * GAP_PX }, outer: out };
      }).filter((h) => h !== null);
      return halves.length ? { i, halves, cls: AXIS_CLASS[i] } : null;
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
    return { n, arms, quads };
  }

  /** Handles on EVERY probe, not just the selected one — the client shows them
   * all at once too. Overlap at a tight zoom is the accepted cost: zooming in
   * resolves it, and a gizmo you have to select a probe to see is one you
   * cannot use to find the probe in the first place. Back to front, so the
   * handles of a nearer probe sit above a farther one's. */
  const gizmos = $derived(
    drawn.map((d) => gizmoFor(d.i)).filter((g) => g !== null),
  );

  /** A handle drag in progress. The maths lives in `probes.ts`; this file keeps
   * only the event plumbing. */
  let handleDrag = $state<HandleDrag | null>(null);

  /** Which probe's axis the pointer is over. The visible arm cannot answer this
   * with `:hover` — it is `pointer-events: none`, and the thing actually
   * hovered is the fat transparent line on top of it. Probe-scoped, because
   * every probe carries its own set of arrows. */
  let hoveredAxis = $state<{ n: number; axis: number } | null>(null);

  /** Whether an axis arm should read as live: being dragged, or about to be. */
  const axisLive = (n: number, axis: number) =>
    (handleDrag?.kind === "axis" && handleDrag.i === n && handleDrag.comp === axis) ||
    (hoveredAxis?.n === n && hoveredAxis.axis === axis);

  /** Whether a plane quad should read as live. `:hover` covers the approach;
   * this covers the drag, when pointer capture has moved off the quad. */
  const planeLive = (n: number, lock: number) =>
    handleDrag?.kind === "plane" && handleDrag.i === n && handleDrag.lock === lock;

  /** Pointer position in viewport units. The SVG is square and scales with its
   * box, so client pixels convert by the box's own width. */
  function local(e: PointerEvent): { x: number; y: number } {
    const box = svgEl!.getBoundingClientRect();
    const k = SIZE / (box.width || SIZE);
    return { x: (e.clientX - box.left) * k, y: (e.clientY - box.top) * k };
  }

  function startAxis(e: PointerEvent, n: number, comp: 0 | 1 | 2) {
    if (e.button !== 0 || !probes[n]) return;
    e.stopPropagation();
    // Grabbing a probe's handle is a way of picking that probe, so the table
    // row follows the drag rather than staying on whatever was chosen before.
    onselect(n);
    const p0 = probes[n];
    const a = axisScreen(p0, UNIT[comp], basis, SIZE);
    // Edge-on: the arrow is invisible and the scale diverges, so there is
    // nothing to drag.
    if (!a) return;
    const l = local(e);
    handleDrag = { kind: "axis", i: n, comp, p0, sx: l.x, sy: l.y, a };
    svgEl?.setPointerCapture(e.pointerId);
  }

  function startPlane(e: PointerEvent, n: number, lock: 0 | 1 | 2) {
    if (e.button !== 0 || !probes[n]) return;
    e.stopPropagation();
    onselect(n);
    // The press point is recorded for the same reason the axis drag records
    // one: the probe moves by how far the pointer travelled across the plane,
    // not to wherever the pointer happens to be on it.
    const l = local(e);
    handleDrag = { kind: "plane", i: n, lock, p0: probes[n], sx: l.x, sy: l.y };
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
  /** How long a background click waits to see whether a second one follows.
   * Comfortably inside a typical system double-click time, and short enough
   * that a plain deselect still feels immediate. */
  const DOUBLE_CLICK_MS = 250;
  let deselectTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => () => clearTimeout(deselectTimer));

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
        // Negated, so dragging left swings the formation left with the
        // pointer rather than against it — you are turning the object, not
        // walking around it.
        yaw: cam.yaw - dx * 0.4,
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
    //
    // Held back by the double-click window rather than fired now, because a
    // double click in empty space is a view flip and must keep the selection:
    // the browser fires the first click of a pair regardless, so the only way
    // to tell the two apart is to wait and let `dblclick` cancel it.
    else if (camDrag?.button === 0 && !camDrag.moved) {
      clearTimeout(deselectTimer);
      deselectTimer = setTimeout(() => onselect(null), DOUBLE_CLICK_MS);
    }
    handleDrag = null;
    camDrag = null;
    // On pointercancel the capture is already gone, and releasing one we no
    // longer hold throws NotFoundError in Chromium.
    if (svgEl?.hasPointerCapture(e.pointerId)) svgEl.releasePointerCapture(e.pointerId);
  }

  /** Orbit around something else. Only the target moves — the distance and the
   * angles you were looking from are yours, and re-deriving them would throw
   * away the view you just built. */
  const focusOn = (t: Vec3) => (cam = { ...cam, target: [t[0], t[1], t[2]] });

  /** Double-clicking empty space flips between the two views the flat panes
   * used to show. It fires on the background alone: the probes, the centre
   * marker and the gizmo handles all have their own meaning for a double
   * click, and the pointer capture taken on press is released before the click
   * lands, so the target here is the real element under the pointer. */
  function onDblClick(e: MouseEvent) {
    const t = e.target as Element;
    if (t !== svgEl && !t.classList.contains("bg")) return;
    // This is the second click of the pair, so cancel the deselect the first
    // one queued: flipping the view is not a reason to lose your probe.
    clearTimeout(deselectTimer);
    cam = { ...cam, ...(Math.abs(cam.pitch) < 45 ? TOP_VIEW : SIDE_VIEW) };
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
       onpointerleave={() => (hoveredAxis = null)}
       ondblclick={onDblClick}
       onwheel={onWheel}
       oncontextmenu={(e) => e.preventDefault()}
       ondragstart={(e) => e.preventDefault()}>
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
      {#each cubeFaces(d.p, d.s.depth, selected === d.i) as f}
        <polygon points={f.pts} fill={f.fill} class="probe-face" />
      {/each}
      <!-- The grab target is a fat transparent square over the small visible
           one, the same way `.grab` fattens the gizmo's arrows: a 10 px marker
           is an awkward thing to hit, and missing it lands on the background,
           which deselects. Kept well under the plane handles' 18 px offset so
           a selected probe's own gizmo stays reachable.

           tabindex="-1", like the gizmo handles below: the <svg role="img">
           makes this whole subtree presentational, so a tab stop here is one
           nothing can activate (these are pointer-only). Out of the tab order,
           but still a role the linter and the pointer contract both want. The
           numeric table is the keyboard path. -->
      <rect x={d.s.x - HIT_PX / 2} y={d.s.y - HIT_PX / 2} width={HIT_PX} height={HIT_PX}
            class="probe-grab"
            role="button" tabindex="-1"
            aria-label={`probe ${d.i + 1}`}
            onpointerdown={(e) => {
              if (e.button !== 0) return;
              e.stopPropagation();
              onselect(d.i);
            }}
            ondblclick={() => focusOn(d.p)} />
    {/each}

    <!-- The formation centre: what every probe coordinate is an offset from,
         and the camera's home. AFTER the probes, so it is not buried under a
         cube — a marker you cannot find is a marker you cannot double-click —
         but BEFORE the gizmo, so a selected probe's handles still win the
         clicks near it. -->
    {#if origin}
      <circle cx={origin.x} cy={origin.y} r="6.5" class="centre" />
      <circle cx={origin.x} cy={origin.y} r="1.6" class="centre-dot" />
      <circle cx={origin.x} cy={origin.y} r="10" class="centre-grab"
              role="button" tabindex="-1" aria-label="formation centre"
              ondblclick={() => focusOn([0, 0, 0])} />
    {/if}

    {#if !vectors}
      {#each gizmos as g (g.n)}
        <g class="gizmo" class:dim={selected !== null && selected !== g.n}>
          {#each g.quads as q}
            <polygon points={q.points} class="handle {q.cls}" class:live={planeLive(g.n, q.lock)}
                     role="button" tabindex="-1" aria-label={`drag probe ${g.n + 1} in plane`}
                     onpointerdown={(e) => startPlane(e, g.n, q.lock)} />
          {/each}
          {#each g.arms as a}
            {#each a.halves as h}
              <line x1={h.inner.x} y1={h.inner.y} x2={h.outer.x} y2={h.outer.y}
                    class="arm {a.cls}" class:live={axisLive(g.n, a.i)} />
              <!-- The grab target is a fat transparent line over the thin
                   visible one, so a 1 px arrow is still catchable with a
                   mouse. It also carries the hover, because the arm it covers
                   cannot. -->
              <line x1={h.inner.x} y1={h.inner.y} x2={h.outer.x} y2={h.outer.y} class="grab"
                    role="button" tabindex="-1"
                    aria-label={`drag probe ${g.n + 1} along ${"XYZ"[a.i]}`}
                    onpointerdown={(e) => startAxis(e, g.n, a.i as 0 | 1 | 2)}
                    onpointerenter={() => (hoveredAxis = { n: g.n, axis: a.i })}
                    onpointerleave={() => {
                      if (hoveredAxis?.n === g.n && hoveredAxis.axis === a.i) hoveredAxis = null;
                    }} />
              <circle cx={h.outer.x} cy={h.outer.y} r="3.5"
                      class="tip {a.cls}" class:live={axisLive(g.n, a.i)} />
            {/each}
          {/each}
        </g>
      {/each}
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
    <span class="meta">
      drag to orbit · right-drag to pan · wheel to zoom ·
      double-click a probe or the centre to orbit it, empty space to flip view
    </span>
  </div>
</div>

<style>
  .viewer { display: flex; flex-direction: column; gap: 0.35rem; align-items: flex-start; }
  .viewer svg {
    border: 1px solid var(--border); border-radius: 3px;
    touch-action: none; /* or a drag scrolls the page instead of orbiting */
    /* An orbit that crosses the axis labels would otherwise start selecting
       their text, and the browser cancels the pointer sequence when it does —
       which fires pointercancel and drops the drag halfway through. Paired
       with the svg's ondragstart guard, which stops the same thing happening
       via a native element drag. */
    user-select: none;
    -webkit-user-select: none;
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
  /* The cube is decoration — the transparent square over it is what hit-tests,
     so a click landing near a probe rather than exactly on it still selects.
     Its fill is per-face and computed, so it is set inline, not here. The
     stroke darkens the shared edges and is what stops three lit faces reading
     as one flat blob. */
  .probe-face { pointer-events: none; stroke: rgba(0, 0, 0, 0.45); stroke-width: 0.6; stroke-linejoin: round; }
  .probe-grab { fill: transparent; cursor: pointer; }
  /* A ring, not a disc, so a probe sitting on the formation centre still reads
     through it. Brighter than the axis stubs it sits among, or it is lost in
     the place they all meet. */
  .centre { fill: none; stroke: var(--fg); stroke-width: 1.3; opacity: 0.75; pointer-events: none; }
  .centre-dot { fill: var(--fg); opacity: 0.75; pointer-events: none; }
  .centre-grab { fill: transparent; cursor: pointer; }
  /* Pressing a handle focuses it, and the UA then outlines its BOUNDING BOX —
     which for a diagonal arrow is a large rectangle across the scene. These
     are all tabindex="-1" and pointer-only, so the ring can never be a
     keyboard affordance here; it is pure noise. What the ring was badly trying
     to say — which handle you are on — is said properly by `.live` below.
     The numeric table remains the keyboard path. */
  .probe-grab:focus, .centre-grab:focus, .grab:focus, .handle:focus { outline: none; }
  .viewer-actions { display: flex; gap: 4px; align-items: center; }
  .meta { opacity: 0.7; font-size: 0.85em; margin-left: 0.5rem; }
  .vec { stroke: var(--accent); stroke-width: 1; stroke-dasharray: 4 3; opacity: 0.7; }
  .vec-head { fill: var(--accent); stroke: none; }
  .toggle { display: flex; align-items: center; gap: 4px; font-size: 0.85em; color: var(--fg-dim); }

  /* Axis colours, the near-universal convention: X red, Y green, Z blue. */
  .gx { stroke: #e06c6c; fill: #e06c6c; }
  .gy { stroke: #7bc47b; fill: #7bc47b; }
  .gz { stroke: #6c9ce0; fill: #6c9ce0; }
  /* Eight gizmos at once is a lot of line. Once a probe is chosen the other
     seven fade back so its own handles stay findable — still drawn, still
     grabbable, just no longer competing. */
  .gizmo.dim { opacity: 0.3; }
  .arm { stroke-width: 1.5; pointer-events: none; }
  .tip { stroke: none; pointer-events: none; }
  .grab { stroke: transparent; stroke-width: 12; stroke-linecap: round; cursor: move; }
  /* The handle under the pointer, or the one being dragged, brightens and
     thickens — it keeps its axis colour, so which axis you are on and which
     one is live are two separate readings rather than one overloaded shade.
     The visible arm and tips carry it; the transparent grab line stays
     invisible. */
  .arm.live { stroke-width: 3.5; filter: brightness(1.5); }
  .tip.live { r: 5; filter: brightness(1.5); }
  .handle { fill-opacity: 0.25; stroke-width: 1; cursor: move; }
  /* A plane handle hit-tests itself, so :hover carries it — but a drag holds
     pointer capture on the svg, and :hover stops matching the moment the
     pointer leaves the quad. `.live` is what keeps it lit for the whole drag. */
  .handle:hover, .handle.live { fill-opacity: 0.55; stroke-width: 2; }
</style>
