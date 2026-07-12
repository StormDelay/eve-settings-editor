# EVE Settings Editor — Design

**Date:** 2026-07-12
**Status:** Approved pending user review
**Audience:** Personal tool shared with friends/corp — "download & run" simplicity, solid robustness. No public-release pipeline in V1, but the project is structured so one can be switched on later (see §11).

## 1. Purpose

A cross-platform (Windows/macOS/Linux) desktop application that edits the EVE Online client settings files (`core_char_<id>.dat`, `core_user_<id>.dat`). Core capabilities:

1. Place and resize UI elements (overview window and others) via a visual canvas **and** direct property editing.
2. Edit the overview window's column configuration (visibility, order, widths).
3. Edit remembered-string / autofill suggestion lists (search suggestions confirmed; others discovered during M0).
4. Load any existing settings file to view and edit **all** its settings via an editable raw tree.
5. Batch-apply chosen setting categories from one character to many (Milestone 4).
6. Display character and account names alongside the numeric IDs from file names, wherever IDs appear in the UI (see §6, *Name display & resolution*).

Target: the **current Tranquility client** (user plays actively; fresh settings files can be generated on demand for validation).

## 2. File format facts (established)

- Settings live at `%LOCALAPPDATA%\CCP\EVE\<install>_<server>\settings_<profile>\` on Windows; equivalent paths under `~/Library/Application Support/CCP/EVE/` (macOS) and Wine/Proton prefixes (Linux, e.g. Steam `compatdata/8500/pfx/drive_c/users/steamuser/AppData/Local/CCP/EVE/`).
- `.dat` files are CCP's **"blue marshal"** binary serialization (magic byte `0x7E` followed by a 32-bit shared-object count, then opcode-tagged values). The format was reverse-engineered by the community; Entity's *reverence* library source is the de-facto specification.
- Character-level UI state (window geometry, overview config) is in `core_char_*.dat`; account-level settings in `core_user_*.dat`. Exact key paths for geometry/columns/suggestions are pinned down in Milestone 0.
- ~50 real historical files (2020–2022) exist on the developer's machine and serve as a round-trip test corpus, supplemented by fresh files from the current client.
- **Risk:** local files predate 2026; the current client may have evolved the format (a `core_public__.yaml` hints at partial YAML migration). Milestone 0 validates against fresh files before anything else is built.

## 3. Architecture

Tauri 2 application. **All file I/O, parsing, and mutation happen in Rust**; the web frontend is a view/edit layer exchanging JSON with the core and can never touch bytes on disk.

### Rust workspace — three crates

1. **`blue-marshal`** (pure library, no app dependencies)
   Decoder/encoder for CCP blue marshal → generic value tree (dicts, lists, tuples, ints, strings, shared references, blobs) and back.
   **Lossless by construction:** any construct the decoder cannot fully interpret is preserved as an opaque byte span the encoder re-emits verbatim. No EVE semantics in this crate.

2. **`settings-model`**
   EVE semantics over the raw tree. Locates known structures and exposes them as typed, JSON-serializable **categories**:
   - `WindowLayout` — per-window x, y, width, height, plus stored flags (pinned, collapsed, minimized).
   - `OverviewColumns` — column visibility, order, widths.
   - `SuggestionLists` — every remembered-string collection found in the file.
   Everything unrecognized remains reachable through a generic raw-tree projection. **Categories are the unit of batch apply** (M4) — they are extractable from one document and applicable to another.

3. **`app`** (Tauri binary)
   Commands: discover profiles, load file, apply mutation, save (backup → verify → atomic write), list/restore backups, batch apply. Owns the canonical in-memory document; the UI holds only a rendered copy.

### Frontend

TypeScript + Svelte. Views: file picker, layout canvas + properties panel, overview-columns editor, autofill-lists editor, raw tree editor, batch-apply flow, backups panel.

## 4. Data flow

- **Discovery:** on launch, scan the OS-standard EVE locations; list every server profile and its `core_char_*` / `core_user_*` files with size and mtime. Manual open-file/folder fallback for non-standard installs.
- **Load:** bytes → `blue-marshal` decode → `settings-model` categories + raw tree → JSON snapshot to UI. Original bytes kept in memory for the session.
- **Edit:** every UI change is sent to Rust as a mutation, applied to the canonical document; UI re-renders from the response so the two cannot drift.
- **Save:** see the invariant chain below.

## 5. Save-path invariant chain

Executed in order; **any failure aborts the save with the on-disk file untouched**:

1. **Encode** the document to bytes.
2. **Verify:** decode our own output and structurally compare with the in-memory document. Mismatch ⇒ abort. (Catches writer bugs before they reach disk.)
3. **Conflict check:** if the on-disk file changed since load (mtime/hash), warn and require explicit confirmation. A standing warning appears when saving into a profile whose files changed within the last few minutes (likely a running client).
4. **Backup (hard requirement):** copy the current on-disk file to `<settings folder>/eve-settings-editor-backups/<name>.dat.<UTC ISO-8601 timestamp>.bak`. **No successful backup ⇒ no write, ever.** All backups are retained by default (files are ~100 KB).
5. **Atomic write:** write a temp file in the same directory, then rename over the original.

**Restore:** a backups panel lists timestamped backups per file with one-click restore; restore itself runs the backup step first, so it is also reversible.

## 6. Editors

### Layout canvas
Scaled mock of the game screen (dimensions from the character's stored resolution) with one rectangle per window found in the file; unrecognized windows appear labeled by their internal key. Drag to move, handles to resize, optional snap-to-grid. The properties panel shows the selected window's exact editable values (x, y, width, height, stored flags); canvas and panel edit the same state — typing a number moves the rectangle.

### Overview editor
Column list for the overview window: show/hide, drag-to-reorder, per-column width. Overview *filter presets* (in-game YAML-exportable tab contents) are **not** given a purpose-built UI in V1; they remain editable via the raw tree.

### Autofill editor
One editable string list per discovered remembered-string collection (add, remove, reorder, clear all). Search suggestions are a confirmed target; M0 discovery enumerates the rest.

### Raw tree editor
The whole document as an expandable tree: type-aware inline editing for scalars, add/remove entries in dicts and lists, read-only hex view for opaque blobs. Serves as fallback editor for anything without a purpose-built UI and as the format-discovery instrument.

### Name display & resolution
Wherever a character or account ID appears (file picker, batch-apply source/target lists, backups panel, window titles), the UI shows a human-readable name alongside it whenever one is available.

Resolution sources, in priority order:

1. **Local extraction (primary).** A string-scan of real files already shows player names embedded in the settings data (e.g., character names in `core_user_*.dat`, plausibly from the login character-select screen). **M0 decodes the files properly to determine whether each file's own character/account name is reliably present and linked to its ID** — as opposed to names of other players (fleet members, contacts) that also appear. If reliable, local extraction is the primary and default source: zero network, works offline, always current with the file.
2. **ESI fallback (character IDs only).** Where local extraction is absent or unreliable, character IDs resolve via ESI's public `POST /universe/names` endpoint — no authentication, batched (one request for all unresolved IDs), cached persistently in the app's data directory. Offline or on ESI failure, the UI falls back to bare IDs; cached names keep working. A settings toggle disables all network access; this is the app's **only** network call. Account IDs have no public API and never resolve online.
3. **User aliases + correlation (accounts).** The user can assign a persistent **alias** to any account ID. M0 also investigates a correlation heuristic — char and user files written in the same login session (matching modification timestamps, or cross-references found inside the files) let the UI suggest "account of *CharacterName*", clearly marked as a guess and confirmable into an alias with one click.

### Batch apply (M4)
Flow: pick a **source** file → choose categories (window layout / overview columns / suggestion lists) → multi-select **target** files (char categories to char files, user categories to user files) → preview summary → apply. Each target runs the full save-path invariant chain independently; one failure does not halt the others; a per-target success/failure report is shown at the end.
**Geometry caveat:** window geometry is absolute pixels; the preview warns for each target whose stored resolution differs from the source's.

## 7. Error handling

- File fails to parse → opens in hex view with error offset; never writable.
- Encode-verify failure → save aborted; diagnostic dump offered for bug reporting.
- Backup failure → save aborted.
- Client format drift after a game patch → fails safe as one of the above: worst case is "can't open / can't save," never a corrupted file.

## 8. Testing

- **Codec unit tests** per opcode.
- **Golden corpus:** all ~50 real historical files plus fresh current-client files. Target: decode → encode reproduces the input **byte-identically**; if a normalization makes that impossible for some construct, the fallback gate is decode-equivalence (re-decode the output and require structural equality), with the normalization documented. The corpus contains personal data and stays on the developer's machine (never committed); corpus tests run locally, while CI runs codec unit/property tests on synthetic fixtures.
- **Property-based fuzz tests** on encode/decode.
- **Save-path integration tests** on temp copies, verifying the full backup-then-atomic-write chain and its abort behavior.
- **Manual M0 checklist:** change a setting in-game → diff files → confirm mapping; edit a file with the tool → launch game → confirm the change took effect and the client accepts the file.

## 9. Milestones

- **M0 — Format validation & mapping:** parse fresh current-client files; diff in-game changes (move a window, run a search) against file changes to pin down where geometry, columns, and suggestions live. Also determine whether each file's own character/account name is stored inside it (local name extraction, the primary source for §6 *Name display & resolution*), and investigate the account↔character correlation heuristic. Explicitly allowed to revise this spec.
- **M1 — Core:** codec + raw tree editor + full save chain with backups. The tool is already useful here.
- **M2 — Layout canvas** + properties panel.
- **M3 — Overview columns + autofill editors;** ESI name resolution + account aliases; packaging for Windows/macOS/Linux.
- **M4 — Batch apply** across characters/accounts.

## 10. Deferred (designed-for, not built)

- Settings diff between two characters.
- Purpose-built overview filter-preset editor.

## 11. Public release path (optional, designed-for)

V1 targets friends/corp, but nothing in the design may block a later public release. Practices adopted **from M1** so the door stays open at near-zero cost:

- **Semantic versioning + tagged releases + changelog** from the first shared build.
- **CI builds** (GitHub Actions + Tauri's official action) already produce the per-OS artifacts (`.msi`/`.exe`, `.dmg`, `.AppImage`/`.deb`) that a public release would ship — "release" is then just making the artifacts public.
- **No personal data anywhere in the repo** (already required by §8: local-only corpus, synthetic CI fixtures).
- **No telemetry.** The sole network call is ESI ID→name resolution (§6) — public data, unauthenticated, disableable via a settings toggle — so a public README has exactly one network behavior to disclose.
- **Fail-safe file handling** (§5, §7) is already public-grade: unknown/corrupt files can never be corrupted further.
- **License chosen up front** (MIT, matching EVE community tooling norms) so early corp-mate distribution doesn't create relicensing friction.

Activated **only if/when going public** (tracked, not built):

- **Code signing:** Windows Authenticode certificate and macOS Developer ID + notarization (both cost money/accounts; unsigned builds are acceptable for corp-mates who trust the source). CI is structured so signing is an added step, not a rework.
- **Auto-update** via the Tauri updater (requires signing keys; pointless before signing exists).
- **Support surface:** README with install/usage docs, issue templates, and a compatibility statement naming the last EVE client version validated against (M0's checklist becomes the recurring post-patch validation routine).
- **CCP compliance note:** the tool only edits local settings files — the same ones players already copy by hand — and never touches the client process, network traffic, or game automation; a public README states this scope explicitly.

## 12. Key decisions log

| Decision | Choice | Why |
|---|---|---|
| Codec strategy | Native Rust reader **and** writer (Approach A) | Full parse needed for raw tree anyway; risk contained by verify step, golden corpus, and mandatory backups. Rejected: Python sidecar (bundling pain, unmaintained libs), surgical byte-patching (offset fixups ≈ full writer, more fragile). |
| Stack | Tauri 2, Rust core, Svelte/TS frontend | Small single binary per OS = best "download & run" for corp-mates. |
| Backups | Timestamped copies in a subfolder **before every write**, keep-all | User requirement; hard invariant of the save path. |
| Multi-character | Batch apply is M4; category model designed for it from M1 | User promoted it from "deferred" into V1 scope. |
| Editing model | Visual canvas + always-available direct property editing | User requirement: never rely purely on visuals. |
| Names next to IDs | Local extraction from the files first (M0 validates); public ESI as character-ID fallback; aliases + correlation for accounts | User requirement; names demonstrably exist inside real files, so avoid network when local data suffices. Accounts have no public API. |
