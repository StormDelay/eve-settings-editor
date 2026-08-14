# Phase 4 — Overview consolidation, and the universal inspector rule

**Depends on:** [Phase 1](01-tokens-and-primitives.md) (tokens, `Button`, `Tabs`,
`Chip`, `ListRow` with `⋯`, `Panel`, `Sheet`, `Field`, `EmptyState`) and
[Phase 2](02-shell.md) (the shell's inspector slot). Changes layout and
affordances only — no document mutation changes, no new Tauri command.

**Changes behaviour?** Only where a control moves. Every backend call, every
argument, and every dirty-flag pairing stays exactly as it is today.

### What v0.34 changed

Re-verified against v0.34.0. **All 63 controls are still there** — PR #77 added
none and removed none; it changed only script, and only what happens to
`tabIndex` after a reorder. The `<select>`/chip-row duplication of §2.1 is
unchanged and the `<select>` is still the one to delete. Every
`OverviewView.svelte` citation below is re-anchored: #77 added 20 net lines of
script — the `keepSelection` block at `:211-220` plus edits inside `moveTab` and
`dropTab` — so everything from `moveTab` (`:184`) down shifted by up to +20 and
the whole markup shifted by exactly +20. `OverviewColumnsTab`,
`OverviewFiltersTab`, `OverviewAppearanceTab`, `WindowPanel`, `LayoutView` and
`+page.svelte` did not change; their citations hold as written.

Two things landed that this phase has to answer to:

1. **Reordering now renumbers the tab table**, because that is the only thing
   EVE reads. `renumber_to_strip_order` (`crates/settings-model/src/overview_tabs.rs:235-268`,
   called from reorder at `:519` and move at `:556`) rewrites the indices so the
   strip comes out ascending. §4.3 is rewritten around it: a tab's index is no
   longer a stable handle across a drag, and `keepSelection`
   (`OverviewView.svelte:211-220`) is the thing that copes. It must move into
   `OverviewTabList`, not be re-derived.
2. **A shipped, known ceiling: reordering two tabs swaps their column widths.**
   Per-tab widths are keyed `(overviewScroll2, tabIndex)` in the *character*
   file, so renumbering leaves them on the slot rather than on the tab
   (`overview_tabs.rs:262-268`, a `ponytail:` comment naming it as its own
   branch). The owner hit this in the live test. This phase makes reordering
   easier — every row draggable, cross-window drag added — so it makes the
   ceiling more likely to bite. §4.3.1 decides what the UI says about it. **The
   width remap itself is not specced here and must not be attempted here.**

The frontend baseline is now **37 test files / 1064 tests**, not 35/1040.

---

## 1. Goal

Two things, and the second is the reason the first is worth doing.

**Overview is the densest view in the app, and its density is duplication.** You
can select an overview tab in two separate controls that do the same job; a third
strip below them selects a sub-tab; a fourth strip lives inside Appearance. The
row above holds eight buttons, two of which appear and disappear as you move the
selection, so the row's membership — and therefore the position of every button
in it — changes under the cursor. The pack Import/Export buttons are wedged into
the sub-tab strip with a comment in the file admitting they are there "for layout
only" and then un-styling themselves to compensate
(`app/src/lib/OverviewView.svelte:571-575`). None of this is a taste problem. It
is one control's job spread across several controls, and the fix is to give each
job exactly one home.

**The home most of those jobs want is a right-hand pane, and the app does not
have one.** Only `LayoutView` has a right pane today
(`app/src/lib/LayoutView.svelte:951-998`), and the app-level right column is
occupied by `BackupsPanel` (`app/src/routes/+page.svelte:646-658`) — a panel with
no relationship to anything selected, which silently changes which file it is
about depending on which tab you are on. Phase 2 moves Backups into a History
popover. That frees the column. Phase 4 claims it, and states the rule that stops
it filling up with unrelated things again:

> **The right-hand pane shows what you can change about the current selection,
> and nothing that is not about the selection.**

That rule is what makes the Overview fix possible. "Move to window…", "Character
(for widths)", the colour swatch and the bold toggle are all properties of the
selected tab. They are in a toolbar because there was nowhere else to put them.

Success is: Overview has one tab picker; every control it has today is still
reachable and is somewhere a person would look for it; and the right side of the
window means the same thing in every view that has one.

---

## 2. Current state (evidence)

### 2.1 Three controls select an overview tab

`OverviewView.svelte:339-363` is a `<select>` labelled `Tab`, grouped with
`<optgroup>` per overview window plus an `Other` group for tabs that belong to no
window (`:354-358`). Options carry plain text only, because an `<option>` cannot
render EVE's tab-name markup — the file says so at `:348-349`.

`OverviewView.svelte:444-466` is a second picker: a draggable chip row (`.ov-tabs`)
that selects the same `tabIndex` (`:461-462`) and additionally reorders tabs
within the current window (`:449-457` → `dropTab`, `:224-238`). It renders each
tab's real colour and weight (`chipStyle`, `:95-99`) — the one place in the app
where a tab looks the way it looks in game.

The chip row only exists when the selected tab's window holds more than one tab
(`:444`). So on a single-tab window the reorder affordance vanishes entirely, and
on a windowless account it never appears at all — drag-reorder is silently
unavailable and nothing says why.

**Both write `tabIndex`, and since v0.34 a third thing does too.** The `<select>`
two-way binds it (`:340`, `bind:value`); the chip row assigns it (`:462`); and
`keepSelection` (`:217-220`) re-points it after every reorder or cross-window
move, because the backend renumbers the table under both. Three writers of one
piece of state across two controls that do the same job is the fault this
section documents, stated in code.

`OverviewView.svelte:472-483` is a third strip selecting Columns / Filters /
Appearance. `OverviewAppearanceTab.svelte:117-123` is a **fourth** strip inside
Appearance selecting Colortag / Background. Stacked, that is up to four rows of
tab-ish controls above the content.

### 2.2 The eight-control toolbar, two of which come and go

`.tab-actions` (`:364-412`) holds, in order:

| # | Control | Line | Rendered when |
| --- | --- | --- | --- |
| 1 | `+ New` | `:365` | always (disabled with no tabs) |
| 2 | `Rename` | `:366` | always (disabled with no selection) |
| 3 | `Delete` | `:367` | always (disabled with no selection) |
| 4 | colour swatch + palette popover | `:368-385` | always |
| 5 | `B` bold toggle | `:386-388` | always |
| 6 | `Move to window…` select | `:389-405` | **only** if the tab is in a window *and* there are ≥2 windows |
| 7 | `+ Window` | `:406-408` | **only** if there is ≥1 window |
| 8 | `Remove Window` | `:409-411` | **only** if the selected tab's window is the *last* one and there are ≥2 |

Controls 6–8 are conditional, and 8's condition depends on the *selection*, not
on the document. Selecting a tab in Overview 1 rather than Overview 2 removes a
button from the middle of the row and shifts everything after it. The row is
`flex-wrap: wrap` (`:509`), so at a typical window width it also reflows.

Control 6 resets itself to the empty placeholder on every change (`:395`,
`el.value = ""`). It is a `<select>` used as a menu — it never reports state, only
issues a command, and the "value" it shows is a permanent instruction rather than
the tab's actual window.

Control 8's condition is not arbitrary: the backend refuses to remove anything but
the last window (`crates/settings-model/src/overview_tabs.rs:669-671`,
`NotLastWindow`) and refuses to remove the only one (`:663-665`, `LastWindow`).
The rule is real. Hiding the button is how it is currently communicated.

### 2.3 Pack buttons wedged into the sub-tab strip

`OverviewView.svelte:479-482` puts `Import pack…` and `Export pack…` inside the
`.subtabs` container, then `:571-575` undoes the tab styling for them:

> The pack Import/Export buttons live inside `.subtabs` for layout only — they
> aren't tab selectors, so undo the flat tab styling above and look like the
> normal action buttons (`.tab-actions`) instead.

A container whose members must opt out of its styling is the wrong container.
(It is also an invalid ARIA tree: `.subtabs` carries `role="tablist"` at `:472`
and these two buttons are non-tab children of it.)

### 2.4 `Character (for widths)`

`OverviewView.svelte:437-442`. The label exists because column widths live in the
character file while everything else on the view lives in the account file — a
storage fact the user is being asked to hold in their head. It sits in the
toolbar, three controls away from the width inputs it governs
(`OverviewColumnsTab.svelte:149-151`). When the account has no paired characters
the selector is empty and a separate sentence elsewhere explains why
(`:467-469`).

That storage split is also what makes the width-swap ceiling of §4.3.1
invisible: the widths a reorder disturbs live in a file this control chose, and
nothing on the tab strip says the two are connected.

### 2.5 Local button styling

`OverviewView.svelte`'s `<style>` block styles buttons nine separate times:
`.pair button` (`:503-506`), `button.danger` (`:524`), `.ov-tabs button.tab-chip`
(`:537`), `.swatch` (`:540-544`), `.bold-toggle` (`:545-549`),
`.palette-grid button` (`:556`), `.palette-none` (`:558-562`),
`.subtabs button` (`:566-570`), `.pack-actions button` (`:574`). The proposal
counted four; the file has nine, still nine at v0.34. Another block re-declares
"dark native control" styling for selects and inputs (`:525-530`) — the same
block that appears in `OverviewFiltersTab.svelte:321-326` and
`OverviewColumnsTab.svelte:164-170`.

All of it is deleted by Phase 1's `Button` and `Field`. Phase 4 must not
reintroduce any of it.

### 2.6 The windowless account is a normal state

`OverviewView.svelte:104-126` and `:413-422`. EVE's own overview-pack importer
deletes the tab-to-window mapping, so any account that has ever imported a pack
through the client has none. `overview_create_window_mapping` writes a complete
one, which **replaces** EVE's own distribution and pins every tab into a single
window; the confirm at `:111-120` says so, including that the editor cannot undo
it because it cannot remove the last window. That warning is load-bearing and
must survive verbatim in meaning.

### 2.7 Layout's status line and hidden actions

`LayoutView.svelte:924-949` is one `<p class="ref">` containing, in sequence: the
reference resolution (a fact), a `Detail` checkbox (a view setting), a drag hint
(instruction, suppressed when read-only at `:930`), a "showing N of M windows"
counter with a `reset` link (`:933-942`), and an "N overridden" counter with a
`clear` link (`:943-948`). Five different kinds of thing in one sentence, set at
`#888` with a `#666` clause inside it (`:1191`, `:1199`) — APCA Lc 37 and Lc 21
against this background.

`WindowPanel.svelte:111-129` builds a per-row menu offering "Show geometry in
tree", "Copy window id", "Select on canvas" and one of three clutter-override
items. It is reachable only by right-clicking the row's name button (`:234`).
Nothing on the row advertises it. The coordinate and flag sub-fields have their
own menus (`:253`, `:264-268`) and at least carry `title="right-click for
actions"`; the row itself carries only `title={w.id}`.

### 2.8 What lives in the right column today

`app/src/app.css:23` makes the app shell a three-column grid; the third column is
`BackupsPanel` (`+page.svelte:646-658`), collapsible to a 24px rail. It is global,
not selection-scoped, and its subject flips with the view. Phase 2 replaces it
with a History popover, which is what frees the column for this phase.

The Raw tree suppresses right-click entirely (`+page.svelte:459-460`) with the
comment *"Tree actions take its place when we add them."* — a placeholder left
open for exactly the pane this phase defines.

---

## 3. Complete control inventory

Every control in the Overview view as it exists today, and where it goes. This
table is the no-functionality-lost proof. **63 controls, re-counted at v0.34 and
unchanged** — PR #77 touched only `moveTab`, `dropTab` and the new
`keepSelection`, none of which is a control. Exactly one ceases to exist, and
only because it is a duplicate of another entry in this same table (see the note
after the tables). Everything else is kept, moved, or restyled.

Line numbers are `app/src/lib/` unless the file is named in full, and are as of
**v0.34.0**.

### OverviewView.svelte — 28 controls

| # | Control | Today | New home | Note |
| --- | --- | --- | --- | --- |
| 1 | "Pair…" button on an unpaired character | `OverviewView:324-328` | Unchanged, as `EmptyState` with an action | Phase 1 primitive |
| 2 | "Open a character or account file" hint | `:330` | Unchanged, as `EmptyState` | |
| 3 | Backend error text | `:332` | `InlineMessage` (error) at the top of the view | |
| 4 | Shared-account banner | `:334` | `ScopeBanner` in the shell (Phase 2) | Same text |
| 5 | "This account file has no overview tabs" | `:335-336` | `EmptyState` inside the tab list panel | |
| 6 | **`Tab` grouped `<select>`** | `:339-363` | **Deleted — the tab list replaces it** | §4. The *only* deletion, and it is a duplicate of #23 |
| 7 | `+ New` (create tab) | `:365` | Tab list footer, `+ Tab` | §4 |
| 8 | `Rename` | `:366` | Inspector → Name field; also row `⋯` → "Rename" which focuses it | §5 |
| 9 | `Delete` | `:367` | Row `⋯` → "Delete tab" (+ right-click) | §5 |
| 10 | Colour swatch button | `:369-372` | Inspector → Colour | §5 |
| 11 | EVE 24-colour palette grid | `:376-379` | Inspector → Colour, in a `Popover` | Same `EVE_PALETTE` |
| 12 | "No colour" | `:381-382` | Inspector → Colour popover footer | |
| 13 | `B` bold toggle | `:386-388` | Inspector → Bold | §5 |
| 14 | `Move to window…` self-resetting select | `:391-404` | Inspector → **In window** `Field` (select) | Stops resetting; shows the real value |
| 15 | `+ Window` | `:407` | Tab list footer, `+ Window` | §4 |
| 16 | `Remove Window` | `:410` | Tab list **group header `⋯`** → "Remove this window" | Always present, disabled with a reason |
| 17 | Windowless explanation paragraph | `:414-419` | `InlineMessage` (info) at the top of the tab list | Same text |
| 18 | "Set up per-window tabs" | `:420` | That `InlineMessage`'s action **and** view `⋯` | §6. Confirm text unchanged |
| 19 | Inline name-entry input | `:425-430` | Tab list: inline `Field` on the new row (create tab / add window) | Rename arm moves to the inspector |
| 20 | Name-entry submit button | `:431-433` | Enter commits; button kept for pointer users | |
| 21 | Name-entry Cancel | `:434` | Escape cancels; button kept | |
| 22 | **`Character (for widths)`** select | `:437-442` | Inspector → **Widths from**, in the Widths section | §5 |
| 23 | Tab chip row: click to select | `:461-462` | Tab list rows | §4 |
| 24 | Tab chip row: drag grip / reorder | `:446-465` | Tab list rows, always draggable | §4, §4.3 |
| 25 | "No characters associated…" hint | `:467-469` | Empty state of the **Widths from** field | Next to what it explains |
| 26 | Columns / Filters / Appearance strip | `:472-478` | `Tabs` primitive, view header row | §6 |
| 27 | `Import pack…` | `:480` | View `⋯` menu | §6 |
| 28 | `Export pack…` | `:481` | View `⋯` menu | §6 |

### OverviewColumnsTab.svelte — 13 controls

| # | Control | Today | New home | Note |
| --- | --- | --- | --- | --- |
| 29 | `Copy columns…` | `OverviewColumnsTab:91-92` | Same place, `Button` | Opens a `Sheet` instead of an inline block |
| 30 | "Column order" checkbox | `:101` | Sheet, question 1 | Structure unchanged |
| 31 | "Visible columns" checkbox | `:102` | Sheet, question 1 | |
| 32 | "Widths" checkbox (+ no-character reason) | `:103-107` | Sheet, question 1 | |
| 33 | `Select all` | `:113` | Sheet, question 2 header | |
| 34 | `None` | `:114` | Sheet, question 2 header | |
| 35 | Per-target tab checkboxes, window-grouped | `:117-122` | Sheet, question 2 | Same grouping |
| 36 | `Copy to N tabs` | `:126-128` | Sheet footer, primary | |
| 37 | Cancel | `:129` | Sheet footer / Escape | |
| 38 | Column row drag grip | `:135-144` | `ListRow` grip | |
| 39 | Column visible checkbox | `:146` | `ListRow` leading control | |
| 40 | Column width number input | `:149-151` | `ListRow` trailing `Field` | Disabled reason now names the Widths-from field |
| 41 | "uses the account-default columns" note | `:155-157` | `InlineMessage` (info) under the list | Same text |

### OverviewFiltersTab.svelte — 13 controls

| # | Control | Today | New home | Note |
| --- | --- | --- | --- | --- |
| 42 | `Preset` select (Default/Your profiles) | `OverviewFiltersTab:211-225` | **Unchanged** — stays in Filters | §5.4 explains why it does not move to the inspector |
| 43 | `Duplicate preset` | `:227` | Unchanged, `Button` | |
| 44 | `Rename preset` | `:228` | Unchanged, `Button` | |
| 45 | `Delete preset` | `:229-231` | Unchanged, `Button` (danger) | |
| 46 | "Shows: X" title | `:236` | Unchanged, `PanelHeader` | |
| 47 | `Filter groups…` box | `:237` | `SearchField` | Gains a result count (Phase 1 primitive default) |
| 48 | Unrecognized-group checkboxes | `:242-249` | Unchanged, `InlineMessage` (warn) | |
| 49 | Category expand/collapse `<details>` | `:251-255` | Unchanged | The render-only-when-open optimisation at `:267` must survive |
| 50 | Category `All` | `:261-262` | Unchanged | |
| 51 | Category `None` | `:263-264` | Unchanged | |
| 52 | Per-group checkbox | `:270-274` | Unchanged | |
| 53 | Exception radios (Show / Hide / Always show) | `:287-292` | Unchanged | |
| 54 | Preset rename inline entry + buttons | `:298-308` | Unchanged | Local `pending` stays local |

### OverviewAppearanceTab.svelte — 9 controls

| # | Control | Today | New home | Note |
| --- | --- | --- | --- | --- |
| 55 | Six appearance boolean checkboxes | `OverviewAppearanceTab:105-114` | Unchanged | |
| 56 | "apply to ships and drones by default" note | `:107` | `InlineMessage` (info) | Same text, same position |
| 57 | Colortag / Background strip | `:117-123` | `Tabs` primitive (secondary variant) | Genuine two-surface switch — kept |
| 58 | "never customised… first change writes them" note | `:125-128` | `InlineMessage` (info) | Same text |
| 59 | State row drag grip | `:136-145` | `ListRow` grip | Tooltip "priority — first match wins" kept |
| 60 | State enabled checkbox | `:147-148` | `ListRow` leading control | |
| 61 | Background colour `<input type="color">` | `:154-157` | `ListRow` trailing control | `.unset` dimming kept — it distinguishes stored from default |
| 62 | `Reset` colour | `:159-160` | `ListRow` trailing `Button` (ghost) | |
| 63 | "default" note with its two tooltips | `:162-165` | `Chip` (mute) with the same tooltips | |

### The one deletion, stated plainly

Control **#6**, the `Tab` `<select>`, is the only control that ceases to exist. It
selects a tab; control #23 selects a tab; they write the same `tabIndex`. Every
capability the select had — grouping by window, an `Other` group for orphans,
working when a window holds one tab, working when there are no windows — is
carried by the tab list (§4), which is why the select can go and the chip row
cannot.

**Re-checked against v0.34, and the verdict is firmer than before.** The
`<select>` is the one that two-way binds `tabIndex` (`:340`), and `tabIndex` is
now a value the backend can invalidate mid-flight: a reorder renumbers the
table, so `keepSelection` (`:217-220`) rewrites `tabIndex` and the `<select>`
follows along by coincidence of binding. A control that reports a value the user
did not set, through a binding, in a list whose keys the backend renumbers, is
the more fragile of the two. The chip row assigns explicitly (`:462`) and
renders identity (`chipStyle`, `:95-99`), which is what a picker should do.

---

## 4. The tab list

One panel, on the left of the Overview view, replacing controls #6, #7, #15, #16,
#17, #18, #19, #23 and #24.

```
┌─ OVERVIEW ─────────────────────────────────────────────────────────────────┐
│ [ Columns ][ Filters ][ Appearance ]                                   [⋯] │
├──────────────────┬───────────────────────────────────┬─────────────────────┤
│ TABS             │  Columns · main                   │ TAB        [account]│
│                  │                                   │                     │
│ Overview 1   [⋯] │  ⠿ ☑ Name                  [180]  │ Name                │
│  ⠿ main          │  ⠿ ☑ Distance               [72]  │ [   main         ]  │
│  ⠿ Mining        │  ⠿ ☐ Corporation             [—]  │                     │
│                  │  ⠿ ☑ Velocity               [64]  │ Colour      Bold    │
│ Overview 2   [⋯] │                                   │ [▨ ▾]       [ B ]   │
│  ⠿ Travel        │  [ Copy columns… ]                │                     │
│                  │                                   │ In window           │
│ Other        [⋯] │                                   │ [ Overview 1    ▾]  │
│  ⠿ loose         │                                   │                     │
│                  │                                   │ WIDTHS   [character]│
│ [+ Tab] [+ Window]                                   │ Widths from         │
└──────────────────┴───────────────────────────────────┤ [ Baguette Comm… ▾] │
                                                       │ Column widths are   │
                                                       │ stored per character│
                                                       └─────────────────────┘
```

### 4.1 Structure

A `Panel` headed `Tabs`. Inside, one group per overview window in `data.windows`
order, each with a group header reading `Overview {index + 1}` — the exact label
the `<optgroup>` uses today (`OverviewView.svelte:345`) — and a trailing `⋯`.
Tabs listed in `w.tab_indices` order, each a `ListRow`.

Since v0.34 that order is also the in-game order rather than a picture of one:
`tab_indices` is guaranteed ascending because `renumber_to_strip_order` makes it
so, which `overview.rs:62-74`'s doc comment now states as the model's contract.
Render it as stored; do not sort it, and do not re-derive it from `data.tabs`.

Tabs belonging to no window go in a final `Other` group, matching `:354-358`.
Its `⋯` carries no window actions (there is no window to act on); it exists so
the group header is visually uniform, and holds only "New tab" with today's
`currentWindowIndex ?? 0` semantics (`:134-142`).

A windowless account renders **one ungrouped list, no headers**, preceded by the
`InlineMessage` of §4.5. There are no windows to name, so naming one would be a
lie.

### 4.2 Each row

`ListRow` with:

- a drag grip (`⠿`), always present — not conditional on the group holding more
  than one tab, which is today's rule at `:444`;
- the tab name rendered through `parseTabName`, with `color:cssColor(n.color)`
  and `font-weight:700` when bold — the same `chipStyle` logic at `:95-99`,
  which is the only truthful rendering of a tab in the app and must not be lost;
- a trailing `⋯`.

Selecting a row sets `tabIndex`. Selection styling comes from `ListRow`'s
selected variant, not from a border-colour override.

**Key the `{#each}` by tab index and accept that the key changes on a reorder.**
Today's chip row does exactly that (`:447`, `{#each cw.tab_indices as idx, i
(idx)}`) and it is correct: after `renumber_to_strip_order` the indices are new
numbers on the same rows, so Svelte tears down and rebuilds the list. That is
cheap for a handful of rows and it is the honest representation — the tabs
really did get renumbered. Do not invent a stable synthetic id to hide it; the
index is what every backend call takes.

**Row `⋯` (and right-click, which keeps working):**

| Item | Action | Disabled when |
| --- | --- | --- |
| Rename | Moves focus to the inspector's Name field and selects its contents | never |
| Delete tab | Today's `deleteTab` (`:169-183`) including the confirm and the `charOpen` dirty pairing (`:178-181`) | never |

Keep the confirm exactly as it is for now. Its "This can't be undone" wording is
false and Phase 5 owns that fix (`00-overview.md`, fault 5); changing the copy
here would fork the work.

**Group header `⋯`:**

| Item | Action | Disabled when |
| --- | --- | --- |
| New tab in this window | `tabCreate(w.index, …)` | never |
| Remove this window | Today's `removeWindow` (`:194-209`) | not the last window, or only one exists |

The disabled reasons come straight from the backend's own error cases: *"Only the
last overview window can be removed — EVE numbers windows by position"*
(`overview_tabs.rs:669-671`, message at `:66`) and *"This is the only overview
window"* (`:663-665`, message at `:62`). This is the "hiding becomes disabling
with a reason" rule from `00-overview.md`, and it is the direct fix for the
button that appears and disappears as you change tabs.

`ContextMenu.svelte`'s `MenuItem` is `{ label, run }` (`:2-5`) with no disabled
state. If Phase 1's `ListRow` overflow menu does not already supply one, extend
`MenuItem` with `disabled?: boolean` and `hint?: string` and render a disabled
`<button>` carrying `title={hint}`. That is a ~6-line change and the menu stays
flat — `ContextMenu` deliberately has no submenus (`:11-15`) and this phase does
not need any.

### 4.3 Drag

Within a group, drop calls `tabReorder(windowIdx, order)` — today's `dropTab`
(`:224-238`) unchanged.

Across groups, drop calls `tabMove(tabIdx, fromWindow, toWindow, pos)` with `pos`
being the drop index. `api.tabMove` already takes a target position
(`api.ts:474-475`); today's `moveTab` (`:184-193`) always appends. So
cross-group drag is roughly five lines more than same-group drag, and it removes
the obvious question the new layout creates — two windows are drawn one above the
other, so of course you will try to drag between them.

This is the one affordance Phase 4 *adds*. If it slips, drop it: the inspector's
**In window** field is the canonical route and is not optional. Dragging into the
`Other` group is not offered — there is no backend operation that un-assigns a
tab from every window.

**`keepSelection` moves with the handlers, verbatim.** v0.34's fix
(`OverviewView.svelte:211-220`) exists because a reorder or a move *renumbers
the tab table* — EVE draws a window's tabs in ascending index, so renumbering is
the only way an order reaches the game (`overview_tabs.rs:235-255`). The tab the
user was looking at therefore gets a new index, and a `tabIndex` left alone
comes to name a *different tab*. `keepSelection` re-points it by **position**:
whatever the backend now lists where the tab landed is that tab.

Three consequences for the tab list, all of them non-optional:

- Both handlers must compute the landing position *before* the call
  (`dropTab:230`, `const selectedAt = order.indexOf(tabIndex)`) and re-point
  *after* it, from the returned `data`. Re-deriving it from the pre-call
  snapshot is the bug the function was written to fix.
- `OverviewTabList` is presentational (§9), so `keepSelection` stays in
  `OverviewView` alongside the handlers and the list receives the corrected
  `tabIndex` as a prop. Do not give the list its own copy.
- A cross-group drop has the same problem and the same fix: `moveTab:189`
  already calls `keepSelection(toWindow, pos)`. The new drop handler passes the
  real drop index instead of the append position, and nothing else changes.

#### 4.3.1 The width-swap ceiling — surface it

Reordering two tabs **swaps their per-tab column widths and sort setting.**
Per-tab widths live in the *character* file keyed `(overviewScroll2, tabIndex)`,
so renumbering moves them with the slot rather than with the tab
(`overview_tabs.rs:262-268`). It is shipped, it is deliberate, and the owner hit
it in the live test. The machinery to fix it already exists —
`remap_tab_scoped_settings` (`:779-802`) does exactly this remap and is wired
into the *delete* path — but reorder does not call it, because an account's
other characters have their own char files the editor does not have open, so
remapping only the open one would be half a fix. **That fix is its own branch.
Do not attempt it here.**

**Decision: the redesign surfaces it, once, at the moment it happens.** Not a
standing warning, not a confirm.

The reasoning, in order:

1. **This phase owns the exposure, so it owns the disclosure.** Today reorder is
   a chip row that only appears when the selected tab's window holds ≥2 tabs
   (`:444`), inside a strip most users never drag. §4.2 makes every row
   draggable, §4.3 adds cross-window drag, and the list is drawn as an obvious
   sortable. A defect whose likelihood a design deliberately multiplies is a
   defect that design has to acknowledge. Staying silent would be shipping a
   better affordance for a worse outcome.
2. **The user cannot deduce it.** Widths are in a different file, chosen by a
   different control, three panes away (§2.4). Nothing on the tab strip connects
   the two. This is exactly the class of surprise the phase's own thesis says to
   remove.
3. **Silence has a worse failure mode than noise here.** The damage is silent,
   plausible-looking and easy to blame on yourself — a column set that looks
   slightly wrong after a drag reads as "I must have mis-set that", not as a
   known limitation. One sentence turns an unexplained defect into a
   documented one.

**What it is not.** Not a confirm dialog on every drag — that would gate the
affordance this phase exists to add, on a consequence that is often harmless
(a tab with no stored widths loses nothing). Not a standing banner — a warning
that is always on screen is furniture, and §8.1 already argues that a chip
appearing is a signal while a permanent clause is just a longer sentence. Not
disabling reorder when a character is open — that is deleting the feature to
avoid documenting it.

**Two places, one message, and they say different halves of it:**

| Where | What | Why there |
| --- | --- | --- |
| Inspector → **Widths** section helper text (§5.2) | Extend the existing sentence: *"Column widths are stored per character. Everything else on this tab is shared by the whole account. Reordering tabs moves widths with the position, not with the tab."* | It is the standing truth about the storage model, and it is already the one place in the redesign that explains the split. One sentence added to a sentence that exists. |
| A `Toast` after a reorder or cross-window move that actually changed the order | *"Tabs renumbered. Column widths stay with the position — check widths on the tabs you moved."* Gate on `charOpen`; with no character open there are no widths on screen to be wrong. | It is the only moment the message is actionable, and it is the moment the user is looking at the thing that changed. |

`Toast` is Phase 1's (`01-tokens-and-primitives.md` §5.12) and this phase
already depends on it; the gate is one boolean the view already holds
(`charOpen`). No new state, no new component, one call site in each drop
handler.

**Also log it in `docs/small-tasks.md`.** The ceiling currently exists only as a
`ponytail:` comment in a Rust file. The UI is about to reference a limitation
that is not in the ledger, and a spec that points at a comment nobody harvests
is how "its own branch" becomes "never".

### 4.4 Footer

`[+ Tab]` creates in the selected tab's window with today's exact fallback rule
(`:134-142`: a windowless account ignores the argument, and an orphan tab's new
sibling goes to window 0 rather than the button being dead). `[+ Window]` is
`overviewWindowAdd`, which asks for the first tab's name and dirties **both**
slots (`:154-166`) — that pairing is easy to lose in a refactor and there is a
test for it.

Both open an inline `Field` on a new row at the end of the relevant group, seeded
with today's placeholders (`"Tab name"` / `"First tab name"`, `:426`). Enter
commits, Escape cancels. `pending` (`:61-66`) loses its `renameTab` arm — that
moves to the inspector — and keeps `createTab` and `addWindow`, both of which are
genuinely "name the new thing".

### 4.5 The windowless account

Above the list, an `InlineMessage` (info) carrying today's paragraph verbatim
(`:414-419`) with "Set up per-window tabs" as its action. The message explains the
*shape of this list*, so it belongs against the list rather than in a toolbar.
`setUpWindowMapping`'s confirm (`:108-126`) is unchanged, including the sentence
about EVE's own importer removing the mapping again — that is the user's only
route back and deleting it would make the operation genuinely one-way.

---

## 5. The Overview inspector

A `Panel` in the shell's inspector slot, headed with the selected tab's plain
name. Two sections, each carrying a scope `Chip`, because "which file does this
write" is the single most persistent confusion in this view and the section
grouping can answer it for free.

### 5.1 Section `Tab` — chip `account`

| Field | Primitive | Writes | Replaces |
| --- | --- | --- | --- |
| Name | `Field` (text) | `tabRename(idx, formatTabName({...parts, text}))` | #8 |
| Colour | swatch `Button` + `Popover` (24 `EVE_PALETTE` swatches + "No colour") | `setNameFormat({ color })` (`:87-93`) | #10, #11, #12 |
| Bold | toggle `Button`, `aria-pressed` | `setNameFormat({ bold })` | #13 |
| In window | `Field` (select) | `tabMove(idx, from, to, pos)` | #14 |

**The Name field must keep typed spacing verbatim.** Padding is how a tab is
widened in game (`tabName.ts:38-42`), and today's rename path is explicit about
using `p.value` rather than the trimmed name (`OverviewView.svelte:145-153`).
`OverviewView.spec.ts:188-204` pins it. The field edits `parseTabName(name).text`,
commits on Enter and on blur, and writes through `formatTabName` so colour and
bold ride along untouched.

**A name the parser cannot decompose is never rewritten by being looked at.**
`parseTabName` returns the raw string as `text` with no colour when it meets
nesting it does not model (`tabName.ts:60`), and `OverviewView.spec.ts:178-186`
pins that no `tab_rename` fires. Binding a `Field` to a derived value is exactly
how that gets broken — the field must be seeded, not two-way bound.

**In window** replaces a `<select>` that showed a permanent instruction and reset
itself (`:395`). It shows the tab's actual window and changes it. For a tab in the
`Other` group it is disabled with the reason *"This tab isn't assigned to a
window — EVE decides where it appears"*, which is the truth today: `moveTab`
requires a source window (`:185`) and the select simply did not render (`:389`).

**It also has to survive the renumbering.** `moveTab` re-points `tabIndex`
through `keepSelection` (`:189`), so after a move the inspector must still be
showing the same *tab*, now under a different index. Read the field's value from
the tab the corrected `tabIndex` names, never from a value captured before the
call. This is the same trap as the Name field below, from the other end.

### 5.2 Section `Widths` — chip `character`

| Field | Primitive | Writes | Replaces |
| --- | --- | --- | --- |
| Widths from | `Field` (select of `characters`) | `onLoadCharacter(id)` | #22 |

Helper text under it, three sentences: *"Column widths are stored per character.
Everything else on this tab is shared by the whole account. Reordering tabs moves
widths with the position, not with the tab."* The first two are what
`Character (for widths)` was trying to say in three words; the third is §4.3.1's
standing half, and it belongs here because this is the only place in the redesign
that already explains the storage split.

When `characters.length === 0` the field is replaced by its empty state — today's
sentence at `:467-469` plus a "Pair a character…" action calling `onShowAccounts`,
matching the pairing offer at `:324-328`. One message, at the control it is about,
instead of a loose paragraph three controls away.

**Why the section is always present rather than shown only under the Columns
sub-tab.** Conditional membership is the fault being fixed; a pane whose fields
appear and disappear as you switch sub-tabs is the toolbar problem moved to the
right. Grouping under a headed section with a `character` chip gets the proximity
the proposal asked for without the flicker.

### 5.3 Empty state

No tab selected — reachable when an account has tabs but the selection was
invalidated — shows an `EmptyState`: *"Select a tab to edit its name, colour and
window."*

### 5.4 What deliberately does **not** move to the inspector

**The preset selector** (`OverviewFiltersTab.svelte:211-225`). It is per-tab, so
by the letter of the rule it qualifies. It stays where it is because it sits
directly above the group checklist it controls, and the checklist is
unintelligible without knowing which preset is being edited — `Shows: {preset}`
at `:236` exists to restate it. Mirroring it in the inspector would create a
second control for one job, which is the exact fault this phase exists to remove.
The inspector holds the tab properties that have **nowhere else to live**; the
preset has a good home already.

**Anything from Appearance.** Appearance is account-scoped, not tab-scoped
(`OverviewAppearanceTab.svelte` takes no `tabIndex`). Putting it in a pane headed
with a tab's name would be false.

### 5.5 `Copy columns…` becomes a `Sheet`

`OverviewColumnsTab.svelte:94-132` already asks the two questions in the right
order — *what to copy* (`:98-109`), then *where to* (`:110-124`) — with a rule
between them (`:182-183`). That structure is correct and is kept exactly. Only
the container changes: an inline block that pushes the column list down becomes a
`Sheet`, so the list stays put and the panel gets a real dismiss.

The comment at `:35-38` says the panel is inline "because the app has no modal".
Phase 1 gives it one. The reasoning that *ticking the targets by hand is the
confirmation step, so there is no second confirm* still holds and no confirm is
added.

`copyWidths` must keep gating on `charOpen` (`:67`) and `runCopy` must keep
dirtying both slots conditionally (`:82-83`) — order/visible dirty the account,
widths dirty the character, and losing the second one silently drops the copied
widths on save.

---

## 6. The `⋯` menu

The view header row is `Tabs` (Columns / Filters / Appearance) with a trailing
`⋯`, matching the shell's own view-level `⋯` from Phase 2 so the position is
learned once.

```
[ Columns ][ Filters ][ Appearance ]                                     [⋯]
                                              ┌────────────────────────────┐
                                              │ Import overview pack…      │
                                              │ Export overview pack…      │
                                              │ ────────────────────────── │
                                              │ Set up per-window tabs…    │
                                              └────────────────────────────┘
