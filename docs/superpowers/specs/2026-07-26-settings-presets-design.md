# Settings presets (design)

Date: 2026-07-26
Status: designed, ready for writing-plans.

Roadmap: the **settings presets** milestone — third on the roadmap after
overview depth (slices 1–4, shipped through v0.14.0) and layout depth (slice 3
shipped v0.15.0, slice 1a/1b shipped v0.16.0/v0.17.0). Designed as **one
slice**: a preset that can be created but not applied is useless, and one that
cannot be shared misses a third of the stated need.

Builds on: `batch.rs` (`Category`, `extract_categories`, `apply_to_tree`), the
`ops.rs` batch planner (`aspect_writes` / `plan_setup` / `setup_preview` /
`setup_apply`), the app-data-dir JSON store `accounts.rs` already writes, the
`Document`/`save` chain, and `@tauri-apps/plugin-dialog`.

## 1. Goal

Every way to move settings around today is anchored to a **live character**.
Batch apply copies char → chars; the settings only exist for as long as some
character still holds them, unchanged. Overview packs (slice 4) are the one
detached artefact, and they are overview-only, account-scoped, and in EVE's
format rather than ours.

A preset is settings with no character behind them. Four uses, all wanted:

1. **A snapshot that outlives its source** — save "my PvP layout" now, apply it
   to an alt in six months after the original character's settings have drifted.
2. **Swapping setups on one character** — keep "Mining" and "PvP" and flip.
3. **Sharing with other players** — hand someone a file.
4. **Seeding a fresh install** — pour a known-good baseline into brand-new
   character files.

And one requirement that shapes everything else: a preset must be **editable in
its own right**, as if it were a character. Not merely captured and replayed —
opened, edited in the Layout/Overview/Autofill/Keybinds editors, and saved.

## 2. The model

> **A preset is a character that isn't a character**: a folder holding a
> char-side and an account-side settings document.

That single sentence is the whole design, and it is what makes the requirement
above nearly free. Nothing in the app learns a new format, because there is no
new format — a preset's two files are settings documents, the same thing
`Document::load` has read since M1.

Three properties of the existing code make this work, all verified:

- **`ops::open_file(slot, path)` is path-agnostic.** It calls `Document::load`
  and drops the result in a slot. Nothing checks the file name, the directory,
  or whether a character owns it. Any decodable marshal file loads into either
  slot today.
- **The editors know slots, not characters.** The Overview/Autofill/Keybinds
  tabs gate on `openCharId !== null || slots.user opened` (`+page.svelte:84-86`),
  so a document pair with no character id still gets the full editor surface.
- **`save` and the backup chain are path-driven.** A preset edit gets the same
  encode → verify → conflict-check → backup → atomic-write chain as an EVE file,
  and its backups land in the preset's own folder.

The second consequence is the one worth stating loudest:

> **No aspect list is stored anywhere.** What a preset holds is *derived* by
> looking at it — `extract_categories` over its two documents. Adding a future
> aspect means adding a `Category` and nothing else: no preset format change, no
> migration, no stored field to keep in sync.

`Category::Keybinds` is the proof case sitting right there. The keybinds editor
shipped, but keybinds are not a batch category. Adding
`Keybinds => &[b"cmd", b"customCmds"]` to `Category::key_path` (the path
`keybinds.rs:51-52` reads) plus an `Aspect` variant would make keybinds a batch
aspect **and** a preset aspect in the same two lines. It is deliberately
out of this slice's scope (it changes batch apply's user-facing behaviour, which
is a separate decision) but it is what "easy expansions" has to mean.

## 3. Storage

```
<app data dir>/
  accounts.json                 (exists)
  presets/
    PvP layout/
      char.dat                  a settings document, pruned to the ticked aspects
      user.dat
      preset.json               ONLY for full presets: {"full": true}
      eve-settings-editor-backups/    (created by the normal save chain)
    Mining/
      char.dat
      user.dat
```

**`app_data_dir`**, alongside `accounts.json` — not `app_config_dir`, where
`preferences.json` lives. Presets are user data, not configuration.

