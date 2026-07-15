# M3c — Overview columns editor (design)

Date: 2026-07-16
Status: approved, pre-plan
Builds on: M1 (codec, raw tree, save chain), M2 (layout canvas — the path-surfacing
projection pattern this reuses), M3a (ESI names), M3b (char↔user pairing / account
roster — this milestone consumes it). Design spec §6 "Overview editor".

Final of the three M3 sub-milestones: **M3a — ESI names** (done) → **M3b —
char/user association** (done) → **M3c — overview columns editor** (this doc).
Packaging and the autofill editor are their own later milestones.

## 1. Goal

Give the overview window's columns a purpose-built editor: **show/hide**,
**drag-to-reorder**, and **per-column width**, per overview tab — instead of
hand-editing the raw tree. This is the first **two-file** category: it reads and
writes both `core_user` (visibility + order, account-scoped) and `core_char`
(widths, per character), so it also introduces the **two-slot** app state the
M3b design anticipated (approach A).

Overview *filter presets* (tab contents) remain raw-tree-only per design spec
§6 and §10 — out of scope.

## 2. Format mappings (from `docs/format-notes.md`, experiments 3a–3b)

Overview column config spans both files:

- **Visibility + order — `core_user`, per overview tab.**
  `root → b"overview" → b"tabsettings_new" → (FILETIME, dict)` keyed by **tab
  index** (`Int`). Each tab dict holds `"name"` (Str label), `b"tabColumnOrder"`
  (list of column-name `Bytes`, the full ordering) and `b"tabColumns"` (list of
  column-name `Bytes`, the **visible** subset), alongside bracket/color/overview
  preset/showAll/showNone/showSpecials keys the editor leaves alone.
- **Account defaults — `core_user`.** `root → b"overview" → b"overviewColumns"`
  (visible) + `b"overviewColumnOrder"` (order). Applied to any tab that has no
  own `tabColumns`/`tabColumnOrder`.
- **Widths — `core_char`, per character, per tab.**
  `root → b"ui" → b"SortHeadersSizes" → (FILETIME, dict)` keyed by the tuple
  `(b"overviewScroll2", tabIndex)` → dict of column-name `Bytes` → width px
  (`Int`). Sibling `b"SortHeadersSettings2"` (per-tab sort state) is left alone.

**Inheritance (experiment 3b).** Per-tab column lists are sparse: a tab without
its own `tabColumnOrder`/`tabColumns` inherits the account defaults. The client
**materializes** a tab's own full lists on the first edit (observed: first drag
on an inheriting tab wrote the complete 14-column `tabColumnOrder`). The editor
mirrors this — see §5.

**Leaf wrappers.** Values live inside `(FILETIME, value)` tuples at the
`tabsettings_new` / `SortHeadersSizes` level; the existing M1 mutate/save layer
already preserves and updates these, so edits reuse it.

## 3. Two-slot app state

`AppState` moves from one open document to **two typed slots** — a **char slot**
and a **user slot** — plus the existing capture snapshot. Both can hold a
document at once.

- **Save chains are unchanged and independent.** Each slot saves through the
  exact M1 path (pre-save backup, on-disk conflict check, atomic write). There
  is **no** coordinated/transactional two-file write; the char file and user
  file are saved as two separate operations.
- Doc-scoped commands (`apply_mutation`, `save_document`, `list_file_backups`,
  `window_layout`, `restore_backup`) take a `slot: "char" | "user"` argument
  identifying which document to act on. `begin_capture` / `resolve_capture`
  exclude **both** open documents from their mtime diff (today they exclude one).
- The frontend tracks which slot is active for the Tree/Layout views (a small
  header switcher when both slots are filled); the Overview view spans both.

## 4. Loading flow (bidirectional, via the M3b roster)

A character belongs to exactly one account, so char → user is a unique
auto-load; an account has many characters and widths are per-character, so
user → char is a pick. Both anchors are supported:

- **Open a character** (`core_char`) → char slot. Look up its account in the
  roster → **auto-load the paired `core_user`** into the user slot. If the
  character has no account association, the Overview view shows a nudge:
  "Associate this character's account to edit overview columns → Accounts".
- **Open a user file** (`core_user`) → user slot. The Overview view shows a
  **character selector** listing the account's associated characters (roster);
  picking one loads its `core_char` into the char slot. If the account has no
  associated characters yet, the same nudge to Accounts.

Either path ends with both slots filled and an identical editor. Widths are
shown/edited for the character currently in the char slot; switching the
selected character reloads the char slot.

