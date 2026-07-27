# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.21.0] - 2026-07-27

Stacks you build by dragging, and a neocom you can put in order.

### Added
- **Build window stacks on the canvas.** Hold Shift and drag one window onto
  another to tab them together, or onto an existing stack to join it. Drag a
  stack's tabs to reorder them, drag a tab onto a different stack to move the
  window between them, or drag it out onto empty canvas to free it — and it
  lands where you dropped it, rather than hiding underneath the stack it came
  from. All of this already existed behind dropdowns and arrow buttons in the
  window list; it now happens where you are actually looking. Dragging a stack
  by its body still moves the whole stack.
- **Reorder the neocom.** The bar down the left edge of the client is muscle
  memory, and setting it up meant dragging buttons one at a time on every
  character. Select it on the canvas and its buttons appear in the panel:
  reorder them, remove one, add back anything missing, or reset to the set your
  client started with. A **Window layout** batch copy now carries the bar with
  it, so a neocom arranged once can be given to every other character at once.

### Fixed
- **A window joining a stack takes the stack's position.** The client discards a
  joining window's own rectangle and gives it the stack's; the editor did that
  when creating a stack but not when adding to one, so a window added through
  the panel's *Add to stack…* kept a stale position until something moved the
  stack. Both paths now match the client.

The neocom's button data turned out to be fully readable from settings files
already on disk, so the capture session this was expected to need never
happened — what the client stores, and what each field means, is recorded in
`docs/format-notes.md`.

## [0.20.0] - 2026-07-27

Settings you can save, name, edit and hand to someone else.

### Added
- **Settings presets.** Save a character's settings as a named preset that
  belongs to no character — a snapshot that outlives its source, a "Mining" /
  "PvP" pair you swap on one character, a setup you hand another player, or a
  known-good baseline poured into a brand-new install. Choose what it holds
  when you save it: window layout, overview, autofill, keybindings, or a
  complete copy of both settings files — and it holds only that, so a layout
  preset you share carries no trace of your autofill history.
- **Presets are editable, not just snapshots.** A preset opens in the sidebar
  like a character, and every editor works on it — Layout, Overview, Autofill,
  Keybinds and the raw tree — through the same save chain and the same
  backups. Overwriting a preset writes into its existing folder rather than
  replacing it, so its backup history survives a re-save. Renaming or deleting
  a preset that's currently open is refused, with the reason spelled out in
  the menu, since pulling the folder out from under an open preset would
  strand whatever you hadn't saved yet.
- **Share a preset as one file.** Export writes a single `.evepreset` file;
  Import reads one back in. Exporting a complete-copy preset warns first,
  because it carries everything the editor doesn't otherwise model — station
  names, searches, typed text — not just the four named aspects.
- **Batch apply can start from a preset.** The batch source picker now offers
  "A character" or "A preset" — everything downstream is unchanged, including
  the collateral-character warning and the per-target backups.

### Fixed
- **The overview column width field is editable whenever a character document
  is open**, rather than only when that file's name happened to contain a
  numeric character id.

## [0.19.0] - 2026-07-26

Windows called what EVE calls them, and clutter you get to define.

### Added
- **Chat windows show their real channel name.** A chat window's id contains
  none of the channel's name, so the list and the canvas could only ever say
  *Chat · private* — and searching for a channel by name found nothing, which is
  how this surfaced. The name was in the character file the whole time; it is now
  read, shown, and searchable.
- **Window stacks show EVE's own label.** *Window stack · 76* becomes
  *Character: Information* — the label the client itself uses for that stack,
  which lives in the account file. A stack the editor created has no such label
  yet and still shows its number.
- **You decide what counts as clutter.** `Hide clutter` guesses from a built-in
  list, and that list can never be complete: EVE's "open windows" flag
  accumulates rather than tracking what is on screen, so a real character has 134
  windows flagged open against about 9 actually visible, one-shot dialogs
  included. Right-click any window in the list to *Treat as clutter* or *Stop
  treating as clutter*, and the counter grows a `· N overridden · clear` note so
  the choice is never invisibly in effect.
