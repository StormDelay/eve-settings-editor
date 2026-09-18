# Fleet editor (design)

Status: designed 2026-09-18, not yet planned.

Milestone context: a new **Fleet** view over settings the app has never modelled
— EVE's Broadcast Settings dialog, the watch list's per-member colours, and the
fleet-warp formation panel — plus a **Fleet** aspect so a batch copy or a preset
can carry them between characters and accounts. Self-contained: two `ui`
sections, no structural editing, one new network call.

## 1. Goal

EVE's fleet configuration is scattered across two dialogs and a right-click
menu, each of which edits **one character or one account at a time**. A player
with eight accounts sets the same broadcast colours eight times; a player who
colour-codes fleet-mates in the watch list redoes it on every alt. The files
hold all of it, and the app can already turn a character id into a name.

This slice ships:

1. **A Fleet view** with three panels — Broadcast settings, Watch list colours,
   Formation — each editing the file it lives in, with EVE's own labels and
   EVE's own nine-colour palette.
2. **Add a watch-list colour for a character by name or id**, resolving names
   through ESI and caching the answer beside the id→name cache the app already
   keeps.
3. **A Fleet aspect** for batch copy and presets, so one character's setup
   moves onto the others.

Out of scope, and said in the view: **saved fleet setups** ("Form fleet with
setup → …") are stored on CCP's servers. The only local trace is the name
history of the "Store fleet setup" dialog, already editable under Autofill →
"Saved fleet setup name". Verified on 2026-09-18: the owner's eight setup names
appear in exactly one place in the files — the account's `editHistory` list for
that dialog — and two of them appear nowhere at all, yet the client lists them.

## 2. The fleet model (confirmed from the corpus)

Measured over 513 distinct-by-content files (194 character, 125 account) from
every corpus snapshot through 2026-09-18, plus two targeted captures on the
owner's B1 account (`core_user_13375506`, character Holy Storm `96821229`)
taken the same day: one after toggling every broadcast checkbox and colour, one
after unticking a never-keyed type and clearing one colour. Both snapshots are
kept in `testdata/corpus/` (`2026-09-18T104725Z_fleet-broadcasts`,
`2026-09-18T110624Z_fleet-broadcasts-2`) because they are the only files that
carry sixteen colour keys.

### 2.1 Wire shape

Everything sits under the root `ui` section of one file or the other, and every
leaf is the file-wide `(FILETIME, value)` wrapper:

```
core_user_<id>.dat  → ui
    listenBroadcast_<Type>          (ts, Int 0|1)        sixteen types
    listenBroadcast_ShowOwnBroadcasts (ts, Int 0|1)      ┐ written together,
    ShowOwnBroadcasts               (ts, Int 0|1)        ┘ same value, same stamp
    fleet_broadcastcolor_<Type>     (ts, (Float, Float, Float)) | (ts, None)

core_char_<id>.dat  → ui
    fleet_watchlistcolors           (ts, { Int charID : (Float, Float, Float) })
    setFleetFormation               (ts, Int)
    setFleetFormationSize           (ts, Int)            metres
    setFleetFormationSpacing        (ts, Int)            metres
    fleetfinder_showGroupAndHighStandingsFleets (ts, Int 0|1)
```

The root `ui` key is `Ref`/`Shared` in real account files, exactly as `hud.rs`
records, so section lookup resolves through `treewalk::section`. Colour tuples
are three floats, not the four the overview's `stateColors` uses.

### 2.2 Broadcast types

EVE's dialog lists seventeen rows. The internal token for each was recovered by
toggling every one on a live account and reading the keys back:

| Row in EVE's dialog | `<Type>` | Listen default | Colour default |
|---|---|---|---|
| Always show my own broadcasts | `ShowOwnBroadcasts` (+ bare key) | 0 | — |
| Broadcast: Need Armor | `HealArmor` | 0 | green |
| Broadcast: Need Capacitor | `HealCapacitor` | 0 | yellow |
| Broadcast: Need Backup | `NeedBackup` | 0 | none |
| Broadcast: Target | `Target` | 1 | red |
| Broadcast: Need Shield | `HealShield` | 0 | blue |
| Broadcast: Warp to | `WarpTo` | 1 | none |
| Broadcast: Travel to | `TravelTo` | 1 | none |
| Fleet event | `Event` | 1 | none |
| Broadcast: Jump to | `JumpTo` | 1 | none |
| Broadcast: Align to | `AlignTo` | 1 | none |
| Broadcast: Repair Target | `HealTarget` | 1 | none |
| Broadcast: In Position at | `InPosition` | 1 | none |
| Broadcast: Spotted an Enemy | `EnemySpotted` | 1 | none |
| Broadcast: Request That the Fleet Hold Position | `HoldPosition` | 0 | none |
| Broadcast: Jump to Beacon | `JumpBeacon` | 1 | none |
| Broadcast: At Location | `Location` | 1 | none |

The order is EVE's row order, and the view keeps it.

**How the defaults were established.** A `listenBroadcast_*` key is written
**only when its checkbox is toggled** — the second capture's `WarpTo` key
appeared the moment it was unticked, and the nine types that stayed ticked have
no key in any of 513 files. So absence means "never touched", and the default
is what the dialog shows for an untouched row. For the nine never-keyed types
that is ticked (the owner's screenshot). For the seven that every account
carries, the values themselves say it: `HealArmor`, `HealShield`,
`HealCapacitor`, `HoldPosition`, `NeedBackup` are `0` in 120–129 of 129 account
files, `Target` and `InPosition` are `1` in 119–128. Nobody unticked five boxes
on every account by hand; those are the client's defaults, written on first
open of the dialog.

The four colour defaults are the values every one of 129 files stores (128 for
Shield/Armor, 103 for Capacitor — older files lack that key), identical
everywhere: they are the client's own floats for the swatches the dialog shows
untouched.

### 2.3 The top checkbox writes two keys

"Always show my own broadcasts" writes `listenBroadcast_ShowOwnBroadcasts`
**and** a bare `ShowOwnBroadcasts`, at the same instant with the same value —
3 of 3 corpus files that have one have both, and the live capture stamped them
identically. The editor writes both. Reading uses the `listenBroadcast_` one.

### 2.4 Colour states, and what ✕ does

A colour leaf has three wire states, and the editor must distinguish all three:

| State | Wire | Meaning |
|---|---|---|
| absent | no key | the type's default (§2.2 — a colour for four types, none for the rest) |
| cleared | `(ts, None)` | "no colour" — EVE's ✕ in the picker (captured 2026-09-18 on `Location`: the key stayed, the value became `None`) |
| set | `(ts, (r, g, b))` | the colour |

The editor writes `set` and `cleared`; it never removes a key. Nothing a
removal does is unreachable — for the four types with a default colour, that
default is one of the nine swatches.

**Opening the dialog re-stamps every existing broadcast key** with one
FILETIME (all 25 keys in the second capture carry `134342031538507931`). The
editor preserves stamps on overwrite as every other editor does and mints
absent keys with a zero FILETIME; this is recorded so the next person reading a
capture diff does not take the stamp churn for a write.

### 2.5 The palette

EVE's "Select Color" popup — the same one in Broadcast Settings and on a
watch-list member — offers nine swatches and ✕. Walking it left to right on
nine uncoloured types yielded every float exactly:

| name | r, g, b | evidence |
|---|---|---|
| yellow | 1.0, 0.7, 0.0 | 517 watch-list entries; `HealCapacitor` default |
| orange | 1.0, 0.35, 0.0 | overview `PALETTE`; live `WarpTo` |
| red | 0.75, 0.0, 0.0 | `Target` default; overview `PALETTE` |
| green | 0.1, 0.6, 0.1 | `HealArmor` default |
| teal | 0.0, 0.63, 0.57 | live `JumpTo` — the one value no file had ever held |
| blue | 0.2, 0.5, 1.0 | 863 watch-list entries; `HealShield` default |
| darkBlue | 0.0, 0.15, 0.6 | overview `PALETTE`; live `HealTarget` |
| black | 0.0, 0.0, 0.0 | overview `PALETTE`; live `InPosition` |
| white | 0.7, 0.7, 0.7 | overview `PALETTE`; live `EnemySpotted` |

Names are labels for tooltips and the datalist; unlike `overview_pack::PALETTE`
they are never written to a file, so EVE's internal name for teal does not
matter. The overview palette stays as it is — its names are pack vocabulary,
and whether EVE's overview "green" is this green is a separate capture.

### 2.6 The watch-list map

`fleet_watchlistcolors` holds 7–16 entries in 78 % of the files that have it,
keyed by character id, valued by a bare three-float tuple. Only blue (863),
yellow (517) and red (2) have ever been chosen. `fleetWathlistMemberInfo`
beside it is `{}` in every file and is left alone.

All 18 distinct watch-listed ids in the corpus are `Int`. Character ids are
still below 2³¹ but will not stay there; a key for such an id must be a `Long`,
and the client writes ids above the `Int` range as **minimal-width
little-endian two's-complement `Long`s** (2,321 of 2,322 id-sized Longs in a
sampled file are 6 bytes for values below 2⁴⁸, one is 5). The writer follows
that convention; it is unit-tested and cannot be corpus-tested yet.

### 2.7 Formation and fleet finder

`setFleetFormation` is `0` in 222 of 225 files (`1` once, `3` twice); size is
20 000 in 160 (10 000–50 000 elsewhere); spacing 2 000 in 154 (1 000–5 000).
The formation ids' in-game names have not been captured, so the id is edited as
a number — at 98 % zeros it is not worth a capture yet.
`fleetfinder_showGroupAndHighStandingsFleets` is `1` in 208 of 231.

### 2.8 Not modelled, and why

`fleetHistoryFilter`, `fleetFinderBroadcastsVisible`, `fleetfinder_{scope,
range,standing}Filter`, `fleettabs`, `fleetComp` — which tab or filter the
fleet windows were left on; `fleetAdvert_lastAdvert*` — the last advert form;
`fleetReconnect` — session state. Window state and session state belong in the
raw tree, per the field reference's standing rule.

## 3. Backend

### 3.1 `crates/settings-model/src/fleet.rs`

**Scalars reuse `hud.rs`.** `hud.rs`'s `Field`, `Located`, `locate`, `leaf`,
`key_present`, `scalar_text`, `mint`, `build_scalar` and `section_dict_mut`
currently read the module constant `FIELDS`. They become `pub(crate)` and take
the table as a parameter; `hud.rs`'s public surface and every test are
unchanged. `fleet.rs` declares its own table:

| name | file | key | kind | default |
|---|---|---|---|---|
| `listen_<type>` ×16 | account | `listenBroadcast_<Type>` | Int | §2.2 |
| `listen_show_own` | account | `listenBroadcast_ShowOwnBroadcasts` | Int | 0 |
| `formation`, `formation_size`, `formation_spacing` | char | `setFleetFormation{,Size,Spacing}` | Int | 0, 20000, 2000 |
| `finder_group_only` | char | `fleetfinder_showGroupAndHighStandingsFleets` | Int | 1 |

Checkboxes are `Int` on the wire, so they stay `HudKind::Int` — a `Bool` field
would write `Value::Bool`. Two table rows share the name `listen_show_own` — one per key of §2.3 — so
`set_fleet_field` writes **every** row carrying the requested name through the
same locate/mint path, and the projection reports the first. That is the whole
of the two-key rule; nothing else in the table repeats a name, and a test pins
that.

**Colour leaves.** `project` reads each `fleet_broadcastcolor_<Type>` as
`Colour::{Absent, Cleared, Set([f64;3])}` — a three-variant enum, because the
UI renders all three differently (§4.2). Anything else (`(ts, Int)`, a
two-float tuple) projects as `Unreadable` and is refused on write, the
`hud.rs` rule against overwriting a key of the wrong shape.
`set_broadcast_colour(user, type, Option<[f64;3]>)` overwrites the wrapped
value in place — `Some` → the tuple, `None` → `Value::None` — or mints the leaf
when absent.

**Watch-list map.** `project` yields `Vec<WatchEntry { char_id: u64, rgb:
Option<[f64;3]> }>` in file order; an entry whose value is not three floats
projects `rgb: None` rather than vanishing, so the UI cannot re-add its id as a
duplicate key. `set_watchlist_colour(char, id, Some(rgb))` overwrites the
entry's tuple or appends `(key, tuple)`; `None` removes the entry. The map is
minted as `(zero FILETIME, {})` when absent, as `set_state_color` mints
`stateColors`. `key` is `Value::Int` when the id fits `i32`, else the
minimal-width `Long` of §2.6. Reading accepts either kind, plus a `Long`
holding a small value, and matches an id by value not by wire kind.

**Palette.** `pub const PALETTE: [(&str, [f64;3]); 9]` (§2.5), sent with the
projection.

The projection:

```rust
pub struct Fleet {
    pub fields: Vec<FleetEntry>,          // name, kind, value, default, scope, set
    pub colours: Vec<(String, Colour)>,   // (type, state), EVE's row order
    pub watchlist: Vec<WatchEntry>,
    pub palette: Vec<(String, [f64; 3])>,
    pub char_open: bool,
    pub user_open: bool,
}
```

`project_fleet(char: Option<&Value>, user: Option<&Value>)` — both optional,
unlike `project_hud`, because an open account with no character is a normal
subject and Broadcast settings is entirely its own.

### 3.2 `ops.rs` and `lib.rs`

- `fleet_settings(state) -> Fleet` — locks user then char (the file's order);
  errors with `no_document` only when **neither** is open.
- `set_fleet_field(state, name, text)` — the `set_hud_field` shape: the
  projection names the slot, `edit_reshared` with the model's `minted` answer
  as the reshare decision, re-project.
- `set_fleet_colour(state, type, rgb: Option<[f64;3]>)` — `edit_slot` on
  `Slot::User`, as `overview_set_state_color` does (inline, always reshare).
- `set_watchlist_colour(state, char_id: u64, rgb: Option<[f64;3]>)` —
  `edit_slot` on `Slot::Char`.

Each setter is one Tauri command and one undo entry, per the governing rule.
`edit_reshared`/`edit_slot` capture history, so undo needs nothing new.

### 3.3 `names.rs::lookup_character(dir, query) -> Option<Found { id, name }>`

- `query` parses as `u64` → the existing `resolve` path (cache, then ESI
  `/universe/names`), returned only if the category is `character`.
- Otherwise → case-insensitive scan of the on-disk cache for a `character`
  entry named `query`; miss → `POST /universe/ids/` with `[query]`, take
  `characters[0]`, write it into the same `names-cache.json` (id → name) via
  `apply_fetch` + `save_cache`, return it. Both directions are cached from then
  on, and the watch-list row for the new id resolves without another call.
- Not found → `None`. Transport failure → `Err(FetchError)`, so the UI can tell
  "no such character" from "could not ask".
- Same 15 s blocking client, same `spawn_blocking` command shape as
  `resolve_character_names`. The fetch is injected so the unit tests never
  touch the network.

### 3.4 `api.ts`

`fleet()`, `setFleetField(name, text)`, `setFleetColour(type, rgb | null)`,
`setWatchlistColour(charId, rgb | null)`, `lookupCharacter(query)`. Types
`Fleet`, `FleetEntry`, `Colour`, `WatchEntry`, `Rgb = [number, number,
number]`.

## 4. The view — `FleetView.svelte`

Written to `docs/ui-redesign/`: Phase 1's tokens and primitives only, Phase 2's
tab-row and inspector rules, Phase 5's message surfaces and copy standard.
Nothing below introduces a hex literal, an `opacity`, a native control rule, a
blocking dialog or a new class of message.

### 4.1 Shell

- `View` gains `"fleet"`; `VIEWS` places **Fleet between Probes and Raw** so
  Raw stays last. `viewAvailable("fleet")` is the shared "open a character or
  an account file" rule. `go.fleet` falls out of `VIEWS` (`commands.ts:149`);
  `keymap.ts` maps `6` → `go.fleet` and `7` → `go.raw`, and `ShortcutsSheet`
  reads the map.
- `ACCOUNT_SCOPED` gains `"fleet"`: the shell renders its `ScopeBanner` above
  the view because one panel writes the account file.
- **No inspector** (04 §7): there is no selection — every control is live on
  its row, as in Autofill and Keybinds. The column shows the shell's
  placeholder.
- No search field: seventeen rows plus at most sixteen is not a list that
  wants one. `viewFocusSearch.fleet` stays unbound, so Ctrl+F falls through as
  it does on Probes.
- Props follow `KeybindsView`: `charOpen`, `userOpen`, `userId`, `charId`,
  `refreshToken`, `onUserDirty`, `onCharDirty`, `onShowAccounts`. The reload
  effect reads `refreshToken` so Discard, restore and undo refresh it (05b's
  finding).
- The view menu `⋯` is empty: nothing here has a per-view action.

### 4.2 Layout

One scrolling column, `max-width: 56rem`, three `Panel`s stacked with
`var(--s4)` between them. Each opens with a `PanelHeader level={3}` whose
`actions` snippet is a `Chip size="sm"` reading **account file** or
**character file** — the treatment `RawInspector` already uses for the same
question. Stacked rather than sub-tabbed: the panels edit different files, and
the chip beside each title says so where a sub-tab would hide it.

**Broadcast settings** — chip *account file*, subtitle "Which broadcasts you
receive, and the colour each shows in". Seventeen rows in EVE's order, one
`<label class="row">` each, `display: contents` onto a three-column grid as
`HudPanel` does so labels, checkboxes and swatches share tracks. Per row:

- The label, sentence-cased per R1: "Always show my own broadcasts", "Need
  armor", "Need capacitor", … "Request that the fleet hold position". EVE's
  proper nouns keep their capitals; "Broadcast:" is dropped because every row
  bar one would carry it.
- `Field kind="checkbox"` (bare, inside the wrapping label). Checked ⇔ the
  field's value is `"1"`; `onchange` writes `"1"`/`"0"`. `disabled` with
  `disabledReason="This value has an unexpected type here"` when the projection
  says so.
- Except on the top row: a swatch — `Field kind="color"` with
  `list="fleet-palette"`, a `<datalist>` of the nine palette hexes rendered
  once per view. Its `ariaLabel` is "Colour for {label}". `Absent` shows the
  type's default hex (or, when the default is none, the same empty treatment as
  `Cleared`); `Cleared` shows the swatch at `--surface-raised` with a
  `--border-strong` outline and a `title` "No colour"; `Unreadable` disables
  it. On pick, `snapToPalette(hex, palette)` — the four lines lifted out of
  `OverviewAppearanceTab.setColor` into `app/src/lib/colour.ts` and used by
  both — writes the exact palette floats when the hex is a palette hex, else
  the inverted floats.
- A `Button variant="ghost" size="sm" iconOnly title="No colour"` ✕ that
  writes `null`; `disabled` with `disabledReason="Already no colour"` when the
  state is `Cleared`, or when the state is `Absent` and the default is none.

No account file → the panel body is an `EmptyState` (05 §3.3): title "No
account paired", description "Broadcast settings live in the account file.",
action `Button` "Pair this character…" → `onShowAccounts`. The other two
panels still render.

**Watch list colours** — chip *character file*, subtitle "The colour a
fleet-mate shows in your watch list". One `ListRow` per entry, in file order:

- `leading`: the swatch, as above, `ariaLabel` "Colour for {name}". An entry
  with `rgb: null` shows a disabled swatch and a `Chip tone="warn" size="sm"`
  "unreadable" in `trailing`.
- Label: the resolved name from the `names` store; `resolveNames(ids)` runs
  on every load. Unresolved → the bare id, which R5 permits for an id the app
  cannot name.
- `trailing`: the id as `--t-caption` `--text-muted` text (the `title` carries
  it too), then `Button variant="ghost" size="sm" iconOnly title="Remove from
  the list"` ✕ → `setWatchlistColour(id, null)`.

Below the list, the add row — a `<form>` so Enter submits:
`Field kind="text" label="Add a character" placeholder="Name or ID"
width="18rem"`, a swatch defaulting to blue (863 of 1,382 corpus entries) with
`ariaLabel` "Colour for the new entry", and a `Button variant="primary"
type="submit"` **Add**, `disabled` while the text is blank
(`disabledReason="Type a character name or ID"`) or a lookup is in flight. On
submit: if the text is an id already in the list, or resolves to one, the
message is "{name} is already in the list" and nothing is written; otherwise
`lookupCharacter` then `setWatchlistColour(id, rgb)`, and the text clears.

Empty list → `EmptyState` inside the panel: title "No watch-list colours",
description "Colours you set on watch-list members in-game appear here. Add
one below to colour a character before you next fleet with them." — no action,
the add row is the action. No character file → `EmptyState` "No character
open", "Watch-list colours live in the character file." with no action (the
sidebar is where a character is opened).

