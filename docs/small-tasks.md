# Small tasks ledger

A holding pen for small, non-urgent improvements the developer wants done
*eventually*. These are **not** milestone-blocking on their own — they are
nice-to-haves that get **revisited at the end of every milestone**, before
release, and each open item is weighed for inclusion in that release.

Workflow:
- Add items here as they come up, newest anywhere in **Open**.
- At each milestone's end (before release), review every **Open** item with the
  developer for possible inclusion.
- When an item ships, move it to **Shipped** with the milestone that included it.

## Open

- [ ] **Make the view seamless when switching files.** Opening another file keeps
  the current editor tab (shipped in 0.5.0), but the switch visibly *blinks*: the
  view flashes to the default Tree view mid-switch before settling back on the kept
  view (e.g. Layout). It should be seamless — no flash to Tree. Likely the tab
  state momentarily resets to the default while the new file loads and its
  supported views are recomputed, then restores; hold the view across the load
  instead of reset-then-restore (and/or don't render the default view during the
  in-between). Lives in `+page.svelte`'s view-switch logic (the
  Tree / Layout / Overview / Autofill switcher). _Added 2026-07-18._

- [ ] **Skip empty-subtree writes in a batch category copy.** In
  `ops.rs::setup_apply`, a category splice is applied to every planned target even
  when the source's subtree for that category is empty or absent (e.g. an Overview
  copy from a source char that has no `SortHeadersSizes` widths still rewrites and
  backs up each target char file with an empty widths splice). Harmless (the splice
  changes nothing) but it inflates the preview's "will write N files" count,
  produces a spurious backup, and grows the target ~1.5× via `inline_all`. Skip a
  write whose extracted subtree is empty — ideally reflected in `plan_setup` so the
  preview count is honest too. _Added 2026-07-18 (M5 whole-branch review, minor M1)._

- [ ] **Extract the batch view's shared candidate filter+sort helper.**
  `BatchView.svelte`'s `sourceOptions` and `candidates` deriveds repeat the same
  `filter(folder-scope) → sort(byResolvedName)` chain; extract one `charsInScope`
  derived and build both from it. Cosmetic. _Added 2026-07-18 (M5 review, minor M2)._

- [ ] **Fill batch-apply edge-case tests.** `plan_setup`'s "account file missing
  from `user_paths`" exclusion branches (source and target), empty/duplicate
  `target_chars`, and the all-targets-on-the-source-account case, plus
  `setup_apply`'s own error branches (`source_error` → `Err`, missing source
  account file), have no unit test — all simple branches, cheap insurance for a
  file-writing feature. _Added 2026-07-18 (M5 review, minor M4)._

## Promoted to milestones

Graduated out of the small-tasks pen into planned milestones on 2026-07-17.
Ordering (updated 2026-07-18): M4 batch apply (shipped v0.5.0) and **M5
character-centric batch apply (shipped v0.6.0)** are both done; next is the
**layout-canvas milestone**, then the **codec/refactor milestone**. (M5 absorbed
the two carried-in M4 items — the resolution-differ preview warning and the
target-list folder-label disambiguation — both now under Shipped 0.6.0.)

**Layout-canvas milestone:**

- **Resize layout windows from any corner.** In the layout canvas, a selected
  window can only be resized from the bottom-right handle today. Add resize
  handles on all four corners (edges optional) once a window is selected, so it
  can be resized from any corner. _Added 2026-07-15._

- **Understand and integrate window stacks in the layout editor.** The layout
  editor surfaces a window's stack id but doesn't model stacking. Work out how EVE
  window stacks actually work (windows tabbed/grouped together, sharing a position)
  and integrate them into the layout canvas — e.g. represent a stack as a group
  and let the editor move/edit stacked windows coherently rather than as
  independent rectangles. _Added 2026-07-17._

**Codec/refactor milestone (after the layout one):**

- **Re-share correctly instead of inlining on overview save.** Overview column
  edits currently inline every `Shared`/`Ref` before encoding to avoid dangling
  refs (`RefBeforeStore`), which produces a valid but ~1.5x larger file that no
  longer matches what the EVE client would write. Revisit: re-derive a correct
  canonical `Shared`/`Ref` numbering after edits (encoder-side auto-dedup, sharing
  structurally-equal values in emit order) so the saved file matches the client's
  dedup. _Added 2026-07-16 (M3c)._

- **Dedup `inline_user` into `treewalk::inline_all`.** The autofill milestone
  added `treewalk::inline_all` (drop all `Shared`/`Ref` sharing); `overview.rs`'s
  private `inline_user` is now functionally identical. Delete the private copy and
  have `overview.rs` call the shared helper. Do it as its own change gated by the
  overview Shared/Ref encode tests — `overview.rs` is delicate. _Added 2026-07-17._

