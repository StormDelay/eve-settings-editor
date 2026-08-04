# Probe formation 3D viewer and per-probe range (design)

Status: designed 2026-08-04, not yet planned.

Builds directly on `2026-08-03-probe-formation-editor-design.md`, referred to
below as **the editor spec**. That document's §10 books two of the three things
built here as deferred — "dragging probes in the visualiser" and "a rotatable /
isometric view" — and this one takes them both up. The third thing reverses one
of its decisions: §2.3's *one range per formation*.

Nothing here changes the file format, the key path, the `-4` scratch-slot rule
or the batch category. Those are settled in the editor spec and are not
restated.

## 1. Goal

The two fixed orthographic panes preview a formation; they do not let you work
on one. Judging whether a probe sits where you want it means reading a dot in
two flat pictures and then editing a number in a table, and the two panes each
hide one axis by construction.

This slice replaces both panes with one perspective viewer that has a free
camera and draggable probes, modelled on the client's own probe view, and makes
scan range editable per probe as the client allows.

## 2. What the client does (from the reference captures, 2026-08-04)

Three states of the in-game probe view were captured:

1. **Default** — one gizmo at the formation centre; dragging it moves the whole
   formation through space. **Not replicated.** The editor edits probe placement
   relative to the formation centre; where the formation gets dropped in space
   is a per-scan decision with nothing to save. Captured for context only.
2. **Shift held** — every probe splits out with its own translate gizmo. This is
   the state the editor is modelled on.
3. **Alt held** — probes draw as vectors radiating from the formation centre.

Camera: left-drag orbits, right-drag pans, wheel zooms. Two buttons at the
bottom of the client's view jump to a side view and a top view.

Each probe's scan range draws as a wireframe sphere. Eight overlapping spheres
dominate the picture; the probe cluster sits inside them.

### 2.1 Per-probe range is real

The editor spec §2.3 measured every one of 984 corpus entries at
`74798935350.0` and concluded the editor should expose **one** range per
formation, written to all probes, with a `mixed_range` flag making a
disagreeing file read-only.

The measurement stands. The conclusion drawn from it does not: **the client
lets a player set scan range per probe.** A corpus that only ever shows uniform
ranges reflects how players use the control, not what the control permits. The
format carries one range per entry (editor spec §2.1) precisely because the
client writes one per entry.

Consequence: `mixed_range` was guarding against a state that is legitimate. It
is deleted, not relaxed — see §5.

## 3. Interaction model

**Selection-based, not modifier-based.** Clicking a probe selects it and its
gizmo appears; clicking empty space deselects. The client's Shift-to-split is
not replicated.

The reason is that the client's modifiers are free — its camera does not use
them — while an editor's are not, and holding a modifier with one hand to drag
with the other is a worse deal in a window you are also typing numbers into.
Selection also already exists here: the table rows and the panes share a
`selectedProbe`, so the viewer's selection is the same selection, and clicking
a probe highlights its row.

The Alt vector view (§2, state 3) becomes a checkbox rather than a held key,
for the same reason.

## 4. Viewer — `app/src/lib/ProbeViewer.svelte`

A new component. `ProbeFormationsView.svelte` is 450 lines before this change;
the viewer is ~250 more, and it has a clean boundary — positions and ranges in,
a moved probe out — so it does not belong in that file.

```
props:  probes: [number,number,number][]   metres, formation-centre relative
        ranges: number[]                   metres, one per probe
        selected: number | null
        onselect: (i: number | null) => void
        onmove:   (i: number, p: [number,number,number]) => void
        oncommit: () => void
```

`onmove` fires per pointer frame so the table's numbers track the drag;
`oncommit` fires once on pointerup. This matches the existing rule that the
file is written on blur, not on keystroke.

### 4.1 Rendering: hand-rolled projection into SVG

No 3D library. The camera is a perspective projection written as pure
functions in `probes.ts`, and the scene is SVG elements.

The decisive argument is picking. Every gizmo handle is an SVG element with its
own `pointerdown`, so hit-testing is the browser's job: no raycaster, no
picking buffer, no scene graph to keep in step with `$state`. Adding three.js
would buy a `TransformControls` gizmo and proper depth sorting at the cost of
~600 KB, a WebGL context inside WebView2, an `examples/jsm` import path, an
imperative bridge between the scene graph and the reactive draft, and camera
maths that no longer runs under `node --test` — `probes.ts` is deliberately
rune-free so it can (editor spec, `probes.ts` header comment).

CSS 3D transforms were the other candidate: `preserve-3d` would handle depth
for free, but arrows, silhouette circles and a controllable camera all fight
it.

### 4.2 Camera

```ts
interface Camera {
  yaw: number;    // degrees, rotation about Y (EVE's up axis)
  pitch: number;  // degrees above the horizontal plane, clamped to ±89.9
  dist: number;   // metres from target to eye
  target: [number, number, number];  // metres — what the camera looks at
}
```

Pitch clamps at ±89.9° rather than ±90 because the up vector degenerates when
the view direction is parallel to Y, which makes the basis undefined and the
projection produce NaN.

