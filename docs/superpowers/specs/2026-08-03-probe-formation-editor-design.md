# Probe formation editor (design)

Status: designed 2026-08-03, not yet planned.

Milestone context: a self-contained account-scoped slice, like the keybindings
editor. One key under `ui`, no cross-file link, no structural editing. The
shape was measured on 2026-08-03 and written into
`docs/settings-field-reference.md` (commit 52fd23e) before this design started;
this document adds the placement measurement (§2.1) and the client-defaults
measurement (§2.5) that the field reference does not carry.

## 1. Goal

`core_user_<id>.dat → ui → probescanning.customFormations` holds the player's
saved probe arrangements — the entries in the scanner's formation menu. The app
has never modelled it. In-game the only way to author one is to drag eight
probes into place by hand and save, and the only way to give a second account
the same formation is to do it again from memory.

This slice ships four things:

1. **A formation editor** — edit the name, the probe count, the scan range, and
   each probe's position, on any custom formation in the open account file.
2. **Creation** — a new formation from scratch, or as a copy of an existing one.
3. **A visualiser** — two fixed orthographic views of the formation, with each
   probe's scan sphere drawn, so the arrangement can be judged rather than
   inferred from twenty-four numbers.
4. **A batch category** — copy one account's whole formation set onto other
   accounts, in the existing batch-apply tool.

A drifter-wormhole context overlay was attempted as a fifth, last phase and cut
in review (§8).

## 2. The formation model (confirmed from the corpus)

The dict shape, the fixed length of 8, the uniform range, the id reuse and the
`-4` scratch slot were measured over the reference snapshot on 2026-08-03 and
are recorded in `docs/settings-field-reference.md`, "Custom probe formations".
That section is the source of truth for those facts and is not restated here.
Two further measurements were taken for this design.

### 2.1 Placement: under `ui`, not at the root

The field reference names the key `probescanning.customFormations` without
saying where it sits. Measured on `core_user_12829261.dat` from
`2026-07-31T012708Z_target-origin-after2`: the key is at indent 4 inside the
root `ui` section (opened at dump line 10836), alongside `neocomButtonRawData`
and `targetOrigin`. `probescanning.selectedFormationID` is its sibling in the
same section.

**The dot is part of the key name, not a path separator.** The key path is
`ui → probescanning.customFormations`, two levels, not three.

```
core_user_<id>.dat
  root → ui                                  Dict — a Ref in account files
       → probescanning.customFormations      Tuple(FILETIME, Dict)  ← ONE stamp
            → { Int id : Tuple(name, List[ Tuple( Tuple(x,y,z), range ) ]) }
       → probescanning.selectedFormationID   Tuple(FILETIME, Int)
```

This is the ordinary value-wrapper convention (`format-notes.md`): one stamp on
the container, bare values inside. Unlike `cmd → customCmds`, nothing here
inverts it.

**Account-file `ui` is itself a `Ref`.** `neocom.rs` already documents this trap
— a bare `is_bytes` match on the root key misses it, and `treewalk::section`
exists to resolve it. This slice reads `ui` through `section`, the same way.

### 2.2 Coordinates

The three floats are metre offsets from the formation centre, in EVE's axes:
**X and Z are the horizontal plane, Y is up**. The one formation dumped in full
makes the axis roles legible:

```
  1  (-1199120384, -115136512, -415997952)   ┐
  2  (-1199120384,  16133054464, -415997952) ├ same X and Z, Y differs
  3  (-1199120384, -15442242560, -415997952) ┘  — a vertical column
  4  ( 22762389504, -115136512, -122200064)  ┐
  5  (  8015765504, -115136512,  21694230528)├ same Y as probe 1,
  6  (-18448932864, -115136512,  16181035008)│  spread in X and Z
  7  (-17591099392, -115136512, -14030192640)┘  — a horizontal ring
```

A horizontal ring plus a vertical column. **This is why the visualiser needs
two panes** (§5): probes 1, 2 and 3 differ only in Y, so a top-down view alone
draws them as a single dot and hides the structure that distinguishes one
formation from another.

### 2.3 Range

Every entry carries its own range slot, so the format permits eight different
ranges. No saved formation uses that freedom: all 984 corpus entries carry
`74798935350.0`, which is 0.5 AU in metres to the metre. Re-verified on
`core_user_12829261.dat` — 8 entries, one distinct range value.

**The editor exposes one range per formation**, written to every probe. A
loaded formation whose entries disagree is reported with a `mixed_range` flag
and its range field goes read-only, so a file we do not understand is shown
rather than silently flattened to its first value.

### 2.4 Probe count

