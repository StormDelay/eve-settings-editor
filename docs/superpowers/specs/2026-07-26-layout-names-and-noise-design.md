# Layout — real names, and noise you control (design)

Status: designed 2026-07-26, not yet planned.

Milestone context: the **layout depth** milestone. Slice 3 (HUD furniture)
shipped as v0.15.0, slice 1a (canvas & list usability) as v0.16.0 and slice 1b
(precision editing) as v0.17.0. This spec is not one of the four planned slices:
it is a **ledger slice**, assembled from the layout-flavoured items that
accumulated in `docs/small-tasks.md` across those three releases. The project
owner asked for them bundled and shipped together.

## 1. Goal

Two of the three shipped layout slices left the same kind of hole. The editor
shows the player *our* name for a window when EVE has a better one sitting in
the file, and it decides for the player which windows are noise using tables
that can never be complete.

- A chat window reads `Chat · private`, because its id is
  `chatchannel_private_<guid>`. The channel's real name is in the character
  file, in a section the layout projection does not read. Searching for the
  channel by name finds nothing, which is how this surfaced during the slice 1a
  smoke.
- A window stack reads `Window stack · 76`, derived from a minted numeric
  container id. EVE's own label for that stack — `Character: Information` — is
  in the account file, likewise unread.
- `Hide clutter` runs off three hard-coded tables in `windowLabels.ts`. They are
  right often enough to be worth having and wrong often enough to be annoying,
  and no amount of curation closes the gap (§4 records why).

## 2. Scope

In scope:

1. Real chat-channel names, from the character file's `ui → chatchannels`.
2. Real stack labels, from the account file's `tabgroups → <id>_names`.
3. A user preferences file, `preferences.json`, whose first tenant is a
   per-window clutter override editable from the window list's context menu.
4. The layout debt sweep — the triaged 1a, 1b, HUD-furniture and window-stack
   follow-up minors from the ledger (§6).
5. In this slice's live smoke, confirm the HUD placement conventions that
   v0.15.0 shipped as assumptions, and correct the constants if they are wrong
   (§7).

Out of scope, and why:

- **Per-environment canvas views** (in space / NPC station / player structure).
  The project owner's call: it needs either curation or an in-game dock/undock
  capture to map windows to environments, which makes it its own slice.
- **Deleting orphaned stack frames.** The ledger item itself says to first check
  in-game whether EVE simply re-creates them. That is an experiment before it is
  a feature, and the write touches ten window-id-keyed flag dicts. Stays
  ledgered.
- **A discard-changes button.** Small and wanted, but it lives in the top bar
  and has nothing to do with the layout editor. Stays ledgered.
- **Two debt items that are their own passes:** `HudPanel`'s hardcoded hex
  colours (a theming pass across the whole app, not one panel) and the
  rejected-number-input desync (a pre-existing pattern shared with
  `WindowPanel`).

## 3. Real names — one backend change, two payloads

Both names are the same shape of problem: a display string EVE already wrote,
in a section `window_layout` does not read. So they are one change.

`crates/settings-model/src/windows.rs`:

```rust
pub fn window_layout(root: &Value, user: Option<&Value>) -> WindowLayout;
```

The new `user` root is the open account document, or `None` when none is open.
`ops::window_layout` already holds both slots, so it passes the account document
whenever one is loaded — which, since the character-centric rework, is whenever
the character is paired. With `None` the projection degrades to exactly today's
output, so an unpaired character keeps working and only loses the stack labels
(the chat names come from the character file and are unaffected).

**Chat names.** The character file's `ui → chatchannels` is a
`List[Tuple(kind, channelKey, label)]`, present in 367 of 384 corpus files. The
`channelKey` is the window id's suffix: `chatchannel_<channelKey>`. Build the
map once per projection and attach the label to the matching window.