The eye sits at `target + dist · (cos p·cos y, sin p, cos p·sin y)`, looks at
the target, and takes its right vector from `cross(forward, +Y)`.

| input | effect |
|---|---|
| left-drag | `yaw += dx·k`, `pitch -= dy·k` |
| right-drag | `target` moves along the camera's own right and up axes |
| wheel | `dist *= exp(±k)` — exponential, so a zoom step feels the same at every scale |
| `side` | yaw 90, pitch 0 — X to the right, Y up: the old side (X/Y) pane |
| `top` | yaw 90, pitch 89.9 — looking down: X to the right, Z **down**-screen |
| `fit` | `dist` set so every probe and its range sphere is in frame |

The two view buttons mirror the client's two buttons, and land on the two views
the panes they replace used to show, so nothing that could be read before
becomes unreadable.

One deliberate difference: the old top-down pane drew +Z **up**-screen, the map
convention, which is a mirror of what a camera above the formation actually
sees. The camera here is a camera, so `top` shows +Z downward. Resolving that
by pointing the camera up from below instead would give the old orientation
under a button labelled "top", which is worse. §4.5's axis indicator makes the
orientation readable either way, which is what removes the ambiguity that the
fixed pane captions used to carry.

`fit` is also the initial camera, so the view opens framing the whole formation
the way `paneScale` used to.

### 4.3 Projection

Perspective, 50° vertical field of view. With `f = (size/2) / tan(fov/2)` and a
point `v` in camera space (`z` forward, positive in front of the eye):

```
x = size/2 + f · v.x / v.z
y = size/2 - f · v.y / v.z        SVG's y grows downward
```

Points at or behind the eye plane do not project and are dropped. This is a
real case during a pan, not a theoretical one.

**Depth sorting** is a painter's algorithm: drawables sort on camera-space `z`
descending and emit in that order, because SVG paints in document order and has
no z-buffer.

### 4.4 Range spheres

Each probe's range draws as its silhouette. For a sphere of radius `R` whose
centre is `d` from the eye, the silhouette is a circle of projected radius

```
f · R / sqrt(d² - R²)          for d > R
```

A sphere's silhouette is a circle from every viewpoint, so one circle is not an
approximation of the shape — it is the shape. The wireframe latitude/longitude
globes the client draws are a texture on the same silhouette and are not
replicated.

When `d <= R` the eye is inside that probe's sphere and there is no silhouette;
the circle is dropped for that probe. With eight spheres at 0.5 AU this is the
normal state at any useful zoom, so it must not throw or produce NaN.

### 4.5 Axis indicator

Three short labelled lines from the formation centre along +X, +Y and +Z,
drawn in the scene so they turn with the camera. The two panes carried their
orientation in a fixed caption ("top-down (X/Z)"); a free camera has no fixed
caption to carry it, so the orientation has to be visible in the picture.

### 4.6 Gizmo

Drawn on the selected probe only.

- **Three double-ended arrows**, along ±X, ±Y, ±Z.
- **Three plane handles**, small quads in the XY, YZ and ZX planes.

Both are sized in **screen pixels**, with the world-space length back-computed
from the handle's depth, so a handle stays the same size and stays grabbable at
every zoom level. A world-sized gizmo is unusable at the zoom range this view
needs — the formation spread and the range spheres differ by more than an order
of magnitude in the corpus data.

### 4.7 Drag

**Axis drag.** At pointerdown, project the probe's position `P₀` and a point a
short step along the axis. The difference gives a screen-space direction and a
pixels-per-metre scale for that axis. Each frame:

```
metres = (pointerΔ · dir) / pxPerMetre
```

No raycasting, and it behaves correctly under perspective for small drags.
When the axis points nearly at or away from the camera, `pxPerMetre`
approaches zero and the movement diverges; below a threshold the drag is
ignored. The arrow is edge-on and nearly invisible in exactly that case, so
there is nothing the user could have meant to grab.

**Plane drag.** Cast a ray from the eye through the pointer and intersect the
plane through `P₀` with the handle's normal. Guard on a near-zero denominator —
the plane seen edge-on — and ignore the frame.

**Precision.** The editor spec §4.2 makes metres the source of truth in the
frontend and asserts that an untouched coordinate survives a save bit-for-bit.
Dragging must not weaken that:

- An axis drag writes **one** component. The other two are not recomputed.
- A plane drag writes **two**. The locked component is copied verbatim from
  `P₀`, *not* taken from the ray-plane intersection, which would return it with
  float noise on top and quietly displace the probe along an axis the user did
  not drag.

Probes other than the dragged one are never rewritten by a drag.

### 4.8 Vector view

A checkbox. Draws a dashed line from the formation centre to each probe with an
arrowhead, replicating the client's Alt state (§2, state 3). The probes and
their range spheres stay drawn; the client hides the gizmos in this state and
so does this.

## 5. Per-probe range

### 5.1 Model

`Formation` in `crates/settings-model/src/probes.rs` loses two fields:

