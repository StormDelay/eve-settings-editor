# Probe formation sharing (design)

Status: designed 2026-08-04, not yet planned.

Builds on the probe formation editor
(`2026-08-03-probe-formation-editor-design.md`) and the 3D viewer
(`2026-08-04-probe-3d-viewer-design.md`). Those two make a formation editable
and judgeable inside one account file. This one makes a formation *portable*:
out of the app as text, and back into any account file.

## 1. Goal

A formation authored in this app currently cannot leave the account file that
holds it, except through the batch tool — which copies one account's **whole**
formation set onto another account of the same user, and only within this app.
There is no way to hand a formation to a fleetmate, paste one from a forum
post, or keep a library of them outside the settings tree.

This slice ships two exchange routes over one text format:

1. **Copy / paste** — one formation to and from the system clipboard, with
   Ctrl-C and Ctrl-V as shortcuts. For quick sharing over chat.
2. **Export / import** — any number of formations to and from a `.yaml` file,
   with a picker on both ends. For libraries and fleet doctrine sets.

Both routes are **additive on the way in**: nothing already in the account file
is replaced or deleted.

## 2. The exchange format

### 2.1 Metres, with comments for legibility

Positions and ranges are written in **metres, exactly as the file stores
them**. This is the same rule the editor already enforces internally
(editor spec §4.2): one metre is 6.7e-12 AU, so any unit conversion in the
exchange format displaces every probe of every formation that round-trips
through it. AU, km and spherical forms were all considered and rejected on that
basis — a shared formation that comes back 150 m off the one that was shared is
a silent corruption dressed up as convenience.

Metres are not legible on their own, so the emitter annotates them with
**comments**, which the parser ignores:

```yaml
# EVE probe formations. Positions and ranges are metres from the formation centre.
formations:
  - name: close
    range: 74798935350          # 0.5 AU
    probes:
      - [-1199120384,   -115136512,  -415997952]   # 1279421 km
      - [-1199120384,  16133054464,  -415997952]   # 16181118 km
      - [-1199120384, -15442242560,  -415997952]   # 15495129 km
      - [ 22762389504,  -115136512,  -122200064]   # 22762723 km
      - [  8015765504,  -115136512, 21694230528]   # 23127210 km
      - [-18448932864,  -115136512, 16181035008]   # 24499399 km
      - [-17591099392,  -115136512, -14030192640]  # 22508766 km
```

The comment on each probe is its distance from the formation centre, rounded to
the kilometre — the one derived number that says most about where a probe sits.
The comment on `range` is the AU value, because AU is how EVE's own scan-range
control is labelled.

Numbers are emitted with Rust's `{:?}` float formatting, the shortest
representation that round-trips the `f64` — the same rule `emit_pack` already
uses for `Node::Float`. Column alignment is cosmetic and not required on read.

### 2.2 No ids

`Formation.id` is deliberately absent from the format. An id is account-local:
it is the key of the `customFormations` dict, it is reused rather than minted
(editor spec §3.3), and `selectedFormationID` points at it. Carrying one across
files would mean either colliding with the target's own ids or overwriting them
— both worse than allocating fresh on import, which is what §4.3 does.

### 2.3 Mixed ranges

The model carries `ranges: Vec<f64>`, one per probe, and the editor exposes a
per-row range picker. All 984 corpus entries are uniform (editor spec §2.3),
so `range:` is the common case and the format leads with it.

**When a formation's probes disagree, the emitter additionally writes
`ranges:`**, a list positionally matching `probes:`, and the parser prefers it
when present:

```yaml
  - name: staggered
    range: 74798935350          # 0.5 AU — first probe's, for readers that ignore `ranges`
    ranges: [74798935350, 74798935350, 149597870700, 149597870700]
    probes:
      - …
```

