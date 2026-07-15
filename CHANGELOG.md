# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