```rust
pub struct Formation {
    pub id: i64,
    pub name: String,
    pub probes: Vec<[f64; 3]>,
    pub ranges: Vec<f64>,     // metres, one per probe — now the only range state
}
```

`range: f64` and `mixed_range: bool` are deleted. `range` existed to name "the
one value the single field edits" and there is no longer a single field;
`mixed_range` existed to mark a file the editor could not safely rewrite, and
per §2.1 that file was always legitimate.

Deleting `mixed_range` removes, in the view: eight `disabled={current.mixed_range}`
bindings, the read-only warning paragraph, the `mixedProbeLabel` derivation and
the "Copy with uniform range" escape hatch that existed only to get out of the
lockout. The change is a net deletion in the frontend.

### 5.2 Write path

```rust
pub fn set_formation(v: &mut Value, id: i64, name: &str,
                     probes: &[[f64; 3]], ranges: &[f64])
    -> Result<Formations, ProbeError>;
```

New error variant `BadRangeCount` when `ranges.len() != probes.len()`. It is
validated with the rest before anything is inlined, so the editor spec §3's
rule holds: a rejected write leaves the document byte-for-byte as it was.

The same signature change carries through `ops.rs`, `lib.rs` and `api.ts`.

### 5.3 Editor controls

- A **range column** in the probe table: a `<select>` per row over
  `RANGE_STEPS_AU`, the same nine slider stops the single field offered, for
  the same reason — the client's control has no free value, so neither does
  this one.
- The **header field stays**, and now writes its value to every probe. Uniform
  range is still the common case and setting eight selects by hand to reach it
  would be a regression.
- A range the file holds that is not one of the nine stops is still offered as
  an extra option on that probe's select, so it is shown rather than snapped to
  a neighbour.

### 5.4 Batch

Unaffected. `Category::ProbeFormations` copies the whole section as bytes; it
never reads the projection.

## 6. What is deleted

- `project()`, `paneScale()` and the `Plane` type from `probes.ts`, with their
  tests.
- The `.panes` block, the `PANES` list, the `at()` helper and the pane CSS from
  `ProbeFormationsView.svelte`.
- `Formation::range`, `Formation::mixed_range` and everything downstream (§5.1).

## 7. Testing

**`probes.test.ts`** — the camera and drag maths are pure, so they carry the
weight:

- the camera basis is orthonormal at several yaw/pitch values
- the target projects to the viewport centre
- in the `side` camera (yaw 90, pitch 0) a point at +X projects right of centre
  and one at +Y projects above it — this pins the handedness, which is the part
  most likely to be wrong and least likely to be noticed
- a point behind the eye does not project
- silhouette radius against a hand-computed value; a sphere containing the eye
  yields none
- axis-drag metres for a screen-aligned axis equal the pointer delta over the
  scale, and a camera-facing axis yields no movement
- a plane drag returns the locked component **exactly** as it went in
  (`Object.is`, not a tolerance — this is the §4.7 precision guarantee)

**`probes.rs`** — existing tests update to per-probe ranges. The mixed-range
test inverts: what was "flagged, not flattened" becomes "round-trips its
per-probe values". `BadRangeCount` gets a case, and a case asserting the
document is unchanged after that rejection.

**`tests/probes_corpus.rs`** — the uniform-range assertion becomes a
non-assertion: it still projects every corpus file without error and still
checks 8 probes per formation, but a uniform range is now a fact about players
rather than a constraint on the format, so locking it in would fail the moment
someone saves a formation the client fully permits.

**`ProbeFormationsView.spec.ts`** — per-probe range writes reach the IPC call;
the header field writes all rows.

**Not tested:** a drag driven through jsdom. `getBoundingClientRect` returns
0×0 there, so every screen-space number the drag depends on is degenerate and
the test would assert against a fiction. The pure functions above cover the
maths; the wiring is covered by looking at it.

## 8. Phasing

| phase | delivers |
|---|---|
| 1 | Per-probe range end to end: `probes.rs`, IPC, table column, header "set all". Ships alone and is useful alone. |
| 2 | Camera and drag maths in `probes.ts`, with tests. No UI. |
| 3 | `ProbeViewer.svelte`: render, camera controls, view buttons. Replaces the panes. |
| 4 | Gizmo and drag. |
| 5 | Vector-view checkbox. |

Own branch (`probe-3d-viewer`) per the branch policy: this changes what the
editor writes and adds a surface that was not there.

## 9. Deferred

- **Wireframe range globes.** §4.4 — cosmetic; the silhouette is the shape.
- **Moving the whole formation.** §2, state 1 — nothing to save.
- **Rotating the whole formation about its centre.** Not in the client's view
  either, but it is the obvious next gizmo if authoring from scratch turns out
  to want it.
- **Snapping** — to a grid, to a sphere of fixed radius, or probe-to-probe.
  Wait until a drag has been used in anger.
- **Deriving the from-scratch formation from the client's own defaults.**
  Still open from the editor spec §2.5; unchanged by this slice.