Everything derivable is derived and nothing is stored twice:

| Fact | Where it comes from |
|---|---|
| Preset name | the folder name |
| Modified date | the newer of the two files' mtimes |
| Which aspects it holds | `extract_categories` over the two documents (§3.1 is the one exception) |
| Source screen resolution | `project_window_layout(char.dat).reference_w/h` |

Rename is a directory rename; delete is a directory delete. There is no index,
no catalogue and no second source of truth, so nothing can drift and no repair
logic is needed when a user adds or removes a folder by hand.

Both files are **always written**, even when one side would be empty (an
Autofill-only preset has no meaningful char side). An empty side is a document
whose root is an empty dict; it encodes, loads `Editable`, and projects empty.
Writing both unconditionally means the open path never branches.

### 3.1 The one bit that is not derivable

`preset.json` carries a single field, `full`, set at creation and never edited.
It distinguishes a preset **pruned** to some aspects from one that is a
**complete copy** of both files (the `Everything` aspect, §4). The distinction
cannot be derived reliably and it is dangerous to guess: applying `Everything`
from a pruned preset would overwrite a character's whole file with a document
holding three keys.

A missing, unreadable or unparseable `preset.json` is treated as `full: false`.
That is the safe direction — the preset is offered as its pruned aspects only,
so the failure mode is "fewer aspects offered", never "a destructive full copy
built on a partial document".

This is deliberately *not* a manifest. It holds one immutable boolean. Name,
date and aspects stay derived, per §2.

### 3.2 Names are a trust boundary

A typed preset name becomes a filesystem path, so validation is not optional and
not a place to be lazy. `sanitize_name` rejects, rather than silently rewrites:

- empty, or whitespace-only
- any of `/ \ : * ? " < > |` or an ASCII control character
- a leading or trailing dot or space (Windows strips these, so `"foo."` and
  `"foo"` would collide)
- the Windows reserved device names, case-insensitively, with or without an
  extension: `CON PRN AUX NUL COM1-9 LPT1-9`
- `.` and `..`
- longer than 100 chars (leaves room for the backup subdirectory's own names
  under the Windows path limit)
- `eve-settings-editor-backups`, which the save chain claims inside every
  preset folder

Rejection returns a message naming the rule. Independently, and as the actual
security guard rather than a nicety, every resolved preset path is checked to
still be a direct child of the presets directory before any read, write or
delete — belt and braces against a sanitiser bug or a symlink.

Creating a preset whose name is already taken is refused with "a preset called X
already exists"; the UI offers to overwrite, which deletes and recreates the
folder (its backups subdirectory included — a preset is not an EVE file and its
history is not precious).

## 4. Creating a preset

New module `app/src-tauri/src/presets.rs`, alongside `prefs.rs` and
`accounts.rs`. The settings-model crate needs no changes for this section:
`extract_categories` and `apply_to_tree` are already exported.

`preset_create(name, aspects, overwrite) -> Result<(), ErrDto>`:

1. Sanitise the name (§3.2). Refuse a collision unless `overwrite`.
2. Read the **in-memory** documents from the char and user slots — not the files
   on disk — so unsaved edits are captured. This matches `pack_export`, which
   made the same choice for the same reason. An aspect whose side is not open is
   **refused**, naming the missing side ("Overview needs the account file open"),
   rather than quietly writing an empty document: a preset that claims an aspect
   it does not hold is worse than no preset.
3. Per side, with `cats` from `aspect_writes(aspects)`:
   - `Everything` → `encode` the whole document unchanged, and write
     `preset.json`.
   - otherwise → `extract_categories(&doc.value, cats)`, then
     `apply_to_tree(&mut skeleton, &extracted)` where `skeleton` is
     `Dict{ ui: Dict{} }`, then `encode`.
4. Create the folder and write `char.dat` and `user.dat`.