```

| Item | Action | Disabled when |
| --- | --- | --- |
| Import overview pack… | `importPack` (`OverviewView.svelte:255-296`) | `packBusy` |
| Export overview pack… | `exportPack` (`:298-314`) | `packBusy` |
| Set up per-window tabs… | `setUpWindowMapping` (`:108-126`) | `data.windows.length > 0` — *"This account already assigns tabs to windows"* |

These three are account-wide and rare, which is what makes them progressive-
disclosure candidates; unlike right-click, a `⋯` is visible.

Both pack flows keep every step: the `documentDir()`-seeded picker at
`Documents/EVE/Overview` (`:247-253`), the preview-then-confirm listing sections
and ignored keys (`:265-279`), the conditional "per-tab column overrides are
discarded" note that only fires for a non-empty `tabSetup` section (`:272-274`),
and the post-import fallback that reselects the first tab when the pack replaced
the tab set (`:283-287`).

**Set up per-window tabs has two entry points on purpose.** The `InlineMessage`
of §4.5 carries the explanation and needs its action next to it; the `⋯` item is
the always-visible home so the command is not invisible once the message is gone.
Two routes to one *command* is fine. Two controls for one *selection* — which is
what §2.1 documents — is not.

`.pack-actions` (`:479-482`) and its style block (`:571-575`) are deleted. That
also fixes the ARIA tree: `.subtabs` keeps `role="tablist"` and stops holding
two non-tab children.

---

## 7. The universal inspector rule, across all views

> **The right-hand pane shows what you can change about the current selection,
> and nothing that is not about the selection.**

The rule is as much a prohibition as a promise. Today's violation is not that
some views lack a pane — it is that the app's right column holds `BackupsPanel`
(`+page.svelte:646-658`), which belongs to no selection and silently changes
which file it is about. Phase 2 removes it. This phase decides who may fill the
column it frees.

**A view is not required to have an inspector.** Requiring one produces panes
that say "nothing selected" forever, which teaches users the pane is broken and
costs the view horizontal space it wanted. The contract that matters is *within* a
view: **a view that declares an inspector always renders it**, showing an
`EmptyState` when nothing is selected, and never removes it mid-session. Across
views the whole screen changes anyway.

| View | Inspector? | Contents | Reasoning |
| --- | --- | --- | --- |
| Layout | Yes — exists | `HudPanel` + `WindowPanel` | §7.1 |
| Overview | Yes — new | Selected tab's properties | §5 |
| Raw | Yes — new | Selected node's properties | §7.2 |
| Autofill | No | — | §7.3 |
| Keybinds | No | — | §7.4 |
| Probes | No | — | §7.5 |

### 7.1 Layout — complies, with a noted variant

`HudPanel` is selection-coupled already: it takes `selectedKind` and highlights
the group matching the furniture selected on the canvas
(`HudPanel.svelte:22-25`), and `onSelectKind` makes the coupling bidirectional.
`WindowPanel` is a list, but each row expands to the selected window's own
coordinates, flags and chat splits (`WindowPanel.svelte:249-296`) — the
selection's properties, rendered inside the list rather than beside it.

That is a legitimate variant for a view whose selection is a rectangle on a
canvas: you need the list to *find* the window, and a separate pane would mean
two places to look. Phase 4 does not re-architect a 765-line component to make a
diagram tidier. Changes are limited to §8.

### 7.2 Raw — gains an inspector, and it is the strongest case in the app

The tree has no selection today, and its per-node metadata is either invisible or
hover-only. `TreeNodeData` carries `path`, `kind`, `display`, `edit_text`,
`editable`, `removable`, `in_shared` (`api.ts:14-24`); the row shows `display`,
encodes `kind` as a text colour (`TreeNode.svelte:107` + `app.css:90-94`),
compresses `in_shared` to a single `&` glyph (`:113-114`), and exposes the
add / remove / reveal buttons at `opacity: 0` until the row is hovered
(`app.css:96-97`). Right-click is suppressed outright (`+page.svelte:459-460`)
with the comment *"Tree actions take its place when we add them."*

So: single click selects a node (double-click still opens the inline editor,
`TreeNode.svelte:71-75`). The inspector shows label, full `path`, `kind` spelled
out as a word, the raw `edit_text` alongside the rendered `display`, which file
the node is in (`treeFile`, `+page.svelte:46`), the `in_shared` warning as an
`InlineMessage` rather than a glyph, and the three row actions as real buttons.

This is a genuine feature, not a redesign side effect — it makes the raw path
copyable and the shared-object warning legible for the first time. If Phase 4's
budget is tight, this is the part to defer; §12 marks it separately.

### 7.3 Autofill — no inspector

There is no selection to inspect. The only per-item state is a drag handle
(`AutofillView.svelte:58`); entries are bare strings in
`RememberedList { widget, entries }` (`api.ts:288-291`), every input is always
live and commits on change (`:127-128`), and the raw widget path is already on
the row (`:109`). There is nothing hidden and nothing to select. Inventing a
selection so the pane has something to show would be building a control to fill a
space.

### 7.4 Keybinds — no inspector

Its only selection is transient: `listening` (`KeybindsView.svelte:14`), cleared
on commit, Escape and reload (`:68`, `:75`, `:20`). More importantly the view
already has the right affordance in the right place — a sticky capture bar pinned
to the bottom edge (`:146-151`), next to the table row being armed rather than
across the window from it. Moving it into a right pane would increase the
distance between the key you press and the feedback you read.

Keybinds does have real hidden content: `stolen` (`:63`) surfaces as a "taken by
X" span (`:128-131`) that any reload wipes, and the Default column is
`opacity: .5` over already-dim text. Those are content and token fixes owned by
Phases 1 and 5, not reasons to build a pane.

### 7.5 Probes — no inspector

It already satisfies the rule under different names. `aside.formation-list`
(`ProbeFormationsView.svelte:479`) is the selection rail, `section.formation`
(`:507`) is the editor for `selectedId` (`:18`), and `ProbeViewer` is a canvas
with its own selection, `selectedProbe` (`:57`), synchronised in both directions
(`:548`, `:622`, `ProbeViewer.svelte:474-477`).

A third pane would take X/Y/Z, distance, azimuth, elevation and per-probe range
out of the table (`:552-585`). Those are only useful read down a column across
all eight probes; one probe's values in isolation answer nothing.

### 7.6 Where the pane comes from

Phase 2 owns the slot. Overview supplies content through whatever contract Phase 2
defines — most likely a named snippet or an `{#snippet inspector()}` prop.

**Phase 4 must remain shippable if Phase 2 slips.** Fallback: `OverviewView`
renders its own three-column grid, mirroring `LayoutView.svelte:1007-1014`:

```css
display: grid;
grid-template-columns: minmax(14rem, 18rem) minmax(0, 1fr) minmax(14rem, 20rem);
```

Same bounded-side-columns reasoning as Layout's `minmax(0, 1fr)` centre. When
Phase 2 lands, the local grid is deleted and the inspector content is handed to
the slot. Build the inspector as its own component (`OverviewInspector.svelte`)
from the start so that hand-off is a move, not a rewrite.

---

## 8. Layout — status bar and row overflow

### 8.1 A real status bar

`LayoutView.svelte:924-949` becomes two groups under the canvas.

```
┌─ canvas ───────────────────────────────────────────────────────────────┐
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
  1920 × 1080 reference    ☑ Detail          (12 of 34 windows ×) (3 overridden ×)
  └──── facts and view settings ────┘        └──── dismissible Chips ────┘
```

**Left — what is true and what you are looking at.** Reference size as
`--text-muted` (not `#888`, `:1191`), and the `Detail` checkbox as a labelled
toggle at `--text-secondary` (not `#888`, `:1203-1207`). Both are stable; neither
appears or disappears.

**Right — what is currently *narrowing* your view.** Two `Chip`s in the warn tone
that `.showing` already uses (`:1196`), each with a dismiss `×` running today's
handler:

- `12 of 34 windows` — `×` resets to `DEFAULT_FILTER` (`:940`). Keep the comment's
  reasoning: reset goes back to the *default*, which hides clutter, not to
  everything.
- `3 overridden` — `×` calls `clearClutterOverrides(documentWindowIds)` (`:946`).

A chip is the right shape because both are exceptional states with an escape, and
both vanish when there is nothing to say — a chip appearing is a signal, whereas a
clause appearing inside a sentence just makes the sentence longer.

**The drag hint moves out.** `· Shift-drag onto another window to stack · drag a
tab to reorder or pull out` (`:931`) is instruction, not status. It becomes the
body of the Layout inspector's no-selection `EmptyState` — which has to say
something anyway, and "what can I do here" is exactly the right thing for it to
say. Keep its `readOnly` gate (`:930`): none of it is true on a read-only file.

That empties `.hintish` (`:1198-1200`, the `#666`/Lc 21 rule) and it is deleted.

### 8.2 A visible `⋯` on every row

`WindowPanel.svelte` gains a `⋯` `Button` (ghost) at the end of `.row-head`
(`:306`, `:416`, `:453`), on `.stack-head` (`:437`) and on `.fam-head` (`:496`),
opening the same `rowMenu(w)` (`:111-129`) the right-click opens. `aria-haspopup="menu"`.
Right-click on `.name` (`:234`) keeps working — the `⋯` adds a route, it does not
replace one.

Position it from the button's bounding rect rather than a pointer event.
`ContextMenu` already clamps to the viewport from a measured rect (`:46-55`), so
this needs no new positioning logic.

**The coordinate and flag sub-fields keep right-click only.** Their menus target a
specific field's tree path (`:253`, `:131-136`), so a `⋯` per field would put six
more buttons on an expanded row and drown the row action. They already carry
`title="right-click for actions"` (`:253`, `:267`), which is a weak affordance but
a real one — unlike the row, which carried none. The row `⋯` also covers the
common case: it already offers "Show geometry in tree" (`:113-116`).

**The canvas right-click stays a right-click.** `LayoutView.svelte:244-257` lists
the rectangles stacked under a point, a disambiguation gesture on a canvas rather
than a row action. There is no row to hang a `⋯` on and no other way to express
it. Recorded here so it does not read as an oversight.

---

## 9. File-by-file change list

| File | Change |
| --- | --- |
| `app/src/lib/OverviewView.svelte` | Becomes a three-pane shell. Delete the `Tab` `<select>` (`:339-363`), `.tab-actions` (`:364-412`), `.ov-tabs` (`:444-466`), `.pack-actions` (`:479-482`) and the entire `<style>` block. Keep `reload`/`$effect` (`:24-39`), `newestTab` (`:52-55`), **`keepSelection` (`:211-220`)**, all mutation handlers, and every `onUserDirty`/`onCharDirty` pairing. `pending` loses its `renameTab` arm. |
| `app/src/lib/OverviewTabList.svelte` | **New.** §4. Props: `data`, `tabIndex`, callbacks for create / reorder / move / delete / add window / remove window / set-up-mapping. Presentational — no `api` import, no `keepSelection` copy, matching `WindowPanel`'s shape. |
| `app/src/lib/OverviewInspector.svelte` | **New.** §5. Props: `tab`, `windows`, `currentWindowIndex`, `charId`, `characters`, callbacks. Owns the colour popover, so `swatchOpen`/`swatchEl` and the outside-click handler (`:317-322`) move here. |
| `app/src/lib/OverviewColumnsTab.svelte` | Copy panel (`:94-132`) becomes a `Sheet`; column rows become `ListRow`; delete the local `<style>`. Logic unchanged. |
| `app/src/lib/OverviewFiltersTab.svelte` | Primitives only: `Field`, `Button`, `SearchField`, `InlineMessage`. Delete the local `<style>`. Preserve `isOpen`/`noteToggle` (`:66-71`) and the render-only-when-open guard (`:267`) — both are load-bearing performance work with comments explaining why. |
| `app/src/lib/OverviewAppearanceTab.svelte` | Primitives only; the local `.subtabs` becomes `Tabs`. Delete the local `<style>`. |
| `app/src/lib/ContextMenu.svelte` | Add `disabled?: boolean` and `hint?: string` to `MenuItem` (`:2-5`) and render disabled items as inert buttons with `title={hint}` — **only if** Phase 1's `ListRow` menu does not already provide it. |
| `app/src/lib/WindowPanel.svelte` | `⋯` on `.row-head`, `.stack-head`, `.fam-head` (§8.2). No change to `rowMenu`/`flagMenu`. |
| `app/src/lib/LayoutView.svelte` | Replace `<p class="ref">` (`:924-949`) with the status bar; move the drag hint into the inspector empty state; delete `.ref`, `.hintish`, `.det`, `.hint` colour rules (`:1190-1224`). |
| `app/src/lib/TreeNode.svelte`, `app/src/routes/+page.svelte` | Raw inspector (§7.2): a `selectedPath` state, click-to-select on the node, and `RawInspector.svelte`. Separable — see §12. |
| `app/src/lib/RawInspector.svelte` | **New**, with §7.2. |

Nothing under `app/src-tauri/` or `crates/` changes. No `api.ts` signature
changes. Every command called after this phase is a command called before it.
That includes the width remap of §4.3.1 — the fix lives in `overview_tabs.rs`
and is out of scope by construction, which is the other reason this phase can
only *say* something about it.

One documentation-only edit outside `app/src`: log the width-swap ceiling in
`docs/small-tasks.md` (§4.3.1).

---

## 10. Tests

vitest + `@testing-library/svelte`, beside the component. The existing suites are
the specification of what must not break.

### Must keep passing unchanged in intent

`OverviewView.spec.ts` gating tests (`:57-92`) are untouched by this phase. The
tab-selection tests (`:94-125`) and the markup tests (`:130-205`) query
`getByLabelText("Tab")` — the deleted `<select>` — so their **queries** change
while their **assertions** must not. Rewrite the queries against the tab list and
inspector; if an assertion needs weakening to pass, that is a regression, not a
test-maintenance task. Specifically:

- `:140-153` — picking a colour writes
  `<color=0xFF40FF40><b>   main   </b></color>`: padding preserved, bold
  preserved. Same string from the inspector swatch.
- `:155-164` — clearing the colour drops only the span.
- `:166-174` — `B` toggles bold off, keeping colour and padding.
- `:178-186` — an unparseable name fires **no** `tab_rename` and shows no
  colour. This one gets more fragile in a `Field`, not less: assert
  `calls.never("tab_rename")` after mount *and* after focusing and blurring the
  Name field without typing.
- `:188-204` — rename keeps colour and typed spacing.

`OverviewColumnsTab.spec.ts` — the width-field gating (`:51+`) and copy-panel
grouping tests must pass against the `Sheet` with query changes only.

### New

**`OverviewTabList.spec.ts`**

1. Tabs are grouped by window, in `tab_indices` order, under `Overview {n+1}`
   headers.
2. Orphan tabs appear under `Other` (fixture: `OverviewColumnsTab.spec.ts:38-47`
   already has one).
3. A windowless account renders one ungrouped list with no headers, plus the
   explanation message.
4. A tab's colour and bold reach the DOM — assert the inline style, since this is
   the one place the app renders them truthfully.
5. Every row is draggable, **including in a group of one** — the direct
   regression test for `:444`.
6. Drop within a group calls `tab_reorder` with the new order.
7. Drop into another group calls `tab_move` with the source window, target window
   and drop index.
8. `Remove this window` is present but disabled on a non-last window, and its
   hint names the reason. Present-and-disabled, never absent.
9. `+ Window` dirties **both** slots (`onUserDirty` and `onCharDirty`) and calls
   `onWindowAdded` with `overview` for window 0 and `overview_{n}` otherwise
   (`:160-166`).
10. **The selection follows the tab across a renumbering** — the v0.34
    regression. Reorder a strip whose backend response renumbers the indices,
    and assert the still-selected row is the *same tab* (by name), not the same
    index. Assert it for the same-group drop and the cross-group drop
    separately: `moveTab` and `dropTab` compute the landing position
    differently and only one of them is covered by the other's test.
11. After a reorder or cross-window move **with a character open**, the
    width-swap toast fires once; with no character open it does not (§4.3.1).

**`OverviewInspector.spec.ts`**

1. Name seeded from `parseTabName(...).text`, padding included.
2. Committing an unchanged name fires no `tab_rename` (mirrors the preset-rename
   no-op rule at `OverviewFiltersTab.svelte:167`).
3. `In window` shows the tab's real window — not a placeholder — and does not
   reset after a change. The direct regression test for `:395`.
4. `In window` is disabled with a reason for an orphan tab.
5. `Widths from` lists `characters` and calls `onLoadCharacter`.
6. With `characters: []`, the field shows its empty state and the pairing action
   calls `onShowAccounts`.
7. No tab selected → `EmptyState`, and no backend call.
8. The Widths helper text carries all three sentences, including the
   reordering one (§4.3.1) — a plain text assertion, and the cheapest possible
   guard against the ceiling's disclosure being dropped in a later tidy.

**`OverviewView.spec.ts` additions**

1. The `⋯` menu offers Import, Export and Set up per-window tabs.
2. Set up per-window tabs is disabled when `windows.length > 0`.
3. Import and Export are disabled while `packBusy`.
4. After an import that replaces the tab set, `tabIndex` falls back to the first
   tab (`:283-287`).

**`WindowPanel.spec.ts` additions**

1. Each row exposes a `⋯` whose items equal the right-click menu's items for the
   same row.
2. Clicking `⋯` then "Treat as clutter" calls `onClutterOverride(id, "clutter")`.

**`LayoutView.spec.ts` additions**

1. With a filter active, a chip reads `N of M windows` and its dismiss restores
   `DEFAULT_FILTER`.
2. With overrides present, a chip reads `N overridden` and its dismiss calls the
   clear handler.
3. Neither chip renders when its count is zero.

### Not covered by tests

Drag-and-drop across groups is asserted through the synthetic `dragstart` /
`drop` events the existing tests already use, which exercises the handler but not
WebView2's real drag. The `dataTransfer.setData` call in `dragstart` is required
by WebView2/Chromium or `drop` never fires (`:450-453`,
`OverviewColumnsTab.svelte:136-139`, `OverviewAppearanceTab.svelte:138-140`) —
carry it into every new drag handler and verify by hand once in the running app.

The width swap itself is not tested here, because this phase does not change it.
Its behaviour is the backend's and is covered on the Rust side
(`overview_tabs.rs`'s `renumbering_keeps_the_order_each_window_shows_its_tabs_in`
at `:1623` and the `remap_tab_scoped_settings` tests around `:1700-1780`). What
Phase 4 tests is only that the UI *says* the right thing (tests 8 and 11 above).

---

## 11. Risks and rollback

**The dirty-flag pairings are the highest-consequence thing in this diff.** Three
operations write both files: add window (`:154-166`), remove window
(`:194-209`), and delete tab, which renumbers tabs and carries the open
character's per-tab widths and sort setting across with them (`:169-183`,
comment at `:178-181`, backed by `remap_tab_scoped_settings` in
`overview_tabs.rs:779-802`). Copy-columns has the same shape
(`OverviewColumnsTab.svelte:82-83`). Miss one and the work is silently dropped on
save with no error. *Mitigation:* move the handlers verbatim, never retype them,
and assert both callbacks in the tests above.

