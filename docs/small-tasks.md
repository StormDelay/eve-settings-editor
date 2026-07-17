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

- [ ] **Write informative GitHub release notes.** Every release (v0.1.0–v0.4.0)
  carries the same generic body — `.github/workflows/release.yml:41` hardcodes
  `releaseBody: "See CHANGELOG.md. Unsigned builds — expect OS warnings…"`. Go
  back and give each published release a real summary (the milestone's headline
  features, drawn from its CHANGELOG section) instead of a bare changelog pointer,
  and improve the release-prep flow so future releases ship an informative body
  (inject the CHANGELOG section into `releaseBody`, or fill in the draft before
  publishing). _Added 2026-07-17._

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

- [ ] **Add a search to the Autofill section.** The autofill view lists every
  remembered-text widget with no way to narrow them, so finding one list (or
  finding which list contains a given entry) means scrolling the lot. Add a
  filter box that narrows the rendered lists as you type, matching both the
  widget's display label (`labelFor`) and the entries themselves — "which list
  has that station name in it?" is the other half of why you'd search. Filter the
  already-derived `sorted` list (`AutofillView.svelte:21`); an empty query shows
  everything. Note `search.ts`'s `searchTree` is not reusable here — it walks a
  `TreeNodeData` tree, whereas this is a flat `RememberedList[]`, so a plain
  case-insensitive filter is all it needs, not new machinery. _Added 2026-07-17._

- [ ] **Keep the current view when switching files.** Opening a file always
  snaps the editor back to the Tree tab (`+page.svelte:171`, `view = "tree"` in
  `openFile`), so switching between two character files while working on window
  placements bounces you out of Layout every time. Instead, keep the current
  `view` when it's still available for the newly opened file, and fall back to
  Tree only when it isn't (e.g. Layout → a user file, which has no layout).
  Availability is already expressed by the tab-button conditions
  (`+page.svelte:374-377`): Layout needs `layoutAvailable`, Overview needs
  `openCharId !== null || slots.user?.status === "opened"`, Autofill needs an
  opened user file. Implementation note: `layoutAvailable` is recomputed at
  `+page.svelte:176`, *after* the reset at line 171 — so the new file's
  availability isn't known at reset time. Preserve the desired view and clamp it
  to Tree once availability is recomputed, rather than deciding at line 171.
  Leave `revealInTree` (`+page.svelte:117`) alone — that jump to Tree is
  deliberate. _Added 2026-07-17._

- [ ] **Collapsible side panels.** Make the side panels (sidebar file list and
  backups panel) retractable/collapsible so the center pane can grow — useful
  when editing window placements on the layout canvas, which wants as much
  horizontal room as possible. _Added 2026-07-15._ **Implemented 2026-07-17
  (collapse chevron → thin reopen rail; in-memory state) — awaiting release.**

- [ ] **Collapsible character/account categories.** Make the sidebar file-list
  group headers (Characters / Accounts / Other) collapsible so a long list is
  easier to navigate — click a category header to fold its files away. _Added
  2026-07-17._ **Implemented 2026-07-17 (native `<details>` per group) — awaiting
  release.**

- [ ] **Sort files alphabetically within each category.** Within each sidebar
  category (Characters / Accounts), sort files alphabetically by their resolved
  character name or account alias. Files still showing a bare numerical id (name
  unresolved / no alias) sort below the named ones. _Added 2026-07-17._
  **Implemented 2026-07-17 — awaiting release.**

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
