# Phase 2 — The shell

Context bar, save cluster, view tab row, History popover, subject browser,
launch state, and the inspector rule.

Companion to `docs/ui-redesign/01-tokens-and-primitives.md` (Phase 1), which
owns every colour, size and shared primitive named here. This document names
tokens (`--surface`, `--text-muted`, `--accent`) and primitives (`Button`,
`Tabs`, `Chip`, `Popover`, `ListRow`, `EmptyState`, `ScopeBanner`) and never
redefines either.

---

## 1. Goal

Phase 1 makes the app *look* like one product. Phase 2 makes it *behave* like
one — because the frame the eight views hang in is currently a single flex row
that grew six unrelated jobs, and three of the five structural faults in the
audit are properties of that frame rather than of any view.

Three things have to become true, and everything else in this document is in
service of them:

1. **The app is about a character, not about two file slots.** The subject is
   named once, in one fixed place, and the char/user split surfaces in exactly
   one place — where an edit reaches the account's other characters.
2. **Save is never hidden.** Not by a view change, not by a takeover screen,
   not by a narrow window. And when it is about to write, it says which files
   and whose settings.
3. **Nothing moves under the cursor.** The tab strip has fixed membership and
   fixed width. A panel never silently changes which file it is describing.

Phase 2 is a *layout* phase. It removes no feature, changes no file format, and
touches the Rust backend not at all. It changes exactly one behaviour on
purpose (§5.5, the post-open tab fallback) and says so.

---

## 2. Current state (evidence)

### 2.1 The grid

`app/src/app.css:23` is the whole spatial model:

```css
.layout { display: grid; grid-template-columns: var(--col-left, 280px) 1fr auto; height: 100vh; }
```

Three columns: sidebar (280px, `app.css:42`), editor (`app.css:51`), backups
(280px fixed, `app.css:34`). The right column is `auto` so it collapses to
nothing when `BackupsPanel` is not rendered, and to the 24px rail when
collapsed (`app.css:26`, `app.css:27-33`). Both side panels have a collapse
rail; the collapse flags are in-memory only and reset on reload
(`app/src/routes/+page.svelte:31-34`).

### 2.2 The file bar

`app/src/routes/+page.svelte:505-542` is one `display: flex; flex-wrap: wrap`
row (`app.css:52-55`) carrying, in order: the composite filename, a
fidelity badge, up to two dirty badges, a conditional Discard button, a
conditional strip of up to six view tabs, a spacer, and Save. The comment at
`app.css:56-57` is candid about what holds it together — "combined with the
filebar's flex-wrap this keeps the Save button reachable on small windows",
i.e. the row is expected to wrap, and wrapping is the mitigation.

The filename itself is three identities concatenated
(`+page.svelte:507`): character name, account alias, and raw file name, joined
by em dashes. It is the only place the subject is stated.

### 2.3 Fault (a) — Save disappears

Verified. `mainView` is `"file" | "accounts" | "batch"` (`+page.svelte:30`).
The Accounts and Copy-settings views are the `{#if}` / `{:else if}` branches at
`+page.svelte:496-499`; **the entire `<section class="editor">` — file bar,
both dirty badges, Discard and Save — lives in the `{:else}` at
`+page.svelte:500-645`.**

So entering either view with unsaved edits removes Save and both unsaved
badges from the screen. `Ctrl+S` still works (`+page.svelte:462-465`), but
nothing on screen says there is anything to save. And `mainView` is only ever
returned to `"file"` inside `openFile()` (`+page.svelte:262`) and
`openPresetPair()` (`+page.svelte:300`) — neither view renders a close control
— so the only exit is to open another file, which is also the action that
prompts to discard (`+page.svelte:198-208`).

### 2.4 Fault (b) — the Backups panel silently changes subject

Verified. The active slot is derived from the view
(`+page.svelte:53-59`):

```ts
const active = $derived<Slot>(
  (view === "autofill" || view === "keybinds" || view === "probes") && slots.user?.status === "opened"
    ? "user"
    : view === "tree" && treeFile === "user" && slots.user?.status === "opened"
      ? "user"
      : "char",
);
const current = $derived(slots[active]);
```

`BackupsPanel` is passed `slot={active}` (`+page.svelte:649`) and refetches
whenever `slot` changes (`BackupsPanel.svelte:23-33`). So switching from
Overview to Autofill silently replaces the character file's backup list with
the account file's. The only marker is `subtitle={openDisplay}`
(`+page.svelte:651`), rendered at `font-size: 0.85em; opacity: 0.7`
(`BackupsPanel.svelte:87-94`) — the exact treatment the audit measured at
Lc 42. Restore is destructive (`BackupsPanel.svelte:35-47`) and its confirm
names only the backup file, not the file being replaced.

Two consequences of the same derivation are worth naming separately, because
they are not obviously part of the same bug:

- **The OS window title flips with the tab.** `setTitle` is driven by
  `openDisplay` (`+page.svelte:125-129`), `openDisplay` reads `current`
  (`+page.svelte:118-122`), and `current` is `slots[active]`. Switching from
  Overview to Autofill retitles the window from "Baguette Commander — EVE
  Settings Editor" to the account alias or the bare `core_user_140.dat`.
- **`AccountsView` and `BatchView` are scoped by it too.** Both take
  `openPath={current?...path}` (`+page.svelte:497,499`) and use it to find
  the profile folder (`AccountsView.svelte:17-27`). Harmless today only
  because both files sit in the same folder.

The derivation's own comment (`+page.svelte:49-52`) claims the active document
"is a consequence of the current view", and lists backups among the things
that "follow the character" — which is precisely what they do not do.

### 2.5 Fault (c) — the tab strip rearranges

Verified. The whole strip is behind one condition
(`+page.svelte:527`) and each of the five non-Tree tabs is behind its own
(`+page.svelte:530-534`). The conditions are duplicated a second time, in
prose-identical form, inside `viewAvailable()` (`+page.svelte:85-91`), which
exists to decide whether to keep the user's tab across a file switch.

The strip therefore changes membership and width as files load, as
`layoutAvailable` resolves from an async call (`+page.svelte:266-270`) and as
an account pairing lands (`+page.svelte:165-169`). When nothing qualifies the
strip is absent entirely, and the existing test at `page.spec.ts:173-181` pins
that absence — including Tree's own button, so the user is given no indication
the other five views exist at all.

### 2.6 The banner that is copy-pasted four times

`sharedLabel` is built once in the shell (`+page.svelte:153-159`) from
`sharedWith()` (`app/src/lib/overview.ts:13-23`), then threaded as a prop into
four views and rendered by each with its own identical markup and its own
identical CSS block:

| File | Markup | CSS |
|---|---|---|
| `OverviewView.svelte` | :314 | :478-481 |
| `AutofillView.svelte` | :97 | :159-162 |
| `KeybindsView.svelte` | :96 | :187-190 |
| `ProbeFormationsView.svelte` | :477 | :704-707 |

All four CSS blocks are byte-identical:
`margin: 0 0 0.6rem; padding: 0.3rem 0.5rem; font-size: 0.85em; color: var(--fg-dim); border-left: 2px solid var(--accent); background: var(--bg-panel);`

Separately, `LayoutView` passes `sharedNames` down to `HudPanel`
(`HudPanel.svelte:188-189`) and `ChatSplit` (`ChatSplit.svelte:45`), which
state scope *per row* inside a view that edits both files. Those are a
different thing and stay — see §5.4.

### 2.7 The sidebar

`Sidebar.svelte:118-178`. Its top block (`:119-135`) holds six buttons of four
different kinds: Open file…, rescan (`⟳`), Refresh names (network), Accounts
(navigate), Copy settings (navigate), About (dialog) — plus the collapse
chevron, all inside a `flex-wrap: wrap` container (`app.css:46`).

Below that: a "Hide non-standard files" checkbox (`:136-139`), flash/error
lines, the presets group, then one `<details>` per profile directory
(`:152-177`), each character rendered as
`name · alias   <size in KB>` (`:167-171`). The "in use by EVE" text
(`profiles.ts:65-67`) is a `<span class="meta">` inside the `<summary>`
(`:160`), so it wraps with the profile label; when not primary it is
recoloured with `var(--warn, #d08770)` (`:198`) — an inline fallback for a
token the app does not define.

Grouping is by `p.dir` and ordering pins the profile EVE wrote last
(`:52-58`, `profiles.ts:35-57`).