Flattening a mixed-range formation to one value is the outcome the editor spec
went out of its way to prevent (§2.3: "a file we do not understand is shown
rather than silently flattened"). The exchange format must not reintroduce it.
`range:` is still written in the mixed case so that a reader — human or future
— that only understands `range:` gets the first probe's value rather than
nothing.

### 2.4 What identifies the format

The top-level `formations:` key. A YAML document without it is rejected as
`NotFormations`, the same job `PackError::NotAPack` does for overview packs —
the user picked the wrong file, and saying so beats applying nothing in
silence.

**No version key.** Nothing about the format is expected to change in a way a
version marker would help with, and an unused field in a hand-edited file is a
field people will get wrong. If a breaking change ever arrives, the absence of
a version is itself the marker for "version 1".

## 3. Model — `crates/settings-model/src/probe_pack.rs`

A new module beside `probes.rs`, mirroring how `overview_pack.rs` sits beside
`overview_states.rs`: `probes.rs` speaks the marshal document, `probe_pack.rs`
speaks YAML, and all format knowledge stays on one side of that line.

```rust
/// A formation as it travels between files: no id, because an id is
/// account-local (§2.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormationSpec {
    pub name: String,
    pub probes: Vec<[f64; 3]>,
    pub ranges: Vec<f64>,
}

pub fn emit_formations(specs: &[FormationSpec]) -> String;
pub fn parse_formations(text: &str) -> Result<Vec<FormationSpec>, ProbeError>;

/// `want`, or `want copy`, or `want copy 2`… — the first that `existing` does
/// not already hold.
pub fn unique_name(existing: &[String], want: &str) -> String;
```

Parsing uses **`yaml-rust2`**, already a dependency of this crate for
`overview_pack.rs`. Emitting is hand-built string, also as `emit_pack` does —
the document is three keys deep and a serializer round-trip would cost the
comments of §2.1.

### 3.1 Errors

Two variants added to the existing `ProbeError`, rather than a second error
enum. `ops.rs` already has one `probe_err` mapper that lifts the `code` tag out
of the serialization; a new enum would need a second.

| variant | when |
|---|---|
| `BadYaml { message }` | the text is not valid YAML |
| `NotFormations` | valid YAML with no top-level `formations:` list (§2.4) |

A `formations:` entry that is malformed — a missing `name`, a probe that is not
three numbers — is `BadYaml` with a message naming the entry. Skipping bad
entries silently would hand the user a partial import they did not ask for.

## 4. IPC — `ops.rs`, `lib.rs`, `api.ts`

Five commands. The first four are three-line wrappers over §3; only
`add_probe_formations` touches the document.

| command | body |
|---|---|
| `probe_yaml(formations: Vec<FormationSpec>) -> String` | `emit_formations` — the Copy button |
| `probe_parse_yaml(text: String) -> Vec<FormationSpec>` | `parse_formations` — the Paste button |
| `probe_export(path: String, formations: Vec<FormationSpec>)` | `emit_formations` + `fs::write` |
| `probe_import(path: String) -> Vec<FormationSpec>` | `fs::read_to_string` + `parse_formations` |
| `add_probe_formations(formations: Vec<FormationSpec>) -> Formations` | §4.3 |

**Parse and apply are separate commands.** The picker has to show a file's
contents before anything is written, so importing is necessarily two steps; the
clipboard path reuses the same two.

**The frontend supplies the formation data on every export path** rather than
naming ids for the backend to look up. This is what makes §5.1's
"export what you see" possible at all: the backend's projection is the *saved*
state, and the draft only exists in the view.

### 4.1 Why not reuse `set_probe_formation` per formation

Importing eight formations through the existing single-formation command means
eight IPC round trips, and each one runs `blue_marshal::reshare` over the whole
document. `add_probe_formations` does one reshare for the batch. It is also the
only place the collision rule of §4.3 needs to live.

### 4.2 Validate the whole batch before writing any of it

`add_probe_formations` checks every spec — 1 to 8 probes, a name that is
non-empty once trimmed, `ranges` the same length as `probes` — **before**
inlining anything. This is the crate's existing rule ("validate before inlining
so a rejected edit leaves the document byte-for-byte as it was", editor spec
§3), and here it also prevents the worse failure: a four-formation import that
writes two and then errors, leaving the user to work out which.