**Stack labels.** The account file's root `tabgroups` section holds, per stack,
`<containerId>` → `Int` (the selected tab) and `<containerId>_names` → `Str`
(that tab's label). The container ids are the same numeric ids minted in the
character file's `windows → stacksWindows`, so the join needs no translation.
`Stack.container_label` — which today is a copy of the container id — carries
the real string when the account file has one.

> **Account-file gotcha, from the field notes:** in account files the root
> section key is a `Ref`, so `is_bytes`/`child_dict` miss it. Resolve section
> keys through `effective`. This has bitten the project before.

**On the wire.** `WindowRect` gains one optional field:

```rust
/// EVE's own display name for this window, when the file carries one.
/// None for the vast majority — only chat windows have one today.
pub name: Option<String>,
```

Not folded into the existing `label`, which deliberately carries the raw id and
is what `format-notes.md` and the raw tree speak.

**Frontend.** `describe(id)` stays pure and unchanged — it is the fallback, and
its tests stay valid. A thin addition in `windowLabels.ts`:

```ts
/** The name to show for a window: EVE's own when the file has one, else the
 *  derived one. The detail and family always come from the id. */
export function nameOf(w: { id: string; name?: string | null }): WindowName;
```

`WindowPanel`, the canvas labels and the stack tabs render `nameOf(w)`.
`windowMatches` builds its search haystack from `nameOf(w)` rather than
`describe(w.id)`, so searching a channel by its real name works for free — which
is the reported symptom that opened this item.

## 4. Preferences — `preferences.json`

The app has never had editor-side preferences. This introduces the file, with
the clutter override as its first tenant; the shape is meant to be added to.

**Location.** `app_config_dir()/preferences.json` —
`%APPDATA%\io.github.stormdelay.eve-settings-editor\preferences.json` on
Windows. `app_config_dir` is Tauri core, so no new plugin. Nothing is written
until the user changes something, so a user who never touches a preference never
grows a file.

**Shape** — `app/src-tauri/src/prefs.rs`, using the `serde`/`serde_json` already
in the crate:

```rust
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Preferences { pub layout: LayoutPrefs }

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct LayoutPrefs {
    /// Window ids the user forced INTO the clutter set.
    pub clutter: Vec<String>,
    /// Window ids the user forced OUT of it.
    pub visible: Vec<String>,
}
```

`#[serde(default)]` on every struct **is** the extensibility contract: a later
slice adds a field, or a sibling section beside `layout`, and files written by
today's build still load. There is deliberately no `version` field — a version
number with no migration code behind it is decoration, and the shape is
self-describing enough to branch on if a real migration ever arrives.

**Failure policy.** This is user data the app now owns, so it is not clobbered
silently. Missing file → defaults, no complaint. Unparseable file → rename it to
`preferences.json.bad` (replacing any previous one), then proceed with defaults,
so a hand-edit gone wrong is recoverable. A failed *write* surfaces as the usual
error dialog and leaves the in-memory state alone.

**Commands.** `preferences() -> Preferences` and `set_preferences(prefs)`, whole
document rather than per-setting, so adding a preference later means adding a
field and not another IPC pair. The file is a few hundred bytes and there is one
UI; granular writes would buy nothing.

**Testability.** `load_from(path)` / `save_to(path)` take the path, so the Rust
tests drive them in a temp dir — round trip, missing file, corrupt file (and
that the `.bad` rename happened), and forward compatibility (a file carrying an
unknown key still loads).

**Frontend.** `prefs.svelte.ts` loads once at startup into `$state` and writes
through on each change. Overrides reach the predicate explicitly:

```ts
export interface ClutterOverrides { clutter: Set<string>; visible: Set<string> }
export function isClutter(id: string, o?: ClutterOverrides): boolean;
```

`o.visible` wins, then `o.clutter`, then the built-in tables — the two sets are
mutually exclusive by construction (setting one removes from the other), so the
precedence only matters against a hand-edited file. `windowMatches(w, f, o?)`
and `visibleIds(windows, f, o?)` take it as a parameter rather than as a field
on `WindowFilter`: it is a preference, not a filter setting, and it persists
while the filter deliberately does not.

**UI.** Two items in the window list's existing right-click menu (`openMenu` in
`WindowPanel.svelte`, which already offers *Show in tree*, *Copy window id*,
*Select on canvas*): **Treat as clutter** on a window the tables consider
ordinary, **Stop treating as clutter** on one they don't — one item, never both,
labelled for what the click will do.

Overrides must never be invisibly in effect, so the existing
`showing N of M windows · reset` line grows `· N overridden · clear` whenever
any override is set, `clear` emptying both lists. That mirrors how the filter
counter already keeps a carried-over filter honest.

