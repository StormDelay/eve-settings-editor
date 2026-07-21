# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
