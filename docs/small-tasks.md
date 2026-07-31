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

- [ ] **Draggable splits and column edges on the canvas.** The chat splits are
  now editable as numeric fields on the selected window (2026-07-30), but not by
  dragging the splitter on the canvas, and the overview column widths are still
  editable only from the Overview view. Dragging was considered and dropped
  twice over: `DetailParts.svelte` is `pointer-events: none` by construction —
  the one declaration that stops decoration swallowing a canvas gesture, pinned
  by a test — so a splitter drag means punching a hole in it, adding `Drag`
  variants and adding hit-test exclusions; and at a typical canvas scale of ~0.3
  a chat window's input band is about 19 screen px tall, which is not a drag
  target. Worth revisiting only if the canvas gains a zoom. Wiring
  `set_overview_width` into the Layout view is the smaller, independent half.
  _Added 2026-07-30, narrowed from the detail layer's original entry._

- [ ] **The overview and chat internals have never been measured.** Most of
  `detail.ts`'s `DETAIL_NOMINAL` is invented, but not all of it: the ability
  and squadron cell WIDTHS alone are pinned by the measured panel width
  (`70 + 86x4 + 53 = 467` for the ability grid, `43 + 86x4 + 80 = 467` for the
  squadron row), and `detail.test.ts` asserts both reach the panel's right edge
  — so correcting either would need to fail that test first. Their HEIGHTS are
  guessed, same as the module slot cell, which is invented in both dimensions
  — nothing pins its width the way the fighter cells' widths are pinned, and
  its own `detail.test.ts` coverage is a bounds check, not an edge-exact one.
  What is genuinely guessed, in full: the module slot cell (both dimensions),
  the ability and squadron cell heights, the ability row pitch, the neocom's
  top EVE-menu cell, the overview tab-strip and header-band heights, and the
  fallback width for a column with no stored width. The HUD and fighter
  PITCHES around all of this are measured (`format-notes.md`, "HUD anchors");
  only what is drawn inside them is guessed. One screenshot session like the
  2026-07-28 one settles all of it, and each is a one-line edit. Also open
  from the same session: whether the chat input box spans the full window
  width or only the message pane — the editor draws the latter.
  _Added 2026-07-30._
- [ ] **Decide whether a Layout copy should carry the target list, and smoke the
  editor's writes in-game.** The anchor is editable now (`targetOrigin` /
  `alignHorizontally`, both account-scoped), but deliberately **not** a
  `batch::Category` — a Layout copy leaves the target account's list where it
  is. Two reasons to think before adding it: the stored value is a fraction
  whose denominator encodes the *source* client's neocom width, so a copy across
  accounts with different neocoms lands up to ~35px off; and `absent_means_default`
  would make a copy from any of the 87 % of accounts that have never dragged
  their list **delete** the target's position. Neither is fatal — both match how
  the other HUD keys already behave — but it changes what a copy writes, so it
  is the developer's call.
  Also still unverified in-game: that a value this editor *writes* lands where
  the canvas drew it (the capture only proved the read direction), and the
  `TARGET_MARGIN = 72` fallback a minted value has to assume. Both want one
  logged-in session with `docs/live-verification-target-origin.md` open.

- [ ] **A drawing layer for the canvas: module slots, fighter abilities, overview
  columns.** The furniture boxes now cover the right area (2026-07-28) but are
  still blank rectangles, and a blank rectangle does not tell a player what they
  are positioning against — recognising the thing is half of why the footprint
  mattered. The ship-HUD and fighter geometry needed for it is already measured
  and tabulated in `format-notes.md` ("HUD anchors"): capacitor centre at x 148
  from the box origin with a ~158px ring, module slot rows starting at x 245 on
  a 50px pitch, up to 8 columns × 3 rows; fighter ability grid at x 70 on an
  86px pitch, up to 5 columns × 3 rows, with the squadron row at x 43 / y ~178.
  All offsets are from each element's own top-left, so they can be expressed as
  percentages of the drawn box and rescale with the canvas for free. Overview
  columns would need their own measuring pass — nothing is captured for them yet.
  Decoration only: it must not reach `hudRects`, the snap lines, or any drag.
  Split out of the HUD-footprint task, which shipped the sizing half.
  _Added 2026-07-28._

- [ ] **Fill `command-defaults.json` by transcribing the in-game keybinding
  screen.** Confirmed in-game 2026-07-27 that it cannot come from a settings
  file: "Reset to default" writes `customCmds: {}`, because `customCmds` only
  ever holds overrides — there is nothing to capture, and the design spec's
  reset-to-default-logout plan is dead. Screenshot EVE's keybinding screen a
  screenful at a time and transcribe `command -> [virtual-key codes]`.
  **Keep the UI exactly as it is meanwhile**: `defaultFor` returns null per
  command (`keybinds.ts`), so the Default cell and the per-row reset light up
  for precisely the commands present and every other row is untouched — partial
  data is the expected state, not a broken one. Use the in-game *labels* to map
  rows back to command ids, and note the two traps found the same day:
  `CmdPickPortrait0..3` are labelled "Pick Portrait 1..4" (ids 0-based, labels
  1-based) and `ToggleCurrentSystemLocationWnd` is labelled "Local Locations".
  Cross-check any ambiguous row by binding it in-game and reading which id moves
  in `customCmds`. _Added 2026-07-27._

- [ ] **Nothing can create an `overview` container from nothing.** A document
  with no `overview` key — a pruned preset, or a genuinely fresh account — is a
  dead end: `overview_tabs::overview_mut` requires the key and returns
  `NoOverview`, and it is the only way in for both the tab editor and
  `overview_pack::apply_pack` (pinned by
  `applying_a_pack_to_a_file_with_no_overview_container_errors`). The only push
  of `b"overview"` anywhere is `overview_tabs.rs:221`, a field *inside a tab
  body*, not the container. `OverviewView.svelte:237` matches: zero tabs renders
  a bare hint, with every control in the `{:else}`, so there is no affordance
  because there is nothing behind one. This makes preset design spec §12.4's
  "mint an overview preset from nothing" (live plan item P5) not implementable
  as written — neither building nor importing works. Needs a decision on the
  minimum container EVE accepts before it is code: `overview-states.json` has
  the default state lists and orders, so the shape is derivable, but it is a
  design call, not a bug fix. _Added 2026-07-27._

- [ ] **Colortag-surface colours are invisible to the editor.** The model reads
  background colours only — `overview_states.rs::background_color_id` filters on
  `BACKGROUND_SURFACE` deliberately (its test is `projects_only_the_background_
  surface`), so `appearance.colors` never holds a flag colour, and
  `OverviewAppearanceTab.svelte:151` gates the swatch on `{#if isBg}` to match.
  The Colortag sub-tab therefore offers a checkbox and a reorder grip but no
  colour control at all. Real packs do set flag colours — `zs_full_v10.06.09`
  sets `flag_48: black`, which is exactly the entry that produced the unknown-
  colour warning — so importing a pack writes colours the user can then neither
  see, review, nor undo. Needs the surface carried through the projection and
  the swatch ungated. _Added 2026-07-27._

