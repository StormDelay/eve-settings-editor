<script lang="ts">
  // The formation in 3D, modelled on the client's own probe view (spec §4).
  //
  // The scene is SVG elements rather than a canvas or a 3D library, so every
  // probe and (in the gizmo) every handle hit-tests itself — no raycaster, no
  // picking pass. SVG paints in document order and has no z-buffer, so
  // everything drawn goes through one depth sort.
  import {
    cameraBasis, projectPoint, silhouette, fitDistance, worldPerPixel,
    axisScreen, dragPosition, cardinals, horizonRing, scenePos,
    PITCH_LIMIT, SIDE_VIEW, TOP_VIEW, type Camera, type HandleDrag, type Vec3,
  } from "./probes";
  import type { Scene } from "./api";
  import Button from "./ui/Button.svelte";
  import Field from "./ui/Field.svelte";

  let { probes, ranges, formationId, selected, scenes = [], onselect, onmove, oncommit }: {
    probes: Vec3[];
    ranges: number[];
    /** The formation on show. The re-fit key: a different formation is a
     * different subject and gets framed, a retyped number is not. */
    formationId: number | null;
    selected: number | null;
    /** Reference geometry available to show alongside the probes. Empty when
     * the app data directory holds none, which is what hides the picker. */
    scenes?: Scene[];
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
  // view does.
  //
  // A cube is an object sitting in the scene, and it is rendered as one: eight
  // corners in WORLD space, axis-aligned to EVE's axes, each projected like
  // anything else. Its orientation is fixed in the world — the +Y face is the
  // up face from every camera — and moving the camera simply shows the scene
  // from somewhere else.
  //
  // A parallel projection was tried here (orientation from the camera basis,
  // corners laid out in screen space) on the theory that off-axis perspective
  // skew was reading as the cubes spinning. It was worse: taking a cube's
  // orientation from the camera is precisely what makes it turn on the spot
  // rather than sit still, so the cubes stopped agreeing with the scene they
  // were in. Do not reintroduce it.

  /** Cube width in screen px — the same for every probe however far away it
   * is, so a distant probe stays as clickable as a near one. Big enough that
   * its three visible faces are each a drag target in their own right, since
   * that is what they now are. */
  const CUBE_PX = 26;
  /** A world-fixed light. Every face normal is a unit axis, so the lambert term
   * for a face is just ±LIGHT[k] — no dot product needed. */
  const LIGHT = [0.34, 0.86, 0.44];
  const AMBIENT = 0.34;
  // The cube's face fills are computed per-face from a shade factor, so they
  // cannot be `var(--accent)` — but they still have to BE --accent, or the
  // probe cubes stay on the pre-Phase-1 palette while everything around them
  // moves. No guard can see these (they are number arrays, not hex literals),
  // so if the tokens are ever re-solved, these move by hand.
  const CUBE_RGB = [141, 206, 255]; // --accent
  const CUBE_SEL_RGB = [243, 189, 110]; // --warn

  /** One probe's cube as its visible faces, back-face culled — only three of
   * the six can ever face the camera — each with its own shade.
   *
   * The half-side is taken from this probe's own depth, which cancels the
   * perspective shrink and leaves every cube the same number of pixels across
   * however far away it is. That is a deliberate readability choice, not a
   * depth cue: a distant probe still has to be findable and clickable. It does
   * not affect orientation — scaling a cube about its own centre changes its
   * size and nothing else. */
  function cubeFaces(p: Vec3, depth: number, sel: boolean) {
    const h = worldPerPixel(depth, SIZE) * (CUBE_PX / 2);
    const rgb = sel ? CUBE_SEL_RGB : CUBE_RGB;
    const faces: { lock: 0 | 1 | 2; pts: string; fill: string }[] = [];
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
      faces.push({
        // The axis this face is perpendicular to — the one a drag across it
        // holds still. The face IS the plane handle (spec §4.6 as the client
        // does it: you drag a face of the cube to move in its plane), so it
        // carries the lock rather than a separate quad floating beside it.
        lock: k as 0 | 1 | 2,
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

  // --- compass -------------------------------------------------------------
  // The horizontal plane, and where EVE's north lies in it. The axis stubs
  // above say which way X/Y/Z run; they cannot say which way is north, because
  // that is a fact about the game and not about the frame. It was measured
  // in-game (see `NORTH_AZ_DEG`) and everything here derives from it.

  /** Ring radius in screen px, the same at every zoom — for the reason the
   * gizmo handles are screen-sized: formation spread and range spheres differ
   * by orders of magnitude, so a world-sized ring is a speck at one zoom and
   * off-screen at the next. It is an orientation cue, not a scale bar. */
  const RING_PX = 150;

  const compass = $derived.by(() => {
    const o = origin;
    if (!o) return null;
    const r = worldPerPixel(o.depth, SIZE) * RING_PX;
    // Projected point by point and joined only between consecutive pairs that
    // BOTH project. A pan can put part of the ring behind the eye, where
    // `projectPoint` returns null, and one <polyline> would tear across the
    // whole view rather than simply going missing there.
    const pts = horizonRing(r).map((p) => projectPoint(p, basis, SIZE));
    const segs = pts
      .map((a, i) => {
        const b = pts[(i + 1) % pts.length];
        return a && b ? { x1: a.x, y1: a.y, x2: b.x, y2: b.y } : null;
      })
      .filter((s) => s !== null);
    // Just outside the ring, so a label never sits on the line it belongs to.
    const marks = cardinals()
      .map(({ label, v }) => {
        const q = projectPoint([v[0] * r * 1.1, 0, v[2] * r * 1.1], basis, SIZE);
        return q === null ? null : { label, x: q.x, y: q.y };
      })
      .filter((m) => m !== null);
    return { segs, marks };
  });

  // --- scene ---------------------------------------------------------------
  // Static reference geometry loaded from a file: a beacon, a wormhole, the
  // volume you can jump it from. Everything here is CONTEXT — it is drawn under
  // the probes, it never hit-tests, and it does not join the probes' depth sort,
  // for the reason the compass does not.
  //
  // The two scales never have to share a frame. A formation is ~0.5 AU across
  // and a drifter site is ~90 km, so at formation zoom the whole scene is
  // sub-pixel and at scene zoom the probes are far outside it — which is why
  // there are two Fit buttons rather than one cleverer one. An earlier attempt
  // at this (formation-editor spec §8) drew both at one scale and rendered the
  // site at 1e-4 px.

  /** Which scene is showing; -1 is none, and is where it starts. Local state:
   * nothing else reads it and nothing persists it. */
  let sceneIndex = $state(-1);
  const scene = $derived(scenes[sceneIndex] ?? null);
  /** The chosen scene's objects in world metres, paired so `fitScene` and the
   * drawing below agree about what is in it. */
  const sceneWorld = $derived(
    (scene?.objects ?? []).map((o) => ({ label: o.label, p: scenePos(o.pos), radius: o.radius_m })),
  );

  const sceneDrawn = $derived(
    sceneWorld
      .map((o) => {
        const s = projectPoint(o.p, basis, SIZE);
        // A zero radius must draw NOTHING. `silhouette` returns 0 for it, not
        // null, and a zero-radius circle is a stray element in the DOM.
        return s === null
          ? null
          : { label: o.label, s, r: o.radius > 0 ? silhouette(s.dist, o.radius, SIZE) : null };
      })
      .filter((o) => o !== null),
  );

  /** Frame the scene rather than the probes. `fitDistance` already takes
   * positions and a matching radius each, which is exactly a scene. */
  function fitScene() {
    cam = {
      ...cam,
      target: [0, 0, 0],
      dist: fitDistance(sceneWorld.map((o) => o.p), sceneWorld.map((o) => o.radius)),
    };
  }

  // --- gizmo ---------------------------------------------------------------
  // Handles are sized in SCREEN pixels: the formation spread and the range
  // spheres differ by more than an order of magnitude in real data, so a
  // world-sized gizmo would be a speck at one zoom and fill the view at
  // another (spec §4.6).
  const ARM_PX = 64; // arrow half-length
  /** Each arrow stops short of the probe by this much, leaving the cube clear.
   * The cube is the plane handle and the click target, so an arrow crossing it
   * would steal both; wide enough to clear the cube's corners. */
  const GAP_PX = 24;

  const UNIT: Vec3[] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
  const AXIS_CLASS = ["gx", "gy", "gz"];

  const step = (p: Vec3, axis: Vec3, k: number): Vec3 =>
    [p[0] + axis[0] * k, p[1] + axis[1] * k, p[2] + axis[2] * k];

  /** One probe's arrows, in viewport pixels. `null` when it does not project.
   * The plane handles are the cube's own faces and live with the cube. */
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
    return { n, arms };
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
    pressed = n;
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
    pressed = n;
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
    // Only a press that landed on the background itself clears what a double
    // click would mean. The centre marker deliberately lets its press through
    // to here so the camera can still be orbited from it, and must not be
    // forgotten on the way.
    const t = e.target as Element;
    if (t === svgEl || t.classList.contains("bg")) pressed = null;
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
        // PLUS, so the scene turns WITH the pointer: drag right and the near
        // side of the formation goes right, like spinning a globe under your
        // finger. This was briefly negated on the theory that it read as
        // turning the object rather than walking around it; measured, the
        // negation did the opposite and moved the scene against the pointer.
        yaw: cam.yaw + dx * 0.4,
        // PLUS for the same reason as the yaw: drag down and the near side of
        // the formation comes down with the pointer. This was minus, which
        // pushed it up — so the vertical drag ran opposite to the horizontal
        // one and the whole thing felt unlike the client.
        pitch: Math.max(-PITCH_LIMIT, Math.min(PITCH_LIMIT, cam.pitch + dy * 0.4)),
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

  /** What the last press landed on, and so what a double click means.
   *
   * A double click cannot be read from its own event target here. Grabbing a
   * cube face or the background takes pointer capture on the svg, and a
   * captured pointer retargets the click that follows to the capture element —
   * so an `ondblclick` on the face never fires, and the svg's sees only itself
   * and cannot tell a cube from empty space. The press, which happens before
   * any capture is taken, is the only reliable witness. */
  let pressed: number | "centre" | null = null;

  /** Double click: on a probe or the formation centre it becomes what the
   * camera orbits around; on empty space it flips between the two views the
   * flat panes used to show.
   *
   * All three are decided here from `pressed` rather than from the event's own
   * target, for the reason given there. */
  function onDblClick() {
    // This is the second click of the pair, so cancel the deselect the first
    // one queued: neither focusing nor flipping is a reason to lose a probe.
    clearTimeout(deselectTimer);
    if (pressed === "centre") return focusOn([0, 0, 0]);
    if (typeof pressed === "number" && probes[pressed]) return focusOn(probes[pressed]);
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
  <!-- No width/height attributes: CSS sizes it and the viewBox keeps the
       coordinate system at SIZE units whatever the pixels come out as. Every
       projection here works in those units, and `local` already divides client
       pixels by the rendered width, so the scene simply scales. -->
  <svg bind:this={svgEl} viewBox="0 0 {SIZE} {SIZE}"
       role="img" aria-label="Formation in 3D"
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

    <!-- The compass, first so everything else paints over it: it is context,
         not subject. It deliberately does NOT join the depth sort the probes go
         through — that sort takes one depth per drawn item, and this is a single
         curve spanning many, so it has no depth to sort by. A thin ring
         occluded by a probe cube reads the same as one drawn under it. -->
    {#if compass}
      {#each compass.segs as s}
        <line x1={s.x1} y1={s.y1} x2={s.x2} y2={s.y2} class="ring" />
      {/each}
      {#each compass.marks as m (m.label)}
        <text x={m.x} y={m.y} class="cardinal" class:north={m.label === "N"}>{m.label}</text>
      {/each}
    {/if}

    <!-- The scene, with the compass: context, painted under everything. Keyed
         by index, not label — two objects may legitimately share a name. -->
    {#each sceneDrawn as o, i (i)}
      {#if o.r !== null}
        <circle cx={o.s.x} cy={o.s.y} r={o.r} class="scene-vol" />
      {/if}
      <circle cx={o.s.x} cy={o.s.y} r="3.5" class="scene-mark" />
      <text x={o.s.x + 7} y={o.s.y - 5} class="scene-label">{o.label}</text>
    {/each}

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
      <!-- A forgiving square UNDER the cube, so a click that lands near a probe
           rather than exactly on it still selects instead of hitting the
           background and deselecting. Under, not over: the cube's own faces
           are the plane handles and have to take the press.

           tabindex="-1", like the arrows below: the <svg role="img"> makes this
           whole subtree presentational, so a tab stop here is one nothing can
           activate (these are pointer-only). Out of the tab order, but still a
           role the linter and the pointer contract both want. The numeric
           table is the keyboard path. -->
      <rect x={d.s.x - HIT_PX / 2} y={d.s.y - HIT_PX / 2} width={HIT_PX} height={HIT_PX}
            class="probe-grab"
            role="button" tabindex="-1"
            aria-label={`probe ${d.i + 1}`}
            onpointerdown={(e) => {
              if (e.button !== 0) return;
              e.stopPropagation();
              pressed = d.i;
              onselect(d.i);
            }} />
      <!-- The cube's faces ARE the plane handles: drag a face and the probe
           moves in that face's plane, which is how the client does it. A
           separate quad floating beside each axis was the old arrangement and
           it put two dozen parallelograms on screen for eight probes. -->
      {#each cubeFaces(d.p, d.s.depth, selected === d.i) as f}
        <polygon points={f.pts} fill={f.fill}
                 class="probe-face" class:live={planeLive(d.i, f.lock)}
                 role="button" tabindex="-1"
                 aria-label={`drag probe ${d.i + 1} in plane`}
                 onpointerdown={(e) => startPlane(e, d.i, f.lock)} />
      {/each}
    {/each}

    <!-- The formation centre: what every probe coordinate is an offset from,
         and the camera's home. AFTER the probes, so it is not buried under a
         cube — a marker you cannot find is a marker you cannot double-click —
         but BEFORE the gizmo, so a selected probe's handles still win the
         clicks near it. -->
    {#if origin}
      <circle cx={origin.x} cy={origin.y} r="6.5" class="centre" />
      <circle cx={origin.x} cy={origin.y} r="1.6" class="centre-dot" />
      <!-- No stopPropagation: the press falls through to the camera so the
           view can still be orbited from here. It only records what a double
           click would mean. -->
      <circle cx={origin.x} cy={origin.y} r="10" class="centre-grab"
              role="button" tabindex="-1" aria-label="formation centre"
              onpointerdown={(e) => { if (e.button === 0) pressed = "centre"; }} />
    {/if}

    {#if !vectors}
      {#each gizmos as g (g.n)}
        <g class="gizmo" class:dim={selected !== null && selected !== g.n}>
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
    <Button size="sm" onclick={() => (cam = { ...cam, ...TOP_VIEW })}>Top</Button>
    <Button size="sm" onclick={() => (cam = { ...cam, ...SIDE_VIEW })}>Side</Button>
    <Button size="sm" onclick={fit}>Fit</Button>
    {#if scenes.length}
      <!-- controlClass, not class: ProbeViewer.spec reads `.scene-pick`'s
           selectedIndex, so the hook has to be on the select itself. -->
      <Field
        kind="select"
        label="Scene"
        class="toggle"
        controlClass="scene-pick"
        bind:value={sceneIndex}
        options={[
          { value: -1, label: "None" },
          ...scenes.map((s, i) => ({ value: i, label: s.name })),
        ]} />
      {#if scene}
        <Button size="sm" onclick={fitScene}>Fit scene</Button>
      {/if}
    {/if}
    <!-- A toggle names what it turns ON. -->
    <Field kind="checkbox" class="toggle" label="Show vectors" bind:value={vectors} />
    <span class="meta">
      Drag to orbit · Right-drag to pan · Wheel to zoom ·
      Double-click a probe or the centre to orbit it, or empty space to flip the view
    </span>
  </div>
</div>

<style>
  /* Fills whatever height the parent column leaves it — the buttons and the
     hint line below stay in view, which a fixed box did not manage. */
  .viewer {
    display: flex; flex-direction: column; gap: var(--s1); align-items: flex-start;
    width: 100%; flex: 1; min-height: 0;
  }
  .viewer svg {
    /* Square, sized off the leftover HEIGHT rather than the width: the panel
       is far wider than it is tall, so width-first would run off the bottom.
       max-width keeps it honest on a narrow window, where width binds first. */
    flex: 1;
    min-height: 220px;
    aspect-ratio: 1;
    width: auto;
    max-width: 100%;
    border: 1px solid var(--border); border-radius: var(--r-sm);
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
  .bg { fill: var(--surface); }
  /* Nothing decorative hit-tests. A range circle's fill is a paint even at
     alpha 0.06, so SVG's default `visiblePainted` would hit-test it — and at
     any fitted zoom the circles blanket the markers, so a click meant for a
     probe would land on a circle, bubble to the background and deselect. */
  .axis, .axis-label, .range, .vec, .vec-head, .ring, .cardinal,
  .scene-vol, .scene-mark, .scene-label { pointer-events: none; }
  /* Fainter than the axis stubs: the ring is the plane those stubs live in, and
     it must not compete with them or with the probes. */
  .ring { stroke: var(--border); stroke-width: 1; opacity: 0.6; }
  .cardinal {
    fill: var(--text-muted); font-size: var(--t-caption); text-anchor: middle; dominant-baseline: middle;
  }
  /* North is the one the other three are read from, so it is the one that has
     to be findable at a glance. */
  .cardinal.north { fill: var(--text); font-weight: 600; }
  /* Brighter than the compass, dimmer than a probe. A scene is the thing the
     probes are being placed against, so it has to be findable — but the probes
     are still the subject and the scene must not compete with them. */
  .scene-vol { fill: var(--text); fill-opacity: 0.04; stroke: var(--border); stroke-width: 1; }
  .scene-mark { fill: var(--text); opacity: 0.75; }
  .scene-label { fill: var(--text-muted); font-size: var(--t-caption); }
  .axis { stroke: var(--border); stroke-width: 1; }
  .axis-label { fill: var(--text-muted); font-size: var(--t-caption); }
  .range { fill: var(--accent); fill-opacity: 0.06; stroke: var(--accent); stroke-opacity: 0.35; stroke-width: 1; }
  /* Each face is a plane handle, so it hit-tests. Its fill is per-face and
     computed, so it is set inline, not here; the stroke darkens the shared
     edges and is what stops three lit faces reading as one flat blob. */
  .probe-face {
    stroke: var(--bg); stroke-opacity: 0.6; stroke-width: 0.6; stroke-linejoin: round; cursor: move;
  }
  /* The face under the pointer, or being dragged, lifts out of the shading —
     brightness rather than a colour change, so which face you are on reads
     without breaking the lit-cube illusion the shading builds. */
  .probe-face:hover, .probe-face.live { filter: brightness(1.45); }
  .probe-grab { fill: transparent; cursor: pointer; }
  /* A ring, not a disc, so a probe sitting on the formation centre still reads
     through it. Brighter than the axis stubs it sits among, or it is lost in
     the place they all meet. */
  .centre { fill: none; stroke: var(--text); stroke-width: 1.3; opacity: 0.75; pointer-events: none; }
  .centre-dot { fill: var(--text); opacity: 0.75; pointer-events: none; }
  .centre-grab { fill: transparent; cursor: pointer; }
  /* Pressing a handle focuses it, and the UA then outlines its BOUNDING BOX —
     which for a diagonal arrow is a large rectangle across the scene. These
     are all tabindex="-1" and pointer-only, so the ring can never be a
     keyboard affordance here; it is pure noise. What the ring was badly trying
     to say — which handle you are on — is said properly by `.live` below.
     The numeric table remains the keyboard path. */
  .probe-face:focus, .probe-grab:focus, .centre-grab:focus, .grab:focus { outline: none; }
  .viewer-actions { display: flex; gap: var(--s1); align-items: center; flex-wrap: wrap; }
  .meta { color: var(--text-muted); font-size: var(--t-caption); margin-left: var(--s2); }
  .vec { stroke: var(--accent); stroke-width: 1; stroke-dasharray: 4 3; opacity: 0.7; }
  .vec-head { fill: var(--accent); stroke: none; }
  .viewer-actions :global(.toggle) { font-size: var(--t-caption); color: var(--text-muted); }

  /* Axis colours, the near-universal convention: X red, Y green, Z blue. */
  .gx { stroke: var(--danger); fill: var(--danger); }
  .gy { stroke: var(--ok); fill: var(--ok); }
  .gz { stroke: var(--accent); fill: var(--accent); }
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
</style>