### 4.3 Collision rule: add, and suffix the name

Every incoming formation is added at a **new id** (`next_id`), never matched
against or written over an existing one. Names are not keys, so a collision is
cosmetic — but two identical `close` entries in EVE's formation menu is a
usability bug, and a paste that appears to do nothing is worse. So a name the
account already holds becomes `close copy`, then `close copy 2`, matching what
the editor's Duplicate button already produces.

Ids are allocated one at a time as the batch is applied, because `next_id`
returns the lowest free gap: allocating all of them up front from the
pre-batch projection would hand the same id to every formation in the batch.

## 5. Frontend

### 5.1 One source for everything that leaves the view

```ts
/** The formation set as the user currently sees it: the loaded projection with
 * the selected formation's uncommitted draft substituted in. */
const visible = $derived((loaded?.formations ?? []).map((f) =>
  f.id === selectedId ? { ...f, name: draftName, probes: draftProbes, ranges: draftRanges } : f));
```

Copy takes the entry at `selectedId`; Export and its picker take all of
`visible`. Exporting the backend's saved projection instead would silently drop
whatever the user is mid-edit — and since blur commits, that state is reachable
by clicking Export while a coordinate field still has focus.

This also removes a race that the saved-projection route would have: the Copy
button's click blurs the focused field, which fires an async `commit()`, so a
copy that read the backend could read either side of that write depending on
timing. Reading the draft is correct either way.

### 5.2 `FormationPicker.svelte` — one modal, two uses

```
  Import formations from tengu-fleet.yaml
 ---------------------------------------
  [x] close                8 probes  0.5 AU
  [x] on grid              8 probes  4 AU
  [ ] pinpoint             4 probes  0.25 AU
  [ ] spread wide          8 probes  16 AU
 ---------------------------------------
  [Select all]        [Cancel]  [Import 2]
```

Built on the existing `.overlay` class (`app.css:101`, used by
`+page.svelte:665`), so this is not a new modal mechanism.

Props: `title`, `items: FormationSpec[]`, `confirmLabel`, `onconfirm(indices)`,
`oncancel`. It knows nothing about ids, files or the clipboard — Export hands
it `visible`, Import hands it what `probe_import` returned, and it hands back
indices either way.

A row shows the name, the probe count, and the range in AU, or `mixed` when the
probes disagree (§2.3). Everything starts ticked; the confirm button carries
the count and is disabled at zero.

### 5.3 Buttons and shortcuts in `ProbeFormationsView.svelte`

Four buttons. Copy and Paste sit with the per-formation controls; Export and
Import are set-wide, so they go with `New` / `Duplicate` / `Delete` in the list
sidebar.

| action | flow |
|---|---|
| **Copy** | `probe_yaml([visible@selected])` → `navigator.clipboard.writeText` |
| **Paste** | `navigator.clipboard.readText()` → `probe_parse_yaml` → `add_probe_formations` |
| **Export…** | `saveDialog` → picker over `visible` → `probe_export` |
| **Import…** | `openDialog` (`yaml`/`yml`) → `probe_import` → picker → `add_probe_formations` |

Import and Paste both call `onUserDirty()`; the user still saves, and the normal
backup chain applies. Neither is available when the slot is read-only — the
existing `Fidelity::ReadOnly` check in `edit_user_probes` covers the write, and
the buttons follow the same disabled rule the editor's other write actions use.

**Ctrl-C and Ctrl-V** via `<svelte:window onkeydown>` inside the view. The tab
is conditionally mounted (`+page.svelte:594`), so the listener does not exist
while another view is open and cannot leak into it. `+page.svelte` already
holds a window handler for Ctrl-S and Ctrl-F; neither letter collides.

**The shortcut bails when the event target is an `input`, `select` or
`textarea`.** A tab full of coordinate fields is exactly where a user expects
Ctrl-C to copy the number they just selected, and stealing that would be a
regression on ordinary text editing.