**Formation** — chip *character file*, subtitle "Fleet-warp formation, and the
fleet finder". Four `HudPanel`-style rows: `Field kind="number"` for
"Formation" (id, `min=0`, `step=1`), "Size" and "Spacing" (metres, `min=0`,
`step=100`, unit "m" as a `--text-muted` suffix), and `Field kind="checkbox"`
"Show only my corp, alliance and high-standing fleets" (`finder_group_only`).
Number fields use `HudPanel`'s `numberEdit` discipline: commit on change,
round to an integer, resync the input to the model whether or not the write
landed. No character file → the same `EmptyState` as the watch list.

The panel closes with one `--t-caption` `--text-muted` line: *"Saved fleet
setups live on CCP's servers and can't be edited here."*

### 4.3 Errors and success

- One `InlineMessage variant="error"` slot per panel, directly under the
  header, nulled at the top of every handler in that panel (05 §3.1's "one live
  message per owning control"; the panel is the control group). Text follows
  R4: "That broadcast setting wasn't changed — {errText(e)}", "That colour
  wasn't changed — …", "{name} wasn't added — …", "{name} wasn't removed — …",
  with `detail={errMessage(e)}`. Lookup failures get their own sentences on the
  add row: "No character called {query}" (ESI answered, empty) and "{query}
  wasn't looked up — couldn't reach ESI" (transport). `role="alert"` is the
  primitive's default for `error`.
- Writes succeed silently: the swatch or checkbox is the visible trace, so no
  toast (05 §3.2). The one toast is **Add** — "Added {name}" with
  `action: undoAction()`, because the new row can land below the fold.
- No `ConfirmDialog` anywhere in the view: removing a watch-list entry is an
  in-memory edit Discard and undo both reverse, the false "can't be undone"
  case 05 §2.8 retired.

### 4.4 Copy, checked against R1–R7

Sentence case throughout; "ESI", "ID", "CCP" stay upper. No ellipsis except
"Pair this character…", which opens a sheet. One verb per concept: *add*,
*remove*, *set*; never *assign*, *delete*, *apply*. No shortcut in a string.
Names, not files, everywhere but the scope chip, which names the file on
purpose. British spelling — "colour" in every user-facing string; the wire key
`fleet_broadcastcolor_*` is not a string the user reads.

## 5. Batch and presets — `Aspect::Fleet`

- `Category` gains a leaf variant per key: char `FleetWatchlistColours`,
  `FleetFormation`, `FleetFormationSize`, `FleetFormationSpacing`,
  `FleetFinderGroupOnly`; account `FleetListen<Type>` ×16, `FleetListenShowOwn`,
  `FleetShowOwn` (the bare key), `FleetColour<Type>` ×16. Thirty-nine variants;
  the key lists are declared once as static tables in `fleet.rs` and a small
  macro in `batch.rs` expands the variants and their `key_path` arms from them,
  so a type added later is added in one place.
- `absent_means_default = true` for all of them, the HUD-leaf rule: a copy
  makes the target match the source key-for-key, which includes **deleting a
  target's watch-list map when the source has none**. That is the established
  meaning of "the two characters match", and the batch preview's existing
  deletion wording covers it as it covers the target list.
- `aspect_writes`: `Fleet` pushes the five char categories and the thirty-four
  account ones. `derive_aspects`: `Fleet` when any fleet category is present on
  either document. `ASPECT_LABELS`: "Fleet (broadcast settings, watch-list
  colours, formation)". `BatchView`'s aspect list and `presetLibrary`'s label
  map gain `fleet`.
