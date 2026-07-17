# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.5.0] - 2026-07-18

Batch apply: copy settings from one file to many, plus sidebar and editor
quality-of-life improvements.

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
