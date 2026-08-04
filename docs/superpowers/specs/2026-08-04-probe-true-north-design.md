# Probe formation true north and compass (design)

Status: designed 2026-08-04. **Measured and built the same day.** §2 ran, §3 is
implemented.

**What §2 actually measured, and how it differed from the plan.** The marker
formation below (one probe per axis direction, unique distances) was launched
first and was *not* conclusive: the ship sat 75 km off the formation centre, so
its overview distances did not match the authored offsets and could not name the
probes, and every bearing was skewed by up to 8°. The offset was recovered by
solving all eight distances (rms 0.3 km), which identified the `+Z` marker as the
one nearest north — suggestive, not decisive.

What settled it was a **cluster formation** rather than a marker one: three
probes on `+X`, two on `+Y`, one on `−Z`, sized 3/2/1 so no probe had to be
identified individually at all. The overlay put them west, up and south.

**The lesson for any future in-space reading: identify by GROUP SIZE, not by
distance.** Distance identification silently assumes the ship is at the formation
centre, and it is not guaranteed to be. §2.2's unique-distance table is kept
below as the record of what was tried, not as the recommended shape.

Result: `+Z` north, `−X` east, `−Z` south, `+X` west, `+Y` up.
`NORTH_AZ_DEG = 90`, `EAST_SIGN = 1`.

**§2.3's rotation control was not run.** Two launches agreed, but both were
probably at a similar heading, so a world-fixed frame is consistent-with rather
than tested. It stays open in `docs/settings-field-reference.md`.

Builds on `2026-08-04-probe-3d-viewer-design.md`, referred to below as **the
viewer spec**. Nothing here changes the file format, the exchange format, or what
the editor writes to a settings file. The only product is a read-only annotation
in the 3D viewer plus two measured constants.

## 1. Goal

A probe formation is authored in a frame the editor names `X`, `Y`, `Z` and
nothing more. In space with the tactical overlay open, EVE distinguishes one
cardinal direction and treats it as north. Those two facts have never been
connected, so a formation authored with a face pointing along `+Z` gives no clue
which way that face will point when the probes are in the water.

This slice connects them: measure which direction in the formation's own frame is
in-game north, then draw a compass in the viewer so the answer is visible while
you place probes.

**Scope is the picture only.** The table's azimuth column keeps meaning what the
editor spec §5 says it means — degrees from `+X` toward `+Z`. No field changes
meaning and no stored value moves.

## 2. The measurement

Everything in §3 is one constant away from being wrong, and that constant cannot
be derived from anything in the repo. It has to be read off a running client.

### 2.1 What is actually unknown

Three things, and one screenshot pair settles all three:

1. **Which direction is north** in the formation's frame.
2. **Which way bearings increase** from it — whether east lies at
   `north + 90°` or `north − 90°` in `toSpherical`'s az convention.
3. **Whether the frame is world-fixed at all.** If the client rotates a
   formation's offsets by the ship's heading when the probes launch, there is no
   north axis, the question is malformed, and §3 does not get built.

(3) is the one that matters most and the one nothing in the repo answers.
`docs/settings-field-reference.md` records that a formation re-saved at 42 AU came
back as an **axis-aligned box** with per-axis power-of-two quantisation — which is
circumstantial evidence for a world-fixed frame, because a frame rotated to an
arbitrary heading would have smeared the quantisation across axes instead of
landing it cleanly on each. Circumstantial is not measured.

### 2.2 The marker formation

Eight probes, one per axis direction plus two diagonals, at **unique distances**
so the overview's distance column names each bracket outright:

| probe | offset (m) | distance | reads |
|---|---|---|---|
| 1 | `(200000, 0, 0)` | 200 km | `+X` |
| 2 | `(0, 0, 300000)` | 300 km | `+Z` |
| 3 | `(0, 400000, 0)` | 400 km | `+Y` |
| 4 | `(-500000, 0, 0)` | 500 km | `−X` |
| 5 | `(0, 0, -600000)` | 600 km | `−Z` |
| 6 | `(0, -700000, 0)` | 700 km | `−Y` |
| 7 | `(565685, 0, 565685)` | 800 km | `+X+Z` diagonal |
| 8 | `(-636396, 0, -636396)` | 900 km | `−X−Z` diagonal |

The `±Y` pair answers whether `Y` is up or down. The two diagonals answer (2):
with `+X`, `+Z`, `−X`, `−Z` and a known 45° point all on screen at once, the sense
of rotation is readable without a second launch.

As the exchange format — paste this straight into the editor, or save it and
Import it:

```yaml
# EVE probe formations. Positions and ranges are metres from the formation centre.
formations:
  - name: 'north markers'
    range: 74798935350          # 0.5 AU
    probes:
      - [ 200000,       0,       0]   # 200 km
      - [      0,       0,  300000]   # 300 km
      - [      0,  400000,       0]   # 400 km
      - [-500000,       0,       0]   # 500 km
      - [      0,       0, -600000]   # 600 km
      - [      0, -700000,       0]   # 700 km
      - [ 565685,       0,  565685]   # 800 km
      - [-636396,       0, -636396]   # 900 km
```

The range is 0.5 AU because that is the combat-probe floor the client will impose
anyway (field reference, *Custom probe formations*). It plays no part in the
measurement.

### 2.3 Protocol

1. Warp to the sun (~1e9 m from it). This is the finest quantisation band in the
   field reference's speculative table, so the markers land as close to the
   authored offsets as the client is capable of.
2. Launch the marker formation. Tactical overlay on. Screenshot, with the
   overview visible so the distance column identifies each bracket.