**`keepSelection` is the second thing that must move verbatim.** v0.34 added it
(`:211-220`) because the backend renumbers the tab table on every reorder and
move; the tab list makes both operations far more common. Retyping it, or
re-deriving the landing position from the pre-call snapshot, silently re-points
the selection at a neighbour — the same class of bug as the dirty flags, with a
louder symptom. *Mitigation:* §10's `OverviewTabList.spec.ts` test 10 asserts
selection by tab *name* across a renumbering, for both handlers.

**The tab-name field is easy to break subtly.** Two-way binding a `Field` to a
derived `parseTabName` result would rewrite unparseable names on mount, and
trimming would silently resize the user's overview. *Mitigation:* seed the field,
commit explicitly, and keep `OverviewView.spec.ts:178-204` as the guard.

**Reordering is index-based and the backend renumbers.** `newestTab` (`:52-55`)
exists because diffing index sets against a pre-call snapshot does not survive
gap-compaction, and the comment explains exactly how that fails. Do not
reimplement it while moving it. `compact_tabs`
(`crates/settings-model/src/overview_tabs.rs:184-233`) is the code it is
defending against; `renumber_to_strip_order` (`:235-268`) is the v0.34 sibling
that `keepSelection` defends against.

**The width swap gets worse before it gets better, and that is accepted.**
§4.3.1 decides to say so rather than to fix it or hide it. The risk this leaves
open is that a user reads the toast, does nothing, and finds their widths wrong
later anyway — which is strictly better than today, where nothing is said at
all. *Mitigation:* the ledger entry, so the real fix has a home.