- `Everything` already copies both files whole and needs nothing.

## 6. Tests

- **`fleet.rs` unit tests**, hand-built fixtures like `keybinds.rs`: projection
  of each shape; an overwrite keeps the leaf's FILETIME; a mint writes the
  zero-FILETIME wrapper and never a bare leaf; present-but-unreadable refuses
  and leaves the document untouched; a listen toggle stays `Int`; `show_own`
  writes both keys; colour `Set`/`Cleared`/`Absent` round-trip and `None`
  writes `Value::None`; watch list add, recolour, remove, mint of the map, a
  `Long` key for an id ≥ 2³¹ and a `Long`-keyed read, an unreadable entry
  projected not dropped, a `Ref`/`Shared`-keyed section resolved.
- **`tests/fleet_corpus.rs`**, the `hud_corpus` gate: every scalar field, every
  colour type and the watch-list map read a value from ≥ 20 real files (≥ 1
  for the twelve colour types only the two 2026-09-18 snapshots carry) and ≥ 1
  synthetic. `gen_fixtures.rs` adds fleet keys to one character and one
  account profile fixture and the synthetic corpus is regenerated.
- **`ops.rs`**: the four commands against temp files, as
  `hud_projects_and_sets_the_ship_offset` does; `fleet_settings` with neither
  file open is an error; with only an account open it projects.