The corpus has exactly 8 in all 123 formations and no other length. EVE
nonetheless lets a player launch fewer than 8 probes, so the editor allows
**1 to 8**. This writes a length the corpus has never shown; it is accepted
deliberately on the grounds that the client produces it in-game, and is to be
confirmed in-client before release.

### 2.5 The client's default formations are not in the file

EVE offers built-in formations at several range increments. **None of them are
stored here.** Measured 2026-08-03 over the 23 account files in
`2026-07-31T012708Z_target-origin-after2` that carry the key, spanning three
server installs (Tranquility ×2, Eternity):

```
c_…tq/core_user_12829261.dat        0: "close"
c_…tq/core_user_13036531.dat        0: "close"
g_…eternity/core_user_29304506.dat  0: "close"   1: "on grid"
g_…eternity/core_user_7214485.dat   0: "close"   1: "on grid"
g_…tq/core_user_13036531.dat       -4: b"tempFormation"   0: "close"
…
```

Every named formation is player-authored, plus the `-4` scratch slot. The key
means what it says: **custom** formations only. The client's own defaults live
in the client, so nothing in a settings file can seed them.

Consequence for §4.4: a new-from-scratch formation cannot be derived from the
client's defaults without a source outside the settings files. It uses an
arbitrary but valid starting arrangement for now. Deriving one from the
client's own data is booked as a later exploration, not part of this slice.

## 3. Data model — `crates/settings-model/src/probes.rs`

A new module beside `neocom.rs`, which it follows closely: same `section`
lookup, same inline-then-write discipline, same "validate before inlining so a
rejected edit leaves the document byte-for-byte as it was" rule.

```rust
pub struct Formation {
    pub id: i64,
    pub name: String,
    pub probes: Vec<[f64; 3]>,   // metres, formation-centre relative
    pub ranges: Vec<f64>,        // metres, one per probe — the file's own values
    pub range: f64,              // metres — the first, what the single field edits
    pub mixed_range: bool,       // the entries disagreed
}

pub struct Formations {
    pub formations: Vec<Formation>,   // ascending id
    pub selected: Option<i64>,        // probescanning.selectedFormationID
}
```

### 3.1 Reading

- **Negative ids are excluded from the projection.** `-4` is the client's
  scratch copy of the formation being edited, not a user formation. It is never
  shown and never rewritten — it stays in the file untouched by every write
  path below.
- **The name reads both `Bytes` and `Str`.** The scratch slot is `Bytes`, every
  user formation is `Str` (field reference, §3 trap 5). A reader that assumes
  one panics or blanks on the other.
- **Metres are the unit everywhere in Rust.** No AU conversion, no rounding.
  That is a display concern and lives in the frontend (§4.2).

### 3.2 Errors

Following `NeocomError`: a `#[serde(tag = "code")]` enum with a `Display` giving
the user-facing sentence.

| variant | when |
|---|---|
| `NoUi` | no `ui` section in the document |
| `NoFormations` | no `probescanning.customFormations` under `ui` |
| `NoSuchFormation` | an id the file does not hold |
| `BadProbeCount` | a write with 0 probes, or more than 8 |
| `BadName` | a name that is empty once trimmed |

### 3.3 Writing

```rust
pub fn set_formation(v: &mut Value, id: i64, name: &str, probes: &[[f64; 3]], range: f64)
    -> Result<Formations, ProbeError>;   // replaces, or creates at a new id
pub fn remove_formation(v: &mut Value, id: i64) -> Result<Formations, ProbeError>;
```

- Both **preserve the existing `(timestamp, dict)` wrapper**, and mint one with
  a zero `Long` when the key is absent — the `overview_states.rs` rule. EVE
  re-stamps on its next save.
- Names are written as `Str`, never `Bytes`. The only `Bytes` name in the
  corpus is the scratch slot, which no write path touches.
- `id < 0` is rejected by `set_formation` and `remove_formation` alike, so no
  caller can reach the scratch slot through them.
- **New id = the smallest unused id `>= 0`.** Ids are small and reused in the
  corpus, not minted; this matches that.
- `remove_formation` **repoints `selectedFormationID`** when it names the
  formation being deleted: to the lowest surviving id, or the key is left alone
  if none survive. Leaving it pointing at a deleted formation is the one
  outcome that could confuse the client.

## 4. Editor — `app/src/lib/ProbeFormationsView.svelte`

A new view tab, "Probes", gated exactly like Keybinds: an open account file,
with the shared-account banner above it (`sharedLabel`), and the Accounts nudge
when the user slot is empty.