**Phase 2 slipping.** Handled by §7.6's local-grid fallback.

**Phase 1 slipping.** This phase cannot start without it: it deletes four
view-local `<style>` blocks — including the nine separate button rules counted in
§2.5 — and replaces them with primitives. If Phase 1 is not done, stop. Do not
hand-roll a `Sheet`.

**Rollback.** Two commits: (a) the Overview restructure, (b) the Layout status
bar and row `⋯`. Both are frontend-only and revert cleanly. §7.2's Raw inspector
is a third, independently revertable commit. No migration, no file-format change,
nothing written to disk differently.

---

## 12. Definition of done

**Overview**

- [ ] Exactly one control selects an overview tab. `getByLabelText("Tab")` finds
      nothing.
- [ ] The tab list groups by window, shows `Other` for orphans, renders one
      ungrouped list for a windowless account, and shows each tab's real colour
      and bold weight.
- [ ] Every row is draggable, including in a group of one.
- [ ] Drop within a group reorders; drop across groups moves. *(Droppable — the
      In window field is not.)*
- [ ] The selection survives the backend's renumbering: after a reorder or a
      cross-window move, the selected row is the same *tab*, not the same index.
      `keepSelection` was moved, not rewritten.
- [ ] The width-swap ceiling is surfaced exactly twice and no more: the third
      sentence of the Widths helper text, and a `Toast` after a reorder or move
      when a character is open. No confirm dialog, no standing banner, no
      disabled drag. Logged in `docs/small-tasks.md`.
