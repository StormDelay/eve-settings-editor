# M5 — Character-centric batch apply (design)

_2026-07-18_

Batch apply, reframed around the **character** instead of the file. Milestone 5
of the design spec (`docs/superpowers/specs/2026-07-12-eve-settings-editor-design.md`
§9). M4 copied one subtree from one source file to many target files of the same
type. Its live smoke exposed the limit: a character's visible setup is split
across two files — *where* its windows sit is char-scoped (`core_char_*`
`windows`), while *which* overview windows exist, plus its overview
columns/tabs/presets, are account-scoped (`core_user_*` `overview`). "Make this
alt look like my main" is therefore inherently cross-file. M5 makes the
character the unit and treats the files as plumbing.

## 1. Model

Source **character** → target **characters** → **aspects**. The user never picks
a settings file. The engine routes each aspect to the file(s) that hold it,
using the M3b char↔account pairing (`accounts.json`) to find a character's
account file.

Aspects are a checkbox multi-select; **Everything** is exclusive and supersets
the rest:

| Aspect        | Writes                                                         | Collateral                                   |
| ------------- | -------------------------------------------------------------- | -------------------------------------------- |
| Window layout | char `windows`                                                 | none (char-scoped)                           |
| Overview      | account `overview` **+** char `ui → SortHeadersSizes` (widths) | account's other characters (the overview part) |
| Autofill      | account `ui → editHistory`                                     | account's other characters                   |
| Everything    | **full byte-copy of the char file + full byte-copy of the account user file** | account's other characters (whole account replaced) |

- **Everything is a full clone of both files.** The char file and the account
  user file are each byte-copied wholesale (M4's `full_copy_to`), so the target
  character and its account become byte-identical to the source's. Identity is
  preserved because EVE reads it from the filename, not the content. This is the
  maximal, most-destructive aspect — its collateral is the entire account.
- **Widths ride with Overview.** Column widths live in the char file
  (`SortHeadersSizes`), keyed by tab index. Because Overview also copies the
  account's tab config, the source's tab indices land on the target, so the
  copied widths stay aligned. Widths are never separately selectable.
- **An account is reachable only through a paired character.** A `core_user_*`
  with no confirmed character cannot be a source or target; the user pairs one
  first via the Accounts view / guided capture. This is the accepted cost of the
  character-only model.

## 2. Collateral & de-duplication

Overview, Autofill, and Everything's account copy all write the **shared**
account file, so making *one* character match rewrites *every* character on that
account. Rules:

- The preview names the **collateral characters** — same account, not among the
  selected targets — for each account write, before anything is touched.
  Everything's message is stronger ("entire account settings replaced") than
  Overview/Autofill's ("overview / autofill settings changed").
- Account writes are **de-duplicated**: N selected targets on one account ⇒ one
  account write, sourced once.
- A target on the **source's own account** skips the account write entirely — its
  account file already holds the source's settings (for Everything the two paths
  are literally equal). Only the char-scoped part of the aspect applies to it.

## 3. Backend — architecture #1 (thin app-layer orchestration)

### `settings-model/batch.rs` — the only engine change

Two new `Category` variants, each a single subtree at a fixed key path, handled
by the existing `extract_categories` / `apply_categories_to` / `inline_all`
unchanged (identical shape to the existing `Autofill` = `ui → editHistory`):

- `Overview` → `root → overview` (user file).
- `OverviewWidths` → `root → ui → SortHeadersSizes` (char file).

`full_copy_to` (M4) is reused as-is for both files of the Everything aspect. No
new write, backup, or verification code.

### `app/src-tauri/ops.rs` — orchestration

A pure, unit-testable core plus thin command wrappers (mirrors the
`accounts.rs` / `names.rs` split of format-vs-orchestration):

- `plan_setup(files, accounts_store, source_char, target_chars, aspects, …) ->
  SetupPlan` — **pure.** Resolves each character to its char file and (via
  `AccountsStore`) its account user file; excludes unpaired targets when an
  account aspect is selected; groups and de-duplicates account writes; computes
  the collateral characters per account write; flags per-target
  resolution mismatches for any aspect that copies the char file's window
  geometry — Window layout **or** Everything (input: each target's stored screen
  resolution, gathered by the orchestrator). Returns the whole plan:
  char-file writes, account-file writes (each with its collateral list), excluded
  targets, and warnings.