- **A preferences file.** Those overrides are the first thing the editor
  remembers about *you* rather than about EVE, so they live in a
  `preferences.json` of its own, alongside the app's settings rather than in the
  game's files. Nothing here writes to an EVE settings file. A file it cannot
  read is moved aside rather than overwritten.

The raw window id is still on hover and one click away in the right-click menu:
friendly names are only ever an improvement on top of it, never a replacement.

## [0.18.0] - 2026-07-26

Set your keybindings once, then give them to every other account.

### Added
- **Keybindings editor.** The account's key bindings are now readable and
  editable in a new Keybinds view: every command the client knows, grouped and
  labelled with EVE's own strings, rebindable by pressing the combination.
  Rebinding a combination already in use takes it from its previous owner, as
  the game does. A new Keybinds batch category copies a whole binding table
  between accounts — the only way to give an account bindings without setting
  them up by hand in-game.

The one thing the view cannot show yet is EVE's factory defaults: the client
stores them nowhere in the settings files, so the Default column and the
per-row reset button stay empty until those bindings are captured separately.
An account that has never opened the in-game keybinding screen has no table at
all rather than a default one — that account is exactly the case the batch copy
exists for.

## [0.17.0] - 2026-07-26

Windows land where you mean them: edges snap, arrow keys nudge.

### Added
- **Windows snap to each other and to the screen.** Dragging a window — or one
  of its corners — locks its edges onto the edges nearby: another window, the
  neocom or ship HUD, or the screen itself. Two windows end up flush instead of
  a pixel apart, which was hard to get by hand because the canvas is drawn well
  under 1:1, where a single pixel of tremor is more than one pixel in-game. A
  thin amber line shows which edge was caught. Hold **Alt** to place freely.
  Only windows the canvas is actually showing attract a drag, so hiding clutter
  with the filter also stops it pulling on you.
- **Arrow keys nudge the selected window** by one pixel, **Shift+arrow** by ten.
  Hold an arrow and the window glides, saving once when you let go. Nudging a
  window that lives in a stack moves the whole stack, exactly as dragging it
  does. The arrows still belong to the filter box and to the panel's own
  coordinate fields whenever one of those has focus.

There is deliberately no grid to snap to: EVE has none, and a fixed step cannot
put two windows flush unless their sizes happen to be multiples of it — which
real EVE window sizes are not.

## [0.16.0] - 2026-07-26

A usable Layout editor: named windows, folded families, and one filter for the
list and the canvas.

### Added
- **Windows have readable names.** The Layout list and canvas showed raw client
  ids — `ChannelSettingsDlg_fleet_1038711647935`, `('corpassets', 1037014587783L)`,
  `76`. They now read as *Chat · fleet*, *Corp assets*, *Window stack · 76*, with
  the raw id still on hover, in the context menu, and one click from the
  clipboard. An id nobody has named falls back to a tidied-up version of itself,
  so it can look plain but never wrong.
- **Repeated windows fold away.** A real character carries a median of 296
  windows, most of them one per chat channel, mail or contact. Those now collapse
  into a single counted row you can expand.
- **A filter that drives the list *and* the canvas.** A search box plus
  `Open only` and `Hide clutter`, so narrowing the list narrows the picture too.
  A `showing N of M windows · reset` line keeps a filter honest — nothing ever
  disappears without saying so. `Hide clutter` drops the windows EVE spawns per
  conversation, item or dialog while keeping the ones you placed: on a real
  character it takes the canvas from 83 rectangles to 18. The filter is kept when
  you switch character, so you can compare the same subset across pilots.
- **Right-click menu** in the window list — *Show in tree*, *Copy window id*,
  *Select on canvas* — replacing the old jump-straight-to-the-raw-tree behaviour.
- **`Ctrl+F`** focuses the window filter while the Layout view is open.