- [ ] `git diff crates/settings-model` is empty — the width remap was **not**
      attempted here.
- [ ] `Remove this window` is always present in the group header `⋯`, disabled
      with a reason where the backend would refuse.
- [ ] The inspector holds Name, Colour, Bold, In window, Widths from — and the
      `In window` field shows the real window and does not reset itself.
- [ ] `Character (for widths)` no longer appears as a label anywhere.
- [ ] The `⋯` menu holds Import pack, Export pack and Set up per-window tabs; the
      windowless explanation and its warning confirm are unchanged in meaning.
- [ ] `Copy columns…` is a `Sheet` and still asks *what* before *where*.
- [ ] All 63 controls in §3 are reachable in the running app.

**The inspector rule**

- [ ] Layout, Overview and Raw have an inspector; Autofill, Keybinds and Probes
      declare none and get the full width.
- [ ] A view that has an inspector never removes it — it shows an `EmptyState`.
- [ ] `BackupsPanel` is not in the right column. *(Phase 2 delivers this; verify
      it, do not re-do it.)*
- [ ] §7.2's Raw inspector is the one item that may be deferred. If deferred,
      log it in `docs/small-tasks.md` and say so — a rule with an unexplained
      exception is worse than a smaller rule.

**Layout**

- [ ] The status line is a status bar: facts and Detail left, dismissible count
      chips right, no drag hint.