**Why the built-in tables can never be enough** (measured, and the reason this
item exists): EVE's `openWindows` flag *accumulates* — it records what EVE would
restore, not what is on screen, and is never reliably cleared. One real
character: 381 windows, 134 flagged open, 83 canvas draw units, versus about 9
windows actually visible in the client. The open set includes one-shot modals
(`setQuantityPopup`, `enterShipPassword`, `ship_name_dialog`). Nothing else in
the file separates them — `minimizedWindows` true for 1, `collapsedWindows` for
0, 82 distinct rects among the 134. So curation cannot win, and one click per
window can.

## 5. What this does not change

`Hide clutter` stays off by default and stays a view filter: nothing here writes
to the EVE files, toggles `openWindows`, or changes what EVE restores. The
override lists are editor state in the editor's own file.

## 6. The debt sweep

Ledger items closed by this slice, each a one-liner unless noted:

- **`hex()` duplicated byte-for-byte** in `hud.rs` and `windows.rs` — fold into
  `treewalk` as `pub(crate)`. The ledger says to do it next time either file is
  touched; §3 touches `windows.rs`.
- **The `40`px cascade offset** in `overview_tabs.rs::add_overview_window_geometry`
  — name it `const OVERVIEW_WINDOW_OFFSET: i64 = 40`.
- **The tautological `drawnWindowCount` test** in `layout.test.ts`, which
  compares `stackUnits(x, null)` against `stackUnits(x)` — the same call, so it
  never exercises the regression it names. Replace with a `Set`-based filtered
  case where a stack container matches the filter and none of its members do.
- **The missing `UnknownWindow` guard test** for `remove_overview_window` — the
  reviewer's pick as most worth doing.
- **`hud.rs::section()`** doesn't push `Step::SharedInner` when it resolves a
  `Shared` root, so a `Shared`-wrapped root would yield paths that fail
  `resolve_mut`. Unreachable on real files, safe failure; fix while in the file.
- **`locate()`'s `Option<String>` half** is computed and discarded on the writer
  path.
- **`mint`'s three separate `Err(HudError::NoSection)` guard returns.**
- **The stack reorder button** enabled on the first *visible* member when a
  hidden member precedes it at true index 0 (slice 1a). Correct per the
  true-index contract, but it swaps with a row the filter is hiding — disable
  the control in that case rather than change the contract.
- **The `container_label` friendly-label item** — closed by §3, which replaces
  the derived label with EVE's own.

Deliberately left open: `HudPanel`'s hardcoded hex colours (a theming pass), the
rejected-number-input desync (a shared pre-existing pattern), the
`set_hud_field` reshare-on-every-write (its own measurement task), and the
account-scoped-rows read-only asymmetry.

## 7. Testing

- **Rust, `settings-model`:** `window_layout` with and without a `user` root;
  a chat window whose `channelKey` matches gets its label and one that doesn't
  keeps the derived name; a stack with a `tabgroups` entry takes it and one
  without keeps the container id; and the `Ref`-wrapped account section resolves
  through `effective` (the gotcha this project has hit before — assert against a
  fixture shaped like a real account file, not a hand-made flat one).
- **Rust, `prefs.rs`:** round trip, missing file, corrupt file plus the `.bad`
  rename, and an unknown-key file still loading (the forward-compatibility
  contract §4 claims).
- **Frontend, `node --test`:** `nameOf` prefers the real name and falls back to
  `describe`; `isClutter` with overrides in all four combinations; the filter
  matches a channel by its real name — the reported symptom, as a test.
- **`ipc.test.ts`** pins the two new commands automatically, since it walks both
  sides.
- **Live smoke**, as every slice: a real character with chat windows open, one
  paired account with stacks, checking the names in the list, the canvas, the
  stack tabs and the filter; the override round trip including a restart, and
  that `preferences.json` lands where §4 says; plus the HUD-convention
  confirmation below.

## 8. The HUD convention check

v0.15.0 shipped the ship HUD, fighter UI and badge with **assumed** geometry:
`HUD_NOMINAL`'s sizes, the centre-relative ship offset, and the top-left point
convention are all guesses, flagged as such in the changelog and in `layout.ts`.
`shipOffsetFromX`/`hudPointFromRect` exist as the matched inverses to correct
together.

This slice's smoke is the cheapest opportunity to settle it: with a real client
running, move each element in-game, save, and compare the file's numbers against
what the canvas draws. If the convention differs, correct the constants and the
matched pair — a small, contained change. If it holds, say so in `layout.ts` and
delete the hedging from the comments. Either outcome is worth having; leaving it
unknown across another release is not.