## Shipped

### 0.6.0

- [x] **Cross-file / character-centric batch apply (M5).** The batch view is now
  character-to-character: pick a source character and target characters, copy
  Window layout / Overview / Autofill / Everything, and the engine routes each
  aspect to the char file and/or the account `core_user` file, dedupes account
  writes, and names the collateral characters an account-wide write also changes.
  Replaces the M4 file-centric flow. _Added 2026-07-17; shipped 2026-07-18._
- [x] **Warn in the batch preview when a target's resolution differs.** The
  preview flags a target whose stored screen resolution differs from the source's
  (a layout copy would land windows off-screen). Built into the M5 flow. _Added
  2026-07-17._
- [x] **Disambiguate the batch target list's folder label.** Target rows under
  "show other folders" use `profiles.ts` `profileLabels`, appending the install
  name on a server/profile collision. Built into the M5 target list. _Added
  2026-07-17._
- [x] **Sort the Accounts-view character pickers.** The "add character" dropdowns
  and the Unassigned list sort by resolved name, matching the file list. _Added
  2026-07-18._
- [x] **Select-all / Clear for the batch target list, and drop excluded targets.**
  A Select-all/Clear control on the target list; an already-selected target that a
  later account-aspect choice excludes now unchecks and is dropped from the write
  list. _Added 2026-07-18._
- [x] **Add a short public-facing README.** A concise root `README.md` — what the
  tool is, features, install (with the unsigned-builds note), scope/safety, build,
  and MIT license. _Added 2026-07-16; shipped 2026-07-18._
- [x] **Backfill release notes for v0.1.0–v0.4.0.** The four already-published
  releases' bodies were rewritten from their CHANGELOG sections (via
  `gh release edit`), replacing the old generic "See CHANGELOG.md" text. _Added
  2026-07-17; shipped 2026-07-18._

### 0.5.0

- [x] **Add a search to the Autofill section.** A filter box narrows the
  remembered-text lists as you type, matching the list label, the raw widget
  path, and the entries. _Added 2026-07-17._
- [x] **Keep the current view when switching files.** Opening a file keeps the
  current editor tab when the new file supports it, falling back to Tree only
  when it doesn't — no more being bounced out of Layout. _Added 2026-07-17._
- [x] **Collapsible side panels.** The sidebar and backups panels collapse to a
  thin reopen rail so the center pane can use the full width. _Added 2026-07-15._
- [x] **Collapsible character/account categories.** The sidebar group headers
  (Characters / Accounts / Other) fold away via native `<details>`. _Added
  2026-07-17._
- [x] **Sort files alphabetically within each category.** Files sort by resolved
  character name / account alias, bare-id files below the named ones. _Added
  2026-07-17._
- [x] **Build GitHub release notes from the CHANGELOG.** `release.yml` extracts
  each tag's CHANGELOG section into the release body, so releases ship a real
  summary instead of a bare pointer. (Backfilling the old v0.1.0–v0.4.0 bodies
  is still open, above.) _Added 2026-07-17._

### M3

- [x] **Migrate legacy overview editing to modern on edit.** Editing an overview
  column in a legacy (`tabsettings`) account renames the tab container to modern
  (`tabsettings_new`) — the two are structurally identical. Validated on a real
  legacy corpus file and live in-game. _Added 2026-07-16 (M3c)._

- [x] **Keep the Save button reachable on small windows.** The filebar now wraps
  and the filename ellipsises, so a narrow/short window no longer pushes Save out
  of view. _Added 2026-07-16 (M3c)._

- [x] **Group the file list by type (character vs account).** The sidebar file
  list is split into Characters / Accounts / Other sections. _Added 2026-07-16 (M3c)._

- [x] **Drop the recent-sibling-writes save warning.** Removed the warning, the
  `SaveReport` field, and the sibling-mtime scan. _Added 2026-07-16 (M3c)._

- [x] **Negative-cache invalid character IDs.** ESI 404s any ID it can't
  resolve; those IDs are never cached, so every launch re-bisects them (extra
  ESI requests, counting against the error limit). Cache a tombstone for
  known-invalid IDs so they're skipped until a manual refresh. _Added
  2026-07-15 (M3a)._

- [x] **Name dialog-opened char files.** The open-file header only shows a
  character name for files discovered by the standard scan; a `core_char_<id>.dat`
  opened via the "Open file…" dialog shows a bare filename. Resolve its name on
  open too. _Added 2026-07-15 (M3a)._

- [x] **Extend name display to more surfaces.** Character names currently show
  in the sidebar and the open-file header only. Add them to the backups panel
  and the native OS window title. _Added 2026-07-15 (M3a)._
