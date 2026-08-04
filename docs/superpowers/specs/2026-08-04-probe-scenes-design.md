# Probe viewer scenes (design)

Status: designed 2026-08-04.

Builds on `2026-08-04-probe-3d-viewer-design.md` (**the viewer spec**) and
`2026-08-04-probe-true-north-design.md` (**the compass spec**), and picks up the
thread `2026-08-03-probe-formation-editor-design.md` §8 left — a drifter-wormhole
overlay that was built and then cut. Nothing here changes the settings file, the
formation model, or the exchange format. The only product is read-only reference
geometry drawn in the 3D viewer, loaded from files the user can edit.

## 1. Goal

A formation is authored against nothing. The viewer shows eight probes, a
compass, and three axis stubs — but not the thing the probes are *for*. The
question "if I warp to that beacon and launch this, where do the probes actually
land relative to the wormhole?" has no answer in the tool.

A **scene** answers it: a named set of static objects, each with a label, a
position relative to the formation centre, and optionally a radius, drawn in the
viewer alongside the probes.

**The formation centre is your ship at the moment it launches.** Every scene
position is relative to that point, so a scene is implicitly a sentence of the
form "assuming you are sitting *here*, these things are around you". Each shipped
scene says where "here" is, in a comment.

Scenes are **files**, not built-in constants. That is the feature, not a delivery
detail: the geometry below is contested (§2), and a user who measures better
numbers must be able to fix them without waiting for a release, and to add sites
this project has never heard of.

**Read-only this slice.** An in-app scene editor is explicitly a later
possibility, not scope here.

## 2. The drifter geometry, and how much of it is known

The shipped content is two scenes: a drifter wormhole seen from the K-space side
and from the J-space side. Their geometry is *estimated*, and this section is the
record of how well.

### 2.1 Published sources disagree, and one of them is wrong about the sign

Reproduced from the formation-editor spec §8, with the March 2026 patch note and
the two wiki figures re-checked:

| source | warp-in → hole | angle |
|---|---|---|
| [Jambeeno's Uni guide](https://jambeeno.com/uni) | 89 km (75 km after March 2026) | 14° **below** outside, 26.5° in |
| [EVE University wiki](https://wiki.eveuniversity.org/Drifters) | ~80 km | "slight downward angle" |
| [Random Eve Stuff](https://randomevestuff.wordpress.com/unidentified-wormholes/) | ~100 km, one measured at 91 km | — |
| developer's own diagram, 2026-08-04 | 87.5 km both sides | 17.45° **above** (K), 30° **above** (J) |

The formation-editor spec §8 already flagged the direction of the 14° as an
assumption to check in-client, because the source says "a slight downward angle"
without stating from which end it is measured.

### 2.2 What the screenshot measures

An in-space screenshot with the tactical overlay open (K-space side, ship 2 454 m
from the beacon, wormhole at 87 km per the overview) settles the sign and
constrains the angle. Measured on the 1732 × 857 capture:

- Ring centre (the ship) ≈ (497, 630) px; the wormhole glyph ≈ (543, 561) px, so
  82.8 px from centre.
- The far-side ring labels at 75 / 100 / 150 / 200 km give a ruler. `1/s` is
  linear in `1/R`, fitting `s(R) = R / (1.2875 + 0.003797·R)` px.
- The wormhole lies **on that ruler's line**, within 1.3 px, so it shares the
  rings' far azimuth and the ruler applies to it.

A point lying *in the horizontal plane* at 87 km would project 53.8 px from
centre. The wormhole is at 82.8 px, in the same screen direction.

**So the hole is above the beacon.** A 29 px gap is not a measurement error, and
it refutes the published "below" outright — the developer's diagram has the sign
the client actually shows.

The angle itself does not come out of one frame. Run the ruler backwards and
82.8 px is where a *plane* object 155.5 km out would sit, so the hole's height
is whatever lifts an 87 km object to that line. Solving
`87·sinθ = (155.5 − 87·cosθ)·tanφ` for the camera pitch φ, which the capture does
not record:

| camera pitch φ | elevation θ |
|---|---|
| 15° | ≈ 13° |
| 20° | ≈ 17.5° |
| 30° | ≈ 33° |

17.45° falls out at an ordinary camera pitch. That is corroboration, not
measurement, and the shipped file says so in a comment.

### 2.3 What is therefore shipped, and how it is labelled

| | K-space side | J-space side |
|---|---|---|
| distance | 87.5 km | 87.5 km |
| bearing | west | west |
| elevation | +17.45° | +30° |
| jump sphere | 16 km radius | 16 km radius |

West is `bearing: 270`, which through the compass spec's measured
`NORTH_AZ_DEG = 90` is `+X`. The J-space elevation and both jump spheres are
sourced, not measured, and are not corroborated by anything above.

**Every one of these numbers carries a comment in the shipped file naming its
source and its confidence.** A scene file whose numbers look authoritative and
are not is worse than no scene, because the whole point of the picture is to be
believed.

**Not asserted:** that the bearing is west for *every* drifter site rather than
the one photographed. The diagram claims it generally; nothing here verifies it.
The comment says so, and the file is editable, which is the mitigation.

## 3. Why the viewer can show this now, when §8 could not

The formation-editor spec §8 cut its overlay because the two flat panes scaled to
the formation (`paneScale`) and a formation's range is ~0.5 AU — 74.8 million km
against an 89 km site, roughly 840 000 : 1. The hole rendered at ~1.5 × 10⁻⁴ px.

The viewer spec replaced those panes with a perspective camera that has wheel
zoom, an explicit `fitDistance`, and a `Fit` button. The two scales no longer have
to coexist in one frame: they are one button apart. §8's stated requirement — "a
separate, drifter-scaled view … its own pane, not the formation panes reused" — is
satisfied by a camera that can *go* there rather than by a second pane.

Two consequences fall out for free:

- **Range spheres self-manage.** `silhouette` already returns `null` when the eye
  is inside a sphere, which at drifter zoom is every probe's 0.5 AU range. §8's
  "a range circle drawn at drifter scale is the same problem this phase hit"
  resolves itself: they simply do not draw.
- **Position, not reach, is what reads.** Which is exactly what §8 concluded a
  future attempt would have to settle for.

## 4. The file format

### 4.1 Location and discovery

`<app data dir>/scenes/*.yaml`, alongside `presets/` — user data, not
configuration, the same call `presets_dir` makes. `.yml` is accepted too: it
costs one condition, and a file ignored for its extension is the same silent
disappearance §4.6 refuses.

Built-in scenes are `include_str!`'d into the binary and **written to that
directory the first time it does not exist**. The guard is on the *directory*, so
a deleted scene stays deleted and an edited one is never overwritten. Shipping
them as real files on disk is the point: the built-ins are the worked examples a
user copies to write their own.

The picker lists scenes by `scene.name`, sorted, and does **not** deduplicate:
the file is the identity, and two files claiming the same name is a thing the
user did on purpose or a thing they can see and fix. Silently hiding one would
make a copied-and-edited scene appear not to have loaded.

### 4.2 Shape

```yaml
# Axes are EVE's: X and Z horizontal, Y up. +X is west, +Z is north
# (measured in-game — see NORTH_AZ_DEG in probes.ts).
scene:
  name: 'Drifter wormhole — K-space side'
  objects:
    - label: 'Beacon'
      km: 0
    - label: 'Wormhole'
      km: 87.5
      bearing: 270
      elevation: 17.45
      radius_km: 16
    # Coordinates work too, if that is how your source gives them:
    # - label: 'Something measured'
    #   xyz_km: [12.5, 3, -8]     # or xyz_m: [12500, 3000, -8000]
```

`scene.name` is what the viewer's picker shows. `objects` is a list; each object
needs a `label` and exactly one position form.

**Polar form** — `km` (distance from the formation centre), `bearing` (compass
degrees) and `elevation` (degrees above the horizontal plane). `bearing` maps to
the az convention of the compass spec as
`az = NORTH_AZ_DEG + EAST_SIGN · bearing`, so 0 is north, 90 east, 180 south and
270 west. `bearing` and `elevation` default to 0, so an object at the centre is
`km: 0` and nothing else.

**Cartesian form** — `xyz_km: [x, y, z]` or `xyz_m: [x, y, z]`.

`radius_km` is optional and draws a sphere. Absent or 0 draws none.

### 4.3 Two departures from the formation format, both deliberate

`probe_pack.rs` is metres-exact because a formation round-trips back into a
settings file and a value that passes through a rounded display comes back
displaced. Neither applies here — a scene never returns to EVE — so:

- **Kilometres.** The geometry is known as "87.5 km, west, 17.45° up", and
  authoring it in metres would be hostile for no benefit.
- **Polar as a first-class form.** Same reason: it is the shape the fact arrives
  in.

The cartesian form exists because scenes will be written from sources this
project does not control, and some of those will hold coordinates rather than
bearings. `xyz_m` exists because EVE's own coordinates are metres, so a source
that has them should not need a division.

### 4.4 Every distance key names its own unit

`km`, `radius_km`, `xyz_km`, `xyz_m`. There is no file-level `units:` default and
no bare `xyz:`.

A unit that lives anywhere other than the key it applies to is where a silent
1000× error comes from, and a scene that is wrong by 1000× still draws — it just
draws somewhere the user will never look. The key name costs nothing and cannot
be got wrong by a hand edit that moves a line.

For the same reason the axis convention is a comment at the top of **every**
shipped file, not only in this document: a cartesian object authored against the
wrong handedness is silently mirrored, and mirrored geometry is the failure this
format can produce that polar geometry cannot.

### 4.5 The cartesian example ships commented out

Each built-in file carries the `xyz_km` / `xyz_m` form as a **commented-out**
object. The shipped scenes are entirely polar — that is how their geometry is
known — but a user reading the file to learn the format must be able to see that
coordinates are an option without the file asserting an object that is not there.

### 4.6 Errors

Exactly one position form per object. Both forms, or neither, is an error naming
the file and the object. No silent default: an object that quietly lands at the
origin is indistinguishable from the beacon, which is the one place a mistake is
hardest to spot.

A file that fails to parse is **reported, not skipped**. `scene_list` returns
`{ scenes, problems }`, where `problems` holds one message per unreadable file.
This is `probe_pack.rs`'s rule — "skipping a malformed one silently would hand
the user a partial import they did not ask for" — applied to a directory scan: a
typo in a hand-edited scene must not make it silently vanish from the picker.

One bad file does not stop the others loading. That is the difference from
`parse_formations`, and it is justified: an import is one user action over one
file, where partial success is a lie about what was imported; a directory scan is
many independent files, where refusing all of them because one has a typo helps
nobody.

## 5. Where the code goes

### 5.1 Rust normalizes units; TypeScript owns angles

`scenes.rs` reads the YAML with `yaml_rust2` — the dependency `probe_pack.rs`
already uses — and hands the frontend:

```rust
enum ScenePos {
    Polar { km: f64, bearing: f64, elevation: f64 },
    Xyz { m: [f64; 3] },
}
struct SceneObject { label: String, pos: ScenePos, radius_m: f64 }
struct Scene { name: String, objects: Vec<SceneObject> }
```

`xyz_km` and `xyz_m` collapse to one variant in metres, and `radius_km` becomes
`radius_m`. That is unit scaling, not geometry: **`scenes.rs` calls no trig
function.**

The bearing convention depends on `NORTH_AZ_DEG` and `EAST_SIGN`, which the
compass spec §3 established as the single calibration knob for the whole product,
in `probes.ts`. Converting a bearing in Rust would fork that knob across two
languages, and a future patch that moves north would then have to be applied
twice. So the polar → cartesian step stays in `probes.ts`, next to the constants
it reads.

### 5.2 One command

`scene_list() -> { scenes, problems }`, following the `probe_*` commands' shape
in `lib.rs`. There is no write path this slice.

### 5.3 The frontend

- **`probes.ts`** gains one function converting a scene object's position to a
  world `Vec3` in metres — the polar branch is `toCartesian` with the bearing
  mapped through `NORTH_AZ_DEG` / `EAST_SIGN`, the cartesian branch is a pass
  through. This is the only new geometry in the slice, and it is pure, so it is
  `node --test`-able like everything else in that file.
- **`ProbeViewer.svelte`** gains a scene `<select>` in its action row (default
  "None"), draws each object, and gains a **Fit scene** button.
- **`ProbeFormationsView.svelte`** fetches the list once and passes it down.

The viewer owns which scene is showing, as local `$state`. It is a view concern
that nothing else reads and nothing persists.

### 5.4 Drawing

Each object draws as a small marker, its label, and — when it has a radius — a
`silhouette` circle, reusing the function the range spheres already use. A sphere
is a circle from every viewpoint, so that is the shape and not an approximation
of it (viewer spec §4.4).

Scene objects draw **before the probes and do not join their depth sort**, for
the reason the compass does not (compass spec §3): they are context, not subject,
and a marker occluded by a probe cube reads the same as one drawn under it. At
the two zooms that matter the question does not arise — at formation scale the
whole scene is sub-pixel, and at scene scale the probes are far outside it.

Nothing in the scene hit-tests. `pointer-events: none` on all of it, joining
`.axis`, `.range`, `.ring` and `.cardinal` in the viewer's existing rule — a
scene marker that swallowed a click meant for a probe would deselect it.

### 5.5 Fit scene

`fitDistance` already takes positions and an optional per-position radius, which
is exactly a scene. The button is `cam = { ...cam, target: [0,0,0], dist:
fitDistance(scenePositions, sceneRadii) }`.

It appears only when a scene is showing. The existing **Fit** keeps framing the
probes and is not touched: framing both is the mistake §8 made, and the fix is
two buttons, not one cleverer one.

## 6. Verification

- **`scenes.rs`**, `#[cfg(test)]` in-module like `probe_pack.rs`: a polar object
  and both cartesian forms parse to the expected normalized values; `xyz_km` and
  `xyz_m` agree for the same physical point; an object with both forms is an
  error; an object with neither is an error; a file with no `scene:` key is an
  error naming the file; one unreadable file among several appears in `problems`
  while the rest still load.
- **`probes.test.ts`**: a bearing of 270 with elevation 0 lands on `+X` (west),
  0 lands on `+Z` (north), and the elevation raises `y` — the smallest check that
  fails if the bearing convention is flipped or the trig is transposed. Pinned
  against `NORTH_AZ_DEG` rather than hard-coded, so moving the knob moves the
  test with it.
- **`ProbeViewer.spec.ts`**: a scene renders its objects' labels; selecting
  "None" removes them. The projection maths is already covered by the viewer's
  own tests and is not re-tested here.

## 7. Not built

- **An in-app scene editor.** Named by the developer as a later stretch goal. The
  file format is designed to be hand-edited, which is what makes deferring it
  cheap: the editor, when it comes, is a writer for a format that already exists.
- **A distance readout** — how far each probe is from a named scene object. The
  likeliest next ask, because "is probe 3 within the jump sphere" is a number and
  the picture only approximates it. Left out to keep this slice to *drawing*, and
  it needs no format change when it arrives.
- **Persisting the chosen scene.** One `<select>` re-picked per session. Add it to
  `prefs.rs` if it becomes annoying.
- **Scenes seeding or snapping a formation.** A scene is reference data; a
  formation is a thing written into a settings file. Letting one generate the
  other is a separate feature with a separate blast radius.
- **A bookmark object in the shipped scenes.** The developer's diagram shows a
  bookmark 10 000 km out along the beacon → hole ray, with the 3 000 km / 5 000 km
  legs describing it. That is *probe and bookmark placement advice* — the answer
  the tool exists to help you find — not geometry that is in space. Drawing it
  would put the tool's own suggestion into the reference data.

## 8. Phasing

| phase | delivers |
|---|---|
| 1 | `scenes.rs` — format, parse, unit normalization, built-in install, tests |
| 2 | IPC (`lib.rs`, `api.ts`) and the two shipped scene files |
| 3 | `probes.ts` conversion, viewer picker, drawing, Fit scene |

Phase 1 is testable with nothing else built; phase 3 is the only part that needs
a running app to judge.

Own branch (`probe-scenes`) per the branch policy: this is a behaviour change,
not corrective work riding an existing branch.