### 2.8 The launch state

`+page.svelte:502-503`: when `current === null` the entire work area is
`<p class="hint">Open a settings file to begin.</p>` — `color: var(--fg-dim); padding: 12px`
(`app.css:64`). The right column is not rendered at all
(`+page.svelte:646`), so on a 1600px window roughly 1300 × 900 px of the
application is one dim sentence in the top-left corner.

### 2.9 The one existing inspector

`LayoutView` runs its own two-column grid
(`LayoutView.svelte:1010-1013`): `minmax(0, 1fr) minmax(14rem, 20rem)`, with
`.canvas-wrap` scrolling on the left (`:1014-1017`) and `.side` scrolling on
the right (`:1019-1024`), holding `HudPanel` and `WindowPanel`
(`:953, :974`). No other view has a right-hand region, so the right edge of
the application means "backups" on five tabs and "window properties" on one.

### 2.10 Ctrl+F means two things, and nothing on four tabs

`+page.svelte:469-476` intercepts Ctrl+F, calls `preventDefault()`, and routes
to `layoutFocusFilter?.()` on Layout, otherwise to `openSearch()`. But
`openSearch()` (`:186-189`) focuses `searchBox`, which is `bind:this` on an
input that only exists inside the Tree branch (`:611-616`). On Overview,
Autofill, Keybinds and Probes `searchBox` is `undefined`, so Ctrl+F is
suppressed *and* does nothing — four of six tabs.

---

## 3. The model shift

> The subject is one character together with its account. Two files back it;
> that is an implementation detail the UI surfaces in exactly one place — where
> an edit reaches the character's siblings.

This is the load-bearing idea and it is worth being precise about what it
does and does not mean.

