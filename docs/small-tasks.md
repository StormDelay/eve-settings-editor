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

- [ ] **Revisit the remove-overview-window "last-window-only" restriction.** Phase B
  of overview tab management only lets the user remove the *last* overview window,
  because the `tabsByWindowInstanceID` position ↔ char-file `overview_N` key link is
  positional: removing a middle window shifts every later window's position out from
  under its `overview_N` geometry key, which would need a re-key cascade across the
  ~6 char `windows` subdicts (plus a promote-the-primary edge case if window 0 were
  removable). Deferred as fiddly cross-file surgery for a rare need. Revisit if users
  want to remove a specific middle window — either implement the re-key cascade, or
  add window-reorder first so a middle window can be moved to the end before removal.
  _Added 2026-07-20 (Phase B design)._

- [ ] **Overview tab-management follow-ups (deferred from the milestone's final
  review, all ship-as-debt).** Non-blocking minors from the whole-branch review:
  (1) `overview_tabs::move_tab` has no `UnknownTab` guard — moving a nonexistent
  tab index inserts a phantom entry into the target window strip (UI-guarded, same
  permissiveness as `reorder_tabs_in_window`); add a `tabs contains tab_idx` check
  to match `delete_tab`; (2) the two name-key predicates diverge —
  `overview_tabs::key_is_name` matches `Bytes("name")` but not `StrUcs2`, while
  `overview::key_is` matches `StrUcs2` but not `Bytes` (neither form occurs on real
  files, which use `StrTable(52)`); unify them into one shared predicate; (3)
  `ops::tab_create` projects the overview twice (once for the preset copy, once in
  `edit_user_tabs`) — harmless on tiny trees; (4) the UI's new-tab selection uses
  `Math.max(...tabs.index)` (`OverviewView.svelte`), coupling it to the backend's
  `max+1` allocation — sound today, but a before/after index set-diff would be
  allocation-agnostic; (5) can't create a tab in an empty (zero-tab) overview
  window (the New-tab target derives from the selected tab's window); (6) a few
  trivial untested branches (`delete_tab`/`move_tab` own `UnknownTab` paths, the
  `create_tab` preset-value assertion); (7) the tab-management **UI/UX is rough**
  (flagged during the live smoke) — defer the polish/rework to the later
  overview-depth slices (filter presets / colors / add-remove windows), which will
  touch this same Overview view anyway. **(Item (3) tab_create double-project is
  now RESOLVED — the tab-fix branch made create clone by index with no preset
  lookup.)** _Added 2026-07-19._

- [ ] **Overview windowless-account + no-fabricate follow-ups (tab-fix branch
  review).** (a) **Per-window placement on a windowless account:** creating a tab
  when the account has no `tabsByWindowInstanceID` now adds it to `tabsettings_new`
  and leaves the window mapping to EVE's default (the tab shows, verified in-game);
  placing it in a SPECIFIC overview window needs the char-side window↔tab mapping,
  deferred to the Phase B overview-window capture. (b) **Align
  `reorder_tabs_in_window` / `move_tab` to the no-fabricate read pattern** —
  `create_tab` and `delete_tab` now avoid materializing an empty
  `tabsByWindowInstanceID` when it's absent (an empty/partial mapping can hide the
  whole overview), but reorder/move still go through `groups_mut`, which fabricates
  it. They're UI-unreachable on a windowless account today, but worth aligning. (c)
  **Orphan-tab create placement:** creating a tab while an "Other" (orphan) tab is
  selected on an account that HAS windows lands the new tab in window 0 (via the
  `?? 0` sentinel) — valid and visible, but arbitrary; keep-disabled or document.
  _Added 2026-07-19._

- [ ] **Window-stacks follow-up: friendlier stack-frame labels.**
  `Stack.container_label` is always == `container_id` (`windows.rs`) — give a
  stack frame a friendlier label when a source name exists. Cosmetic. (The other
  five minors from the milestone's final review — `StackError` serde wiring,
  stack-move fanning w/h, `runStack` reselection, the panel select
  pre-filter/reset, and the test / `.stacked`-CSS debt — were all already
  resolved by the 0.8.0 polish commits; re-verified 2026-07-19. The one remaining
  "unreachable defensive branch" in `stackUnits` is intentional defense and has a
  test covering it — leave it.) _Added 2026-07-19._

- [ ] **Profile the reshare (deduplication) pass.** Every structural editor
  (overview / autofill / batch / window-stack membership) runs
  `blue_marshal::reshare` over the WHOLE document after each edit to re-derive
  canonical `Shared`/`Ref` sharing before save (inline → tally by `encode(v)`
  bytes → rebuild — an O(tree) traversal, plus an `encode` per node for the
  dedup key). It hasn't been profiled; on the largest real settings files it may
  add noticeable latency to a structural edit. (The geometry drag path does NOT
  reshare — it's plain `set_scalar` — so this is specifically the membership /
  overview / autofill edit path.) Measure it on the biggest corpus files and, if
  it's a bottleneck, consider caching the per-node encode key, an incremental
  reshare, or scoping the pass to the edited subtree. _Added 2026-07-19._