- [ ] `#666` and `#888` do not appear in `LayoutView.svelte`.
- [ ] Every window, stack and family row has a visible `⋯` with the same items as
      its right-click menu, and right-click still works.

**Global**

- [ ] `npm test` passes; the frontend suite has grown by at least the files in
      §10 (baseline **37 files / 1064 tests** at v0.34.0).
- [ ] `npm run check` is clean — `svelte-check --fail-on-warnings`.
- [ ] No `<style>` block in any touched file declares a button, a select, an
      input, or a colour literal.
- [ ] `git diff --stat app/src-tauri crates` is empty.

_Added 2026-08-13 (UI/UX redesign proposal, Phase 4)._

---

## 13. What actually shipped, and where it differs

**Read this before trusting any `file:line` or any claim in §§1–12.** Phase 4 was
built on 2026-08-14 on `feat/ui-redesign-phase-4`, in the three commits §11
plans. Written against v0.34; Phases 1–3 have moved things since, and the spec
was wrong about several of them.

**Baseline.** §§ throughout cite "37 test files / 1064 tests". That was v0.34.
The real pre-phase baseline was **55 files / 1259 tests** and the phase ends at
**59 files / 1334 tests**. `npm run check` clean over 500 files, `npm run build`
clean, and `git diff origin/master -- app/src-tauri crates` empty.