Step 3's second branch is the whole pruning implementation, and it is two
existing functions. `apply_to_tree`'s insert-when-absent branch does exactly
this job — it creates the leaf key under an existing parent — and it already
ends with `blue_marshal::reshare`, so the written file is compact rather than
the ~1.5× fully-inlined blob. The `ui` dict is in the skeleton because
`descend_mut` skips a category whose parent is missing, which is how
`ui -> editHistory` and `ui -> SortHeadersSizes` find a home.

A `ui` dict left empty after pruning (a Layout-only preset) is dropped before
encoding, so the file contains exactly what the preset is and nothing else.

The written files are always `Fidelity::Editable` by construction: we produce
them with `encode`, so `encode(decode(bytes)) == bytes` holds trivially. No
preset can open read-only unless a user hand-edits one.

`preset_delete(name)` and `preset_rename(old, new)` are a directory remove and a
directory rename, both behind the §3.2 containment check. Rename refuses a
collision.

## 5. Opening and editing

`preset_list() -> Vec<PresetInfo>`, where `PresetInfo` is
`{ name, dir, modified_unix, aspects: Vec<Aspect>, full: bool, error: Option<String> }`. Implementation
is `read_dir` over `presets/`, and per entry: read `preset.json` if present,
decode both documents, and derive `aspects` by running `extract_categories` with
every category over each side.

An aspect is present when its **defining** category is:

| Aspect | Present when |
|---|---|
| Layout | char has `windows` |
| Overview | user has `overview` (char's `SortHeadersSizes` is a bonus, not a condition) |
| Autofill | user has `ui -> editHistory` |
| Everything | `preset.json` says `full` |

A preset whose files fail to decode is listed with `error` set and no aspects,
rather than omitted — a preset that silently vanishes from the list is worse
than one that says it is broken. Such a row cannot be opened or applied, but can
still be renamed, exported and deleted.

`ponytail:` listing decodes every preset's two files on every call. Presets are
small (pruned) or settings-file-sized (full), there will be a handful, and the
list is only rebuilt on user action. If a large library ever drags, cache by
`(path, mtime)`.

**Opening**: the sidebar's preset row calls `api.open("char", <dir>/char.dat)`
and `api.open("user", <dir>/user.dat)` — both slots filled in one action. This
must *not* go through `+page.svelte`'s `openFile`, whose char branch calls
`reconcileUserSlot` to find the paired account. The reconcile `$effect`
(`+page.svelte:160`) only fires when `slots.user === null`, so filling both
slots keeps the pairing machinery quiet without any change to it.

`+page.svelte` gains `openPreset: string | null`, set by the preset-open path
and cleared by `openFile`. It drives three cosmetic corrections and nothing
else: the file header and OS window title show the preset's name instead of
`char.dat`, and the unsaved badges read `preset: unsaved` instead of
`character: unsaved` / `account: unsaved`.

Everything else — Tree, Layout (including HUD and stacks), Overview (all three
sub-tabs), Autofill, Keybinds, Save, the backups panel — is untouched.

### 5.1 The one latent assumption presets expose

`OverviewColumnsTab` disables per-column width editing on `charId === null`
(lines 17 and 52), using the character *id* as a proxy for "a char document is
open". A preset has no id but can hold `OverviewWidths`, so width editing would
be wrongly disabled.

The fix is to pass `charOpen: boolean` and gate on that. I checked every other
`charId` site in the app — `AccountsView`, `overview.ts:64`, `OverviewView:226`
(the "pair this character" prompt), `+page.svelte:84-86/148/443-449` — and the
rest are genuinely about *pairing*, where an id is the right question. This is
the only conflation.

## 6. Applying a preset

Applying goes through the existing batch pipeline. The Batch view's source
becomes **Character | Preset**; the target list, aspect ticks, exclusions,
collateral-character warning, resolution-mismatch warning and per-target backups
are all untouched.

Three changes in `ops.rs`, all small:

**`plan_setup`'s `source_char: u64` becomes `Option<u64>`.** `Some(id)` is
today's behaviour unchanged. `None` means a preset source, which skips exactly
three things, each for the same reason (there is no source character):
self-exclusion of the source from the target list, the source-account pairing
checks, and the "this account already carries the source's settings" skip in the
account-write loop.

**`scoped_files` takes an explicit anchor directory** instead of deriving it
from the source path. A character source passes its own profile dir, which is
what it computes today; a preset source passes the primary profile dir
(`primaryProfileDir`, already used by the sidebar and the batch source picker).
The existing "show other folders" toggle keeps working for both.

**`setup_preview`/`setup_apply` take a source enum** — a char path, or a preset
directory. For a preset, `char_extracted`/`account_extracted` come from
`extract_categories` over its two files, and the `Everything` full-copy path
uses those files' bytes directly. Both are just different ways of filling
variables the rest of the function already has.

Two things fall out for free and are worth recording as evidence the model is
right:

- **The resolution-mismatch warning needs no metadata.** `gather_resolutions`
  reads `reference_w/h` straight out of a char file, and a preset's `char.dat`
  *is* a char file. A preset with no Layout has no resolution, and the warning
  is correctly silent.
- **`source_side_empty`'s no-op-write suppression works unchanged**, because it
  operates on a path and a category list.

The aspects offered on apply are intersected with what the preset holds, so
Autofill cannot be ticked on a preset that has none. `Everything` is offered
only when `preset.json` says `full` (§3.1).

## 7. Sharing

A single `.evepreset` file, produced and consumed only at the export/import
boundary. The working form is always the folder — there is no unpack/repack
lifecycle, no temp directory, and nothing to lose if the app dies mid-edit.

The container is a marshal blob, because the codec is already here and already
round-trip tested against the whole corpus:

```
Dict{ "preset": Bytes(name), "char": Bytes(<char.dat bytes>),
      "user": Bytes(<user.dat bytes>), "full": Bool }
```

- **Export** — `save()` dialog defaulting to `<name>.evepreset`, then read both
  files, wrap, `encode`, write. Exporting a **full** preset warns first: it
  carries a complete copy of both settings files, including autofill history
  (station names, searches, typed text) and everything else the editor does not
  model. That warning is the reason pruning exists.
- **Import** — `open()` dialog filtered to `.evepreset`, `decode`, check the
  four keys are present and of the right kind, sanitise the embedded name
  (offering a suffixed alternative when taken), write the folder. A file missing
  the `preset` key is rejected as "not a preset file" — a wrong file chosen in
  the dialog must not look like a successful no-op, the same stance
  `PackError::NotAPack` takes.
- The two embedded documents are **decoded before the folder is written**, so an
  import that would produce an unopenable preset fails cleanly with nothing on
  disk.

No new dependency, no new serializer, and the format grows a key whenever a
preset does.

## 8. Frontend surfaces

- **Sidebar — a `Presets` group**, beside Characters and Accounts, using the
  same collapsible `<details>` pattern. Each row: name, the aspects it holds as
  small labels, and a right-click context menu (`ContextMenu.svelte`, already
  used by the window list) with Rename, Export…, Delete. The group header
  carries **New from open character…** and **Import…**.
- **New from open character…** opens an inline form in the sidebar — a name
  field plus the aspect checkboxes — matching the inline-name-input pattern the
  overview-window slice introduced when it replaced `window.prompt`. Disabled
  when no character is open. Ticking `Everything` disables the other three,
  since it subsumes them.
- **Batch view** — the source picker gains a Character/Preset toggle and a
  preset dropdown showing what each holds.
- Native form controls in the new form get explicit dark backgrounds and colors:
  the WebView2 light-control trap this project has hit before.

## 9. Testing

`presets.rs` unit tests, filesystem-backed in a temp dir the way `prefs.rs` and
`batch.rs` already do it:

- **Name sanitisation** — each rejection rule, plus the containment check
  refusing `../escape` and an absolute path even if sanitisation were bypassed.
- **Pruning** — a Layout-only preset's `char.dat` decodes and holds `windows`
  and nothing else; its `user.dat` is empty; the source's autofill and overview
  are *absent* (this is the privacy guarantee, so it gets an explicit assertion,
  not an implication).
- **Everything** — both files decode equal to the source documents, and
  `preset.json` says `full`.
- **Derived aspects** — a preset built from each aspect combination lists back
  exactly the aspects it was built with. A missing/corrupt `preset.json` yields
  `full: false`.
- **Round trip through the container** — export then import yields two files
  byte-identical to the originals and the same derived aspects.
- **Rejects a non-preset file** on import.

`ops.rs` planner tests: `plan_setup` with `source: None` includes every valid
target, applies no self-exclusion, and skips no account.

The genuinely new risk is **a sparse document inside the editors**. A `user.dat`
with no `overview` key at all is legal and has never existed before — every file
the app has ever opened was written by EVE. So:

- `project_overview` on a user document with no `overview` key projects empty
  rather than erroring, and the same for `project_edit_history`,
  `project_keybinds`, `window_layout` and `project_hud` on their missing
  sections.
- The container-minting paths established in slice 2b (a freshly minted
  zero-timestamp `overviewProfilePresets`) build from a document that has no
  `overview` container at all — i.e. editing an empty preset into a useful one
  works, which is the "swap setups" story's second half.

Several of these already hold (`keybinds.rs` has a
`no_file_and_no_section_are_unavailable` test); the plan should verify before
adding, and only fill the genuine gaps.

Closing with a live in-game smoke, as with every previous slice.

## 10. Approaches considered and rejected

- **A preset as a synthetic profile in the discovery tree** — bend the folder
  layout to `<install>_<server>/settings_<name>/core_char_1.dat` so `discover()`
  finds it and the existing sidebar renders it with no new listing code.
  Rejected: synthetic character ids would be sent to ESI for name resolution and
  presets would appear in the Accounts view as fake characters awaiting pairing.
  It reuses more code by making the data lie about what it is.
- **A single container file as the working form**, unpacked to a temp directory
  on open and repacked on save. Best sharing story, but it adds a lifecycle no
  other editor has, a temp directory to manage, a save hook, and an answer to
  "what if the app dies mid-edit". The container belongs at the boundary.
- **A `presets.json` catalogue** holding names, dates, aspects, tags and
  ordering. Rejected: a second source of truth that drifts from the folders, and
  repair logic for the user who deletes one by hand. Everything in it except one
  boolean is derivable (§3).
- **Capture everything, choose aspects only at apply time.** Rejected on
  sharing: every shared preset would carry the sharer's full settings regardless
  of what it claimed to be.
- **A preset-native format of our own** (JSON with base64 subtrees, or similar).
  Rejected: it would make presets the one thing in the app the editors cannot
  open, which is exactly the requirement this design exists to meet.

## 11. Non-goals

- **Presets as a batch *target*** — copying a character's overview *into* an
  existing preset. "New from open character…" over an existing name covers it,
  and so does opening the preset and editing it directly.
- **Tags, search, notes, ordering, favourites.** A folder listing.
- **Version history beyond the backup chain** every preset save already gets for
  free in its own folder.
- **`Category::Keybinds`.** Two lines and clearly wanted (§2), but it changes
  batch apply's behaviour too, which is the user's call to make separately.
- **Auto-snapshot on save**, or any implicit preset creation.
- **Cross-machine sync.** Export/import is the sync story.

## 12. To confirm at live smoke

1. Create a Layout-only preset from a real character, apply it to a *different*
   character, launch EVE, and confirm the windows land where the preset had
   them — and that the target's overview and autofill are untouched.
2. Open that preset, move a window, save, re-apply, and confirm the edit landed:
   the preset is genuinely editable, not just a capture.
3. Create an `Everything` preset from a character with a fully configured
   client, apply it to a character whose settings files EVE has only just
   created, and confirm the client comes up configured (the fresh-install case).
4. Open a preset that holds **only** Autofill and confirm the Overview and
   Layout editors show honest empty states rather than erroring — then add an
   overview preset to it from scratch and confirm EVE accepts the result (the
   slice-2b minting path from nothing).
5. Export a preset, import it back under a new name, and confirm the two behave
   identically.
6. Confirm the per-column width field is editable with a preset open (§5.1).