- [ ] **The state colour swatch is a free colour picker, not EVE's palette.**
  `OverviewAppearanceTab.svelte:154` is a bare `<input type="color">`, so any of
  16.7M colours can be chosen, but EVE's palette is eight named colours and
  `overview_pack.rs::color_name` matches floats **exactly** (correctly — a
  near-miss would silently rewrite the user's colours). A colour picked off the
  palette is therefore dropped from a pack export with no warning. Offer the
  palette as swatches, with free-form as a labelled escape hatch that says the
  colour will not survive an export. Blocked on the palette being complete —
  `PALETTE` has 6 of the 8 names, missing `green` and `purple` (`black` was
  harvested 2026-07-28); see the live verification plan item 26b and
  `overview_pack.rs:274`, which records why a sampled hex cannot be inverted into
  the exact floats `color_name` needs. _Added 2026-07-27._

- [x] **No way to reach a window that sits underneath another in the layout
  view.** Overlapping windows in `LayoutView` can only be selected topmost-first,
  so anything fully covered is unreachable — you have to drag the top window away
  and put it back. Worth investigating: alt/right-click to cycle the stack under
  the cursor, a "select from list" affordance keyed off `WindowPanel`'s existing
  window list, or hit-testing that skips the current selection so repeated clicks
  descend. Note the real files carry hundreds of window entries with heavy
  overlap (`format-notes.md`: one character had 381 windows, ~9 actually on
  screen), so this is a common case, not an edge one. _Added 2026-07-27._

  **Done 2026-07-30 — but read this, because the entry above was wrong when it
  was written.** "Unreachable, you have to drag the top window away" was already
  false: `.win.selected` has carried `z-index: 1` since the canvas landed
  (`0f814e0`, 2026-07-15), so a selected window paints above every other one,
  and the list has always selected onto the canvas — `02c7d42` (2026-07-26)
  even added an explicit `Select on canvas` item to its right-click menu. A
  covered window was reachable and movable by name the whole time. The second
  of the three suggested routes was therefore already built.

  What was genuinely missing was discovery *from the canvas*: `unitAt` returns
  the topmost hit, so there was no way to learn what else sat under the cursor
  without knowing its name — which is exactly the part that fails at 381
  windows. Shipped as **right-click lists every rectangle containing the point**
  (windows topmost-first, then furniture) via the existing `ContextMenu`;
  picking one selects it, and the existing z-index and `scrollOnSelect` do the
  rest. Not built: click-cycling (it fights the drag gesture, and a list beats
  blind traversal), keyboard traversal, and any menu action beyond selecting —
  the panel's own menu already has those, and the pick lands there.
  See `docs/superpowers/specs/2026-07-30-canvas-overlap-pick-design.md`.

- [ ] **Neocom button editor follow-ups (whole-branch review, all ship-as-debt).**
  Non-blocking minors from the layout-depth milestone's final slice (the neocom
  button editor): (1) `reorder` reassembles the bar via `clone()` rather than a
  true move — bars are ≤24 tiny entries, so this buys nothing measurable; (2)
  the `Tuple`-payload normalize branch in `neocom.rs`'s `bar_list_mut` is
  unreachable defensive code that silently rewrites shape instead of erroring —
  the next person in that function should delete it and let the `_ =>
  Err(NoBar)` arm handle it; (3) ~~no `ops.rs`-level tests for the new wiring~~ — **done 2026-07-30**:
  no-document, read-only (against a non-canonical stream, the fixture
  `document.rs` already uses) and a happy path through reorder, add and reset,
  ending on a round-trip check after the reshare each edit runs. The Tauri
  camelCase↔snake_case binding is still only exercised by the live smoke —
  nothing below the command layer can see it;
  (4) ~~`NeocomButtons.spec.ts`'s read-only test samples 3 of 5 interactive
  controls~~ — **done 2026-07-29**: it checks all five; (5) ~~the corpus gate never inspects `bar.original` and never
  asserts the projected button count equals the raw list length~~ — **done
  2026-07-30**: it does both, and a counter asserts the length comparison
  actually ran rather than skipping every file. Both hold across the real corpus,
  so the projection drops nothing today; (6) ~~a
  failed add clears the dropdown selection before the command runs~~ — **done
  2026-07-29**: the add no longer clears it; landing on the bar does, since a
  button on the bar is no longer addable, so a failure leaves the pick alone.
  Note for the next test in that file: a bare `.click()` leaves the DOM
  unflushed and made the first version of the assertion vacuous — `fireEvent`
  is what makes it real; (7) the panel shows raw ids (`job_board`,
  `map_beta`, `airCareerProgram`) rather than friendly labels — the same debt
  as the open `container_label` item, worth solving once for both; (8)
  icon-path casing varies across catalog entries, faithfully reflecting the
  client's own data. _Added 2026-07-27._

  **(1) and (8) are closed with no change (2026-07-30).** `reorder` rebuilding
  the bar with `clone()` is what the entry itself says it is — a bar is ≤24 tiny
  entries, so a true move buys nothing measurable and costs the clarity of the
  current version. The icon-path casing is the client's own; normalising it would
  make our catalog disagree with the files it describes. **(7) is the only item
  left:** raw ids instead of friendly labels, which wants solving once alongside
  the open `container_label` item rather than twice.
  **(2) is now RESOLVED — closed by the 2026-07-29 backend debt sweep:** the
  Tuple-payload branch is gone and `_ => Err(NoBar)` handles the shape. Worth
  recording *why* it was unreachable, since the comment defending it read as a
  reason to keep it: the corpus stores the live bar as a `List`, and `reset`
  writes a `List` whatever Original was stored as — so the arm only ever fired
  on a file we do not understand, and its answer was to rewrite it. The rest
  remain open.