**It does not mean hiding the account file.** An account-scoped edit really
does change other characters, and that is the single most consequential fact
in the application. It has to be *more* visible than it is today, not less.
What changes is that it is stated **as a consequence** ("this also changes
Clea Otsada") rather than **as a storage location** ("core_user_140.dat").

**It means every place that currently asks the user to think in slots stops
asking.** The audit lists them; here is where each one lands:

| Slot leak | Where | Becomes |
|---|---|---|
| Two dirty badges + Discard | `+page.svelte:515-526` | One save cluster, §5.4 |
| Composite filename | `+page.svelte:507` | Subject block, §5.2 |
| Backups panel whose subject flips | §2.4 | History popover, both files named, §5.6 |
| "Character file / Account file" toggle | `+page.svelte:605-610` | Stays — it is the *raw* view, where files are the subject |
| "Character (for widths)" selector | `OverviewView.svelte:417-418` | Phase 4 (folds into the subject switcher) |
| Shared-settings banner ×4 | §2.6 | One `ScopeBanner`, rendered by the shell, §5.4 |
| Window title follows the tab | §2.4 | Follows the subject, §5.13 |

**The one place the split stays honest is the Raw tab.** A view whose entire
job is "show me the bytes as a tree" must name the file, and its file switch
(`+page.svelte:605-610`) is correct exactly as it is. That is the exception
that proves the rule, and it is why the tab is renamed from "Tree" to "Raw"
(§5.5) — the name should say "this is the escape hatch", not "this is the
default way to look at your settings".

### 3.1 What the shift buys structurally

The `active` derivation (§2.4) exists to serve a *display*: the file bar's
name, the badges, the backups panel. Autofill, Keybinds and Probes do not
mutate through `runMutation` at all — they are passed `userId`, `userOpen` and
`onUserDirty` and call their own commands (`+page.svelte:574-603`); only
`LayoutView` receives `runMutations` (`:547`) and only the Raw tree calls
`runMutation` via `handleEdit`/`handleRemove` (`:417-420`).

So once the file bar and the backups panel stop reading `current`, **the first
clause of `active` can be deleted outright**:

```ts
// after
const editSlot = $derived<Slot>(
  view === "raw" && treeFile === "user" && slots.user?.status === "opened" ? "user" : "char",
);
```

The `view === "raw"` guard must stay: without it, a user who left the raw view
on the account file would hand `slot="user"` to `LayoutView`
(`+page.svelte:546`). That guard is already present in the surviving clause
(`:56`), so this is a pure deletion.

That is the root-cause fix for fault (b). Everything else in §5.6 is about
making the History popover unambiguous *even so*.

---

## 4. Shell layout spec

### 4.1 Regions

| Region | Grid area | Present when |
|---|---|---|
| Context bar | row 1, all columns | Always |
| View tab row | row 2, columns 2–3 | Always |
| Subject list | rows 2–3, column 1 | Always (collapsible to rail) |
| Work area | row 3, column 2 | Always |
| Inspector | row 3, column 3 | Always (collapsible to rail; empty for views with no selection) |

The tab row spans columns 2–3 rather than the full width because the tabs
govern the work area and its inspector, and govern nothing about the subject
list. Spanning the full width would put a control above a panel it does not
control — the same category error the current file bar makes by putting view
tabs in a row about a file.

### 4.2 Wide (≥ 1200px)

```
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ ☰   Baguette Commander · stormdelay2 ▾   ⟨read-only⟩    ⌕ Search or run a command   Ctrl+K   │
│                                                          2 unsaved ▾   [ Save ]   History ▾  │
├──────────────────────────────────┬───────────────────────────────────────────────────────────┤
│ Profile  tranquility ▾           │  Layout │ Overview │ Autofill │ Keybinds │ Probes │ Raw ⋯ │
│ ● in use by EVE                  ├────────────────────────────────┬──────────────────────────┤
├──────────────────────────────────┤                                │ SELECTION                │
│ ● Baguette Commander  stormdelay2│                                │                          │
│   Clea Otsada         stormdelay2│           WORK AREA            │ Market                   │
│   De l'Opera      stormdelayghost│        (the active view)       │   x 640    y 220         │
│   Fourth Pilot                   │                                │   w 480    h 300         │
│ ▾ Presets                        │                                │                          │
│     A1_layout_preset             │                                │ SCOPE                    │
│                                  │                                │ account — also changes   │
│ Open file…                       │                                │ Clea Otsada              │
└──────────────────────────────────┴────────────────────────────────┴──────────────────────────┘
  --col-left 17.5rem                 minmax(0, 1fr)                   --col-right 20rem
```

One flat list, alphabetical, with the account as a chip on the row rather than
as a heading above it — §5.7 says why. Fourth Pilot has no chip because it has
no account, which is a fact worth seeing while scanning.

The context bar wraps to two lines only in the diagram; in the app it is one
row, `flex-wrap: nowrap`, with the subject block and the palette control each
carrying `min-width: 0` and ellipsis. Nothing in the context bar is allowed to
wrap — wrapping is what the current file bar does, and it is the fault being
fixed.

### 4.3 Narrow (< 1200px, inspector railed; < 900px, both railed)

```
┌────────────────────────────────────────────────────────────────────┐
│ ☰  Baguette Commander ▾   ⌕ Ctrl+K   2 unsaved ▾  [Save]  History ▾│
├────────────────────────┬─────────────────────────────────────────┬─┤
│ Profile tranquility ▾  │ Layout│Overview│Autofill│Keybinds│…│Raw ⋯│«│
│ ● in use by EVE        ├─────────────────────────────────────────┤ │
├────────────────────────┤                                         │ │
│ ● Baguette Commander   │           WORK AREA                     │ │
│   Clea Otsada          │                                         │ │
└────────────────────────┴─────────────────────────────────────────┴─┘

< 900px:
┌────────────────────────────────────────────────────────────────────┐
│ ☰  Baguette Commander ▾    ⌕ Ctrl+K    2 unsaved ▾  [Save]   ⋯     │
├─┬────────────────────────────────────────────────────────────────┬─┤
│»│ Layout │ Overview │ Autofill │ Keybinds │ Probes │ Raw       ⋯  │«│
│ ├────────────────────────────────────────────────────────────────┤ │
│ │                      WORK AREA                                 │ │
└─┴────────────────────────────────────────────────────────────────┴─┘
```

The subject list's width does not change with the window until it rails, so the
account chips (§5.7) survive both these sizes; they are elided above only for
diagram space.

Rules for narrowing, in the order they apply:

1. **Below 1200px** the inspector auto-rails (the user's manual state still
   wins once they set it in this session).
2. **Below 900px** the subject list auto-rails.
3. **The tab row never collapses, never scrolls, never wraps.** Below 900px
   the tab labels drop to their short forms and the row's horizontal padding
   goes to zero; at the app's minimum window width all six still fit. This is
   non-negotiable — fixed membership is worth nothing if the strip reflows
   instead.
4. **The context bar sheds in a fixed order** as width runs out: the account
   alias in the subject block (still in the switcher and the save disclosure),
   then the palette control's label (leaving `⌕ Ctrl+K`), then the History
   label (leaving its icon). **Save and the unsaved count never shed.**

The auto-rail thresholds are container queries on the shell, not media
queries — the app is a desktop window whose size is the only thing that
matters and a container query says so directly.

### 4.4 The grid

```css
.shell {
  display: grid;
  grid-template-columns: var(--col-left, 17.5rem) minmax(0, 1fr) var(--col-right, 20rem);
  grid-template-rows: auto auto minmax(0, 1fr);
  height: 100vh;
}
.shell.subjects-railed  { --col-left: 1.5rem; }
.shell.inspector-railed { --col-right: 1.5rem; }
```

`--col-left` / `--col-right` replace `--col-left` and the hardcoded 280px
widths at `app.css:23,34,42`. The rail width is one value in both directions,
where today the left rail is 24px (`app.css:28`) and the right column is
`auto`-sized by whatever it contains.

Row 3 is `minmax(0, 1fr)` so a scrolling child cannot push the grid past the
viewport — the same reason `LayoutView` already writes `minmax(0, 1fr)` for
its canvas column (`LayoutView.svelte:1011-1012`).

### 4.5 How a view fills two columns

The inspector must be one column of the *shell*, or the right edge of the app
means different things on different tabs — which is the state today (§2.9).
But Svelte snippets flow parent→child, so a view cannot hand markup upward
into a sibling region without a portal.

**Decision: `display: contents` on the view root.** A view that has an
inspector renders exactly two children and marks its own root as not
participating in layout:

```svelte
<!-- LayoutView.svelte -->
<div class="view-split">          <!-- display: contents -->
  <div class="work">…canvas…</div>
  <aside class="inspector">…HudPanel, WindowPanel…</aside>
</div>
```

```css
.view-split { display: contents; }
```

The two children become direct grid items of `.shell` and land in columns 2
and 3. No portal, no prop hoisting, no new dependency — for `LayoutView` it is
a three-line CSS change, because `.canvas-wrap` and `.side` already own their
own `overflow: auto` (`LayoutView.svelte:1014-1023`) and the root's
`height: 100%; overflow: hidden` (`:1012-1013`) is exactly what
`display: contents` makes redundant.

A view with no inspector renders one child, which lands in column 2; the
inspector column then holds the shell's own placeholder (§5.9).

**Risk, and the fallback:** `display: contents` on a grid item has a history of
accessibility-tree bugs in WebKit, and the Linux Tauri target is WebKitGTK.
This must be checked in the real app on all three platforms before the phase
lands. If it misbehaves, the fallback costs nothing: leave `LayoutView`'s own
grid alone, set `--col-right: 0` while Layout is active, and let Phase 4 do the
prop hoisting properly. The rule ("the right pane is properties of the
selection") is unaffected either way — only the mechanism is.

---

## 5. Each region in detail

### 5.1 Context bar

One row, always rendered, **outside every `mainView` branch**. That single
placement change is the whole of the fix for fault (a): whatever occupies the
work area — the editor, the Accounts takeover, the Copy-settings takeover, or
(after Phase 3) a sheet over the work area — the context bar and its save
cluster are above it and unaffected.

Contents, left to right:

| Slot | Control | Notes |
|---|---|---|
| 1 | `☰` app menu | §5.10 |
| 2 | Subject block | §5.2 |
| 3 | Status chips | §5.11 |
| 4 | *flex spacer* | |
| 5 | Command palette entry | §5.3 |
| 6 | Save cluster | §5.4 |
| 7 | History | §5.6 |

Height is one Phase-1 control row plus vertical padding; background
`--surface`, bottom border `--border`. It is the only full-width element in
the app, which is what makes it read as "the frame" rather than as "a toolbar
belonging to something".

### 5.2 Subject block

`Baguette Commander · stormdelay2 ▾` — one button opening the subject
switcher.

- Character name at `--text` weight-600, from `names[id]` as
  `openCharName` does today (`+page.svelte:101-107`).
- Account alias at `--text-muted`, from `aliasFor()`
  (`+page.svelte:110-114`); omitted when the account is unnamed or the
  character is unpaired.
- With a preset open, the block reads `A1_layout_preset ⟨preset⟩` — a Chip,
  replacing today's `${openPreset} (preset)` string (`+page.svelte:119`).
- With nothing open: `No character open` at `--text-muted`, and the ▾ still
  works — it is the fastest way to open one.

The **raw file name is not in the context bar.** It is in three places that
each have a reason to carry it: the save disclosure (§5.4), the History
popover (§5.6), and the subject switcher's row tooltip. Nothing is lost; the
name simply stops being the headline for a subject that has a real name.

**The switcher** (`SubjectSwitcher.svelte`) is a Popover over a type-ahead field
and **the same flat list the sidebar renders** — same source, same
`byResolvedName` order, same account `Chip`, same "no chip means no account"
rule (§5.7):

```
  ⌕ Type a character or account name

    Baguette Commander   ⟨stormdelay2⟩       core_char_950.dat
    Clea Otsada          ⟨stormdelay2⟩       core_char_951.dat
    De l'Opera           ⟨stormdelayghost⟩   core_char_970.dat
    Fourth Pilot                             core_char_980.dat
  ▾ Presets
    A1_layout_preset                         layout, overview
```

Two lists of the same characters in two different orders, inside one app, is
exactly the class of inconsistency this redesign exists to remove — so the
switcher takes §5.7's treatment whole rather than earning an exception. The
grouping it was first drafted with is rejected for the same reason and by the
same note (§5.7); "it is entered by typing" does not earn the exception,
because a user who opens it *without* typing gets precisely the broken ordering
the flat rule exists to prevent.

Like the sidebar it lists **the selected profile** (§5.7): flattening a list
that spans folders would put a duplicated id next to its own copy with nothing
to tell them apart. Phase 5's palette widens the scope instead of the ordering —
its Characters source carries a profile label as a third column, which is what
makes cross-folder rows distinguishable (`05-dialogs-copy-palette.md` §6.2).

**The type-ahead is what makes a flat list fast, and it matches the account
alias as well as the character name.** That is the flat list's answer to
"switch to my other character on this account", which is a real multiboxing
workflow and the one thing grouping genuinely did here: typing `stormdelay2`
filters to exactly that account's characters, still alphabetical. A filter beats
a grouping at this because it is temporary — it costs nothing to every other
opening, which is looking for one name. The account chip on every row shows the
relationship at rest, and §5.4's save disclosure and `ScopeBanner` name the
siblings at edit and save time. If that trio ever proves insufficient, a
dedicated affordance is a follow-up; it is not a reason to keep two orderings
now.

Choosing a character calls the same `openFile(path)` the sidebar calls
(`+page.svelte:239`), so the unsaved-changes prompt
(`+page.svelte:198-208`) and the slot reconciliation
(`:274-275`) are unchanged. Choosing a preset calls `openPresetPair`
(`:285`). The raw file name sits on each row and in its tooltip, which is the
third of the three places this section keeps it.

This component is deliberately the seed of the Phase-5 command palette: same
list, same type-ahead, one extra section of commands. Building it twice would
be the mistake.

### 5.3 Command palette entry

A bordered control — not a hint — reading `⌕ Search or run a command` with the
accelerator on its right edge, per the proposal's discoverability rules. It
sits where a search field would sit, so it is found by looking rather than by
knowing.

**In Phase 2 it opens the subject switcher (§5.2) with an additional "Go to…"
section listing the six views.** That is the honest minimum: a control that
opens nothing is worse than no control, and jump-to-character plus jump-to-view
is already the majority of what a palette is for here. Phase 5 adds commands
and the full ranking.

**The two never compete, because in Phase 2 they are one component and in
Phase 5 one becomes the other.** `Ctrl+K` and the subject block's `▾` open the
same switcher; when Phase 5 lands, the switcher's character list *is* the
palette's Characters source (`05-dialogs-copy-palette.md` §6.2) rather than a
second implementation of it. **"Jump to a character" is the switcher's job in
both phases** — nothing else acquires one.

One thing the flat rule does **not** forbid: once a query is typed, Phase 5
ranks rows by match quality rather than alphabetically. That is not a second
ordering of the same list — an unfiltered list is scanned by eye and must be
alphabetical, a filtered one is read top-down and must be ranked. The rule is
about what the user sees before typing, which is where finding a name actually
happens.

**Platform-aware accelerator, with no new dependency:**

```ts
// app/src/lib/keys.ts
export const MOD = /mac/i.test(
  (navigator as { userAgentData?: { platform?: string } }).userAgentData?.platform ??
    navigator.platform ?? "",
) ? "⌘" : "Ctrl";
export const accel = (key: string) => (MOD === "⌘" ? `⌘${key}` : `Ctrl+${key}`);
```

`@tauri-apps/plugin-os` would also answer this, but it is not a dependency
today (`app/package.json`) and two lines of `navigator` do not justify adding
one. Phase 5 imports `accel` for every menu item's shortcut column.

### 5.4 Save cluster

**One control** replacing the two dirty badges (`+page.svelte:515-518`), the
Discard button (`:520-526`) and the fidelity badge (`:509-513`).

**States:**

| Subject state | Renders |
|---|---|
| Nothing open | `[ Save ]` disabled, no count. Present, not hidden — see below. |
| Open, clean | `[ Save ]` disabled, no count |
| Open, dirty | `2 unsaved ▾`  `[ Save ]` (primary) |
| Open, dirty, document read-only | `2 unsaved ▾`  `[ Save ]` disabled; the disclosure says which file refuses and why |

Save's disabled rule is `canSave` exactly as it stands (`+page.svelte:62-65`),
which already folds in read-only via `slotSaveable`. **Reuse it, do not write a
second rule** — a save button that disagrees with what the save loop will
actually write (`:422-425` applies the same predicate) is the next bug.

The cluster is rendered even with nothing open, disabled. The existing test
"nothing open means no toolbar to save from" (`page.spec.ts:142-146`) asserts
its absence; that assertion inverts (§8). The reason is that a control which
appears and disappears is exactly the class of problem this phase exists to
remove, and a permanently-placed disabled Save teaches where Save is before
the user has anything to save.

**The disclosure** (a Popover from the `2 unsaved ▾` trigger):

```
WILL WRITE
  Baguette Commander    character   core_char_950.dat
  stormdelay2           account     core_user_140.dat

  ⚠ Account settings are shared — this also changes Clea Otsada.
  Each file is backed up before it is written.

                                        [ Discard changes ]
```

- Rows come from the same `slotSaveable` predicate, so a read-only or clean
  slot is simply not listed. A dirty-but-read-only slot gets its own row with
  the reason from `fidelity.reason` (today on a `title` attribute at
  `+page.svelte:510`, so nothing new is needed).
- The shared line is `ScopeBanner` fed by the existing `sharedNames`
  (`+page.svelte:153-155`), shown only when the account file is among the
  rows and `sharedNames` is non-empty.
- Discard is the existing `discardChanges()` (`:219-237`), unchanged,
  including its confirm.
- **No "Save both" button in the disclosure.** Save is on the trigger, three
  pixels away; a second one is redundancy that has to be kept in sync.

**Where the four banners go.** The `ScopeBanner` primitive gets exactly two
call sites, both owned by the shell:

1. Inside the save disclosure, at the moment of writing (above).
2. As a one-line strip under the tab row, rendered when the active view edits
   account-scoped data — i.e. `view ∈ {overview, autofill, keybinds, probes}`,
   the same set the four current copies cover.

Call site 2 exists because scope has to be legible *before* the edit, not only
at save time; that is what today's banner does and dropping it would be a
removal. What goes away is the duplication: the `sharedLabel` prop leaves four
component signatures, four `<p class="shared-banner">` disappear, and four
identical CSS blocks are deleted (§2.6).

`HudPanel` and `ChatSplit`'s per-row scope text (`HudPanel.svelte:188-189`,
`ChatSplit.svelte:45`) is **out of scope and stays**: those mark individual
rows inside Layout, which edits both files, so a view-level strip cannot
replace them. Phase 4 may restyle them onto the primitive.

**Not in Phase 2:** `saveFile()`'s two post-save `message()` dialogs
(`+page.svelte:430-431`) stay exactly as they are. The dialog diet and the
"backup path moves into History" move are Phase 5; doing them here would mix a
layout change with a behaviour change and make the phase un-revertable in one
piece.

### 5.5 View tab row

Its own row (§4.1), one visual style, built on Phase 1's `Tabs`. **All six
present, always, in this order:**

`Layout · Overview · Autofill · Keybinds · Probes · Raw`

Raw is last because it is the escape hatch. It is renamed from "Tree" for the
same reason — "Tree" reads like a first-class way to view settings, and the
name is why it currently sits in position one and is the default view
(`+page.svelte:48`).

**Disabled, never hidden.** `viewAvailable()` (`+page.svelte:85-91`) changes
signature from `(v) => boolean` to `(v) => string | null`, returning `null`
when available and the actionable reason when not. **The availability
conditions themselves do not change** — that would alter which features are
reachable, and this phase removes nothing:

| Tab | Condition (unchanged) | Reason when it fails |
|---|---|---|
| Raw | always | — |
| Layout | `layoutAvailable` (`:87`) | nothing open: "Open a character to edit its window layout." · open: "This file has no saved window layout." |
| Overview | `openCharId !== null \|\| user opened` (`:88`) | "Open a character or an account file." |
| Autofill | same (`:89`) | same |
| Keybinds | same (`:90`) | same |
| Probes | same (`:91`) | same |

The reason is on the tab's `title` and is repeated in the work area when a
disabled tab is clicked (a disabled `Tabs` item is focusable and announces its
reason; it does not swallow the click silently).

This single function then has three consumers where the conditions are
currently written twice: the tab's disabled state, the tab's tooltip, and the
post-open fallback. **The duplicated inline conditions at
`+page.svelte:527-535` are deleted.**

**The one deliberate behaviour change in this phase.** Today the post-open
fallback is `if (!viewAvailable(priorView)) view = "tree"`
(`+page.svelte:276, 308`). With all six tabs visible and Raw sitting last,
landing a first-time user on a raw dict tree while a Layout canvas is one tab
away is indefensible. The fallback becomes: keep `priorView` if it is
available, else the **first available tab in row order** (Layout → Overview →
… → Raw). Raw is always available, so this always resolves. `view`'s initial
value (`:48`) becomes `"layout"`, resolved through the same fallback on the
first open.

The `⋯` at the row's right end is the **view menu** — per-view actions that
have nowhere better (Import pack, Export pack, Delete empty stack frames…).
Phase 2 renders the slot; Phase 4/5 fill it. It is empty and hidden when the
active view contributes nothing.

### 5.6 History popover

Replaces the permanent 280px column. Triggered from the context bar, built on
`Popover`, body adapted from `BackupsPanel.svelte`.

**How it makes its subject unambiguous: it stops having one.** `list_file_backups`
is per-slot server-side (`app/src-tauri/src/ops.rs:238-242`, it lists backups
of `doc.path` for the slot it is given), so the popover simply asks for every
open slot and renders one titled group each:

```
HISTORY

  Baguette Commander — core_char_950.dat
    2026-08-11 14:32        119 KB    [ Restore ]
    2026-08-09 20:15        119 KB    [ Restore ]

  stormdelay2 — core_user_140.dat
    2026-08-11 14:32         81 KB    [ Restore ]

  Every save writes a backup first. Restoring one also backs up the file it replaces.
```

- A group is headed by **both** the subject name and the file name, at
  `--text` — not a 0.85em, 0.7-opacity subtitle (`BackupsPanel.svelte:87-94`).
- A slot with no open document contributes no group (rather than an empty
  one). A slot with no backups yet keeps its heading and the existing
  "No backups yet. Every save creates one." line
  (`BackupsPanel.svelte:58-60`).
- Nothing is derived from `view`. The popover's content is identical on every
  tab. That is fault (b) closed by construction, on top of the deletion in
  §3.1.
- `restore()` keeps its confirm (`BackupsPanel.svelte:35-47`), with the
  wording extended to name the file being replaced — today it names only the
  backup, which is the half the user already picked.
- The refetch trigger stays `savedAt` (`BackupsPanel.svelte:23-33`), plus
  popover-open. Closing the popover unmounts the body, so a stale list cannot
  survive a save.

`onRestored` currently writes back into `slots[active]` (`+page.svelte:653-657`);
it now takes the slot from the group the entry belongs to, which is the same
value it should always have had.

### 5.7 Subject list (the sidebar)

The left column becomes a **subject browser and nothing else**.

**Out:** the six global buttons at `Sidebar.svelte:120-132` — Accounts,
Copy settings, About, Refresh names, and the `⟳` rescan — all move to the app
menu (§5.10). **"Open file…" stays**, moved to the bottom of the list as a
quiet link: it *is* a file-list operation, and it is the only route to an
account file directly (`page.spec.ts:23-25` documents that the sidebar
deliberately lists characters, not account files).

**Structure, top to bottom:**

```
Profile   tranquility / Default  ▾        ← selector, not a group header
● in use by EVE                           ← Chip, status, not a wrapped fragment
──────────────────────────────────────
  ● Baguette Commander  ⟨stormdelay2⟩     ← ListRow; ● = open, Chip = its account
    Clea Otsada         ⟨stormdelay2⟩
    De l'Opera          ⟨stormdelayghost⟩
    Fourth Pilot                 [Link…]  ← no chip = no account
▾ Presets                                 ← PresetGroup.svelte, unchanged
    A1_layout_preset
──────────────────────────────────────
  Hide non-standard files  ☐
  Open file…
```

**One flat list, ordered exactly as it is today.** The rows are every character
in the selected profile, sorted by `byResolvedName` (`filesort.svelte.ts:22-32`,
imported as `byName` at `Sidebar.svelte:34` and applied at `:155`): named
characters alphabetically, files still showing a bare id after them, ordered
among themselves by file name. **Nothing about the ordering changes**, which is
the point — the sidebar's one job is finding a character, that happens
constantly, and alphabetical is how a name is found.

**The account becomes a chip on the row, not a level above it.** The data is
already on every row: `accountOf(charId, roster)` (`overview.ts:25-27`) resolved
through `aliasFor()` and rendered as a dim `· alias` suffix
(`Sidebar.svelte:169`), styled at `:195` as `0.85em` in `--fg-dim` — one of the
treatments the audit measured as failing. It becomes a Phase-1 `Chip`: same
information, same position, legible, and not one row moves.

> **Decided: account grouping was proposed and rejected.** An earlier draft of
> this section grouped rows by `accountOf()` so that characters sharing settings
> sat together. It is not built, and the reason should not be relitigated:
> grouping breaks alphabetical order and makes a character *harder to find*.
> Browsing is the sidebar's whole job and it happens on every session; knowing
> which characters share settings matters at **edit** time, not at **browse**
> time, and it is already stated at both of the moments it bites (below). The
> chip carries the same fact at zero cost to the ordering.
>
> **This applies to every list of characters in the app, not just this one.**
> §5.2's subject switcher was drafted grouped and is flattened by this same
> decision: alphabetical order is what makes a character findable, and that is a
> property of character lists, not of the sidebar. A per-surface exception would
> have to earn itself and "it is entered by typing" does not — the list is still
> scanned by eye whenever it is opened without a query. Phase 4's "Character
> (for widths)" selector inherits the rule when it folds into the switcher.

**A character with no chip is unpaired, and that is worth seeing.** Absence is
the whole rule — `accountOf()` returning `null` means no chip, so no
intersection with `roster.unassigned` is needed at all. What the absence tells
the user is not cosmetic:

- an unpaired character **cannot receive account-scoped aspects in a batch
  copy**: `targetDisabled` (`BatchView.svelte:151-152`) disables its checkbox
  outright and labels it "pair in the Accounts view to include" (`:349`);
- four of the six views refuse account-scoped editing until it is paired and
  nag to do so — `OverviewView.svelte:304-307`, `AutofillView.svelte:85-87`,
  `KeybindsView.svelte:85`, `ProbeFormationsView.svelte:467`.

Under grouping this was a single heading the eye passes once; as a per-row
property it is visible while scanning, which is when the user can still act on
it. The rows keep a `Link…` action opening the Accounts sheet — the same
affordance those four views already offer from inside themselves.

**The sharing relationship is not lost — it moves to where it bites.** Two
places, both already specified in this document: the save disclosure names the
other characters at the moment of writing (§5.4, "this also changes Clea
Otsada"), and `ScopeBanner` under the tab row states it on every account-scoped
view *before* the edit (§5.4, call site 2). The sidebar does not need to be a
third statement of the same fact, and for anyone who asks "whose account is
this character on?" while browsing, the chip on the row answers it.

**Profile: single-select, decided.** An account id can exist in several profile
folders at once — `docs/small-tasks.md:32-45` records an install with **ten**
folders each holding `core_user_13036531.dat` — so listing every folder at once
means the same character appears several times, and in a flat alphabetical list
the copies sit adjacent and indistinguishable with no folder heading to tell
them apart. Single-select removes the hazard rather than papering over it:
exactly one folder is in scope, so each character appears exactly once and one
list can be sorted by name alone. It also makes "in use by EVE" a property of
the current selection (one chip) rather than a fragment repeated per folder,
and it matches what the app already does internally — `pairedFilePath()`
resolves a pair strictly within the anchor file's own folder
(`overview.ts:34-47`).

Default selection is `primaryProfileDir(profiles)` (`profiles.ts:35-57`), which
is already the profile pinned open today (`Sidebar.svelte:52-58`).

The cost is accepted, not denied: a user comparing two folders clicks the
selector instead of scrolling, and a character in a non-selected folder is one
click further away rather than visible. The selector carries the folder count,
so it is never a secret that other folders exist.

**The "in use by EVE" chip.** Today it is a `<span class="meta">` inside the
`<summary>` (`Sidebar.svelte:160`) using `var(--warn, #d08770)` — an inline
fallback for an undefined token (`:198`). It becomes a `Chip`: `● in use by EVE`
in the success role when primary, `▲ not in use — EVE has not written here` in
the warning role otherwise. Text comes from `profileNote()`
(`profiles.ts:65-67`) unchanged; only its treatment and position change. The
full folder path stays on the selector's tooltip, as it is on the summary's
today (`Sidebar.svelte:159`).

**The KB per row goes — and I agree with the judgement.** `Sidebar.svelte:170`
renders `{Math.round(f.size / 1024)} KB` on every character row. Every
`core_char_*.dat` in a profile is within a few KB of every other (the
proposal's own capture shows 119 / 121 / 81), so the number never separates
two rows, never indicates health, and never answers a question anyone brought
to the app. It is pure per-row noise in the one list that has to be scannable.

It is **moved, not deleted**: the row already carries `title={f.file_name}`
(`:167`), which becomes `core_char_950.dat · 119 KB`. Anyone who wants the byte
count still has it, one hover away, next to the file name it belongs to. Sizes
that *do* answer a question stay visible where they do it: `bytes_written` in
the save result (`api.ts:63-66`) and per-backup size in History
(`BackupsPanel.svelte:65`).

**Kept as-is:** `PresetGroup.svelte` (whole component, untouched), the
"Hide non-standard files" toggle and its `isStandardName` filter
(`Sidebar.svelte:44-45,136-139`), the empty-state hints and their exact wording
(`:142-150`) — all three of which have tests (`Sidebar.spec.ts:50-81`), and the
resolved-name sorting via `byResolvedName` (`filesort.svelte.ts:22-32`).

The intersection with `roster.unassigned` that `AccountsView` has to do
(`AccountsView.svelte:29-33`) has **no counterpart here**, and that is a
consequence of the flat list rather than a coincidence: a grouped sidebar needs
to know the membership of an extra bucket, a per-row chip only needs
`accountOf()` to return `null`.

### 5.8 Work area and the launch empty state

The work area is column 2, row 3. It holds exactly one of: the active view, or
the launch empty state, or (Phase 3) a sheet.

Today's launch state is one dim sentence (§2.8). It becomes an `EmptyState`
that offers the thing the sentence describes:

```
                        Open a character to begin

           EVE Settings Editor edits the settings files EVE
           writes for each of your characters and accounts.

           tranquility / Default          ● in use by EVE

             Baguette Commander      ⟨stormdelay2⟩
             Clea Otsada             ⟨stormdelay2⟩
             De l'Opera              ⟨stormdelayghost⟩
             Fourth Pilot
                                              … 3 more

           [ Open file… ]     or press Ctrl+K to search
```

- The list is **the same data the subject list already has, rendered the same
  way** — the selected profile's characters, resolved-named, sorted by
  `byResolvedName`, account as a chip, no chip when unpaired (§5.7). No recents
  store, no new backend command, no new state. Show up to eight, then "… N
  more" which focuses the subject list.
- Clicking a row is `openFile(path)`, identical to a sidebar click.
- Naming `Ctrl+K` here is the proposal's fourth discoverability rule and costs
  one line.
- When the profile holds no characters, the existing sidebar hints
  (`Sidebar.svelte:142-150`) are the right words and are reused verbatim —
  including the one that names the "Hide non-standard files" filter as the
  cause, which `Sidebar.spec.ts:58-65` pins.

**Decision: no recents list in Phase 2.** It would need new persisted state for
a list the app can already derive, in a window where the full list fits. If it
is ever wanted, `prefs.svelte.ts` is the home — it already round-trips a whole
`Preferences` blob through `api.preferences()` / `setPreferences()`
(`prefs.svelte.ts:26-34, 70-78`) and a `recent: string[]` field is a
backend-side struct field plus three lines here.

### 5.9 The inspector rule

> **The right-hand column always means: properties of the current selection in
> the current view.** Never a second navigation surface, never a global panel,
> never something that belongs to a different subject than the work area.

Phase 2 establishes the rule, the column, and the slot mechanism (§4.5). It
fills it for one view, because one view already has it: `LayoutView`'s `.side`
(`LayoutView.svelte:1019`) becomes `.inspector` and its root becomes
`display: contents`. Nothing about `HudPanel` or `WindowPanel` changes.

For every other view in Phase 2 the column shows the shell's placeholder —
`EmptyState`, quiet, at `--text-muted`: *"Select something to see its
properties."* This is a deliberate, visible promise rather than a collapsed
column, because a column that exists on one tab and not on others is the same
class of fault as a tab strip that changes membership. Users who want the width
back rail it (§4.3), and the rail state persists for the session exactly as
`backupsOpen` does today (`+page.svelte:34`).

Phase 4 fills it for Overview (tab properties, "widths from") and then the
rest.

### 5.10 The app menu (`☰`)

Phase 5 specifies the menu's full contents and every accelerator. Phase 2
specifies **where it lives and what arrives in it from the sidebar**:

- Position: slot 1 of the context bar, a `Popover` anchored under the `☰`.
- Arriving from `Sidebar.svelte:120-132`: **Accounts…**, **Copy settings…**,
  **About**, **Refresh names**, **Rescan profiles** (today's `⟳`).
- `Refresh names` keeps its busy state and its "Names refreshed" flash
  (`Sidebar.svelte:94-105`); the flash becomes a toast in Phase 5, so for now
  it stays inline in the menu.
- `Accounts…` and `Copy settings…` still set `mainView` in Phase 2. Phase 3
  turns them into sheets. Because the context bar is now outside that branch
  (§5.1), Save survives either way — that is the point.

**One free line while we are here:** clicking any view tab sets
`mainView = "file"`. That alone gives Accounts and Copy settings a way out
before Phase 3 lands, at the cost of one assignment.

### 5.11 Status chips

Chips sit between the subject block and the spacer, at most two at a time:

| Chip | Role | Source |
|---|---|---|
| `read-only` | danger | `current.fidelity.state === "read_only"`, reason on the tooltip (`+page.svelte:509-511`) |
| `preset` | info | `openPreset !== null` (`:43`) |

The `editable` badge (`+page.svelte:512`) is **not** carried over as a chip.
Editable is the normal state; a permanent badge announcing normality is noise,
and the audit measured it as one of the three badges failing contrast. Its
information survives in the negative: no chip means editable, and the save
cluster is live. `page.spec.ts:164, 177` currently waits on the literal strings
"read-only" and "editable" — see §8.

### 5.12 Keyboard in Phase 2

**Phase 5 owns the complete keyboard model.** Phase 2 does three things and
stops:

1. `Ctrl+S` is untouched (`+page.svelte:462-465`).
2. `Ctrl+K` opens the palette entry's popover (§5.3). New.
3. **Ctrl+F stops being a no-op on four tabs.** The existing bindable
   `layoutFocusFilter` (`+page.svelte:96`, bound at `:556`, forwarded from
   `WindowPanel`) is generalised to `viewFocusSearch` — one bindable the
   *active view* sets. The handler becomes
   `viewFocusSearch?.() ?? openSearch()`. Layout binds it exactly as it does
   today; the Raw view binds it to `openSearch`; the other four bind nothing
   yet and so fall through to the tree search, which is no worse than the
   current silent nothing. Phase 4 binds each view's own field as it gains one.

   No registry, no keymap object, no new abstraction — one rename and one
   fallback expression. The split meaning of Ctrl+F is *recorded*, not
   resolved: resolving it means every view having a search field, which is
   Phase 4/5 work.

### 5.13 The OS window title

`setTitle` (`+page.svelte:125-129`) stops reading `openDisplay` (which follows
`active`, §2.4) and reads a new `subjectLabel`:

```
preset open        → "A1_layout_preset (preset) — EVE Settings Editor"
character open     → "Baguette Commander — EVE Settings Editor"
account only       → "stormdelay2 — EVE Settings Editor"
account, no alias  → "core_user_140.dat — EVE Settings Editor"
nothing open       → "EVE Settings Editor"
```

Same precedence `openDisplay` already implements (`:118-122`) — the only change
is that it resolves against the *subject* rather than against `slots[active]`,
so switching tabs never retitles the window. `openDisplay` itself is deleted;
its other consumer, the backups subtitle (`:651`), is replaced by the History
popover's per-file headings (§5.6).

---

## 6. State ownership: what moves where

`+page.svelte` is 679 lines and holds every piece of application state. Most of
it should stay there. The test is not "is this state global?" but **"does
something that is not a descendant of `+page.svelte`'s view switch need to read
it?"** — because that is the only thing props cannot do.

### 6.1 New rune module: `app/src/lib/subject.svelte.ts`

The subject — who is open, what is dirty, who else an edit reaches — is read by
the context bar, the save cluster, the History popover, the subject list, the
empty state, and four views. The context bar is a *sibling* of the view switch,
not an ancestor of it, so threading this as props is not merely verbose, it is
structurally impossible for at least two consumers.

The repo already reaches for this pattern for exactly this reason.
`accounts.svelte.ts:1-3`: *"A Svelte-5 rune module so the sidebar, the
open-file header and the Accounts view all react to the same state."* Same
sentence, one level up.

**Moves in:**

| State | From | Why |
|---|---|---|
| `slots` | `+page.svelte:37-40` | Read by cluster, History, tabs, sidebar, title |
| `dirtySlots` | `:41` | Read by cluster (sibling of the view switch) |
| `openPreset` | `:43` | Read by subject block and sidebar |
| `profiles` | `:75-76` | Read by sidebar, switcher, empty state, pair resolution |
| `selectedProfileDir` | *new* (§5.7) | Read by sidebar and empty state |
| `layoutAvailable` | `:81` | A property of the open document; drives the Layout tab |
| `savedAt` | `:80` | Read by History and by every view's `refreshToken` |

**Derived, moving with them:** `openCharId`, `openUserId` (`:133-144`),
`openCharName`, `openUserAlias` (`:101-114`), `openAccountCharacters`
(`:145-147`), `sharedNames` (`:153-155`), `canSave` + `slotSaveable`
(`:62-65`), and the new `subjectLabel` (§5.13) and `saveTargets` (§5.4).

**Functions, moving with them:** `openFile`, `openPresetPair`, `clearSlot`,
`reconcileUserSlot`, `reconcileCharSlot`, `loadCharacter`,
`confirmDiscardIfDirty`, `discardChanges`, `saveFile` (`:198-383, 422-454`).
These are the state's own transitions and splitting them from it is what
produced the current 679-line file.

`sharedLabel` (`:156-159`) is **deleted** — the string it builds exists only to
be handed to four views that no longer take it. `ScopeBanner` consumes
`sharedNames` directly.

### 6.2 Stays in `+page.svelte`

Everything that is a fact about *where the user is looking*, which only the
shell reads:

`view`, `treeFile`, `editSlot` (renamed from `active`, §3.1), `current`,
`selectedWindowId`, `reveal`, `query` / `searchBox` / `found`, `insertTarget`,
`sidebarOpen`, `inspectorOpen` (renamed from `backupsOpen`), `mainView` (until
Phase 3), `viewFocusSearch` (renamed from `layoutFocusFilter`), and
`runMutation` / `runMutations` / `handleEdit` / `handleRemove`, which write to
`editSlot` and belong beside it.

Moving these would buy nothing and would cost the ability to reason about the
shell's own behaviour from one file.

### 6.3 The test hazard this creates, and its guard

`page.spec.ts:55-62` already documents the exact problem module-level rune
state causes in this suite: *"the preferences and roster stores are
module-level rune state shared by every test in this file, and a load still in
flight when `afterEach` clears the stubs resolves to `undefined` and poisons
that state for the next test."*

`subject.svelte.ts` is a third such store and a much bigger one. It **must**
export `resetSubject()` — clearing slots, dirty flags, preset, profiles and
`savedAt` — called from `afterEach` in every suite that mounts the shell. This
is a hard requirement of the phase, not a nicety; without it the suite fails
intermittently in a way that will be blamed on the layout change.

### 6.4 Not new modules

No store for view state, no store for the popovers' open flags, no event bus,
no context API. Each has one owner and one reader.

---

## 7. File-by-file change list

### New

| File | Contents |
|---|---|
| `app/src/lib/subject.svelte.ts` | §6.1. The only new *state*. |
| `app/src/lib/ContextBar.svelte` | §5.1. Composition only; every child is a Phase-1 primitive or one of the below. |
| `app/src/lib/SaveCluster.svelte` | §5.4. Trigger + Popover disclosure + Discard. |
| `app/src/lib/ViewTabs.svelte` | §5.5. Wraps Phase-1 `Tabs`; owns the disabled-reason table and the `⋯` slot. |
| `app/src/lib/SubjectSwitcher.svelte` | §5.2/§5.3. Type-ahead over the same flat `byResolvedName` list the sidebar renders, chips and all — matching on the account alias as well as the name. Phase 5 extends it into the palette. |
| `app/src/lib/AppMenu.svelte` | §5.10. Popover with the five migrated actions. |
| `app/src/lib/HistoryPopover.svelte` | §5.6. Popover wrapping the adapted backups list. |
| `app/src/lib/keys.ts` | §5.3. `MOD`, `accel()`. Two exports. |

No `LaunchEmpty.svelte`: the empty state is ~15 lines of `EmptyState` plus a
list, with one call site, in `+page.svelte`. A file for it is scaffolding.

### Modified

| File | Change |
|---|---|
| `app/src/routes/+page.svelte` | The bulk. Delete the file bar (`:505-542`) and the conditional tab strip (`:527-536`); hoist `ContextBar` + `ViewTabs` above the `mainView` branch (`:496`); delete the first clause of `active` (`:53-59`) and rename to `editSlot`; delete `openDisplay` (`:118-122`) and `sharedLabel` (`:156-159`); repoint `setTitle` at `subjectLabel` (`:125-129`); replace the `.hint` launch state (`:502-503`) with `EmptyState`; drop `BackupsPanel` and its rail (`:646-663`); rename `backupsOpen`→`inspectorOpen`, `layoutFocusFilter`→`viewFocusSearch`; drop the `.tree-area` wrappers around each view (`:543-604`); move most state to `subject.svelte.ts` and re-import. Rename view `"tree"`→`"raw"`. Target: under 300 lines. |
| `app/src/lib/Sidebar.svelte` | §5.7. Delete the action toolbar (`:120-132`) and its `onShowAccounts`/`onShowBatch` props; add the profile selector + chip; collapse the per-profile `<details>` loop (`:152-177`) into one flat list of the selected folder's characters, sorted by the same `byName`/`byResolvedName` (`:34, :155`); the `· alias` suffix (`:169`) becomes a `Chip` and `.acct` (`:195`) is deleted; fold KB into the row `title` (`:167-171`); keep the filter toggle, hints and `PresetGroup` untouched. |
| `app/src/lib/BackupsPanel.svelte` | Becomes the History popover's body: takes a list of `{slot, subjectName, fileName}` groups instead of one `slot`+`subtitle`; the collapse chevron (`:52-53`) goes with the column; the fetch effect (`:23-33`) runs per group; restore's confirm names the file it replaces (`:36-40`). |
| `app/src/lib/LayoutView.svelte` | `.layout-view` → `display: contents` (`:1010-1013`); `.side` → `.inspector` (`:829-831, 1019`); `focusFilter` prop renamed `focusSearch` (`:32, 51-53, 997`). No logic touched. |
| `app/src/lib/OverviewView.svelte` | Drop the `sharedLabel` prop (`:11, 14`), its `<p>` (`:314`) and its CSS (`:478-481`). |
| `app/src/lib/AutofillView.svelte` | Same: `:6, 11, 97, 159-162`. |
| `app/src/lib/KeybindsView.svelte` | Same: `:6, 8, 96, 187-190`. |
| `app/src/lib/ProbeFormationsView.svelte` | Same: `:11, 13, 477, 704-707`. |
| `app/src/app.css` | `.layout` → `.shell` grid (`:23-26`); rails generalised (`:27-33`); **delete** `.backups*` (`:34-39`), `.filebar` (`:52-55`), `.filename` (`:58`), `.badge*` (`:59-62`), `.viewtabs*` (`:119-121`), `.spacer` (`:111`), `.save` (`:112`), `.discard` (`:113-115`), `.sidebar-actions` (`:46`). `.tree-file` (`:122`) stays — the Raw view keeps its file switch. |

### Untouched, deliberately

`AccountsView.svelte`, `BatchView.svelte` (Phase 3), `PresetGroup.svelte`,
`AboutPanel.svelte`, `TreeNode.svelte`, `InsertForm.svelte`, `WindowPanel.svelte`,
`HudPanel.svelte`, `ChatSplit.svelte`, every `.ts` helper
(`overview.ts`, `profiles.ts`, `filesort.svelte.ts`, `accounts.svelte.ts`,
`names.svelte.ts`, `prefs.svelte.ts`, `api.ts`), and the **entire Rust
backend**. No new Tauri command, no new dependency in `app/package.json`.

---

## 8. Tests

vitest + jsdom, `*.spec.ts` beside the component, following `page.spec.ts` and
`Sidebar.spec.ts`: stub commands through `calls.stub` from `$lib/test/setup`,
mount, assert on roles and text.

### 8.1 The three faults, pinned

These are the tests the phase exists for. Each fails on `master`.

1. **`page.spec.ts` — "Save survives entering Accounts with pending edits".**
   Open a char file, mutate to dirty, click Accounts in the app menu; assert
   `getByRole("button", {name: "Save"})` exists and `.disabled === false`, and
   that the unsaved count is on screen. *Fault (a).*
2. **`page.spec.ts` — "History lists the same files on every tab".** Open a
   paired character, open History on Overview, record the group headings;
   switch to Autofill, reopen History, assert identical headings and that
   `list_file_backups` was called for `char` and `user` both times. *Fault (b).*
3. **`page.spec.ts` — "the OS window title does not change with the view tab".**
   Replace the `setTitle` mock (`page.spec.ts:19-21`) with a `vi.fn()`; assert
   the last title after switching Overview → Autofill → Probes is unchanged.
   *Fault (b), second head.*
4. **`ViewTabs.spec.ts` — "all six tabs render with nothing open, five
   disabled, each with a reason".** Assert six buttons by name and a non-empty
   `title` on each disabled one. *Fault (c).*
5. **`ViewTabs.spec.ts` — "tab membership and order are identical before and
   after a file opens".** Snapshot the six accessible names, open a file,
   snapshot again, assert equal. This is the property, not an instance of it.

### 8.2 Save cluster

6. Clean subject → Save present and disabled, no count rendered.
7. Both slots dirty → disclosure lists both files with subject name, role and
   file name; the shared line names the account's other characters; Discard is
   present.
8. Dirty but read-only char document → Save disabled; the disclosure shows the
   file with its `fidelity.reason` and does not list it under "will write".
   (`slotSaveable` is the single source — assert the disclosure and the button
   agree.)
9. Only the account file dirty → exactly one row, and the shared line still
   appears.

### 8.3 History popover

10. Two open slots → two groups, each heading containing its file name; each
    group's Restore calls `restore_backup` with **that group's** slot.
11. One open slot → one group, no empty second group.
12. A slot with no backups → heading present, "No backups yet" line present.

### 8.4 Sidebar

13. **The list is flat and in `byResolvedName` order.** Fixture: two named
    characters and one still showing a bare id; assert the accessible names in
    sequence, alphabetical with the bare id last. This is the assertion that
    fails if anyone reintroduces a grouping level (§5.7).
14. A paired character's row carries its account alias as a chip; a character
    in `roster.unassigned` carries **no** account chip and does carry `Link…`.
15. Changing the profile selector replaces the rows with the other folder's.
16. The five migrated actions are **not** in the sidebar
    (`queryByRole("button", {name: /accounts|copy settings|about|refresh names/i})`
    is null) **and are** in the app menu. Both halves, in one test file each —
    this pair is what stops the migration silently dropping one.
17. "Open file…" is still in the sidebar and still routes an account file to
    the user slot (`page.spec.ts:110-121` survives with a new query).
18. The existing empty-hint tests (`Sidebar.spec.ts:50-81`) pass unchanged
    except for the mount props. If any needs rewording, the list is wrong.

### 8.5 Subject switcher

19. **`SubjectSwitcher.spec.ts` — the switcher's order is the sidebar's order.**
    Mount both against the same fixture and assert the character rows come out
    in the same sequence. One assertion, and it is the one that fails if either
    surface reintroduces a grouping level (§5.2, §5.7).
20. Typing an account alias filters to that account's characters, still in
    `byResolvedName` order — the flat list's answer to "my other character on
    this account" (§5.2).

### 8.6 Empty state

21. Nothing open → the character rows are listed, in the same order and with
    the same chips as the sidebar; clicking one calls `open_file` with that
    path.
22. Nothing open, profile has no characters → the existing hint text appears
    (same string as the sidebar's).

### 8.7 Existing tests that change, and why

Three current assertions pin behaviour this phase deliberately reverses. Each
is rewritten in place with a comment naming the reason, never deleted:

| Test | Now asserts | Becomes |
|---|---|---|
| `page.spec.ts:142-146` "nothing open means no toolbar to save from" | Save is absent | Save is **present and disabled** (§5.4) |
| `page.spec.ts:173-181` "a file with no character id offers no view tabs" | all six buttons absent | all six **present**, five disabled with a reason (§5.5) |
| `page.spec.ts:196-203` "Layout stays hidden for a document with no windows" | Layout button absent | Layout **disabled**, `title` naming the reason (§5.5) |

Also mechanical: `page.spec.ts:164,177` wait on the strings `"read-only"` and
`"editable"` — `editable` no longer renders (§5.11), so `:177` waits on the
subject name instead. Every `mount()` gains `resetSubject()` in `afterEach`
(§6.3).

### 8.8 Not tested here

Layout/positioning (grid columns, rail thresholds, `display: contents`) is not
asserted in jsdom, which computes no layout. It is verified in the running app
per §10 — on all three platforms for the `display: contents` question (§4.5).

---

## 9. Risks & rollback

| Risk | Likelihood | Mitigation |
|---|---|---|
| `display: contents` misbehaves on WebKitGTK (§4.5) | Medium | Verify in the app before merge. Fallback is one line (`--col-right: 0` while Layout is active) and defers the mechanism to Phase 4; the *rule* is unaffected. |
| Module-level rune state poisons the vitest suite (§6.3) | **High if unguarded** | `resetSubject()` in `afterEach`, mandatory. The suite already documents this failure mode for two smaller stores. |
| Single-select profile hides a character a user expected | Medium | Accepted with the decision (§5.7) — it is the only way one flat list can be unambiguous with ids duplicated across folders. Selector shows the folder count; the empty-profile hint names the alternative. |
| A user wants to see at a glance which characters share an account | Low | The chip on every row is the same fact without reordering; the consequence is stated where it bites, in the save disclosure and `ScopeBanner` (§5.4). Grouping was considered and rejected (§5.7). |
| Multiboxing: "switch to my other character on this account" is slower without grouping | Low | The switcher's type-ahead matches the account alias, which filters to that account's characters without permanently reordering anything (§5.2). If that proves insufficient it earns its own affordance as a follow-up — not a second ordering. |
| History being a popover makes restore less discoverable than a docked column | Medium | Persistent labelled control in the context bar, present on every tab (today the column vanishes entirely with nothing open, `+page.svelte:646`). Phase 5's save toast will name the backup it wrote. |
| The 679-line shell is rewritten in one commit and something silently drops | Medium | Land it as **two commits** (below). |
| Six tabs at minimum window width overflow | Low | Short labels below 900px (§4.3); check at the app's minimum size, which is where the current bar already wraps. |
| A view's markup does not tolerate `display: contents` | Low | Only `LayoutView` uses it in this phase, and its two children already own their own scrolling (`LayoutView.svelte:1014-1023`). |

### 9.1 Land it in two commits

**Commit 1 — pure refactor, no visible change.** Extract
`subject.svelte.ts` (§6.1) and rewire `+page.svelte` to import from it, keeping
the existing markup byte-for-byte. The full suite must pass **unmodified**. If
it does not, the extraction is wrong and nothing else is at risk yet.

**Commit 2 — the shell.** Everything else, with the test changes of §8.7.

Rollback is `git revert` of commit 2 alone: the old three-column grid and file
bar come back on top of the extracted store, which is a state the app is
verified in. Reverting both restores `master`. Neither commit touches the
backend, so there is no data-shape or file-format exposure at any point.

---

## 10. Definition of done

**Structure**

- [ ] The context bar renders above and outside every `mainView` branch, and
      Save is reachable with `mainView === "accounts"` and `"batch"`.
- [ ] Six view tabs, always present, fixed order `Layout Overview Autofill
      Keybinds Probes Raw`; every disabled one has a reason on its `title`.
- [ ] Tab membership and pixel width are identical before and after a file
      opens, and across every tab switch.
- [ ] `viewAvailable()` is the single source for tab availability; the
      duplicated inline conditions at `+page.svelte:527-535` are gone.
- [ ] The first clause of `active` is deleted; `editSlot` is a function of the
      Raw view's file switch only.

**Subject**

- [ ] The subject is named once, in the context bar, and the OS window title
      matches it and does not change with the tab.
- [ ] The sidebar is **one flat list** in `byResolvedName` order — byte-for-byte
      the ordering it has today — with a single-select profile at the top and
      "in use by EVE" as a chip.
- [ ] Every paired character's row carries its account as a chip; an unpaired
      character carries none, and no "Not linked" grouping level exists.
- [ ] The subject switcher lists the **same characters in the same order with
      the same chips** as the sidebar — one rule for every character list in the
      app — and its type-ahead matches the account alias as well as the name.
- [ ] Per-row KB is gone from the sidebar rows and present in their tooltips.
- [ ] Accounts, Copy settings, About, Refresh names and Rescan are in the app
      menu and nowhere else; each is still reachable in one click.

**Save & History**

- [ ] One save control. Its disclosure lists exactly the files
      `slotSaveable` will write, names the account's other affected
      characters, and carries Discard.
- [ ] `sharedLabel` and all four `.shared-banner` blocks are deleted;
      `ScopeBanner` has exactly two shell-owned call sites.
- [ ] History is a popover, lists every open slot as its own group headed by
      subject *and* file name, and is byte-identical on every tab.

**Frame**

- [ ] The launch state offers the selected profile's characters and names
      `Ctrl+K`.
- [ ] The inspector column exists on every tab, holding Layout's panels or the
      placeholder, and rails independently.
- [ ] Ctrl+F focuses *something* on all six tabs.
- [ ] The palette control renders with the platform-correct accelerator and
      opens the subject switcher.

**Hygiene**

- [ ] `npm run test` green, including the three rewritten assertions of §8.7
      and `resetSubject()` in every mounting suite.
- [ ] `npm run check` clean (`--fail-on-warnings`).
- [ ] No new dependency in `app/package.json`; no change under `app/src-tauri/`.
- [ ] No hardcoded colour, radius or spacing value — Phase 1 tokens only.
- [ ] Verified in the running app at 1600px, 1100px and the minimum window
      width, on Windows and (for `display: contents`) macOS and Linux.
- [ ] Every feature reachable on `master` is reachable after: Accounts, Copy
      settings, About, Refresh names, Rescan, Open file…, presets, the
      non-standard filter, restore, discard, the Raw file switch, and all six
      views.