```
┌ Formations ──┬ close ─────────────────────────────────────────────────┐
│ ▸ close      │ name [close            ]      range [0.5    ] (AU|km)  │
│   on grid    │                                                        │
│              │  #   X         Y         Z    │  dist     az°    el°   │
│ [New]        │  1  -0.00801  -0.00077  -0.00278│ 0.00857  -180.0  -5.2│
│ [Duplicate]  │  2  -0.00801   0.10784  -0.00278│ 0.10812  -180.0   86.1│
│ [Delete]     │  …                            │                        │
│              │  [+ probe]   (1-8)                                     │
├──────────────┴────────────────────────────────────────────────────────┤
│      top-down (X/Z)                    side (X/Y)                     │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘
```

### 4.1 Three ways to move a probe

Every probe row is editable three ways, all bound to the same underlying
metres:

1. **Cartesian** — X, Y, Z.
2. **Distance from the formation centre** — changing it scales the probe along
   its existing direction, *preserving the angles*.
3. **Angles** — azimuth and elevation, *preserving the distance*.

With EVE's axes (§2.2):

```
r   = sqrt(x² + y² + z²)
az  = atan2(z, x)        the horizontal bearing
el  = asin(y / r)        the angle above the horizontal plane
```

**At `r == 0` the angles are undefined.** The row keeps the last angles the user
entered rather than snapping to zero, so typing `0` into the distance field and
then typing a distance back does not silently move the probe to the X axis.

### 4.2 Precision: metres are the source of truth in the frontend too

Displayed AU, km and angle text is **derived**; only a field the user actually
types into is converted back to metres. An untouched coordinate keeps its exact
`f64`.

This is not a nicety. One metre is 6.7e-12 AU, so any display rounding that
round-tripped through the model would corrupt **every probe on every save** — a
whole formation quietly displaced because one field was edited. `ipc.test.ts`'s
sibling `ProbeFormationsView.spec.ts` asserts an untouched coordinate survives a
save bit-for-bit (§6).

The AU/km toggle is one control for the whole view, covering the coordinate
columns, the distance column and the range field together.

### 4.3 Range

One field per formation, in AU or km, written to all probes. When the loaded
formation has `mixed_range`, the field shows the differing values read-only with
a note naming which probes differ, rather than offering an edit that would
flatten them (§2.3).

### 4.4 Creating

- **Duplicate** copies the selected formation at a new id, named `<name> copy`.
  Names are not unique keys — the id is — so no collision handling is needed
  beyond that.
- **New** starts from 8 probes at the corners of a cube of half-side
  `range / 2`.

The cube is arbitrary. It is valid, it is symmetric, and it is not derived from
anything the client ships, because the client's defaults are not in the settings
file (§2.5). It carries a `ponytail:` comment naming that: *arbitrary starting
cube, derive from the client's own defaults if a source for them is found*.

## 5. Visualiser

Two **fixed orthographic SVG panes** — top-down (X/Z) and side (X/Y) — sharing
one scale and one selection, so a probe highlighted in one highlights in both.
Each probe draws its scan range as a circle. §2.2 is the argument for two panes
rather than one.

Fixed views, no camera. A rotatable or isometric view is a later change that
does not invalidate this one: it would add a third pane or replace both, and
either way the projection maths here is the trivial part.

Plain SVG in the component, following `LayoutView`'s scale-and-project helpers
in spirit but not sharing them — that canvas is a 2D screen-space model in CSS
pixels, and forcing a 3D metre-space one through it would bend both.

## 6. Testing

**`probes.rs` units**, following `neocom.rs`'s own test module:

- the `(timestamp, dict)` wrapper survives an edit, with the original stamp
- an absent key is minted as `(zero Long, dict)`, not a bare dict
- negative ids are excluded from the projection and still present in the file
  after a write
- a `Bytes` name and a `Str` name both read; a written name is always `Str`
- 1 and 8 probes are accepted, 0 and 9 are `BadProbeCount`
- a rejected write leaves the document byte-for-byte unchanged
- `remove_formation` repoints `selectedFormationID` when it named the deleted
  formation, and leaves it alone otherwise
- a mixed-range formation is flagged, not flattened

**`tests/probes_corpus.rs`**, following `neocom_corpus.rs`: every real account
file carrying the key projects without error, holds 8 probes per formation, and
carries a uniform range — locking in the measurements of §2 so a future change
that breaks them fails loudly.

**`ProbeFormationsView.spec.ts`**: AU↔km conversion, cartesian↔spherical round
trips including the `r == 0` case, and the untouched-coordinate precision
guarantee of §4.2.

## 7. Batch category

- `Category::ProbeFormations` → key path `&[b"ui", b"probescanning.customFormations"]`
- `Aspect::ProbeFormations` → the **account** side, labelled
  *Probe formations (custom scan formations)*

`absent_means_default()` stays **false**. This is a whole-section category, so a
source with no formations must skip, never delete the target's — the same rule
that protects `overview`. The removal semantics in `batch.rs` are for leaf HUD
keys only and this must not join them.

