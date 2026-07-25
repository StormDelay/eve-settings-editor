# Overview depth — slice 4: import/export overview packs (design)

Date: 2026-07-25
Status: designed, ready for writing-plans.
Roadmap: **slice 4 of the "overview depth" milestone**, and the last one. Slice 1
(tab management) shipped v0.9.0; slice 2 shipped as 2a (preset management +
tab→preset mapping, v0.11.0) and 2b (preset group contents + built-in default
profiles, v0.12.0); slice 3 (states, colours and tags) shipped v0.13.0.
Builds on: the `overview` container vocabulary the three previous slices
established (`overview.rs`, `overview_presets.rs`, `overview_states.rs`,
`overview_tabs.rs`), the `edit_user_overview` idiom in `ops.rs` (lock →
inline-first → mutate → `reshare` → re-project → mark dirty), and the
`@tauri-apps/plugin-dialog` file-picker already used by the sidebar's "Open
file…".

## 1. Goal

EVE can export an account's overview settings to a YAML file and import one
back (Overview Settings → Misc → Import/Export Overview Settings). The community
distributes overview **packs** in exactly that format. This slice makes the
editor read and write that same file.

- **Import** a pack onto the open character's account, out of game, without
  logging in.
- **Export** the account's overview as a pack EVE itself can load.

Import is **replace-per-section**: every section the file defines replaces that
part of the account's overview wholesale; a section the file omits is left
alone. That second half is not a concession — it is what makes the modular
"preset-only" or "appearance-only" packs people actually publish work.

Scope is **one account per import** — the open character's. Fanning a pack out
to every character is already covered: import once, then use batch apply's
Overview aspect. Everything here is `core_user` only; no `core_char` writes.

## 2. The pack format (confirmed from real packs and the corpus)

### 2.1 Encoding

A pack is a YAML document whose **dicts are encoded as lists of `[key, value]`
pairs** — the shape python's `yaml.dump` produces for a list of tuples:

```yaml
presets:
- - A preset name
  - - - alwaysShownStates
      - []
    - - filteredStates
      - - 21
        - 36
    - - groups
      - - 25
        - 26
```

Only the top level is a real YAML mapping. Everything below is sequences and
scalars. Real packs carry **unicode** in preset and tab names, **EVE markup**
(`<fontsize=12><color=0xFF66CCFF>…`), single-quoted scalars containing `''`
escapes, and **multi-line quoted scalars** (a tab label with embedded newlines).
Files observed carry a UTF-8 BOM.

Verified against four published pack files (Z-S pack, 2019 vintage): a "Core"
pack, a full "Appearance" pack, a tab-layout pack and a preset pack. All four
carry the same 13 top-level sections.

### 2.2 Sections, and where each one lives in the file

Every section is optional. All targets are keys of the `core_user` root →
`overview` container — the same container all of slices 1–3 edit.

| Pack section | `overview` key | Shape | Modelled today |
|---|---|---|---|
| `presets` | `overviewProfilePresets` | name → `{alwaysShownStates, filteredStates, groups}` | yes (2a/2b/3) |
| `tabSetup` | `tabsettings_new` | index → `{bracket, name, overview}` | yes (slice 1) |
| `columnOrder` | `overviewColumnOrder` | list of column tokens | yes (M3c) |
| `overviewColumns` | `overviewColumns` | visible subset | yes (M3c) |
| `backgroundStates` | `backgroundStates2` | list of state ids | yes (slice 3) |
| `backgroundOrder` | `backgroundOrder2` | list of state ids | yes (slice 3) |
| `flagStates` | `flagStates2` | list of state ids | yes (slice 3) |
| `flagOrder` | `flagOrder2` | list of state ids | yes (slice 3) |
| `stateColorsNameList` | `stateColors` | `background_<id>` → colour **name** | yes (slice 3, as RGBA) |
| `stateBlinks` | `stateBlinks` | `background_<id>`/`flag_<id>` → bool | **no** — pass through |
| `shipLabels` | `shipLabels` | label → `{pre, post, state, type}` | **no** — pass through |
| `shipLabelOrder` | `shipLabelOrder` | list of label names | **no** — pass through |
| `userSettings` | the six `OVERVIEW_BOOLS` | name → bool | partially (slice 3) |