**Phase 1 had already done more than §2 says.** The `<style>` blocks §2.5 counts
nine button rules in were already tokenised and mostly gone; `.pack-actions` was
already out of the tablist (Phase 1 moved it beside, with a comment naming this
phase); the shared-account banner was already the shell's; and every control was
already a `Button`/`Field`. What was left for Phase 4 was structural, not
cosmetic — which is the same lesson Phase 3 recorded.

### Load-bearing divergences

1. **§7.6's slot contract does not exist, so Overview took Layout's route.**
   Phase 2 shipped an always-present `<aside class="inspector">` holding a
   placeholder, and no snippet or slot API. `OverviewView`'s root is
   `display: contents` with exactly two children, exactly as `LayoutView` does
   it — no portal, no prop hoisting. Consequences: `scopeLabel` became a prop
   (Overview renders the shell's banner itself), `+page.svelte` grew a
   `viewOwnsInspector` derived, and Overview is now pinned by the same
   shell-grid-contract test LayoutView has. §7.6's local-grid fallback was not
   needed and was not built.

2. **The DoD's "Autofill, Keybinds and Probes … get the full width" is NOT met,
   deliberately.** Phase 2 shipped the inspector column on *every* tab with a
   written rationale — a column present on one tab and gone on the next is the
   same fault as a tab strip that changes membership — and §7's rule contradicts
   it. Two merged positions, one owner decision; the shipped one was left
   standing rather than re-litigated in a phase that does not own it. Everything
   else in §7 holds: Layout, Overview and Raw have real inspectors, and the
   other three still show the shell's placeholder.

3. **The inspector's hide control was missing on Layout and would have gone
   missing on Overview too.** The shell's `»` lives in the aside those two views
   replace, so the column could be reopened from every tab but closed only from
   the four that have the shell's own. Both views render it now and
   `.inspector-head` moved to `app.css`, where the three components that draw
   into that column can all reach it. Not in the spec; it is a regression this
   phase would otherwise have doubled.

4. **Cross-window drag can move a tab that is not the selected one, which
   `moveTab` never could.** `keepSelection` itself moved verbatim, as §4.3 and
   §11 demand. What changed is its *call*: both handlers now compute the
   selection's position in the affected strip **before** the call and re-point
   from the returned data after it, instead of assuming the moved tab is the
   selected one. This is sound because `renumber_to_strip_order` redistributes
   only the renumbered window's own index slots
   (`overview_tabs.rs:249-251`) — a selection in any other window is untouched.

5. **A drop in place is no longer an edit.** Today's `dropTab` called the backend
   even when the order was unchanged. §4.3.1 asks for a toast after a reorder
   "that actually changed the order", so the guard had to exist; it also stops a
   no-op drag dirtying the file.

6. **`.stack-head` and `.fam-head` get no `⋯`.** §8.2 asks for one. They are
   *group* heads with no `WindowRect` behind them and no right-click menu today,
   so there is nothing to reveal — a menu there would mean inventing commands,
   which this phase does not do. All three `.row-head` sites do have one.

7. **The row `⋯` lives inside `rowHead`, not at the end of each `.row-head`.**
   Three call sites render `rowHead`, and two of them append their own trailing
   controls (a count chip, the stack buttons). Inside the snippet the `⋯` sits
   in the same place relative to the name it acts on in all three.

8. **A row's "Rename" also selects the row.** §4.2 says only "moves focus to the
   inspector's Name field", but the inspector shows the *selected* tab — so
   renaming from a non-selected row's menu had to select it first.

### Smaller ones

- **`MenuButton` is a new `lib/ui/` primitive**, not in §9's file list. Five call
  sites needed "a visible ⋯ opening a flat menu, positioned from the button's own
  rect": the tab row, the window-group header, the Overview view header and
  WindowPanel's three row heads. `ListRow`'s inlined copy became a use of it.
- **`ContextMenu`'s `MenuItem` gained `disabled` and `hint`**, as §4.2's fallback
  anticipated — Phase 1's `ListRow` menu did not supply them.
- **`+ Window` is present-and-disabled on a windowless account** rather than
  hidden, which is today's rule (`{#if data.windows.length >= 1}`). Same
  "hiding becomes disabling with a reason" rule as `Remove this window`; the
  spec did not say either way.
- **`page.spec.ts` stubbed `overview_columns` as `{ tabs: [], columns: [] }`.**
  `columns` was never a field of `OverviewColumns` and `windows` was missing.
  The tab list reads `windows`, so the lie surfaced as an unhandled throw — and
  an unhandled throw exits `npm test` non-zero with every test passing, which is
  the trap Phase 2 recorded. The fixture was fixed rather than the component
  taught to defend against an impossible reply.
- **The palette grid's 24 swatches are still bare `<button>`s** with a local
  style rule, moved verbatim from `OverviewView`. A 1.1rem colour square is not
  a `Button`; Phase 1 shipped it this way and this phase did not change it. It
  is the one exception to §12's "no `<style>` block declares a button".

### The live pass happened, and it moved four things

The visual pass ran on 2026-08-14 against the owner's own session. Four
changes came out of it, and the first is a reversal of a decision in §5.

1. **The inspector is docked under the tab list, not across the window from it,
   and Overview has no right-hand column at all.** §5 puts the tab's Name,
   Colour, Bold and In window in the shell's inspector; on a 2560px monitor that
   put the list you click on the far left and the field you type in on the far
   right. Owner's call: the properties join the list. `OverviewView` therefore
   spans columns 2–4 with ONE child, and `+page.svelte` draws neither an aside
   nor a rail for this view. §7's rule survives in substance — the properties
   are still exactly the selection's, and nothing unrelated is beside them —
   but Overview is no longer an example of it in the *right-hand pane* sense.
   `OverviewInspector` did not change; only where it is rendered did.

2. **The column and appearance lists are capped at a reading width.** `ListRow`
   pushes its trailing control to the container's right edge, which is correct
   in a 20rem panel and absurd in a work area that is most of a wide monitor:
   the width box for `Name` sat about 1,700px from the word `Name`. Capped at
   26rem, the widest row either list can produce.

3. **The view's `⋯` is a real button, beside the sub-tabs.** As a small ghost
   pinned to the far right margin it was, in the owner's words, "very hard to
   find" — several hundred pixels from anything, reading as decoration. It is
   the only home for three account-wide commands, so it now looks like a
   control and sits next to the tabs it belongs to.

4. **The sidebar's account chip gives up its width before the character name
   does** (`Sidebar.svelte`, outside this phase's file list — small corrective
   work riding the branch). The chip was `nowrap`, so the *name* ellipsised, and
   an account whose characters share a prefix left four rows all reading
   "Storm Holde…". The name identifies the row; the account qualifies it.
   Worth recording the actual bug, because the first attempt did not work:
   `ListRow`'s `.label` is `flex: 1`, i.e. a **zero** basis, so it never takes
   part in shrinking at all — it just grows into whatever the chip leaves and
   ellipsises. A shrink factor on the chip does nothing until the label has a
   content basis (`flex: 1 1 auto`) to compete with. The *second* attempt then
   over-applied it: an unpaired row's trailing content is the `Link…` **button**,
   which got squeezed and clipped against the panel edge. The rules are scoped
   to a `paired` class now — a squeezed button is worse than a truncated name,
   and on a row with no chip there is nothing to compete with anyway.

5. **Renaming happens ON the row, and §5.1's Name field is gone.** Starting a
   rename from the row's `⋯` and finishing it in a panel below was two places
   for one gesture, and it cost a Name field that sat there permanently doing
   nothing for as long as nobody was renaming. The row becomes an inline `Field`
   — the same editor `+ Tab` and `+ Window` already use — seeded with
   `parseTabName(...).text`, committing on Enter and on blur, cancelling on
   Escape. The inspector keeps Colour, Bold, In window and Widths from.
   `OverviewTabList` hands back the **readable text**; `OverviewView` composes
   the colour and bold around it, which is where the no-op guard lives. Note
   that guard is byte-comparison and always has been: `formatTabName` is the
   inverse of `parseTabName` only *up to tag nesting*, so re-committing
   `<color=…>   <b>x</b>   </color>` untouched really does write — same
   rendering, different bytes, documented in `tabName.ts` and true before this
   phase too.

6. **A paired character on an UNNAMED account rendered as unpaired.**
   `accountAliasOf` returned the alias and `null` otherwise, so an account
   nobody had named drew no chip and got a `Link…` button offering to create a
   pairing that already existed — breaking the rule its own doc comment states,
   that "a chip means a CONFIRMED pairing and absence means no account". It
   falls back to `#<id>` now. The id alone, not `core_user_<id>`: the prefix is
   on every account, so it distinguishes nothing while taking more than half the
   chip in the narrowest column in the app — which was visible immediately, as
   names that had been whole started truncating the moment the fallback landed.

### Not verified

**"All 63 controls in §3 are reachable in the running app" is still not walked
control by control.** The layout was seen and corrected (above) and the
sidebar's chip behaviour was confirmed from a capture, but nobody has ticked off
all 63. Everything else in §12 is covered by a test, and Overview's half of the
shell-grid contract — the part that breaks silently, because jsdom computes no
layout — is pinned by a structural test of its own.

A note for the next phase, because it cost time here: the app was already
running **from this worktree**, so every edit hot-reloaded straight into the
owner's window, and `PrintWindow` with `PW_RENDERFULLCONTENT` captures it
without touching focus. That is what caught the failed first attempt at (4).
Check what is on port 1420 *and which directory it serves* before concluding a
dev server belongs to another session.