## 5. The editor

A new **Overview** view, offered next to Tree (and Layout, when applicable),
available once the loading flow above has produced the files it needs.

- **Tab selector** — the account's overview tabs by stored `name` (falling back
  to the tab index). Selecting a tab drives both the column list and the widths.
- **Column rows** for the selected tab, in `tabColumnOrder`:
  - **Visibility** — a checkbox toggling membership of `tabColumns` (user slot).
  - **Reorder** — drag to rearrange `tabColumnOrder` (user slot), matching the
    layout canvas's existing drag interaction.
  - **Width** — a number input writing the `(overviewScroll2, tabIndex)` →
    column entry (char slot). Blank when the char file has no stored width for
    that column/tab yet; setting it creates the entry.
- **Inheritance made explicit.** An inheriting tab shows its effective
  (account-default) columns with an "inherits account defaults" note; the first
  edit **materializes** the tab's own `tabColumnOrder` + `tabColumns` from the
  effective set, then applies the edit — mirroring the client.
- **Column labels** — the stored token lightly prettified for display
  (`TRANSVERSALVELOCITY` → "Transversal Velocity"); the raw token is available
  on hover. A curated friendly-name map is deferred.
- **Save** — one Save action writes **every dirty slot** (each through its own
  save chain); the header shows per-slot dirty state. Read-only fidelity on
  either file disables edits to that file's fields only.

## 6. Backend — `settings-model::overview`

A new module beside `windows.rs`, following M2's pattern: a JSON-serializable
projection that also **surfaces the `NodePath` to each editable field**, so
width edits reuse the generic `set_scalar` mutation the raw editor already has.

- `project_overview(user: &Value, char: Option<&Value>) -> OverviewColumns`
  — tabs (index, name, `inherits` flag), each with ordered columns (name,
  prettified label, `visible`, `width`), plus the `NodePath`s the edits target.
- Visibility toggle and reorder need list-membership / ordering semantics and
  the materialize-on-first-edit behavior, so they get **dedicated model
  functions** (unit-tested) rather than being expressed as generic mutations;
  width reuses `set_scalar`. Exact command surface (one `apply_overview_edit`
  command vs. extending `Mutation`) is a plan decision.

Commands added in `lib.rs`/`ops.rs`: `overview_columns(slots)` (read), the
overview edit command(s) above, and `open_overview_character(char_id)` /
paired-user auto-load wiring. All delegate to plain functions in `ops.rs` for
unit testing without a Tauri runtime.

## 7. Scope

**In:** two-slot state; bidirectional roster-driven loading; per-tab show/hide,
reorder, and width editing; inheritance/materialize; prettified labels.

**Unsaved-changes guard across both slots.** Loading any file — from the
sidebar, the character selector, or the char→user auto-load — while **either**
the char or user slot has unsaved edits warns the user that confirming will
discard those changes, and aborts the load if they decline. This extends the
existing single-file discard prompt to check both slots (the warning names which
file(s) would lose changes).

**Deferred / out:**
- Overview filter presets (raw tree only, per §6/§10).
- Editing the account-level defaults directly (raw-tree editable; per-tab
  editing with materialize covers the real use).
- A curated column friendly-name map (prettify heuristic for V1).
- Cross-account validation that the two loaded slots actually pair (the loader
  fills them from the roster, so mismatches only arise from manual raw opens).

## 8. Testing

`overview.rs` gets pure unit tests over hand-built `Value` trees (zero network,
no Tauri), same as `windows.rs`/`names.rs`: read projection (tab names, order,
visible set, widths joined from the char tree), visibility toggle, reorder,
width set, and the inheritance→materialize case (editing an inheriting tab
writes its own full lists from the account defaults). `ops.rs` gets a two-slot
open/save test. A corpus check that `project_overview` runs without panic across
the historical `core_user` corpus, where present.

## 9. Risks

- **Two-slot command refactor** touches every doc-scoped command (adds a `slot`
  arg). Mechanical but broad; the plan sequences it before the editor so the
  existing single-file views keep working throughout.
- **Materialize correctness** — writing a tab's own lists from defaults must
  reproduce what the client writes (full ordering, correct visible subset) or a
  later in-client edit could look inconsistent. Covered by the corpus and the
  experiment-3b observations; verified against a fresh file in live smoke.
- **Tab-index keys are `Int`** and sparse; the projection must not assume a
  contiguous 0..n range.