Four facts that shape the design:

- **The export drops the `2` suffix.** Packs write `backgroundStates` where the
  file holds `backgroundStates2` (same for `backgroundOrder`, `flagStates`,
  `flagOrder`). The reader accepts **either** spelling; the writer emits the
  unsuffixed one that real packs use.
- **Bracket presets are ordinary presets.** A tab's `bracket` field names an
  entry of the same `presets` list; the corpus has exactly one preset store
  (`overviewProfilePresets`, plus the `_notSaved` working copy). The 2a spec's
  "brackets have their own store" note is superseded.
- **`stateColorsNameList` is names, not RGBA.** Entries look like
  `[background_16, darkBlue]`. The file holds RGBA. §2.3.
- **Two sections we do not model at all** — `shipLabels`/`shipLabelOrder` (the
  in-space label markup) and `stateBlinks`. They are written through verbatim on
  import and read back verbatim on export. No UI, no projection, no
  interpretation: without them an imported pack would not look right in space,
  which is most of why people install packs.

### 2.3 The colour palette, and where it comes from

Import must turn `darkBlue` into RGBA; export must turn RGBA back into a name.
The mapping is derivable from the corpus with no in-game capture, because EVE
stores **the last imported pack verbatim** in the same file:

- `overview` → `presetHistoryKeys` is the imported-pack MRU; its `overviewName`
  is a pack name (a corpus file carries a published pack's name).
- `overview` → `restoreData` → `data` holds that pack's payload **in the pack's
  own vocabulary** — `backgroundOrder`, `backgroundStates`, `columnOrder`,
  `stateColorsNameList`, and the rest.

So one corpus file gives both the names EVE was handed and the RGBA it wrote.
Confirmed pairs from a single file:

| Name | RGBA |
|---|---|
| `darkBlue` | `(0.0, 0.15, 0.6, 1.0)` |
| `blue` | `(0.2, 0.5, 1.0, 1.0)` |
| `red` | `(0.75, 0.0, 0.0, 1.0)` |

The full table is harvested the same way slice 3 harvested `defaultColors`: a
throwaway research bin (the `overview_dump.rs` pattern — not committed) walks
every corpus user file, joins `restoreData.data.stateColorsNameList` against
`stateColors`, and prints the distinct name→RGBA pairs. The result is committed
as a const table in `overview_pack.rs`, with a comment recording the derivation.
`restoreData` itself is **read-only research input** — the editor never writes it
(§7).

Colours outside the palette are the one place the two formats do not line up:

- **Import**, unknown name → skip that entry, count it in the report. The state
  keeps whatever colour it had.
- **Export**, RGBA with no exact palette name → omit that entry, count it in the
  report ("2 custom colours had no pack name"). Emitting a near-miss name would
  silently change the user's colours; omitting falls back to EVE's default,
  which is at least honest.

### 2.4 `userSettings`

Packs carry a small `userSettings` list of booleans. Observed keys include
`overviewBroadcastsToTop` (which matches `OVERVIEW_BOOLS` exactly) and
`applyOnlyToShips` (which has **no** identically-named key in the file — the
current file holds `applyToStructures` and `applyToOtherObjects`, a later split
of the same idea). The mapping table therefore covers the keys we can confirm,
and **unrecognised `userSettings` entries are ignored and counted**, never minted
as new keys. `set_overview_bool` already rejects keys outside the allow-list and
that guard stays. Resolving `applyOnlyToShips` is a live-smoke item (§8).

## 3. Backend

### 3.1 New module: `crates/settings-model/src/overview_pack.rs`

One purpose: convert between a pack and the `overview` container. It owns all
pack-format knowledge; nothing else in the crate learns the YAML vocabulary.

```rust
pub struct Pack { /* one Option<…> per section of §2.2 */ }

pub fn parse_pack(text: &str) -> Result<Pack, PackError>;
pub fn emit_pack(pack: &Pack) -> String;
pub fn read_pack(user: &Value) -> Pack;                     // for export
pub fn apply_pack(user: &mut Value, pack: &Pack) -> Result<PackReport, PackError>;
pub fn summarize(pack: &Pack) -> PackSummary;               // for the confirm dialog
```

- `Pack` keeps the two unmodelled sections (`shipLabels`, `shipLabelOrder`,
  `stateBlinks`) as already-built `Value`s, so pass-through costs no modelling.
- `parse_pack` strips a leading BOM, then walks the parsed YAML into `Pack`.
  Sections it does not recognise are dropped and counted. A document with **no**
  recognised section is `PackError::NotAPack` — a wrong file chosen in the
  dialog must not look like a successful no-op.
- `emit_pack` is a hand-written writer (~60 lines): top-level keys sorted, pair
  lists, single-quote-and-escape any scalar that needs it. EVE parses valid
  YAML; reproducing its exact dump style buys nothing and costs a generic
  emitter. It writes **no BOM** (real packs carry one, and `parse_pack` accepts
  either).
- `apply_pack` is inline-first (`inline_all`) and **builds every replacement
  value before mutating**, so a pack that fails half way through conversion
  leaves the document untouched. It then replaces only the keys the pack
  defines. Timestamps on minted `(ts, value)` wrappers follow the module
  convention already used by `overview_states.rs` (zero `Long`) — EVE accepts a
  freshly minted zero-timestamp container, established in 2b.
- `PackReport` carries the applied counts plus a `warnings: Vec<String>` for the
  skipped colours, ignored sections and ignored `userSettings` keys.

### 3.2 Tab mapping after a tab replacement

Replacing `tabsettings_new` invalidates `tabsByWindowInstanceID`: the pack's tab
indices are not the account's, so a stale mapping leaves both dangling
references and orphan tabs (the "Other" bucket slice 1 exposed).

Rule: **assign every pack tab to the primary window (position 0), and drop from
any secondary window's list every index the pack does not define.** No orphans,
no dangling references, and a multi-window account keeps its windows. This is a
smoke-verify item (§8); the alternative — reconciling by position — has nothing
to reconcile against, since a pack carries no window model.

`apply_pack` only rewrites the mapping when the pack defines `tabSetup`, and only
when the mapping exists: it never fabricates `tabsByWindowInstanceID` on an
account that lacks it (the no-fabricate rule from the tab-fix branch — an
empty/partial mapping can hide the whole overview).

### 3.3 `ops.rs`

Three commands, all routed through the existing `edit_user_overview` wrapper
(lock → inline → mutate → `reshare` → re-project → mark the user slot dirty):

- `pack_preview(path) -> PackSummary` — reads and parses only. No lock, no
  mutation.
- `pack_import(path) -> PackReport` — parse, then `apply_pack`.
- `pack_export(path) -> PackReport` — `read_pack` + `emit_pack`, writes the file.

Import **does not save**. It marks the slot dirty like every other editor, so
the user presses Save and gets the normal backup. Export writes its own file
directly; it touches no settings file, and it exports the **in-memory**
document, so unsaved edits are included.

Preview and import each parse the file independently. Parsing a 150 KB pack
twice is free and keeps both commands stateless.

### 3.4 The YAML dependency

`parse_pack` uses **`yaml-rust2`** (pure Rust, maintained, no serde derive
needed; its `Yaml` enum maps directly onto the pair-list shape). It is the first
YAML dependency in the tree, and it lands in `settings-model`, which already
takes `serde`/`serde_json`. `blue-marshal` stays dependency-free.

Rejected: `serde_yaml` (unmaintained since 2024) and its forks; hand-rolling the
parser (~400 lines plus the edge cases of quoting, multi-line scalars and
unicode, all of them on the *untrusted* side of the boundary — arbitrary files
users download); parsing in the frontend with a JS library (splits format
knowledge away from the corpus-driven Rust tests where every other format
concern lives).

## 4. Frontend

Two buttons in the `OverviewView.svelte` header, beside the
Columns/Filters/Appearance sub-tabs — the pack is account-wide, so it does not
belong inside one sub-tab. No new frontend dependency:
`@tauri-apps/plugin-dialog` already provides `open` (used by the sidebar) and
`save`.

**Import pack…**

1. `open()` filtered to `.yaml`/`.yml`, defaulting to `Documents/EVE/Overview`
   when that folder exists (EVE's own export destination), otherwise no default.
2. `pack_preview(path)` → `confirm()` naming what the pack contains and what it
   replaces: "12 presets, 5 tabs, columns, colours, ship labels — this replaces
   your current overview".
3. `pack_import(path)` → the view re-projects, the slot is dirty, Save is armed.
   Warnings from the report show in a `message()`.

**Export pack…** → `save()` (default name `overview.yaml`) → `pack_export(path)`
→ a `message()` with the counts and any omitted-colour warning.

Both buttons are disabled with the same guard the rest of the Overview view
uses when no account file is open or the file is read-only.

## 5. Testing

`overview_pack.rs` unit tests, against a fixture **written by us** in the real
shape — unicode, EVE colour markup, a multi-line single-quoted tab label, a
`''`-escaped name — rather than a copied community pack, which keeps a
third-party licence out of the repo while covering the same parser edges:

- `parse_pack` on the fixture: every section lands, with the pair-list shape
  decoded.
- Round trip: `parse → apply_pack → read_pack → emit_pack → parse_pack` equals
  the input for every modelled section, and preserves `shipLabels` /
  `stateBlinks` unchanged (structural equality) through the pass-through path.
- **Partial pack**: a presets-only pack leaves tabs, colours and columns
  untouched.
- Both suffix spellings (`backgroundStates` / `backgroundStates2`) parse to the
  same field.
- Palette: name→RGBA and RGBA→name; an unknown name is skipped and reported; a
  non-palette RGBA is omitted from the export and reported.
- Tab mapping (§3.2): a pack with fewer tabs than the account drops the dangling
  indices; a windowless account gains no fabricated mapping.
- `apply_pack` atomicity: a pack whose conversion fails leaves the document
  equal to its input.
- `NotAPack` on a valid YAML document with no recognised section.

Corpus test: `read_pack` then `emit_pack` over every corpus `core_user` file must
re-parse to an equal `Pack`. That is the real-data check on quoting, unicode and
markup — the corpus contains packs people actually shipped.

Closing with a live in-game smoke, as with every previous slice.

## 6. Approaches considered and rejected

- **Our own pack format.** Rejected: same-machine copying is already batch
  apply, and a tool-native file cannot import the packs people actually
  download, which is the whole point.
- **Per-preset merge on import.** Rejected as unrequested UI; replace-per-section
  already lets a user take someone's presets while keeping their own colours,
  which was the real motivation for granularity.
- **Multi-account fan-out inside the import flow.** Rejected: it would rebuild
  target selection, account dedup and the collateral warning that M5's batch
  apply already does. Import once, then batch apply.
- **A live in-game capture to pin the schema.** Turned out unnecessary —
  published packs *are* EVE exports, and `restoreData` gives the internal
  mapping (§2.3).

## 7. Non-goals

- **Multi-account import.** §1.
- **UI for ship labels, blinks, or bracket presets.** Pass-through only; no
  projection, no editor.
- **Writing `restoreData` / `presetHistoryKeys`.** EVE's own bookkeeping. We read
  `restoreData` in research to derive the palette and never write either — the
  same stance 2a took.
- **Per-tab column overrides survive an import.** They do not: pack columns are
  account-global, so replacing `overviewColumns`/`overviewColumnOrder` and
  `tabsettings_new` discards per-tab `tabColumns`/`tabColumnOrder`. This is the
  correct behaviour for a pack import, and it is a real loss for a user who had
  customised tabs — the confirm dialog says so.
- **Column widths.** `core_char`, not in the pack format.
- **Byte-identity with EVE's own exporter.** Valid YAML EVE accepts is the bar.

## 8. To confirm at live smoke

1. Import a published community pack, launch EVE, confirm tabs, presets,
   colours and in-space ship labels all match the pack.
2. Export from the editor and confirm EVE's own Import Overview Settings accepts
   the file.
3. Which internal boolean `applyOnlyToShips` corresponds to (§2.4) — and whether
   a current-client export still uses that name.
4. Whether a current-client export uses the suffixed or unsuffixed state-list
   names (the reader accepts both either way).
5. The tab mapping (§3.2) on an account with two overview windows.
