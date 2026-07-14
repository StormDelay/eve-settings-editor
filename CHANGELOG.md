# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Tuples are editable sequences: add or remove elements, and build a new one
  with the "empty tuple" value kind. Real entries (a chat channel, a
  `(timestamp, value)` leaf) are tuples, so they could not be created before.
- The rescan button reports what it found instead of changing nothing visibly.

- The profile whose files changed most recently is pinned to the top of the
  sidebar and expanded; the rest stay alphabetical and collapsed.
- Search (Ctrl+F) over the value tree: it filters to matching labels and
  values plus the path down to them, so collapsed nodes are findable. The
  webview's own find-on-page only ever saw the handful of nodes on screen.

### Fixed
- Profiles sharing a server and profile name (two installs, both
  `settings_Default`) no longer show the same sidebar label — the install
  name disambiguates them, and the full path is on the tooltip.
- Rejected input in the add-entry form (`df` as an int) no longer throws the
  form away with it: the message appears next to the offending field and the
  entry being typed survives.
- Mutation errors read as sentences instead of debug-printed Rust variants.
- Right-click no longer opens the webview's stock context menu.

## [0.1.0] - 2026-07-14

First usable build (Milestone 1).

### Added
- Blue-marshal codec (decoder + encoder) proven byte-identical on a
  5000-file corpus of real settings files.
- Desktop app: discovers EVE settings profiles, opens `core_char_*` /
  `core_user_*` files into an editable raw tree (scalar edits, add/remove
  entries), with undecodable files shown read-only as hex.
- Save chain: timestamped backup → encode-verify → conflict check → atomic
  write. No successful backup, no write — ever. One-click restore from the
  backups panel (itself backed up).
