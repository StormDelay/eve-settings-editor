# M4 — Batch apply (design)

_2026-07-17_

Copy settings from one **source** file to many **target** files of the same
type, either wholesale (full copy) or by selected category. This is milestone 4
of the design spec (`docs/superpowers/specs/2026-07-12-eve-settings-editor-design.md`
§5, §42–43, §94): the projection **categories** are the unit of batch apply, and
they are extractable from one document and applicable to another.

## 1. Scope

File type gates everything. A source file offers only the operations valid for
its type, and targets are always the same type as the source.

| Source type    | Operations offered            | Targets           |
| -------------- | ----------------------------- | ----------------- |
| `core_char_*`  | Full copy · **Window layout** | other char files  |
| `core_user_*`  | Full copy · **Autofill**      | other user files  |

**Overview columns are out of M4.** They are scattered across many keys, span
both file types (config in the user file, widths in the char file), and are the
repo's most bug-prone code (mis-modeled three times). A batch overview copy
between accounts with different tab structures is semantically fraught. Overview
batch copy is its own future milestone.

Category → file-type mapping (single clean subtrees, already located by the
existing projections):

- **Window layout** → char file, `root → "windows"` (`windows.rs`).
- **Autofill** → user file, `root → "ui" → "editHistory"` (`autofill.rs`).

## 2. Operations

- **Full copy** — replace the entire target file with the source's content.
  Wholesale; implies every category and everything else. Mutually exclusive with
  category selection.
- **Category copy** — replace one or more selected category subtrees in each
  target, leaving the rest of the target untouched. Multi-select: today each
  file type exposes a single category besides full copy, but the model and UI
  support selecting several so new categories (overview, …) drop in without
  rework.

## 3. Mechanism — `crates/settings-model/src/batch.rs` (the new unit)

Orchestration over the existing save chain (`save.rs`) and tree helpers
(`treewalk.rs`, `document.rs`). No new write, backup, or verification code.

### Full copy

Read the source bytes once. Per target:

1. `save::backup_current(target)` — hard prerequisite; no backup ⇒ no write.
2. `save::atomic_write(target, source_bytes)`.

Byte-perfect (the source is already a valid file EVE wrote or we verified), no
decode, backup guaranteed. `backup_current` and `atomic_write` are already
`pub(crate)`.

### Category copy

Decode the source once. For each selected category, deep-clone its subtree and
`treewalk::inline_all` it so no `Ref`/`Shared` dangles when it lands in a
different document. Per target:

1. `Document::load(target)` — a throwaway document; its fresh load baseline
   means the conflict check never trips.
2. Splice each selected, inlined category subtree at its root path in
   `doc.value` — replace the entry's value, or insert the entry if the target
   lacks it.
3. `save::save(&mut doc, force_conflict = true)` — one save per target,
   regardless of how many categories were spliced.

This inherits, per target, the full invariant chain: encode → verify (decode own
output, bit-compare) → backup → atomic write, plus **ReadOnly refusal** (a
non-canonical target is refused by `save()` and surfaces as a skipped target).

Splice is coarse by design: replacing the whole `windows` subtree _is_ "copy the
layout"; replacing the whole `editHistory` subtree _is_ "copy the autofill
lists." Categories are the unit; there is no per-window or per-list granularity
in batch apply.

## 4. Command layer (app crate, `src-tauri`)

Two commands, mirroring the thin-command pattern of `names.rs` / `accounts.rs`.

- `batch_targets(source_path, allow_other_folders) -> Vec<Candidate>`
  Discovered files of the source's type. Restricted to the source's own profile
  folder unless `allow_other_folders`. Each `Candidate` carries its path,
  resolved character name / account alias, and a folder label. The source itself
  is excluded from its own target list.

- `batch_apply(source_path, op, target_paths) -> Vec<TargetResult>`
  `op` is `FullCopy` **or** `Categories(Vec<Category>)` where
  `Category ∈ { Layout, Autofill }`. The source is read (and for categories,
  decoded + extracted + inlined) **once**, then reused across every target. One
  target's failure never halts the others. Each result is
  `{ path, ok: bool, backup_path | error }`.

## 5. Frontend — `BatchView.svelte`, a new `mainView`

A standalone main-pane view (mirrors the Accounts view), reached from a sidebar
button. `+page.svelte` gains `mainView: … | "batch"`. Flow, top to bottom:

1. **Source** — list/dropdown of discovered files, defaulting to the
   currently-open file (if any). Names via the existing `names.svelte.ts` /
   `accounts.svelte.ts` stores.
2. **Operation** — checkbox list of the source type's categories, **plus** a
   Full-copy option that is mutually exclusive with the category checkboxes
   (selecting Full copy disables them). Pick one-or-more categories, or Full
   copy.
3. **Targets** — checkbox multi-select of candidates, source's folder only by
   default, with a **"Show other folders"** toggle (matching file type still
   enforced). Names shown per row.
4. **Preview + confirm** — a summary ("Overwrite N files — each is backed up
   first") listing the exact targets, and an explicit **Apply** button.
5. **Report** — per-target ✓/✗ with the backup path or the error; ReadOnly or
   failed targets show as skipped.

## 6. Safety

- Every target is backed up before it is overwritten (inherent to both paths —
  `backup_current` precedes every write).
- ReadOnly (non-canonical) targets are refused by the save chain and reported as
  skipped, never silently written.
- Mandatory preview/confirm before any file is touched.
- Same-folder + same-type by default; crossing folders is an explicit opt-in
  toggle, and the type match is enforced regardless.
- The source can never appear in its own target list.

## 7. Known ceilings

- **Category copy inlines sharing.** `inline_all` drops all `Ref`/`Shared` in the
  copied subtree, so a category-copied target grows ~1.5× until EVE re-dedups it
  on next write (it self-heals). This is the existing re-share debt already
  logged in the small-tasks ledger, not new to M4. Full copy is unaffected (raw
  bytes).
- **Layout copy is char-scoped, but not all window state is.** The Layout
  category replaces the char file's `windows` subtree wholesale, and that part
  works — but *which* windows EVE recreates on the next login is partly driven
  by state living outside that subtree, so a copied target does not end up
  window-for-window identical to its source. Two sources of drift, both
  confirmed by live smoke (2026-07-17, char → char):
  - **Overview windows are account-scoped.** How many overview windows exist is
    defined in the `core_user_*` file's `overview` key (the window groups
    `project_overview` reads), not in the char file. Copying a source char whose
    account defines 2 overview windows onto a target char whose account defines
    3 left the target with `overview_2` recreated at default geometry on next
    login. The char file only stores *where* each overview window sits; the
    account decides *that it exists*.
  - **Chat/convo windows follow the character's runtime state.** EVE recreated
    the target's own `chatchannel_*` windows (a channel it is in, an open
    convo) after the copy removed them.
  The splice itself is correct and was verified against the live files: the
  target's `windows` went from 299 entries to exactly the source's 40. EVE then
  re-added the 3 windows its account/runtime state required and flushed the file
  on logout. So the ceiling is scope, not correctness — **matching two
  characters fully also requires their accounts' overview config to match**,
  which is cross-file and belongs to the M5 milestone (master design §9). Note
  the corollary: a layout copy discards the target's accumulated per-character
  window state (old convos, fitting windows, mail) — that is the intended
  wholesale-replace semantic, and EVE self-heals what it still needs.
- **Full copy relies on filename identity.** EVE reads a file's character /
  account identity from its **filename**, not its content (confirmed by the
  developer; it is the premise every settings-manager copy-and-rename workflow
  relies on). A full copy therefore leaves the target's identity intact — only
  its settings change.

## 8. Testing

- `batch.rs` unit tests: category extract → splice replaces the target's subtree;
  `inline_all` leaves zero `Ref`/`Shared`; full-copy byte-identity; the
  missing-category **insert** path; multi-category splice + single save.
- `tests/batch_realshape.rs` guard, mirroring `overview_realshape.rs` /
  `autofill_realshape.rs`: synthetic-but-structural fixtures through
  encode → decode → apply → re-decode.
- Command-layer test: target scoping (folder + type filter, source excluded) and
  the partial-failure report (one bad target does not stop the rest).
- **Live smoke = the real merge gate.** Against the real client: copy one char's
  window layout to another char; copy one user's autofill lists to another user;
  full-copy a whole char file. Verify EVE accepts each result, the settings
  appear in-game, and every written file is valid (decodes, no duplicate keys).
  Record the result in `docs/format-notes.md` under `## Status`.