- **`names.rs`**: `lookup_character` with an injected fetch — numeric query
  hits the cache without a call; a name hits the cache case-insensitively; an
  ESI miss returns `None`; an ESI hit is persisted and the next call needs no
  fetch; a transport error is `Err`.
- **`setup.rs` / `presets.rs`**: `aspect_writes(&[Fleet])` routes both sides;
  `derive_aspects` reports `Fleet` for a preset holding only a watch-list map.
- **Vitest** — `FleetView.spec.ts`: the three panels and each empty state; a
  checkbox change calls `setFleetField` with `"1"`/`"0"`; a swatch pick on a
  palette hex writes the exact floats; ✕ writes `null`; the add flow (lookup
  then set, the toast, and each failure sentence); remove; "already in the
  list" writes nothing; the reload effect fires on `refreshToken`.
  `keymap.spec.ts` (the chord table and the `1`–`7` row), `commands.spec.ts`,
  `ViewTabs.spec.ts` and `page.spec.ts` for the new tab; `BatchView.spec.ts` and `PresetGroup.spec.ts` for the
  aspect; `colour.test.ts` for `snapToPalette`; `tokens.test.ts` keeps passing
  (no literals).
- **Live pass**, appended to `docs/live-verification-plan.md` for the next
  session: a minted watch-list entry and a recoloured broadcast show in-game; a
  listen key the editor minted for a never-keyed type is honoured; a `Long`
  key is honoured if a character above 2³¹ can be found.