### Fixed
- **The overview appearance checkboxes now read and write correctly on real
  accounts.** They only understood a boolean value, but the client stores these
  flags as a number on almost every account — 132 of the 135 corpus accounts
  that carry "hide corp ticker" store it that way. Reading one returned nothing,
  so the box showed unticked whatever the account had; ticking it then appended
  a second value instead of replacing the first, leaving a malformed entry whose
  stale original value the client kept using. Both halves are fixed: the box
  shows the real state, and toggling it takes effect.
- **The ship HUD offset now reads when stored as a whole number** rather than a
  decimal, as two corpus characters have it.

### Internal
- **A committed synthetic corpus** (`fixtures/`, see `fixtures/README.md`).
  The corpus gates previously skipped whenever `testdata/` was absent, so they
  asserted nothing on CI. Thirteen generated fixtures now cover the whole shape
  surface — including a negative `LONG`, a non-empty `REDUCE` tail, the `STREAM`
  opcode and the deprecated string opcodes, none of which appear in any real
  file — and the gates run everywhere. `EVE_SYNTHETIC_ONLY=1` skips the real
  corpus for a fast local loop.
- **The real corpus is deduplicated by content hash.** Of its 6140 files only
  413 are distinct; decoding the rest proved nothing. Full-suite corpus gate
  time drops from ~295 s to ~30 s.
- **New cross-feature projection gate** (`projection_smoke.rs`): every
  projection runs over every corpus file, and the curated fixtures assert each
  one actually reads the data it was built to contain. This is the guard against
  the "passes every hand-built unit test while reading nothing from a real file"
  class that produced the read-side bugs above.
- **New mutation gate** (`mutation_smoke.rs`): 24 tests that run each editor's
  write path through the app's real chain — mutate, reshare, encode, decode,
  bit-exact verify — and then re-project to prove the edit survived the round
  trip. It covers every mutation the app exposes, including on an account whose
  every list is reached through a shared reference, which is the shape that turns
  a structural edit into an unsaveable file. This is what caught the appearance
  write bug above.
- **Component tests.** The frontend had 4248 lines of Svelte covered by nothing:
  the pure-module suite could only reach logic that had already been extracted.
  `vitest` + `jsdom` + `@testing-library/svelte` now mount components, fire
  events and assert both on the DOM and on what the component sends over IPC.
  The two suites split by extension — `*.test.ts` stays on `node --test` with no
  framework, `*.spec.ts` is vitest — and `npm test` runs both. Conventions and
  the two traps that cost the most time are written down in
  `app/src/lib/test/README.md`.