- [x] **Run the settings-presets live in-game smoke.** Nothing in the feature
  has been verified against a running EVE client. From the spec's §12: (1)
  create a Layout-only preset from a real character, apply it to a *different*
  character, launch EVE, and confirm the windows land where the preset had
  them and the target's overview and autofill are untouched; (2) open that
  preset, move a window, save, re-apply, and confirm the edit landed — proving
  the preset is genuinely editable, not just a capture; (3) create an
  `Everything` preset from a fully configured client and apply it to a
  character whose settings files EVE has only just created (the fresh-install
  case), and confirm the client comes up configured; (4) open a preset holding
  only Autofill and confirm the Overview and Layout editors show honest empty
  states rather than erroring, then add an overview preset to it from scratch
  and confirm EVE accepts the result (the slice-2b minting-from-nothing path);
  (5) export a preset, re-import it under a new name, and confirm the two
  behave identically; (6) confirm the per-column width field is editable with
  a preset open. _Added 2026-07-27._ _Done 2026-07-28 (live plan P1, P2, P4, P6, P7, P8 in Session A; P3 in Session B — an Everything preset poured onto EVE's own virgin files came up fully configured). P5 could not be run: nothing can mint an overview container from nothing, tracked separately above._

 **(7) is now RESOLVED — closed by the 2026-07-29 backend debt sweep,** exactly
  as this entry prescribed: both tests pair their character to an account with a
  real file, assert the account side is untouched too, and the pruned one gained
  the message assertion its sibling already had. Verified load-bearing by
  neutralising each guard in turn — the target loses its `UNTOUCHED` marker and
  both byte assertions fail, where before they passed. Items (1)-(6) and (8)-(10)
  remain open.

- [ ] **Run the names-and-noise live in-game smoke — deliberately deferred past
  the merge.** The slice merged on a green CI and a clean whole-branch review, but
  nothing in it has been proven against a running client. Outstanding checks, in
  the order the reviews ranked them:
  1. ~~**The chat join is the one thing no test can settle.**~~ **DONE
     2026-07-28** — the id is `chatchannel_` + the tuple's FIRST element, the same
     shape for a named channel and a private conversation; written up in
     `settings-field-reference.md`. Items 2-5 below are still open, and 4-5 need
     no client at all.
  2. A stack the **editor** minted should still read `Window stack · N`: per
     `format-notes.md`, an editor-created stack gets no `tabgroups` entry.
  3. The frame row's new label renders between the FRAME marker and the open
     checkbox, and its `span.detail` is a bare flex child with no `nowrap` — judge
     whether a long label like "Character: Information" should sit after the name
     instead.
  4. Preferences round trip: `preferences.json` appears only after the first
     override, survives a restart, and hand-corrupting it yields
     `preferences.json.bad`. The copy-vs-rename fallback has **no CI coverage at
     all** — CI is Linux-only and the test that exercises it is `#[cfg(windows)]`.
  5. Two rapid override toggles on one window: the file must end up matching the UI.
  6. ~~`overrideCount()` counts overrides across every character~~ — **done
     2026-07-28**: the counter and its `clear` are scoped to the windows the open
     document has, so the line beside "showing N of M windows" describes that
     layout rather than every character's. The stored list stays application-wide
     by design; only what is reported and cleared is narrowed.

     **The guarantee is narrower than "another character's overrides are safe",**
     and the first wording of this entry overclaimed it. Window ids are
     per-character dict keys and the common ones (`overview`, `market`) repeat
     across characters, so clearing from A still drops B's override on a window
     they share. What it cannot do is remove an override for a window *this
     document does not have*. Real isolation would mean keying the stored list by
     character — a different and larger change, and arguably the wrong one, since
     marking a window as clutter is a statement about the window.

     Scoped to the document's windows rather than the drawn ones for stability:
     a counter that moved while you typed in the filter box would be worse than
     one that is slightly broad. (An earlier version of this note also argued a
     drawn-set count "could never include a clutter override" — that only holds
     while Hide clutter is on, so the stability argument carries the decision by
     itself.)

     **Known cost:** an override naming a window that no longer exists anywhere
     is now never counted and never cleared, so the list is append-only in
     practice. Harmless — `isClutter` only consults ids that are present — but
     there is no longer a UI path to prune one. Worth a "forget overrides for
     windows nothing has" action if `preferences.json` ever grows enough to
     notice.

- [x] **Capture EVE's factory keybindings.** `app/src/lib/data/command-defaults.json`
  ships empty, so the Keybinds view's Default column and per-row reset are
  disabled. Populating it: on a throwaway account open the in-game keybinding
  screen, choose Reset to default, log out, and read the table out of the
  resulting `core_user_<id>.dat`. No factory bindings exist anywhere else — an
  account that never opened the screen has an empty table, not a default one.
  **The keybindings live smoke was deferred to this same session** — it needs a
  running client either way. Three gates, from the slice's design spec §7.4:
  (1) rebind in the app, log in, confirm EVE honours it and does not revert;
  (2) batch-copy a table onto an account whose `customCmds` is empty and
  confirm the in-game screen shows it — the copied table carries another
  account's timestamp, a shape the client has not been observed to read;
  (3) spot-check labels against the in-game keybinding screen, particularly
  `OpenAgencyNew`, `OpenSkillQueueWindow` and the hand-corrected
  `CmdPickPortrait0..3` / `ToggleCurrentSystemLocationWnd` — they are
  provenance-verified from the client's localization data but never seen
  in-game, and `gen-default-preset-names.py`'s header records that its own map
  was only coincidentally right. Note the write order: the editor saves on
  demand, EVE writes its settings on **logout**, so log the character out
  before saving or the client overwrites it on exit.
  _Added 2026-07-26._ _Closed 2026-07-28: the method does not work and cannot be made to. "Reset to default" writes `customCmds: {}` — an EMPTY dict — because `customCmds` only ever holds overrides. There is nothing to read out of the resulting file. Superseded by the transcription task above. The three keybinding gates it also carried were run in Session A and passed._

- [x] **Confirm the HUD placement conventions v0.15.0 shipped as assumptions.**
  `HUD_NOMINAL`'s sizes, the centre-relative ship offset and the top-left point
  convention in `app/src/lib/layout.ts` are all guesses, flagged as such in the code
  and the changelog. Scope item 5 of the names-and-noise spec (§8) planned to settle
  them during that slice's smoke; the slice merged first, so it is still open. Move
  each element in-game, quit the client so it writes, reload in the editor, and
  compare against what the canvas draws. If a convention is wrong, correct it
  **together with its inverse** — `shipOffsetFromX` for the ship offset,
  `hudPointFromRect` for the fighter/badge point — and update the `layout.test.ts`
  round-trip cases that pin them. If they hold, delete the hedging from the
  comments. _Added 2026-07-26._ _Done 2026-07-27/28. Both conventions were RIGHT: the ship offset is centre-relative anchoring the HUD's own centre, and the point tuples are top-left corners in absolute screen px. The nominal SIZES are still invented — see the footprint task above._

- [x] **Run the overview-pack live in-game smoke — deliberately skipped before
  merge.** Slice 4 (import/export packs, PR #18, merged `210007e`) shipped without
  its live smoke; the user chose to come back to it. Nothing in the branch has been
  proven against a running client, so the checklist below is still entirely
  outstanding. From the spec's §8: import a published community pack and verify
  tabs, presets, colours and in-space ship labels in-game; export from the editor
  and confirm EVE's own Import Overview Settings accepts the file; determine which
  internal boolean `applyOnlyToShips` maps to, and whether a current-client export
  uses the suffixed (`backgroundStates2`) or unsuffixed state-list names (the
  reader accepts both either way). Plus the additions the whole-branch review
  earned: use a real **three-window** account (`[[0,1,2,3,4,7],[5],[6]]` — the
  shape 812 of 825 mapped corpus accounts have, NOT a hand-made two-window one)
  and import packs with both more and fewer tabs than the account, checking
  whether EVE renders a duplicated tab and what it does with a secondary window
  whose list ends up empty; import onto an account with **no tab settings at all**
  (385 of 1925 accounts — the first real exercise of the zero-tab fallback path);
  import a pack whose ship labels carry **colours** (the C2 regression); export →
  let EVE import it → export again from EVE and **diff the two exports**, which is
  what tells you whether EVE round-trips what we wrote or quietly normalises it;
  and import a **tab-layout-only** pack (no `presets` section) to see what EVE does
  with tabs whose `overview`/`bracket` names the account has no preset for.
  _Added 2026-07-26._ _Done 2026-07-27. Community pack imports and renders; EVE's own importer accepts our export; `applyOnlyToShips` has no key on current clients; EVE emits UNSUFFIXED state-list names. Two bugs found and fixed. Remaining pack questions are tracked as their own tasks above._

- [ ] **Tab order inside a window is not expressible in a pack, so export → re-import
  resets it.** A window's tab order comes from the per-window list in
  `tabsByWindowInstanceID`, and that mapping has no representation in EVE's pack
  format at all — `read_pack` writes `tabSetup` sorted ascending by index, and
  `apply_tabs` rebuilds window 0's list from the pack's order. So a user who has
  drag-reordered tabs inside a window loses that ordering on a round trip through
  a pack. Inherent to the format rather than a bug (a community pack genuinely
  should decide its own tab order), but a re-import of the user's *own* export
  could preserve the existing relative order of indices the window already had.
  Decide whether that asymmetry is worth the code. _Added 2026-07-26 (overview
  pack whole-branch review)._
  **Confirmed 2026-07-28 by the client itself:** `tabsByWindowInstanceID` appears in neither our export nor EVE's own (0 occurrences in both), and EVE's importer deletes the key from the account outright. So this is inherent to the format, exactly as suspected — the decision left is only what to do for a user re-importing their own export.

- [x] **Per-environment canvas views (in space / NPC station / player structure).**
  A player's screen differs by environment, and the canvas currently mixes all of
  them into one picture — which is part of why it shows far more windows than are
  ever visible at once. Explore a view selector that shows only the windows
  relevant to a chosen environment.

  **What the data actually supports (measured on a live char file — read this
  before designing):** EVE's context concept is real and explicit, but it is
  **per-feature, not a whole-layout switch**, and there are more than three:
  - `ui → InfoPanelModes_<context>` enumerates the client's own context list:
    `hangar`, `inflight`, `structure`, `charsel`, `planet`, `starmap`,
    `starmap_new`, `systemmap_new`, `skill_plan` (plus `ActivityTracker`).
  - The Inventory window carries three context-specific **window ids** —
    `InventoryStation`, `InventoryStructure`, `InventorySpace` — each with its own
    `ui → containerSortIconsBy_*` entry. Note these are the *same* unified
    Inventory window per docking context; they are NOT parents of the standalone
    `ShipCargo_<itemID>` windows, which are a different type.
  - `dockPanels` stores separate `widthProportion_docked` / `heightProportion_docked`.
  - **But `windows → windowSizesAndPositions_1` is FLAT**: one geometry per window
    id, with no per-environment copies. So EVE does not store three layouts — most
    windows have a single position shared across every environment.

  **Design implication:** this is a *view filter* over the existing single layout,
  in the same shape as the slice-1a clutter filter (a third dimension on
  `WindowFilter`), not a new data model and not a backend change. The hard part is
  the mapping — which window belongs to which environment — and only a few are
  self-evident from their ids (the three Inventory variants, `lobbyWnd` for
  station services, `StructureItemHangar`/`StructureShipHangar` for structures).
  The rest needs either curation or an in-game capture: dock and undock with known
  windows open and diff the files. Worth pairing with the user-editable clutter
  list below, since both are "let the user say which windows count".
  _Added 2026-07-26._

  **Done 2026-07-30.** Shipped as a two-environment view filter (docked / in
  space — not three; NPC station and player structure are collapsed into one
  "docked" view, per §3 of the design spec) on `WindowFilter`, with a curated
  exclusives table in `windowLabels.ts` where an unlisted id shows in both
  views. Inventory — the only context-split family in the geometry dict —
  folds to one rectangle in the docked view and fans a drag onto both copies.
  A corpus re-measurement (6,502 char files) confirmed the flat-geometry
  finding, and found Inventory's copies had already drifted apart on a real
  character. Still open, deliberately: splitting NPC station from player
  structure, per-window user overrides of the env table, and the in-game
  dock/undock capture that would replace the curated mapping with a measured
  one (live-verification item 35).
  The in-game by-eye verification of this view filter has not yet been done.

- [ ] **Decide what a one-member stack should do.** Slice 2 lets a tab be
  dragged out of a stack, which can leave the stack with a single member. The
  editor leaves it alone: what the client does with a one-member stack was
  never captured, and the file evidence points at leaving frames behind (a real
  character file carried 8 orphaned containers, below). Settle it in a live
  capture — drag the second-to-last window out of a stack in-game, log out, and
  look at whether `stacksWindows` / `preferredIdxInStack3` still name the last
  member — then either auto-dissolve on the drag-out or leave this closed.
  _Added 2026-07-26 (layout stack polish)._

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

- [ ] **Overview tab-management Phase B follow-ups (whole-branch review, all
  ship-as-debt).** Non-blocking minors from the Phase B (add/remove overview
  window) final review: (1) `remove_overview_window` reassigns tabs via
  `groups.get_mut(0).and_then(list_inner_mut)` — if window 0's value is somehow a
  non-list, the `if let Some(..)` silently drops the reassigned tabs (mirrors
  `delete_tab` house style; theoretical, window 0 is always a list on real files) —
  an `else` error branch would be more defensive; (2) `remove_overview_window`'s
  `UnknownWindow` branch (`window_idx >= count` with `count >= 2`) has no test — the
  UI only ever passes the last index so it's unreachable in practice, but a
  one-liner would close the guard (**the reviewer's pick as the most worth doing
  next**); (3) the `40`px cascade offset in `add_overview_window_geometry`
  (`overview_tabs.rs`) is a bare magic number at a single call site — a named
  `const OVERVIEW_WINDOW_OFFSET: i64 = 40` would document intent; (4)
  `ops::overview_window_add`/`overview_window_remove` duplicate the char-slot
  best-effort boilerplate (lock / `if let Some(doc)` / read-only check / reshare)
  across two sites — extract an `edit_char_geometry` helper IF a third cross-file op
  appears (borderline premature for two); (5) cosmetic — a windowless
  `remove_overview_window` returns `LastWindow` ("must keep at least one window")
  rather than a "no mapping" message (harmless, the UI never surfaces it).
  **(2) and (3) are now RESOLVED — closed by the layout-names-and-noise debt
  sweep:** `remove_overview_window`'s `UnknownWindow` guard now has a test, and
  the cascade offset is `overview_tabs.rs`'s named `OVERVIEW_WINDOW_OFFSET: i64 =
  40`. **(1) and (5) are now RESOLVED — done 2026-07-29:** a window 0 that cannot
  take the removed window's tabs is refused rather than silently dropping them (a
  tab present in `tabsettings_new` but in no window is invisible in-game), and an
  account with no mapping at all now gets `NoWindowMapping` instead of "keep at
  least one window", which described a different situation. **Only (4) remains,**
  and it is still conditional on a third cross-file op appearing. _Added
  2026-07-20; partially done 2026-07-26 (layout names-and-noise) and 2026-07-29._

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
  `edit_user_tabs`) — harmless on tiny trees; (4) ~~the UI's new-tab selection uses
  `Math.max(...tabs.index)`~~ — **done 2026-07-29**: it diffs the index set
  against the one it had, so it no longer assumes `max+1` allocation; (5) ~~can't create a tab in an empty (zero-tab) overview
  window~~ — **closed 2026-07-30 with no change.** The window is not hidden (its
  optgroup still renders, empty), and the way back is the "Move to window…"
  control, which lists every window including the empty one: create the tab
  anywhere and move it. Wiring a second target picker into the New button for a
  state only reachable by moving a window's last tab out is more UI than the case
  earns; (6) ~~a few
  trivial untested branches~~ — **done 2026-07-29**: `delete_tab`'s own
  `UnknownTab` path (with two tabs present, so `LastTab` cannot be what refuses
  it), `move_tab`'s (closed by the backend sweep), and that a created tab
  inherits its sibling's preset, next to the bracket and colour assertions; (7) the tab-management **UI/UX is rough**
  (flagged during the live smoke) — defer the polish/rework to the later
  overview-depth slices (filter presets / colors / add-remove windows), which will
  touch this same Overview view anyway. **Note 2026-07-30: all three of those
  slices have since shipped**, so this is no longer waiting on anything — it needs
  a fresh look at the view as it stands now, and re-filing as its own entry with
  whatever is actually still rough. **(Item (3) tab_create double-project is
  now RESOLVED — the tab-fix branch made create clone by index with no preset
  lookup.)** _Added 2026-07-19._
  **(1) and (2) are now RESOLVED — closed by the 2026-07-29 backend debt sweep.**
  `move_tab` refuses an index no tab has, via a new non-fabricating `has_tab`
  rather than `tabs_mut` — a guard that refuses an edit must not leave a minted
  `tabsettings_new` behind, which is also what lets it sit ahead of the
  `NoWindowMapping` guard. The two name predicates are one `treewalk::key_is`
  covering all four key shapes; the union is what each was missing an arm of, and
  the missing arms turned out to matter — a fixture keying the tab name as
  `Bytes` had been reading back as "Tab 0", with an `ops.rs` assertion pinning
  that as expected. Real files key it `StrTable(52)`, so no character was
  affected. Items (4)-(7) remain open ((5) and (7) are UI work).

- [ ] **Overview windowless-account + no-fabricate follow-ups (tab-fix branch
  review).** (a) **Per-window placement on a windowless account:** creating a tab
  when the account has no `tabsByWindowInstanceID` now adds it to `tabsettings_new`
  and leaves the window mapping to EVE's default (the tab shows, verified in-game);
  placing it in a SPECIFIC overview window needs the char-side window↔tab mapping,
  deferred to the Phase B overview-window capture.
  (b) ~~Align `reorder_tabs_in_window` / `move_tab` to the no-fabricate read
  pattern~~ — **done 2026-07-28**: both now refuse with `NoWindowMapping` before
  reaching `groups_mut`, so a refused edit no longer leaves an empty mapping
  behind.
  (c) ~~**Orphan-tab create placement**~~ — **documented 2026-07-30**, which was
  one of the two options this entry offered. Creating a tab while an "Other"
  (orphan) tab is selected lands it in window 0: arbitrary, but visible and
  movable, where disabling the New button would leave it dead for a selection
  that looks perfectly ordinary. The comment at the call site now covers both
  reasons `currentWindowIndex` can be null, not just the windowless one.
  _Added 2026-07-19._

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

## Shipped

### Unreleased (on master)

- [x] **A drawing layer for the canvas: module slots, fighter abilities,
  overview columns.** Shipped as a `Detail` toggle beside the canvas's
  reference-resolution line. With it on, the ship HUD draws its capacitor ring
  and module racks, the fighter panel its ability grid and squadron row, the
  neocom its real buttons in their real order, each overview window its real
  tabs and its real columns at their real stored widths — a column set too wide
  for its window now runs visibly off the edge — and each chat window its
  member-list and input splits, a data source this entry did not know about,
  added because the design spec found it cheap once the other read paths
  existed. Decoration only, as the entry required: `DetailParts.svelte` is
  `pointer-events: none`, and nothing here reaches `hudRects`, the snap lines,
  or any drag. The overview columns needed their own measuring pass, exactly as
  this entry predicted — nothing was captured for the overview or chat
  internals, so a handful of sizes drawn inside them are still guessed; see the
  new entry above.
  See `docs/superpowers/specs/2026-07-30-canvas-detail-layer-design.md`.
  _Added 2026-07-28; done 2026-07-30._

- [x] **A "discard changes" button beside the unsaved badges in the top bar.**
  Shown only while something is dirty, prompts once, and re-reads the open
  file(s) from disk. It discards **both** slots and says so on the prompt — the
  entry's own recommendation, and the right one: the editors write to both (an
  overview edit touches the account's tabs and the character's column widths), so
  reverting one would leave the half-reverted pair the slot-pairing machinery
  exists to prevent. A re-read, not a restore: the backup chain is untouched, and
  the view, the selection and an open preset all stay put, because exactly the
  files that were open are the ones reopened.

  Which slots get re-read is `slotsToReload` in `overview.ts`, beside the other
  slot-pairing decisions, and tested there. The wiring around it — prompt,
  reopen, clear the flags — has no component test, because mounting the page
  means stubbing the whole app. **The developer ran the click-through in the dev
  app on 2026-07-30 and it works**, so the gap is now "no automated coverage of
  the wiring" rather than "unverified". Worth knowing if that wiring is touched
  again: nothing but a human will notice it breaking. _Added 2026-07-26; done
  2026-07-30._

- [x] **HUD furniture follow-ups — all nine closed.** (3)(4)(5)(6) went in the
  2026-07-26 names-and-noise sweep, (1)(2)(7)(9) on 2026-07-29. (8) done
  2026-07-30, in both panels at once as the entry asked: Svelte patches an
  input's `value` only when the EXPRESSION changes, so an edit that left the
  model where it was — blank input, a refused write, or an int rounding back to
  what it already held (326.4 → 326) — kept the typed text on screen beside a
  value that is not it. `HudPanel` and `WindowPanel` resync the element on every
  commit; a write that lands is overwritten by the parent's re-render a moment
  later. The rounding case is the test worth having: it fails without the
  resync, where the blank case can pass either way depending on what re-rendered.
  _Added 2026-07-25; done 2026-07-30._

- [x] **Precision-editing follow-ups — both closed.** (2) done 2026-07-30:
  `commitUnit` re-resolves its unit from the live list by anchor id and falls
  back to the one captured at pointerdown. That is the third path the entry asked
  about, and it earns its keep at two lines — re-resolving alone silently skips
  the commit when the unit has since been filtered out, which is the worse of the
  two failures, and the captured unit alone can diff against pre-reload `geom`
  and skip a write that is needed. **(1) is closed with no change:** a keyup for a
  different arrow than the one being held ends the glide a round-trip early. The
  outcome is already correct — the next keydown re-acquires — and ref-counting
  held keys to avoid one extra round-trip is state this view does not need, which
  is what the entry itself concluded. _Added 2026-07-26; done 2026-07-30._

- [x] **Improve the auto-derived autofill category labels.** Curated needles now
  cover **206 of the 290 distinct widget paths** the corpus carries, up from 62.
  They are keyed on the window or the field, taken from a dump of the real paths
  rather than guessed, and `BOILERPLATE` learned EVE's layout scaffolding
  (`__maincontainer`, `headerCont`, `panelCont`, …) so the fallback lands on a
  panel name rather than a container one — the character sheet's skill search
  derived "Header Cont" purely because the informative segment was two levels up.
  The test carries thirteen real paths with what each used to derive in a comment.
  What is left uncovered derives something readable ("Skins Panel", "Sell
  Filter", "Edit Division3"), which is the bar for leaving one alone. _Added
  2026-07-18; done 2026-07-30._

- [x] **Overview-pack follow-ups — all seven closed.** (1)(3)(4) were done in the
  2026-07-29 sweeps: `BadSection { name }` replaced the reused `NotAPack`,
  `USER_SETTINGS` is gone (the live smoke settled `applyOnlyToShips` — no such key
  on current clients), and the three unused crate-root re-exports with it. Done
  2026-07-30:
  - **(2) half a ship-label pair applied nothing, silently.** The labels are
    rebuilt from the order list plus the name-keyed bodies, so a pack carrying one
    without the other can do nothing at all — it now says so.
  - **(7) a preset field that is not a list of numbers was written as `[]`.**
    `ints()` returns an empty vec for any other shape, so the account read "this
    setting is empty" from a pack that never said that. Such a field is reported
    and skipped now, leaving the account's own value alone; an empty list still
    means an empty list.
  - **(5) the export's report is `PackReport::exported`** rather than hand-built
    at the call site, where `applied` read as a claim about the account instead of
    about the file just written. `read_pack` keeps returning its warnings bare,
    documented: a read has nothing to report as applied.

  **(6) is closed with no change:** a pre-existing cross-secondary duplicate index
  in `tabsByWindowInstanceID` surviving an import. No corpus account has that
  corruption, the key is not expressible in a pack at all (confirmed 2026-07-28 —
  EVE's own importer deletes it), and repairing damage a pack cannot describe is
  not the importer's job. _Added 2026-07-26; done 2026-07-30._

  (A paragraph about `create_preset`'s guard order had been appended to this
  bundle by mistake; it belongs to the slice-2a entry above, which already records
  it, and was dropped here rather than duplicated.)

- [x] **Profile the reshare (deduplication) pass.** Measured 2026-07-30 on the
  largest real account files (~390 KB), release build: decode 3 ms, inline 3 ms,
  **reshare 10 ms**, encode 1 ms. A debug build is ~2.5x that (reshare ~25 ms),
  which is what a `cargo run` session feels — worth knowing before mistaking
  dev-mode latency for a shipped cost. **Not a bottleneck:** 10 ms once per
  structural edit, on the biggest file anyone has. The per-node encode-key cache,
  the incremental reshare and the subtree-scoped pass this entry floated all cost
  more complexity than they buy. The harness stays as `tests/reshare_cost.rs`,
  `#[ignore]`d with the baseline in its doc comment, so a re-run after a codec
  change has something to compare against. _Added 2026-07-19; done 2026-07-30._

- [x] **Character-centric entry-point follow-ups — all four closed.** (2) and (3)
  were done 2026-07-29 (the misleading `sharedWith` test renamed; the
  `AutofillView` hint keyed off `charOpen` rather than `charName`, which the entry
  had wrongly filed as unreachable dead copy). (4) done 2026-07-30: a profile with
  no listable character file draws no header, so a machine where that is true of
  EVERY profile showed a blank sidebar. It now says why, and distinguishes "these
  profiles hold no character files" from "the non-standard-name filter hid them",
  because the fix differs. New `Sidebar.spec.ts` pins both, plus the toggle
  clearing the second one and a real character file showing neither.
  **(1) is closed with no change:** the doubled, idempotent `api.open("user", …)`
  in the rare scheduler-flush window self-heals, and the entry's own instruction
  was to add an in-flight flag only if it ever showed as noise. It has not.
  _Added 2026-07-20; done 2026-07-30._

- [x] **Settings-presets follow-ups — all ten closed.** Nine were fixed across
  the 2026-07-29/30 sweeps ((5)(7)(8) in the backend batch, (9)(10) in the
  frontend one) and four here on 2026-07-30:
  - **(1) `import_from` wasn't atomic across its two writes.** The writes moved
    into `write_sides`, which removes the folder if any of them fails. A half
    folder was already invisible to `list()` (it needs both documents), but
    invisible junk accumulates and the next import of the same name would suffix
    itself around it. Tested against `write_sides` directly — `import_from`
    cannot be steered into that state from outside, because anything pre-placed
    at the target path makes the dedup loop skip past it.
  - **(2) `bytes_field` matched a bare `Bytes` only**, so a canonically-shared
    bundle (two identical sides dedup to `Shared` + `Ref`) was rejected as
    "missing its account side". Import inlines the document first.
    `blue_marshal::inline` is public, which made this a one-liner rather than the
    `treewalk` plumbing the entry expected. Verified the test fails without it.
  - **(3) the dedup suffix at the 100-character limit** now has a test. It stays
    an error: `" (2)"` pushes a legal 100-char name to 104 and `preset_path`
    re-validates. Truncating a user's name to make room is its own surprise for a
    case this narrow.
  - **(4) an apply walked `discover()` five times.** `setup_apply` ran the whole
    preview and then resolved the source again; an internal `preview_with_sides`
    hands the `SourceSides` over, so it is three. The public `setup_preview`
    keeps its signature.

  **(6) is closed with no change:** a character-source `Everything` copy still
  full-copies raw bytes without decoding them first. That is the point — a full
  copy of a file the editor cannot model should still copy, and adding a decode
  gate is the behaviour drift the slice deliberately avoided. _Added 2026-07-27;
  done 2026-07-30._

- [x] **Overview filter-presets slice 2a follow-ups — all six closed.** (2) the
  `create_preset` guard order was fixed by the 2026-07-29 backend sweep. Done
  2026-07-29 here: (3) `set_tab_preset`'s insert branch — a tab carrying no
  `overview` field — has a test, which is the half that decides the key encoding
  for a tab that never had one; (4) `rename_preset` no longer rewrites the key on
  a rename to the same name (it still validates that the preset exists, but
  writing identical bytes would turn a key the file stored as `Str` into `Bytes`);
  (5) the delete-neighbour test uses `alpha`/`Beta`/`gamma`, which a raw byte sort
  and the case-insensitive one disagree about, where `alpha`/`beta` sorted the
  same either way; (6) the realshape fixture wraps its sections with a `Long`
  timestamp like every real file. **(1) is closed with no change:**
  `preset_key_name`'s `Str`/`StrUcs2` arms are dead on real files by design —
  plan-mandated parity with `str_field_r`, and deleting them would make the reader
  narrower than its sibling for no gain. _Added 2026-07-20; done 2026-07-29._

- [x] **Make `treewalk::inline_all` Stream-scope-safe.** Routed through
  `blue_marshal::inline`, which has treated an embedded `Value::Stream` as a hard
  slot-scope boundary since the codec re-share milestone; the local
  `inline_shares` walk is deleted. A test builds an outer slot 1 and a stream
  carrying its own slot 1 and pins that each resolves in its own scope — it fails
  against the old flat-table walk (checked). Still unreachable on real data: no
  corpus file contains a STREAM opcode. _Added 2026-07-18; done 2026-07-29._

- [x] **Fill batch-apply edge-case tests.** Covered: a source whose account file
  is missing from the folder, a target whose account file is missing, a target
  with no character file, an empty target list, and `setup_apply` refusing a plan
  that carries a source error (with the target proven unwritten). The
  all-targets-on-the-source-account case named in the entry was already covered by
  `target_on_source_account_skips_the_account_write`. Writing them turned up one
  real hole, now fixed: a repeated id in `target_chars` planned the same file
  twice — two writes and two backups of one target. The UI passes a set, so the
  guard sits on the command boundary rather than fixing anything observed.
  _Added 2026-07-18; done 2026-07-29._

- [x] **Creating an `Everything` preset says nothing about what it captures.**
  The privacy confirmation lived on *export* only (`PresetGroup.svelte`: "carries
  everything the editor does not model, including your autofill history — station
  names, searches and typed text"), which is the right place to guard sharing —
  but the snapshot is taken at create time, and that is where the user picks
  `Everything`. The create form now carries the same sentence as a plain note
  under the checkbox list, not a blocking prompt; the export gate is unchanged.
  It sits after the `{#each ASPECTS}` loop, which puts it under `Everything`
  because that aspect is last in the list — noted in a comment there, since a
  reorder would move it. Pinned by `PresetGroup.spec.ts`'s "the form says what
  Everything captures", which is a *disclosure still exists* test rather than a
  copy assertion (verified to fail with the note removed). _Added 2026-07-27;
  done 2026-07-29._

- [x] **"Presets" now means two things in the UI.** Closed with **no rename**.
  The suggested fix was to relabel the sidebar group to "Templates" if the word
  ever confused anyone; nobody has hit it, and the cost is real — `.evepreset`,
  every `settings_preset_*` command and type, the docs and the changelog all say
  preset, so the UI would be the only place using a different word. The two
  meanings are also well separated in practice: the sidebar group is the only
  named group in a list whose other entries are profile folders, and EVE's own
  filter presets live inside the Overview view's Filters tab. Reopen if a user
  actually confuses them. _Added 2026-07-27; closed 2026-07-29._

- [x] **Extract the batch view's shared candidate filter+sort helper.** One
  `charsInScope` derived now does the folder-scope filter and the
  `byResolvedName` sort; the source dropdown uses it directly (`sourceOptions`
  is gone) and `candidates` is that list minus the source character. The
  redundant `.slice()` before each sort went too — `filter` already returns a
  fresh array, so there was nothing to protect. _Added 2026-07-18 (M5 review,
  minor M2); done 2026-07-29._

- [x] **Fold `treewalk::text` and `treewalk::bytes_str` together.** Two
  near-duplicate string readers that arrived from opposite sides of a merge:
  `bytes_str` (from the keybindings slice's helper consolidation) takes a raw
  `&Value` and handles `Bytes`/`Str`; `text` (from the names-and-noise slice)
  resolves through `Shared`/`Ref` via `effective` first and also handles
  `StrUcs2`. `text` is the strictly more capable of the two, so the fold is
  probably "delete `bytes_str`, pass a `SharedTable` at its call sites" — but
  check each call site first, since a caller that deliberately does NOT want
  `Ref` resolution would change behaviour. Kept both at merge time rather than
  refactoring two live APIs inside a conflict resolution. _Added 2026-07-26._
  _Done 2026-07-29: both call sites (autofill's widget keys, keybinds' command
  keys) already resolved through `effective` before calling `bytes_str`, so
  neither wanted to skip `Ref` resolution and the fold was the whole change._

- [x] **Add a cycle/depth guard to `blue_marshal::inline`'s `resolve`.** `resolve`
  recurses `Ref → table lookup → resolve` with no bound; a hand-built
  self-referential `Ref` (the shape `encode`'s `cyclic` test rejects) would
  stack-overflow rather than error. Unreachable via `decode` (rejects cycles) or
  the edit paths, but it's *less* guarded than the pre-existing
  `treewalk::effective` (bounded `0..64`) — add a `MAX_DEPTH` bound mirroring
  encode/decode. _Added 2026-07-18 (codec re-share final review, minor M-2)._
  _Done 2026-07-29._ The counter tracks **consecutive `Shared`/`Ref` hops and
  resets on descent into a child**, deliberately not container depth: encode and
  decode already bound that, and a real file carries a `Shared` at many nesting
  levels at once, so a single counter spanning both would false-trip and leave a
  valid tree half-inlined — which encode would then reject. Both halves have a
  test. At the bound the `Ref` is left in place, so the failure is
  `RefBeforeStore` at encode rather than a process abort.

  **That reasoning had a hole, closed 2026-07-29 later the same day.** Resetting
  on descent means the counter only ever caught a chain of `Shared`/`Ref` nodes
  pointing straight at each other. A cycle closed THROUGH a container —
  `Shared { 1, List([Ref(1)]) }` — reset the count every lap and still overflowed
  the stack; verified by running it against the merged code, which aborted the
  test binary with `STATUS_STACK_OVERFLOW`. The counter is now a set of the slots
  currently open in the recursion, which catches a cycle wherever it closes and
  cannot false-trip on a long legitimate chain (each hop consumes a distinct slot
  from a finite table), so the concern the counter was shaped around is met
  exactly rather than approximately. Both original tests still pass unchanged.

- [x] **Decide what the Layout aspect should mean, then make it carry that.**
  `Category::Layout => &[b"windows"]` copies that section, but the nine HUD
  fields span three (`hud.rs:72–101`): `windows` has `shipuialignleftoffset` and
  `neocomWidth`, `ui` has `fightersDetachedPosition` / `shipuialigntop` /
  `detachFighterUI` / `displayFighterUI`, and `notifications` has
  `notification_badge_offset`. So a Layout copy or preset moves the ship-HUD
  offset and neocom width and leaves the fighter UI and badge behind — half
  applied, which is more confusing than carrying none. Confirmed on live files:
  after an A1 → A2 Layout copy, `shipuialignleftoffset` matched at `0.0` while
  `fightersDetachedPosition` stayed at A2's own `(326, 54)` against A1's `(0, 0)`.
  Presets share the key sets (`presets.rs:113`), so both surfaces are affected.

  **This entry's own count was wrong.** It read `neocomWidth` as a sibling of
  `shipuialignleftoffset` because both sit under a key called `windows` — but
  the two are in different *files*: `shipuialignleftoffset` is in the character
  file's `windows`, `neocomWidth` is in the account file's `windows`, and the
  aspect only ever wrote the character side. So the copy it shipped with carried
  **one** of nine, not the two claimed above — the A1 → A2 confirmation above
  happened to demonstrate the one field that actually travelled.

  **Decided: pull all nine in**, rather than split a HUD aspect out — "Window
  layout" now covers the whole HUD editor's field set, so a copy leaves nothing
  for the two screens to disagree on. What it cost: the aspect now writes the
  account file, so a copy reaches **every character on the target's account**,
  not only the one targeted (the preview names them, matching Overview and
  Autofill's existing account-wide writes), and a character with no paired
  account can no longer receive one — it needs pairing in the Accounts view
  first. Where the source stores no value for a HUD field (EVE's own default),
  the target's own value is now removed rather than left in place, which is
  what makes the two characters actually match instead of merging. **The live
  smoke this entry called for has not been run** — nothing here is verified
  against a running client yet. _Added 2026-07-27; done 2026-07-28._

- [x] **The keybinds "taken by" note overflows its row.** Ellipsised rather than
  wrapped, with the full command name on the `title`. The combo column is a fixed
  16rem in a `table-layout: fixed` table, so a growing row was the other option —
  a keybinding table is scanned vertically, where uneven rows cost more than a
  truncated note. Precisely: the note is an `inline-block` capped at the cell, so
  it can never spill over the row beneath, but because the binding button holds
  `min-width: 7rem` on the same line a long note can still drop to a second line
  *inside* the cell rather than truncating on the first. Bounded at two lines,
  not one — tightening further means a fixed `max-width` or making the cell a
  flexbox, and the overlap this was raised for is gone either way.

  Note `.meta` is shared by three spans in that file, including a searchbar
  instruction with no `title` fallback; the constraint is scoped to `.combo .meta`
  so the others keep wrapping. _Added 2026-07-27; done 2026-07-28._

- [x] **Test fixtures encode bare container payloads, a shape EVE never writes.**
  Half of this was already done and the entry did not know it: `overview_pack`'s
  repair shipped, and its `user_doc()` fixture's bare `overviewColumnOrder` is now
  *deliberate* — it is the input for `apply_pack_rewraps_a_bare_payload`, and
  `apply_pack_wraps_every_list_section` records the same history the entry
  describes. Wrapping that fixture would have deleted the only coverage of the
  repair, so it was left alone.

  The `overview_tabs` side is what remained. Its fixtures now build
  `tabsettings_new` in the `(timestamp, dict)` shape every real file uses, which
  unblocked the `rewrap` repair that was written during the live session and
  backed out because those fixtures failed it. `tabs_mut` and `groups_mut` now
  restore a missing wrapper the way `overview_pack::put` does — and leave an
  existing timestamp alone, which has its own test, because resetting a real one
  to zero would be a different kind of damage. _Added 2026-07-27; done
  2026-07-28._

- [x] **Every chat window is labelled just "Chat".** The entry read as a feature
  request; it was **two** bugs in a feature that already shipped. The join it
  asked for existed end to end — `windows.rs::chat_channel_names` reads
  `ui → chatchannels`, `window_layout` assigns `WindowRect.name`, and
  `windowLabels.ts::nameOf` already prefers it. `PARAM.chatchannel = "Chat"` is
  the *fallback*, correct as one.

  (1) The join keyed on the tuple's SECOND element while the window id is built
  from the FIRST — for `player_*` rows those are the same string, so it looked
  right and missed every standing channel. (2) More seriously, it matched a bare
  `Value::List` while the section is `(timestamp, list)` in **281 of 281** corpus
  files carrying it, so it returned an empty map on every real file. Fixing the
  index alone changed nothing; both were needed. Now reads via
  `treewalk::as_list`, which already existed for exactly this.

  Measured across the corpus: **0 of 11,480 chat windows named before, 1,113
  after** — Corp, Alliance, Local, Fleet, Militia and named private groups.
  (`fleet`, `faction`, `incursion` and `system` rows were never sampled while
  diagnosing this and all key correctly on element 0; every one of the 1,114 rows
  across 281 files matched a window, with no duplicate keys to shadow each
  other.) The ~70 stale windows per character are closed conversations the file
  holds no name for; they keep the derived `Chat · <detail>` label, which is
  right — thinning that list is what `Hide clutter` is for.

  Neither bug was visible to the tests: the chat fixtures seeded a bare list AND
  led with an `Int` key, so one of them could only pass while the code read the
  wrong element. A fixture that shares the code's assumptions cannot falsify
  them — the third instance of that shape in one day, after the column-wrapper
  bug and the `inline_all` guard. So this closes with the thing the other two
  didn't get: **`tests/chat_names_corpus.rs`**, a real-data gate in the style of
  the nine already here. It runs on the committed synthetic corpus as well as the
  real one, and `gen_fixtures.rs` has carried the correct wrapped element-0 shape
  since that corpus was created — so the gate would have failed from day one on a
  checkout with no `testdata/` at all. Verified to fail: reverting either defect
  drops it to 0 named. _Added 2026-07-27; done 2026-07-28._

- [x] **The neocom renders differently docked vs in space.** Settled in Session C
  by photographing the same character docked and in space and diffing the two
  bars pixel-for-pixel. **It is an addition, not a filter and not a reorder, and
  nothing reflows.** In space two extra icons (drones and the scanner, by their
  glyphs) appear in slots that sit *empty* while docked; every other icon is at
  an identical y in both shots. Apart from those two bands, the clock digits and
  the screen edge, the strips are byte-identical — 63 differing rows out of 1440.

  So the editor's model is right and the panel is not "showing the docked bar":
  `neocomButtonRawData` holds one fixed ordered list (12 instances on this
  character), positions are fixed, and the client shows or hides
  environment-specific buttons in place on top of it. Nothing in the file drives
  it and nothing needs to. The earlier "different set/order in space" reading was
  the additions appearing, not the stored list being reordered — so item 1's "the
  bar order matches the editor's" is meaningful in **both** environments.
  _Added 2026-07-27; done 2026-07-28._

- [x] **Confirm the ship HUD's anchor at a second offset.** Done in Session C,
  and it settled more than it set out to. The client wrote `-1052` after the drag
  (410px left of the `-642` the first measurements used), and at that offset the
  capacitor wheel's centre measures **228.0** against `2560/2 + (-1052) = 228`
  — exact, on a wheel span of 50px matching the reference's 51. The anchor model
  now holds at two offsets 410px apart.

  **The inferred number is now measured.** The left-hand ship-control button
  column moved by exactly the offset delta: its runs go `(490, 512)` → `(80,
  102)`, i.e. −410 and −410. It travels with the HUD, so the 148px left extension
  is a measurement rather than an assumption, and `HUD_NOMINAL` has no guessed
  numbers left except the badge.

  **And the bottom margin was wrong.** The bottom-aligned shot (the flag flipped
  in the same session) shows the element's rack block is 127px tall either way,
  sitting 4px into the element, at y 32 top-aligned and y 1272 bottom-aligned —
  so bottom-aligned the element runs 1268..1428 and the gap below is **12px, not
  the 28 the code mirrored from the top margin**. The element is not vertically
  symmetric. Corrected with its own `SHIP_BOTTOM_MARGIN`; the old guess drew the
  box 16px high, which is the snap-line bug this whole task exists to fix, just
  on the other edge. _Added 2026-07-28; done 2026-07-28._

- [x] **The HUD furniture must cover its real in-game footprint — module racks
  and fighter abilities included.** Measured off three native screenshots
  2026-07-28 and corrected. The ship HUD was wrong in a way the entry did not
  anticipate: not merely mis-sized but mis-*anchored*. The stored offset centres
  the capacitor wheel (measured 638.5 against 638 predicted), and the element
  extends 148px left of it and 495px right — so the old box, centred on the
  anchor, sat 195px too far left and missed 152px of module rack. Now 643×160
  from `anchor-148`, with `shipOffsetFromX` corrected as its exact inverse and a
  round-trip test pinning the pair. The fighter panel keeps its (already correct)
  anchor and grows from 400×120 to 467×253 — the old height covered the squadron
  row alone, so windows snapped straight through the ability grid above it. Both
  boxes now COVER their contents; actually drawing the racks and the ability grid
  inside them was left out — the internal geometry (slot pitch, grid pitch, ring
  diameter) is recorded in `docs/format-notes.md` § "HUD anchors" for whenever a
  drawing layer wants it. One assumption remains unverified: that the
  left-hand ship-control button column moves with the HUD rather than being
  screen-anchored — both shots share one offset, so they cannot separate the two.
  _Added 2026-07-28; done 2026-07-28._

- [x] **A windowless account is a normal state, and the editor treats it as an
  error.** Reworded throughout, and given a way out that does not lie. The
  entry's suggested fix — rebuild a single-window mapping "since that is
  evidently what the client does" — was not taken, because it over-reads the
  evidence: Session B proved the client *deletes* the mapping and keeps working,
  not that it rebuilds one. An absent mapping means EVE distributes tabs across
  its char-side windows, which this crate cannot read, so fabricating one pins
  every tab into a single window and silently flattens a multi-window overview.
  Instead `create_window_mapping` writes a COMPLETE mapping (every tab, ascending
  index) and is reachable only from an explicit "Set up per-window tabs" action
  behind a confirm that says what it replaces. `NoWindowMapping` now describes a
  configuration rather than a fault, and `reorder_tabs_in_window` / `move_tab` no
  longer fabricate an empty mapping on a refused edit — which was item (b) of the
  windowless/no-fabricate follow-ups entry, closed with this. _Added 2026-07-28;
  done 2026-07-28._

- [x] **`tabsettings2` exists and the editor has never read it.** Resolved by a
  corpus scan rather than an in-game test, and the read order stands unchanged.
  Scanned all **174 distinct account files** (2,897 copies deduped by content);
  130 carry at least one tab key, 72 carry `tabsettings2`. Two facts settle it:
  `tabsettings2` is **never the only tab key** (0 of 130), so the editor never
  shows an empty tab list where it holds one; and on **every** file carrying
  `tabsettings_new` — the key the current client actually rewrites —
  `tabsettings2` is older, with no exceptions. The entry's worry that "one corpus
  account carries the newest timestamp of all three" does not survive: the 11
  files where `tabsettings2` is newest carry **no `tabsettings_new` at all**. They
  are pre-Photon backups (`settings_Default - before photon mandatory`, one
  `- old`), where `tabsettings2` was written ~0.08s after `tabsettings` in the
  same save. So "stale relative to the authoritative key" holds everywhere it
  matters. The reasoning is recorded in `settings-field-reference.md` §5.2 with
  one honest caveat: on such a pre-Photon backup the editor reads `tabsettings`
  while that era's client may have read `tabsettings2` (their contents differ —
  `tabsettings2` is roughly twice the size with different presets and columns).
  Untestable and unreachable in practice: no current client reads those files.
  _Added 2026-07-27; done 2026-07-28._

- [x] **The overview filter list is slow.** Categories now render their checkbox
  rows only while open, and the filter box is debounced 150ms. The entry's count
  was wrong in a way that mattered: the tab rendered **649** rows across 15
  categories, not 1,605 — `all_group_ids` (1,605) is the ESI sync list, not the
  rendered tree. 400 of the 649 sit in `Entity` alone, which is why "render only
  what is open" was the right lever: a collapsed tab now costs zero rows, and
  only someone who deliberately opens `Entity` pays for 400. Of the three costs
  listed, (1) was misattributed — `filterCatalog` over 649 strings is
  microseconds; the cost on that keystroke was the force-expand — and (3)
  (`presetGroupSet` rebuilds) was never worth touching. (2), the round trip per
  tick, is unavoidable but now re-evaluates only rendered rows. Virtualisation
  stayed unneeded. _Added 2026-07-27; done 2026-07-28._

- [x] **Offer to delete orphaned stack frames from the file.** `delete_orphan_frames`
  in `stacks.rs` removes every numeric-string id that is neither a stack member
  nor a container, from `windowSizesAndPositions_1`, all eight `BOOL_FLAGS`
  dicts, `stacksWindows` and `preferredIdxInStack3` — one action rather than the
  5-6 hand-deletions each frame needs. The window panel offers it whenever the
  open file carries any, behind a confirm that names the count. Safe because the
  client was verified not to re-create them (2026-07-28: two frames deleted from
  a real file survived a full login/logout). The backend re-derives the orphan
  set rather than trusting an id list over IPC. _Added 2026-07-26 and 2026-07-28
  (two entries, one task); done 2026-07-28._

- [x] **Nothing marks which settings folder EVE actually uses.** The diagnosis in
  this entry was wrong: the marker existed (`primaryProfileDir` pinned a profile
  on top and opened it) and it was pointing at the wrong folder. Ranking on the
  newest mtime *across any file* — which is what this entry asked for — is the
  bug, because the editor's own saves move it: staging four edits into
  ` - USE THIS ONE` through this editor is what promoted a weeks-stale backup to
  the top. `primaryProfileDir` now ranks on the anonymous `core_char__.dat` /
  `core_user__.dat` / `core_public__.yaml` files, which only EVE writes (11
  editor-only captures touched none; all 4 post-client captures touched three),
  falling back to any file when no profile carries one. The sidebar says "in use
  by EVE" and marks every other folder "not in use — EVE has not written here" in
  warning colour. _Added 2026-07-28; done 2026-07-28._
- [x] **The open character file is missing from the batch target list when the
  source is a preset.** `candidates` now excludes the source only when the source
  is a character (`batchSource?.kind === "character"`) rather than filtering on
  the `sourcePath` seeded from `openPath`, so switching to a preset source no
  longer outlives its reason. Decided deliberately, per this entry's note: the
  open file *is* a legal target — the item below warns about it instead of hiding
  it. _Added 2026-07-27; done 2026-07-28._
- [x] **A batch apply onto the open file warns too late.** The plan summary warns
  whenever an effective target is the open document, regardless of dirty state —
  the on-screen copy goes stale either way, and that needs no dirty-tracking
  plumbing. The save-time on-disk check stays as the backstop; this just moves the
  news to the point of decision. _Added 2026-07-27; done 2026-07-28._

- [x] **Add a search/filter to the window list in the Layout editor.** Shipped by
  layout slice 1a as a filter box plus `Open only` and `Hide chat & session
  windows` toggles — and the predicate drives the *canvas* as well as the list,
  so narrowing one narrows the other, with a `showing N of M windows · reset`
  counter so nothing hides silently. _Added 2026-07-20; done 2026-07-26._
- [x] **Panel right-click is a context menu, not a direct tree jump.** The M2
  deferral (and its `TODO(revisit)` in `WindowPanel.svelte`) is closed: rows,
  coordinate fields and flags open a menu with *Show in tree*, *Copy window id*
  and *Select on canvas*. _Done 2026-07-26 (layout slice 1a)._

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
- [x] **Real chat-channel names in the Layout view (and its filter).** `window_layout`
  now reads the character file's `ui → chatchannels` and attaches EVE's own label
  to the matching chat window, so the list, the canvas and the filter show and
  match the real channel name instead of `Chat · private`. _Added 2026-07-26
  (slice 1a live smoke); done 2026-07-26 (layout names-and-noise)._
- [x] **Stack labels from the account file's `tabgroups`.** `window_layout` now
  takes the open account document and reads `tabgroups → <containerId>_names`,
  so `Stack.container_label` carries EVE's own string (e.g. "Character:
  Information") instead of a copy of the container id. _Added 2026-07-26 (slice
  1a design); done 2026-07-26 (layout names-and-noise)._
- [x] **Window-stacks follow-up: friendlier stack-frame labels.**
  `Stack.container_label` now carries EVE's own `tabgroups` string when the
  account file has one — closed by the two items above. The last call site not
  honouring it (`WindowPanel.svelte`'s `.stack-head` fallback, still reading
  `describe(stack.container_id).label` unconditionally) now falls back to it
  too. _Added 2026-07-19; done 2026-07-26 (layout names-and-noise)._
- [x] **Layout slice 1a follow-ups (final whole-branch review, both ship-as-debt).**
  (1) The tautological `drawnWindowCount` test in `layout.test.ts` (comparing
  `stackUnits(x, null)` against `stackUnits(x)`, the same call) is replaced with a
  `Set`-based filtered case asserting a stack container matching the filter while
  no member does draws nothing. (2) The stack ↑/↓ reorder buttons now disable
  when the neighbour they'd swap with is hidden by the filter (`memberVisible`),
  instead of staying enabled and swapping with a row the filter is hiding. _Added
  2026-07-26; done 2026-07-26 (layout names-and-noise)._
- [x] **Let the user edit the Layout clutter list.** A window's right-click menu
  now offers "Treat as clutter" / "Stop treating as clutter", persisted in a new
  editor-owned `preferences.json` (`app_config_dir()`, never written to an EVE
  file) that `isClutter` consults ahead of the built-in tables; the window list's
  counter line grows `· N overridden · clear` whenever an override is set. Turns
  the unwinnable curation problem the built-in tables can never fully solve (EVE's
  `openWindows` accumulates rather than reflecting real visibility — the
  measurement this item used to carry now lives in `windowLabels.ts`'s own
  comment above `isClutter`) into a one-click fix per window. _Added 2026-07-26;
  done 2026-07-26 (layout names-and-noise)._

### 0.10.0

- [x] **Resize layout windows from any corner.** All four corner handles, not
  just the bottom-right (`LayoutView.svelte`'s `.resize.tl/.tr/.bl/.br` and
  `layout.ts`'s `Corner`/`resizeRect`), and the coherent stack resize reuses
  them as planned. _Added 2026-07-15; shipped 2026-07-18 (`9998e40`), noticed
  still listed as pending 2026-07-28._

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