## 7. File-by-file change list

New: `crates/settings-model/src/fleet.rs`, `crates/settings-model/tests/fleet_corpus.rs`,
`app/src/lib/FleetView.svelte`, `app/src/lib/FleetView.spec.ts`,
`app/src/lib/colour.ts`, `app/src/lib/colour.test.ts`, `app/src/lib/fleet.ts`
(the seventeen labels in EVE's order, the row → type map).

Modified: `hud.rs` (helpers take the table), `lib.rs` (module + exports),
`batch.rs` (variants, macro, `absent_means_default`), `gen_fixtures.rs`,
`fixtures/synthetic/**` (regenerated), `app/src-tauri/src/ops.rs`, `lib.rs`
(four commands + `lookup_character`), `names.rs`, `setup.rs`, `presets.rs`,
`app/src/lib/api.ts`, `views.ts`, `keymap.ts`, `aspects.ts`,
`presetLibrary.svelte.ts`, `BatchView.svelte`, `OverviewAppearanceTab.svelte`
(uses `snapToPalette`), `routes/+page.svelte` (mount, `ACCOUNT_SCOPED`),
`docs/settings-field-reference.md` (fleet keys move to "modelled"),
`docs/format-notes.md` (§2.3, §2.4, §2.6 findings), `CHANGELOG.md`.

## 8. Definition of done

- [ ] `cargo test` green across both crates and the app crate, including the
      corpus gate with the real corpus present.
- [ ] `npm test` exit code 0 and `npm run check` clean.
- [ ] Every user-facing string in `FleetView` passes R1–R7; no hex, `rgba()`,
      `opacity` or native-control rule in its `<style>`.
- [ ] A batch copy with only Fleet ticked previews both files and names the
      characters on the account.
- [ ] The Fleet tab is in the strip, disabled with a reason when nothing is
      open, `Ctrl+6` reaches it, `Ctrl+7` reaches Raw.
- [ ] Field reference and format notes updated; the two 2026-09-18 snapshots
      are in the corpus.