- `setup_preview(source_char_path, target_char_paths, aspects,
  allow_other_folders) -> SetupPlan` — loads discovery + the accounts store,
  gathers each selected target's stored layout resolution, calls `plan_setup`,
  serializes the result for the UI. Touches disk only to read.
- `setup_apply(source_char_path, target_char_paths, aspects,
  allow_other_folders) -> Vec<TargetResult>` — **re-resolves from scratch** (never
  trusts the caller, a Tauri boundary), then executes: char-file writes (subtree
  splice or full copy) followed by the de-duplicated account-file writes (subtree
  splice or full copy), via the `batch.rs` primitives. The source is read (and,
  for splices, decoded + extracted + inlined) once. Result is per-**file**
  `TargetResult` (M4's type); one file's failure is recorded and never halts the
  rest. The frontend regroups files back to characters/accounts.

Source resolution: any account aspect (Overview / Autofill / Everything) needs
the source's account file, so an **unpaired source** with such an aspect fails
the whole op up front, before any target is touched.

## 4. Frontend — rework `BatchView.svelte`

- **Source** — folder picker + character picker; defaults to the currently-open
  file when it is a char file.
- **Aspects** — Window layout / Overview / Autofill / Everything. Selecting
  Everything disables the other three.
- **Targets** — character multi-select, the source's folder by default with the
  retained **"show other folders"** toggle. An unpaired candidate renders
  disabled with "pair in the Accounts view to include" whenever an account aspect
  is selected (layout-only selections leave it enabled). Names via the existing
  `names.svelte.ts` / `accounts.svelte.ts` stores; other-folder rows are
  disambiguated with `profiles.ts` `profileLabels` (two installs sharing a
  server/profile stay distinguishable).
- **Preview** — calls `setup_preview` and shows the file-write count, the
  **collateral characters for each account write**, the resolution-mismatch
  warnings, and the excluded (unpaired) targets.
- **Apply** — calls `setup_apply`; a grouped per-file ✓/✗ report (backup path or
  error; ReadOnly targets shown as skipped).

No new store — the view holds its own `$state` and reads the existing stores.
Every new native control gets explicit dark colors (see
[[eve-editor-dark-native-controls]]).

## 5. Safety & ceilings

- Every file is backed up before it is overwritten (inherent to the save chain
  and to `full_copy_to`).
- Mandatory preview/confirm; collateral characters are named before any write.
- Category splices inline all sharing (`inline_all`), so a spliced target grows
  ~1.5× until EVE re-dedups it on next write (self-heals; the existing re-share
  debt in the small-tasks ledger, not new here). Full copy / Everything are
  unaffected — raw bytes.
- ReadOnly (non-canonical) targets are refused by the save chain and reported as
  skipped, never silently written.
- Aspects only ever route to the correct file kind; account writes are deduped;
  the source is never in its own target list.
- The resolution-differ case (a copied layout landing off-screen on a
  lower-resolution target) is a **warning, not a block** — every target is backed
  up, so it is recoverable.

## 6. Testing

- `batch.rs` unit + `tests/batch_realshape.rs`: `Overview` and `OverviewWidths`
  extract → splice → re-decode replaces exactly that subtree; `inline_all` leaves
  zero `Ref`/`Shared`.
- `plan_setup` (pure): char→account resolution, unpaired-target exclusion,
  account-write de-duplication, collateral computation, and the same-account skip.
- Command-layer: a partial failure spanning a char write **and** an account write
  in one op (one bad file does not stop the others).
- **Live smoke = the real merge gate.** Against the real client: make an alt's
  window layout + overview match a main; run Everything (full clone of both
  files) onto another alt. Verify EVE accepts each result, the settings appear
  in-game, the **warned collateral characters actually changed**, and every
  written file is valid (decodes, no duplicate keys). Record the result in
  `docs/format-notes.md` under `## Status`.