### 5.4 Clipboard read, and the fallback

`navigator.clipboard.writeText` is already used in this codebase
(`WindowPanel.svelte:99`) and needs no permission. **Reads are the uncertain
half**: `navigator.clipboard.readText()` requires a permission WebView2 may
refuse without showing a prompt.

So the Paste button tries `readText()`, and on rejection shows a message asking
the user to press Ctrl-V instead. Ctrl-V is handled by a `paste` event listener
reading `event.clipboardData`, which is a plain DOM event and needs no
permission at all — it cannot be refused, because the user pressing the key
*is* the grant.

The Tauri clipboard-manager plugin would remove the uncertainty at the cost of
an npm dependency, a cargo dependency and a capability entry, for a feature two
DOM APIs already cover. Declined; revisit only if the fallback proves to be the
common path in practice rather than the rare one.

## 6. Testing

**`probe_pack.rs` units:**

- emit → parse round-trips every `f64` **bit-for-bit**, on a formation whose
  coordinates are the corpus's own values
- a mixed-range formation survives via `ranges:`, and a uniform one does not
  emit that key
- a minimal hand-typed document — no comments, no alignment, no trailing
  `ranges` — parses, since §2.1 promises hand-editability
- comments are ignored on read, including a comment that looks like a number
- junk text is `BadYaml`; a valid YAML document with no `formations:` key is
  `NotFormations`; a probe that is not three numbers is `BadYaml`
- `unique_name` against an empty set, a colliding set, and a set that already
  holds `x copy`

**`ops.rs` units:**

- `add_probe_formations` allocates distinct ids across a batch (§4.3)
- a batch containing one invalid spec writes **none** of them, and leaves the
  document byte-for-byte unchanged (§4.2)
- a name colliding with an existing formation is suffixed, and the existing one
  is untouched

**`ProbeFormationsView.spec.ts`:**

- Copy sends the **draft**, not the loaded projection: edit a coordinate, copy
  without blurring, assert the copied text carries the edited value (§5.1)
- Ctrl-C inside a coordinate input does not trigger a formation copy (§5.3)
- picker indices map to the formations the user ticked

**`FormationPicker.spec.ts`:** select-all toggles every row, the confirm button
is disabled at zero selected and carries the count otherwise.

## 7. Phasing

| phase | delivers |
|---|---|
| 1 | `probe_pack.rs` — format, emit, parse, `unique_name`, unit tests |
| 2 | IPC: the five commands, `api.ts`, `ops.rs` tests |
| 3 | `FormationPicker.svelte` and its spec |
| 4 | Copy / Paste in the view, with the Ctrl-C/Ctrl-V shortcuts and the read fallback |
| 5 | Export / Import in the view |

Phases 1–2 are the whole format and are testable without any UI. 4 and 5 are
independent of each other and both depend on 3 only for the picker, which 4
does not use — so 4 can ship before 3 if the picker turns out to be the slow
part.

Own branch (`probe-formation-sharing`) per the branch policy: this is a
behaviour change, not corrective work riding an existing branch.

## 8. Deferred

Named here so they are decisions rather than omissions:

- **Replacing an existing formation on import.** Everything is additive (§4.3).
  Overwrite needs per-row add-or-replace in the picker and a rule for what
  "same formation" means when names are not keys; delete-then-import covers it
  today.
- **A format version key.** §2.4.
- **Importing EVE's own exports.** The client has no formation export, so there
  is no foreign format to accept. If one ever appears, `parse_formations` is
  where it would be sniffed.
- **Sharing through the preset library.** `Aspect::probe_formations` already
  moves whole formation sets between accounts of the same user. It stays what
  it is: this slice is the text-based, per-formation, leaves-the-machine route,
  and the two are not merged.
- **Drag-and-drop of a `.yaml` onto the window.** The Import button covers the
  same need; drop targets are a whole-app concern, not a probe-tab one.