- **First component coverage**: the HUD panel (the rounding and
  refuse-to-write rules that decide what text reaches the backend's parser) and
  the Batch view (which files a copy actually overwrites — "Everything" being
  exclusive, unpaired characters excluded from account-scoped aspects, apply
  sending the effective targets rather than the raw ticks, and a stale preview
  response not clobbering a newer one).
- **IPC contract test.** `invoke` is stringly-typed on both sides, so a renamed
  command or argument compiled, type-checked and then failed at runtime. All 53
  commands are now pinned: every one `api.ts` calls exists in Rust, every
  `#[tauri::command]` is registered in `generate_handler!`, no Rust command is
  unreachable, and the argument names agree.

## [0.15.0] - 2026-07-26

The ship HUD, fighter UI and neocom on the layout canvas.

### Added
- **The layout canvas draws your screen furniture.** The ship HUD (capacitor
  ring and module racks), the detached fighter UI, the neocom and the
  notification badge now appear alongside your windows, so you can arrange
  windows around the things that were previously invisible to the editor. The
  ship HUD drags sideways, the fighter panel and the badge drag freely, and
  each element can be selected from either the canvas or the panel — selecting
  one highlights the other.
- **A HUD & Neocom panel** with the exact numbers behind those elements: the
  ship HUD's offset and top alignment, the fighter UI's position and its
  detached/shown toggles, the neocom's width, and the notification badge's
  position. A value EVE has never stored shows its built-in default, marked as
  such, and is created the first time you edit it.
- **Account-wide fields are marked as such.** Four of the nine — the HUD's top
  alignment, the fighter UI's detached and shown toggles, and the neocom width
  — are stored once per account rather than per character, so a legend names
  the other characters an edit will also change.
- **The per-window "pinned" flag** is now listed with the other window flags in
  the Layout editor.

### Fixed
- **Window-stack edits could be silently discarded.** Un-stacking a window,
  adding one to a stack, reordering stack tabs or creating a stack marked
  nothing as unsaved — so unless you also dragged or resized something, Save
  skipped that file and the change was gone on reload.

## [0.14.0] - 2026-07-26

Overview pack import and export.

- **Treat this release's overview features as unstable.** Import and export of
  overview packs has not yet been tested against a running EVE client — it is
  covered by an extensive automated suite, including a check that 1771 real
  settings files survive a full export-and-reimport unchanged, but nothing here
  has been confirmed in-game. Back up your settings before importing a pack, and
  expect rough edges in the Overview editor generally while this milestone
  finishes.

### Added
- **Import and export overview packs.** The Overview editor reads and writes the
  same YAML file EVE's own Overview Settings → Import/Export uses, so a
  downloaded community pack can be applied to an account without logging in, and
  your own overview can be shared as a pack EVE loads. Every section the pack
  defines — presets, tabs, columns, state colours and colortags, blink flags,
  in-space ship labels and the overview toggles — replaces that part of the
  account; sections the pack omits are left alone, so modular "preset only"
  packs work. Importing marks the file dirty: you still press Save, and the
  usual backup is taken. Two limits worth knowing: importing a pack that
  defines tabs discards your per-tab column overrides (pack columns are
  account-global), and only the five colours EVE's own pack format has names
  for round-trip — a custom colour outside that list is dropped on both
  import and export rather than approximated.

## [0.13.0] - 2026-07-25

Overview states, colours and tags.

- The Overview editor now covers **state colouring** — the background colours and
  colortags EVE paints on an overview row for war targets, criminals, fleet
  members, standings and the rest — plus per-preset exceptions that hide or
  always show a state.

### Added
- An **Appearance** sub-tab in the Overview editor: tick which states colour a
  row, drag them into priority order (the first match wins), and set each state's
  background colour. Background and Colortag are managed separately, as EVE does.
- A state you have never customised shows EVE's own built-in colour instead of a
  blank swatch, and **Reset** clears your override to restore it.
- The six Appearance checkboxes — small colortags, small font, apply to
  structures, apply to other objects in space, fleet broadcasts at the top, and
  hide corporation ticker.
- An **Exceptions** editor on each filter preset: set any state to Show, Hide or
  Always show, so a preset can hide blues or always surface war targets whatever
  its group filters say.

### Changed
- The Overview editor is split into **Columns**, **Filters** and **Appearance**
  sub-tabs, mirroring EVE's own Overview Settings window.

## [0.12.0] - 2026-07-22

Overview filter preset contents, and full support for EVE's built-in presets.

- You can now edit **what an overview preset shows** — which ship and entity
  groups appear on it — and every EVE built-in preset is fully usable on any
  character, including ones whose overview was never customised.

### Added
- A group checklist under each preset in the Overview editor: tick which entity
  groups (ships, drones, structures, asteroids, NPCs, …) the preset shows, with a
  filter box and collapsible categories. (Editing a preset's *state* filters —
  standings, war targets, and so on — is still a later release.)
- EVE's built-in default presets now all appear in the per-tab preset dropdown,
  grouped ("Default profiles" / "Your profiles") and labelled with their real
  in-game names ("General: All", "Target Capsuleer: Carriers", "Mining: Mining",
  …) — so you can assign any of them to a tab, even on a character that has never
  customised its overview (where EVE stores none of them in the settings file).
- Editing a built-in default automatically creates an editable copy, leaving the
  original untouched; Duplicate does the same and switches the tab to the copy.

### Changed
- Built-in default presets are read-only — Rename and Delete are disabled on them.
  Edit or duplicate one to make a copy you can change.

## [0.11.0] - 2026-07-21

Overview filter presets.

- The Overview editor can now manage the account's overview filter presets and
  choose which preset each tab uses — assign a preset to a tab, and duplicate,
  rename, or delete presets. (Editing what a preset shows — its ship/entity types
  and state filters — comes in a later release.)

### Added
- A per-tab preset picker in the Overview editor, plus Duplicate / Rename /
  Delete controls for the account's presets. Renaming a preset re-points every
  tab that used it; deleting one moves its tabs to the neighbouring preset and
  won't remove your last preset.
- EVE's built-in presets (stored with internal ids like `DefaultPreset_639431`)
  now show their real names — Carriers, Fleet, Mining, and so on — resolved from
  the client's localisation data.

### Fixed
- Switching between characters on different accounts now refreshes the Overview
  and Autofill editors, so they no longer show the previous account's presets or
  remembered-text lists.

## [0.10.0] - 2026-07-20

Character-centric editing.

- The tool is now organised around characters, not files. The sidebar lists your
  characters (with their account shown alongside), and opening one loads its
  account file automatically — so you edit account-wide settings through a
  character instead of picking account files yourself.

### Added
- Account-scoped editors (Autofill and Overview columns) show a "shared account
  settings" note naming the other characters on the same account that an edit
  also affects.
- An unpaired character shows a prompt to link it to an account; once linked, the
  account editors appear without reopening the character.
- The raw Tree view has a Character-file / Account-file switch when an account
  file is loaded.

### Changed
- The sidebar lists characters only; account files are no longer separate entry
  points (open one directly with "Open file…" if you need to).
- The editor no longer has a Character/Account toggle — the tab you are in
  (Layout, Overview, Autofill, or Tree) determines which file you are editing.

## [0.9.0] - 2026-07-20

Overview tab management.

- Manage overview tabs from the Overview editor: create, rename, delete,
  reorder, and move tabs between overview windows.
- Add and remove overview windows. A window you add appears immediately in the
  Layout editor, ready to position — no need to launch EVE first.

### Added
- Overview tab management: create a tab (cloned from a sibling so it carries the
  brackets and colour a real EVE tab needs), rename it, delete it, drag-reorder
  tabs within a window, and move a tab to another overview window.
- Add and remove overview windows. Adding one drops you into the Layout editor
  with the new window selected so you can place it; removing the last window
  moves its tabs back to the first window.

### Changed
- Naming a new tab or window now uses an inline field instead of a browser
  prompt dialog.

### Fixed
- Switching between account or character files no longer briefly flashes the tree
  view before restoring your editor tab.
- A batch category copy skips the backup and write for a category the source
  file has nothing in, so the preview's write count is honest.

## [0.8.0] - 2026-07-19

Window stacks, and resize from any corner.

- The layout canvas now understands EVE window stacks: a stack of tabbed windows
  draws as one rectangle you can move and resize as a unit, instead of a pile of
  overlapping rectangles.
- Edit stack membership from the window panel — unstack a window, reorder its
  tabs, add a free window to a stack, or tab two free windows into a new stack.
- Resize a layout window from any corner, not just the bottom-right.

### Added
- Window stacks on the layout canvas: each open stack draws as a single tabbed
  rectangle at the stack's position, and moving or resizing it moves every window
  in the stack together (repairing any that had drifted). Click a tab to select
  that window.
- Stack membership editing in the window panel: unstack, reorder tabs, add a
  window to a stack, and create a new stack from two free windows. Stack groups
  are collapsible to keep a long window list navigable.
- Four-corner resize: a selected layout window can be resized from any of its
  four corners (previously only the bottom-right).

### Changed
- Moving or resizing a stack writes all of its windows' positions in a single
  step, so edits land quickly even for large chat stacks.

### Removed
- The dead "stack id" number field in the window panel (it never applied to real
  files), replaced by the stack grouping UI.

## [0.7.0] - 2026-07-18

Leaner settings files.

- When you edit overview columns or autofill lists, or copy settings between
  characters, the tool now writes a compact file instead of a larger, fully
  expanded one — closer to what EVE itself writes, and no longer leaning on the
  game to tidy the file up on next logout.

### Changed
- Structural edits (overview, autofill, batch copy) re-derive a compact,
  canonical shared-object layout before saving, so a saved file is no longer
  ~1.5× larger than it needs to be. This is internal to how files are written;
  what the settings mean, and how they load in-game, is unchanged.

## [0.6.0] - 2026-07-18

Batch apply, reimagined around the character.

- Copy a character's setup onto other characters — window layout, overview
  (columns, tabs, presets), autofill, or everything — and the tool works out
  which files to write.
- When a copy also changes settings shared by a whole account, the preview names
  the other characters it will affect, before anything is written.
- Warns when a target's screen resolution differs from the source's (a layout
  copy could otherwise land windows off-screen).

### Added
- A character-centric batch view: pick a source character and target characters,
  then choose what to copy — Window layout, Overview, Autofill, or Everything (a
  full clone of both the character file and its account file). Each written file
  is backed up first, and one file's failure never stops the rest.
- Cross-file copies. Overview and autofill live in the account file, so copying
  them to make one character match also changes every other character on that
  account — the preview lists those "collateral" characters (and notes that
  characters you have not paired yet are affected too) so it is never a surprise.
- A resolution-mismatch warning in the batch preview, and a Select all / Clear
  control for the target list.

### Changed
- Batch apply is now character-to-character and replaces the previous
  file-by-file batch flow; where each setting physically lives is handled for you.
- The Accounts view's character pickers are sorted by name, matching the file list.

### Fixed
- A layout copy can now actually reproduce another character's overview windows,
  because the account-scoped overview configuration is copied alongside the
  character's window positions — the limitation noted in 0.5.0.

## [0.5.0] - 2026-07-18

Batch apply, plus sidebar and editor quality-of-life improvements.

- Batch apply: copy settings from one file to many — whole file, window layout
  (character → character), or autofill lists (account → account) — each target
  backed up first.
- The editor keeps your current tab when you switch files.
- A filter box in the Autofill view.
- Collapsible sidebar panels and file-type groups; files sorted by name.
- Release notes are now generated from this changelog.

Heads-up: a layout copy is not window-for-window identical (overview-window
count is account-scoped), and the preview does not yet warn on resolution
mismatch — see the changelog for details.

### Added
- Batch apply — a new sidebar view that copies settings from one source file to
  many same-type targets. Copy the whole file, or just a category: window layout
  between characters, or remembered-text (autofill) lists between accounts. Every
  target is backed up before it is overwritten, one target's failure never stops
  the rest, and a per-target result is shown at the end. The source is picked in
  two steps — profile, then file — so characters with the same name across
  profiles are never ambiguous, and target files are sorted the same way as the
  sidebar list.
- The editor keeps your current tab when you switch files, instead of snapping
  back to Tree — so moving between characters while working on window layouts no
  longer bounces you out of the Layout canvas. It falls back to Tree only when the
  new file doesn't support the current tab.
- The Autofill view has a filter box that narrows the remembered-text lists as you
  type, matching the list name, its widget path, or any remembered entry.
- The sidebar's file-list side panels collapse to a thin rail so the editor can
  use the full width, and the Characters / Accounts / Other groups fold away.
  Files within each group are sorted by their resolved character name or account
  alias, with still-unresolved files listed below.

### Known limitations
- Copying window layout between characters does not make them window-for-window
  identical: how many overview windows exist is account-scoped, not stored in the
  character file, so EVE recreates any the source lacked at their default
  position on next login. Cross-file batch apply (overview settings and the
  account-scoped part of window layout) is planned for a following release.
- The batch preview does not yet warn when a target's screen resolution differs
  from the source's; window positions are absolute pixels, so copying between
  differently-sized displays can place windows off-screen (recoverable — every
  target is backed up).

## [0.4.0] - 2026-07-17

Autofill editor: edit the client's remembered text-input history.

### Added
- Autofill view — a Tree / Layout / Overview / Autofill switch on account files —
  edits the text the client autocompletes in search boxes, filters, and name
  fields. Per list, add an entry, edit one in place, remove, drag to reorder, or
  clear the list; a "Clear all remembered text" button wipes every list at once.
  Each list is labelled by a friendly name with its raw widget path shown
  alongside. Edits go through the usual backup → verify → atomic-write chain.

## [0.3.0] - 2026-07-16

Milestone 3: character names, character↔account association, and an
overview-columns editor.

### Added
- Character names, resolved from ESI (EVE's name service), shown in the sidebar,
  the open-file header, the backups panel, and the OS window title. Names are
  cached to disk; a Refresh button re-fetches them.
- Accounts view: give accounts readable names and associate characters with them.
  Pair a character manually, or use guided capture — snapshot your files, make an
  account-wide change in-game (e.g. toggle Camera Shake), log out, and the app
  detects which character and account advanced and confirms the pairing.
- Overview columns editor: per overview tab, show or hide columns, drag to
  reorder, and set each column's width. Visibility and order live in the account
  file, widths in the character file, and the app edits both through the usual
  backup → verify → atomic-write chain. An uncustomized tab shows the
  account-default columns until you edit it.
- The sidebar file list is grouped into Characters and Accounts, so an account
  whose alias matches a character's name is never ambiguous.

### Changed
- Editing a legacy overview file (`tabsettings`) upgrades it to the modern shape
  (`tabsettings_new`); the two are structurally identical.

### Fixed
- The Save button stays reachable on small windows — the file bar now wraps.
- Invalid character ids are remembered so they are not re-requested from ESI on
  every launch.

### Removed
- The "other files changed recently" warning on save (the backup already
  protects against the client overwriting changes on logout).

## [0.2.0] - 2026-07-15

Milestone 2: a visual window-layout editor for character files.

### Added
- Layout view, reached by a Tree / Layout switch on character files: a scaled
  mock of the game screen with one draggable, resizable rectangle per open
  window.
- Window list panel — every window with an open/closed toggle; selecting one
  shows its exact geometry (x, y, width, height), its stored flags (locked,
  collapsed, minimized, compact, …), and its stack id.
- Two-way editing: drag or resize on the canvas, or type exact numbers in the
  panel — both edit the same document and save through the existing
  backup → verify → atomic-write chain.
- Reveal in tree: right-click a value in the properties panel, or use the
  locate button on a search result, to jump to that value in the raw tree.

### Fixed
- Window ids stored as shared-object references now resolve to their real
  names; previously several could collapse to the same placeholder.

## [0.1.0] - 2026-07-15

First usable build (Milestone 1). Validated against the live client: a real
settings file edited through this app was accepted by EVE, with the edit
visible in-game.

### Added
- Blue-marshal codec (decoder + encoder) proven byte-identical on a
  5000-file corpus of real settings files.
- Desktop app: discovers EVE settings profiles, opens `core_char_*` /
  `core_user_*` files into an editable raw tree, with undecodable files shown
  read-only as hex. The profile whose files changed most recently is pinned to
  the top and expanded.
- Editing: change scalars in place, and add or remove entries in dicts, lists
  and tuples. Tuples matter — real entries (a chat channel, a
  `(timestamp, value)` leaf) are tuples, so without them there is nothing to
  build such an entry with.
- Search (Ctrl+F) over the value tree: filters to matching labels and values
  plus the path down to them, so nodes that are collapsed — nearly all of them
  — are still findable.
- Save chain: timestamped backup → encode-verify → conflict check → atomic
  write. No successful backup, no write — ever. One-click restore from the
  backups panel (itself backed up).