**`selectedFormationID` is deliberately not carried.** It is `0` in every corpus
file that has it, and a copy brings the ids along with the formations, so
copying it would be a no-op on today's data and an override of a per-account
preference on any data where it is not. The formations move; which one is
active stays the target's own.

One consequence of that choice: a batch copy can replace the target's whole
formation dict while leaving its `selectedFormationID` pointing at an id that
no longer exists in it. `probes.rs`'s own `remove_formation` goes out of its
way to prevent exactly that state on its own write path, calling it "the one
outcome that could confuse the client" — the two paths disagree about how much
it matters. This one accepts the risk as the cost of leaving the target's
selection alone. Near-zero on real data: the corpus's `selectedFormationID` is
`0` everywhere.

Preset support needs no separate work: `presets::prune` builds its parent dicts
from `Category::key_path`, so listing the category in the aspect is sufficient.

## 8. Drifter-wormhole overlay — built, then cut

Phase 5 (§9) built this; the branch's final review round cut it. Left in place
rather than deleted so the reasoning and the sourced-not-measured geometry are
not lost to a diff nobody rereads.

The overlay drew the warp-in beacon at the formation centre, the hole 89 km out
on a 14° downward axis, and its 16 km jump sphere, on the same two panes that
draw the formation — the idea being that "does this formation cover the hole
when dropped on the beacon?" could be judged by eye.

**Why it was cut: the two things it drew are not at comparable scales.** A
formation's range is ~0.5 AU — 74.8 million km; the drifter site is 89 km
across. That is roughly an 840,000:1 gap. The panes scale to the formation
(`paneScale`), so at that scale the hole rendered about 1.5 × 10⁻⁴ px from the
pane centre and its 16 km jump sphere at a radius of about 2.6 × 10⁻⁵ px — both
far below one screen pixel. Ticking the checkbox painted a dot on top of a dot.
The coverage question the overlay existed to answer read as unconditionally
true regardless of the actual formation, which is worse than not answering it
at all. (The shipped hint text claimed the drifter geometry was "about 800
times" narrower than a formation; the real ratio is roughly three orders of
magnitude further off than that.)

These numbers are sourced, not measured, and the sources disagree — kept here
because that disagreement is still useful to whoever picks this back up, and
says nothing here should be read as measured fact regardless of which number a
future attempt uses:

| source | warp-in → hole |
|---|---|
| [Jambeeno's Uni guide](https://jambeeno.com/uni) | exactly 89 km, 14° outside / 26.5° in, 16 km jump sphere |
| [EVE University wiki](https://wiki.eveuniversity.org/Wormholes) | ~80 km, both sides deadspaced |
| patch-note summary, March 2026 | 75 km k-space side, was 88 km |
| [Random Eve Stuff](https://randomevestuff.wordpress.com/unidentified-wormholes/) | ~100 km, one measured at 91 km |

Jambeeno's was taken because it is the only full 3D geometry and the most
recent. The direction of the 14° was flagged as an assumption to check
in-client — the source says "a slight downward angle" without stating from
which end. Both points are moot until a future attempt has a view that can
actually show the geometry.

**What a future attempt needs:** a separate, drifter-scaled view — its own
pane, not the formation panes reused, because a formation is always orders of
magnitude bigger than an 89 km site and no single scale shows both. That view
would plot the probes' *positions* relative to the beacon at drifter scale,
without their range circles: every corpus range is 0.5 AU, millions of km, so
a range circle drawn at drifter scale is the same problem this phase hit —
too large to read next to an 89 km hole. Position, not reach, is the only part
of a formation small enough to judge by eye at that scale.

## 9. Phasing

| phase | delivers |
|---|---|
| 1 | `probes.rs` model, unit tests, corpus test |
| 2 | IPC (`ops.rs`, `lib.rs`, `api.ts`) and the editor view — list, edit, create, duplicate, delete, AU/km, spherical fields |
| 3 | The two SVG panes |
| 4 | Batch category and aspect |
| 5 | Drifter overlay — built, then **cut** in the final review round (§8) |

Phases 1–2 are the usable product; 3 makes it judgeable; 4 makes it worth
having across eight accounts; 5 shipped and was removed — see §8.

Own branch (`probe-formation-editor`) per the branch policy: this is a
behaviour change, not corrective work riding an existing branch.

## 10. Deferred

Named here so they are decisions rather than omissions:

- **Dragging probes in the visualiser.** Numbers edit, the picture previews.
  Adding drag later does not change the model or the projection.
- **A rotatable / isometric view.** §5.
- **Deriving the from-scratch formation from the client's own defaults.** §2.5 —
  needs a source outside the settings files.
- **Configurable or additional site overlays.** §8.
- **Carrying `selectedFormationID` in a batch copy.** §7.