3. **The control.** Recall the probes, turn the ship roughly 90°, relaunch the
   same formation, screenshot again.
   - Bearings unchanged → frame is world-fixed. Read north off shot 1 and build
     §3.
   - Bearings rotated with the ship → frame is ship-relative. **Stop.** Record
     the finding in the field reference and close this spec unbuilt; a compass
     would be a decoration that lies.

Scale is not load-bearing: a bracket's bearing is a direction and reads at any
distance. If 200–900 km clusters too tightly against the overlay ring to separate,
multiply every coordinate by 10 and nothing about the answer changes.

Quantisation is not a threat at this scale either. Near the sun the step is
sub-metre; even at 42 AU it was ~500 m per axis, which on a 200 km marker is a
bearing error under 0.15°.

### 2.4 What gets recorded

A bullet under **Custom probe formations** in `docs/settings-field-reference.md`,
in the same shape as the 2026-08-04 precision finding: what was launched, from
where, what each screenshot showed, and the rotation control's result. Screenshots
go to `G:\Downloads`, where every other native EVE capture this repo's numbers
were measured from already lives.

Facts and hypotheses stay separated the way that section already separates them.
The bearings read off the shots are measurement; any account of *why* EVE picked
that direction is not, and does not go in.

## 3. The compass

### 3.1 The two constants

In `probes.ts`, beside the existing frame documentation:

```ts
export const NORTH_AZ_DEG = 90;   // +Z is north
export const EAST_SIGN: 1 | -1 = 1; // east is -X, i.e. azimuth 180 = 90 + 90
```

This is the calibration knob. If north turns out not to sit on an axis, or a
patch moves it, it is one number and a re-measure — no geometry changes.

### 3.2 Two pure functions

Both go in `probes.ts` beside the projection helpers, so they stay
`node --test`-able with no runes, per the viewer spec's split:

```ts
/** The four cardinal directions as labelled unit vectors in the horizontal
 *  (X/Z) plane, N first, going the way bearings increase. */
export function cardinals(): { label: string; v: Vec3 }[];

/** A horizontal circle of world radius `r` about the formation centre, as `n`
 *  points. */
export function horizonRing(r: number, n?: number): Vec3[];
```

### 3.3 Drawing it

One `$derived` block in `ProbeViewer.svelte`:

- **Radius** — `worldPerPixel(origin.depth, SIZE) * RING_PX`, with
  `RING_PX = 150`. Screen-sized for the same reason the gizmo handles are
  (viewer spec §4.6): formation spread and range spheres differ by orders of
  magnitude, so a world-sized ring is a speck at one zoom and off-screen at the
  next. It is an orientation cue, not a scale bar, and must not read as one.
- **The curve** — `horizonRing(r)` projected point by point, emitted as line
  segments **only between consecutive pairs that both project**. A pan can put
  part of the ring behind the eye, where `projectPoint` returns `null`; a single
  `<polyline>` would tear straight across the view.
- **Labels** — `cardinals()` projected at `1.08 × r`. Any that fail to project
  are skipped individually.
- **Paint order** — immediately after the background `<rect>`, before the
  vectors and probes. It is context, and SVG's document order then puts every
  probe, gizmo, and the centre marker on top of it.

  It deliberately does **not** join the depth sort the rest of the scene goes
  through. That sort takes one depth per drawn item; the ring is a single curve
  spanning many, so it has no depth to sort by. Splitting it into per-segment
  sortable pieces is the only way to make it participate, and it buys nothing —
  a thin ring occluded by a probe cube reads the same as one drawn under it.
- **`pointer-events: none`**, with the range circles and axis stubs. Nothing
  decorative hit-tests in this view, for the reason its stylesheet already gives.

Degenerate cameras need no special case. At pitch 0 the ring is edge-on and
collapses to a line through the origin, which is still a true reading of where
the horizontal plane is; the pitch clamp of ±89.9° means it never fully
degenerates from above.

### 3.4 What stays

The `X`/`Y`/`Z` axis stubs are **not** replaced. The gizmo arms are colour-coded
per axis and the table is in `X`/`Y`/`Z`, so the axis names still have to be
readable in the picture. At 60 px they do not collide with the ring's labels at
150 px.

### 3.5 Two decisions taken without a toggle

- **Always on.** One thin ring against eight shaded cubes, twenty-four gizmo
  arms and eight range circles is not what makes this view busy. If it nags, the
  checkbox goes next to Vectors — one line, later.
- **Four cardinals, no degree ticks.** A ring you can read a bearing off is the
  "azimuth becomes a compass bearing" scope that §1 rules out. Ticks without that
  change would invite reading a number the table then contradicts.

## 4. Verification

One test in `probes.test.ts`, the smallest thing that fails if the constants or
the trig get flipped:

- `cardinals()` returns four vectors, each unit length with `y === 0`, 90° apart,
  N at `NORTH_AZ_DEG`, and E on `EAST_SIGN`'s side of it.
- `horizonRing(r)` returns points all at radius `r` with `y === 0`.

`ProbeViewer.spec.ts` gains nothing. The drawing is projection code already
covered by the viewer's own tests; what is new and worth pinning is the direction
maths, and that is pure.

## 5. Not built

- **Bearing readout on the table.** Ruled out in §1. If it is wanted later it is
  a derived column over `toSpherical` and `NORTH_AZ_DEG`, not a re-plumbing.
- **A north that varies per system.** The measurement is taken in one system. If
  north is a per-system property rather than a universal one, that is a finding
  for the field reference and a second measurement, not a data model — and there
  is no reason yet to think it is.
- **Rotating a formation to a bearing.** Placing probes by compass rather than
  reading them by one is a formation-level transform and its own slice.
