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

- [ ] **Add a short public-facing README.** The repo root has no README and
  `app/README.md` is still the stock Tauri template. Write a brief one: what the
  tool is, that it edits local EVE settings files (with backups), where to
  download the installers, and a note that builds are unsigned so expect OS
  warnings. A design §11 "go public" item; low effort, high value once anyone
  outside the dev downloads an artifact. _Added 2026-07-16 (packaging check)._

- [ ] **Backfill informative release notes for v0.1.0–v0.4.0.** The release-prep
  *flow* is fixed as of v0.5.0 — `release.yml` now extracts each tag's CHANGELOG
  section into `releaseBody` (verified live: v0.5.0's draft body is its full
  CHANGELOG section), so future releases ship a real summary automatically. What
  remains is the backfill: the already-published v0.1.0–v0.4.0 releases still
  carry the old generic "See CHANGELOG.md" body. Go back and rewrite each from its
  CHANGELOG section. _Added 2026-07-17; flow shipped in v0.5.0, backfill deferred._

- [ ] **Warn in the batch preview when a target's resolution differs.** Master
  design §6 requires it — *"window geometry is absolute pixels; the preview warns
  for each target whose stored resolution differs from the source's"* — but the
  M4 spec dropped the requirement and no task built it, so a layout copy from a
  2560×1440 character onto a 1920×1080 one silently puts windows off-screen.
  Recoverable (every target is backed up) but exactly the surprise the warning
  exists to prevent. The projection already has the data: `WindowLayout` carries
  `resolution_matches`, and `WindowRect` carries `screen_w`/`screen_h`
  (`windows.rs:42,54-55`). The gap is that the batch flow never reads a target's
  layout — `batch_targets` returns `Candidate { path, file_name, id, folder,
  same_folder }` and nothing more, so surfacing this needs each candidate's
  stored resolution (either widen `Candidate`, or a separate command the preview
  calls for the selected targets). Weigh cost against M5, which will revisit this
  flow anyway. _Added 2026-07-17 (found documenting the M4 smoke)._

## Promoted to milestones

Graduated out of the small-tasks pen into planned milestones on 2026-07-17.
Ordering (updated 2026-07-17): **M4 batch apply** (code-complete, awaiting live
smoke), then **M5 cross-file batch apply**, then the layout-canvas milestone,
then the codec/refactor milestone.

**M5 — cross-file batch apply** (master design §9): batch apply for the sections
that span two files — overview settings (user → user), and the account-scoped
overview-window groups that decide which windows a char-scoped layout copy will
actually produce. Added after M4's live smoke showed a layout copy can't make two
characters match on its own: how many overview windows exist is account state,
where each sits is char state. See the M4 spec's §7 ceiling for the evidence.
_Added 2026-07-17._

Also carried into M5 (deferred from M4, 2026-07-17):

- **Disambiguate the batch target list's folder label.** With "Show other
  folders" ticked, each target renders `Candidate.folder`, built backend-side as
  `format!("{}/{}", p.server, p.profile)` (`ops.rs`) — which omits the install
  name, so two installs holding the same server *and* profile (a SharedCache dir
  and a legacy one both with `settings_Default`) render identically and the user
  cannot tell which file they are about to overwrite. Confirmed present on the
  developer's machine (two `tranquility / Default` profiles). Display-only —
  `same_folder` is driven by `p.dir` equality, so the safety filter is
  unaffected. The frontend already solves this for the sidebar and the batch
  *source* picker via `profiles.ts`'s `profileLabels`, which appends the install
  name only when a collision exists; the fix is to give the target list the same
  label, which means either widening `Candidate` or labelling frontend-side from
  the discovered profiles. Grouped into M5 because that milestone reworks this
  flow anyway.

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