- [ ] **Improve the auto-derived autofill category labels.** In
  `app/src/lib/autofill.ts`, widget paths not matched by the `CURATED` substring
  map fall through to `derive()`, which just title-cases the last non-boilerplate
  path segment — for many real EVE widgets this yields cryptic or generic labels
  (the raw path is always shown too, so it's never *confusing*, just ugly). Fix
  by expanding `CURATED` to cover the common real widget paths and/or making
  `derive()` smarter (pick a more meaningful segment, or fold in more context
  than the last one). _Added 2026-07-18._

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

- [ ] **Make `treewalk::inline_all` Stream-scope-safe (or route it through
  `blue_marshal::inline`).** `treewalk::inline_all`/`collect_shared`/`inline_shares`
  resolve `Ref`s against one flat slot table that spans embedded `Value::Stream`
  boundaries, but an embedded stream is an independent marshal blob whose slots
  restart at 1 — so a stream with internal sharing would collide/corrupt. The
  codec re-share milestone fixed exactly this in the new `blue_marshal::inline`
  (Stream is a hard scope boundary) but left `treewalk::inline_all` — which the
  structural editors call *before* reshare — unfixed. Pre-existing and unreachable
  today (STREAM opcode count is 0 across the whole corpus), but it's an
  inconsistency: route `inline_all` through `blue_marshal::inline`, or mirror the
  per-stream scoping. _Added 2026-07-18 (codec re-share final review, minor M-1)._

- [ ] **Add a cycle/depth guard to `blue_marshal::inline`'s `resolve`.** `resolve`
  recurses `Ref → table lookup → resolve` with no bound; a hand-built
  self-referential `Ref` (the shape `encode`'s `cyclic` test rejects) would
  stack-overflow rather than error. Unreachable via `decode` (rejects cycles) or
  the edit paths, but it's *less* guarded than the pre-existing
  `treewalk::effective` (bounded `0..64`) — add a `MAX_DEPTH` bound mirroring
  encode/decode. _Added 2026-07-18 (codec re-share final review, minor M-2)._

## Promoted to milestones

Graduated out of the small-tasks pen into planned milestones on 2026-07-17.
Ordering (**re-sequenced 2026-07-18**): M4 batch apply (shipped v0.5.0) and **M5
character-centric batch apply (shipped v0.6.0)** are both done. Next is the
**codec/refactor (Shared/Ref) foundation**, *then* the **layout-canvas window
stacks** milestone — reordered because window-stack membership editing is the
heaviest structural editor yet and should sit on a correct encoder rather than
on the inline-first hack it would otherwise have to be un-built from. (M5
absorbed the two carried-in M4 items — the resolution-differ preview warning and
the target-list folder-label disambiguation — both now under Shipped 0.6.0.)

**Codec/refactor (Shared/Ref) foundation — NEXT.** Designed 2026-07-18:
`docs/superpowers/specs/2026-07-18-codec-reshare-foundation-design.md`. Goal: a
`blue_marshal::reshare` canonicalization pass (immutable-only dedup) that the
inline-first editors run before encode, so any editor can inline → edit →
reshare → encode and ship a compact, self-contained file instead of a ~1.5× one
the client re-deduplicates. Byte-identity to the client and dropping the
`Shared`/`Ref` fidelity tags are explicit non-goals (CCP's slot numbering is
opaque). This subsumes both items below:

- **Re-share correctly instead of inlining on overview save.** Overview column
  edits currently inline every `Shared`/`Ref` before encoding to avoid dangling
  refs (`RefBeforeStore`), which produces a valid but ~1.5x larger file that no
  longer matches what the EVE client would write. Re-derive a correct canonical
  `Shared`/`Ref` numbering after edits (encoder-side auto-dedup, sharing
  structurally-equal values in emit order) so the saved file matches the client's
  dedup. _Added 2026-07-16 (M3c)._

- **Dedup `inline_user` into `treewalk::inline_all`.** The autofill milestone
  added `treewalk::inline_all` (drop all `Shared`/`Ref` sharing); `overview.rs`'s
  private `inline_user` is now functionally identical. Delete the private copy and
  have `overview.rs` call the shared helper. Do it as its own change gated by the
  overview Shared/Ref encode tests — `overview.rs` is delicate. _Added 2026-07-17._

**Layout-canvas window stacks — AFTER the codec foundation.** Design worked out
and written up 2026-07-18 in
`docs/superpowers/specs/2026-07-18-layout-canvas-window-stacks-design.md`
(includes the corpus-verified stack model: `stacksWindows` member→container +
`preferredIdxInStack3` tab order; stack ids are window-id refs, never ints, so
the current Int-only stack field is dead). Scope: model stacks, draw one tabbed
rectangle per open stack, coherent move/resize, and membership editing
(unstack / add-to-existing / reorder); new-stack creation gated on a live
capture experiment. Membership editing depends on the codec foundation above.
_Added 2026-07-17; designed 2026-07-18._

**Resize layout windows from any corner — independent, ship anytime.** In the
layout canvas a selected window resizes only from the bottom-right handle today;
add handles on all four corners (edges optional). No codec dependency — its
resize handles are what the coherent stack resize reuses. _Added 2026-07-15._

## Shipped

### Unreleased (on master)

- [x] **No flash to Tree when switching files.** `+page.svelte` holds the current
  view across the file load instead of reset-to-Tree-then-restore, falling back to
  Tree only if the new file can't support that view. _Added 2026-07-18; done
  2026-07-19._
- [x] **Skip no-op splice writes in a batch category copy.** `setup_preview` now
  drops the char/account writes when the source lacks every category a splice
  aspect would copy (e.g. an Overview copy from a char with no `SortHeadersSizes`
  widths), so there's no spurious backup/rewrite and the preview's write count is
  honest. (The ~1.5× file-inflation half was already fixed by the 0.7.0 reshare
  pass.) _Added 2026-07-18; done 2026-07-19._

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
