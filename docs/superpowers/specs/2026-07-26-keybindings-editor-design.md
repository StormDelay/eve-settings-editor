# Keybindings editor (design)

Status: designed 2026-07-26, not yet planned.

Milestone context: this is the **keybindings** slice, ranked Tier 1 / #1 in
`docs/settings-field-reference.md` §10 as the largest untouched settings
surface. It is self-contained: one account-scoped dict, no cross-file link, no
structural editing. It runs independently of the overview and layout depth
milestones.

## 1. Goal

`core_user_<id>.dat → cmd → customCmds` holds the player's keyboard bindings.
The app has never modelled it, so the only way to see or change a binding is
EVE's own in-game screen — which edits **one account at a time** and offers no
way to copy a setup. A player with eight accounts configures the same bindings
eight times, by hand, from memory.

This slice ships two things:

1. **A remapping editor** — every command the account knows, grouped and
   labelled with EVE's own strings, rebindable by pressing the key combination.
2. **A batch category** — copy one account's whole binding table onto other
   accounts. This is the part EVE has never shipped, and (see §2.5) it is also
   the *only* way to give an account a binding table it does not already have.

## 2. The keybinding model (confirmed from the corpus)

Measured over the 2026-07-22 corpus snapshot (`2026-07-22T120910Z_states-after`,
175 account files) with a throwaway `settings-model` bin, per the research-tool
convention. **12,117 binding entries across 132 files**; the other 43 files have
an empty `customCmds`.

### 2.1 Wire shape

```
core_user_<id>.dat
  root → cmd                      bare Dict, NOT timestamp-wrapped
       → customCmds               Tuple(FILETIME, Dict)   ← ONE stamp, whole table
            → { Bytes command : value }
                  None            unbound
                  Tuple(Int…)     [17?, 18?, 16?, key]
```

Verified on a live TQ account (`core_user_12236996.dat`, 93 entries):

```
cmd section     : Dict[1]                       -> NOT a 2-tuple
customCmds      : Tuple[2]<Long[8], Dict[93]>   -> IS a 2-tuple
leaves that look (FILETIME, value): 0 of 93
dict key kinds  : {"Bytes"}
sample leaves   : Tuple[1]<Int(82)>   Tuple[2]<Int(16), Int(83)>
```

**This inverts the usual convention** (`format-notes.md`, "Value-wrapper
convention"). The timestamp sits on the `customCmds` container, and the
individual leaves are bare. A writer that wraps the leaf produces a malformed
`(Long, Tuple)` — the same class of defect as the overview appearance writer's
`(Long, Int, Bool)`, which the client silently ignored while keeping its stale
value.

On the sampled file the root `cmd` key is a plain `Bytes`, but sibling root keys
are `Ref`/`Shared`, so section lookup must still resolve through `effective`
exactly as `hud.rs::section` does. Do not match `Value::Bytes` on the section key
directly.

### 2.2 Value invariants

Over all 4,765 bound entries (the remaining 7,352 are `None`), with **zero**
exceptions:

| invariant | evidence |
|---|---|
| Exactly one non-modifier code per binding | `{1: 4765}` — no binding has 0 or 2+ |
| Modifiers precede the key | 4,765/4,765 |
| Modifier order is Ctrl(17) → Alt(18) → Shift(16) | `Alt+Shift` = `(18,16,…)`, `Ctrl+Alt+Shift` = `(17,18,16,…)` |
| No duplicate combination within a file | 0 of 132 files contain one |
| Dict keys are `Bytes` | 132/132 files |

Observed combinations: none (1,575), Alt (1,282), Ctrl (1,004), Shift (768),
Alt+Shift (131), Ctrl+Alt+Shift (5). Tuple lengths run 1–4.

The no-duplicates result means **EVE enforces uniqueness**: rebinding a
combination that is already in use steals it from its previous owner. The editor
must do the same (§5.2), or it writes a file the client never produces.

### 2.3 `None` means unbound, not "use the default"

The client writes `None` for 61 % of entries. Two readings were possible —
`None` as "user explicitly cleared" versus "no binding" — and they are not
distinguishable from a single file. They do not need to be: the editor displays
`None` as unbound and writes `None` to unbind, which is byte-identical to what
the client itself writes. Whatever EVE does with a `None` entry, the app
reproduces it faithfully.

### 2.4 EVE writes the whole command table, not a diff of user edits

`docs/settings-field-reference.md` §5.3 inferred that `customCmds` "holds only
what the user changed". **That inference is wrong** and this spec supersedes it.

The 132 files reduce to only 10 distinct command-name sets and 13 distinct full
binding maps — they are copies of one player's configuration, so clustering
alone proves nothing. The discriminator is that the name sets **nest**:

| set size | files | newest | relation |
|---|---|---|---|
| 32 | 1 | 2022-12-06 | not nested — Project Awakening |
| 79 | 1 | 2021-06-02 | ⊂ next (+11) |
| 90 | 18 | 2021-05-05 | ⊂ next (+1) |
| 91 | 8 | 2022-01-16 | ⊂ next (+1) |
| 92 | 27 | 2022-12-12 | ⊂ next (+1) |
| 93 | 70 | 2026-07-22 | ⊂ next (+1) |
| 94–96 | 7 | 2026-07-22 | not nested — other builds |

(132 files, 10 distinct sets.)

A strictly growing chain ordered by client generation is what a client-owned
table looks like as CCP adds commands. Under "only user edits" the player would
have had to touch exactly one additional command per client version,
monotonically, never removing one. The non-nesting 94–96 sets are the Project
Awakening and Singularity builds, which carry their own commands
(`CmdPickPortrait0..3`, `OpenAccessGroupsWindow`) — a different client, a
different table, which is consistent with the same conclusion.

Corroborating: EVE's factory module bindings are F1–F8 (VK 112–119). Those codes
appear **zero times** in all 12,117 entries, while `CmdActivateHighPowerSlot1` is
`(81)` = Q in every one of the 132 files that carry it, with no other value. The
factory default does not survive anywhere in the file once overwritten.

**Consequence for the design:** the file's table *is* that client build's
command set. The editor lists what the file contains and never invents rows.
Listing the 101-name union would offer Project Awakening commands on a TQ
account.

### 2.5 An untouched account has an *empty* table, not a default one

`core_user_32945923.dat` — live Tranquility, written 2026-07-22, 21 KB of real
settings — has a `cmd` section whose `customCmds` is **empty**. 43 of 175 corpus
accounts are in this state.

So the table materialises only once the in-game keybinding screen has been used.
An account that has never had its keybinds touched offers nothing to read, and
nothing for a "capture the defaults" exercise to harvest (§4).

This is what makes the batch category load-bearing rather than a convenience:
copying `customCmds` wholesale is the only way to give such an account a table.

## 3. Command labels

`staticdata/commandsets.fsdbinary` (a feature-blocker file, not the catalog)
revealed the client's naming convention: `cmd.name.<CommandName>` and
`cmd.category.<group>`. Following it into the SharedCache localization pickle
resolves the command names to the strings EVE shows in-game:

| source | resolves |
|---|---|
| `UI/Commands` | 80 / 101 |
| `UI/Fleet/FleetBroadcast/Commands` | 4 / 101 |
| de-camelcase fallback | 17 / 101 |

Examples: `CmdActivateHighPowerSlot1` → "Activate High Power Slot 1",
`CmdDronesReturnAndOrbit` → "All Drones: Return and Orbit",
`CmdFleetBroadcast_HealArmor` → "Broadcast: Need Armor".

The 17 fallbacks read acceptably ("Toggle Probe Scanner", "Open Industry"); two
need a hand-fix (`CmdPickPortrait0` → "Pick Portrait 0",
`ToggleCurrentSystemLocationWnd` → "Toggle Current System Location Window").

**`tools/gen-command-names.py`** generates
`app/src/lib/data/command-names.json` (`{command: {label, group}}`), following
`gen-default-preset-names.py`: stdlib-only, reads the local EVE install, not
shipped to users, and carrying the same DO-NOT-RERUN-BLINDLY header, since the
committed file is hand-corrected. Groups are assigned from the command-name
prefix families in `settings-field-reference.md` §5.3: Modules, Overload,
Drones & Fighters, Navigation, Targeting, Fleet broadcasts, Windows, Misc.

A command absent from the map falls back to its de-camelcased name at runtime,
so a client update that adds commands degrades to a readable label rather than a
blank row.

## 4. Factory defaults — shape now, data later

No factory-default bindings exist anywhere in the settings files (§2.4), and an
untouched account cannot supply them (§2.5). Capturing them requires opening the
in-game keybinding screen on a throwaway account, choosing **Reset to default**,
and logging out — and whether that writes explicit values or simply clears the
dict is itself unverified.

Per the developer's decision, the feature is **built assuming defaults**, with
the data deferred:

- `app/src/lib/data/command-defaults.json` ships as `{}`.
- The view renders a **Default** column and a per-row reset control, both
  disabled and showing `—` while the map is empty.
- Populating it later is an edit to one JSON file. No Rust, no schema, no view
  changes. `KeybindEntry` deliberately carries no `default` field — defaults are
  a display concern, like labels.

Tracked in `docs/small-tasks.md` as "capture factory keybindings from a
reset-to-default account".

## 5. Rust surface

### 5.1 `crates/settings-model/src/keybinds.rs`

New module, structured like `overview_states.rs`; all format knowledge lives
here and nothing else mutates the section.

```rust
pub struct KeybindEntry {
    pub command: String,          // "CmdActivateHighPowerSlot1"
    pub keys: Option<Vec<i64>>,   // None = unbound, else [17?,18?,16?,key]
    pub set: SetTarget,
}

pub struct Keybinds {
    pub entries: Vec<KeybindEntry>,
    /// false when there is no `cmd` section or `customCmds` is empty —
    /// drives the empty state (§2.5).
    pub available: bool,
}

pub fn project_keybinds(user_root: Option<&Value>) -> Keybinds;
```

Entries are reported in file order. Grouping, ordering and labelling are display
concerns and stay in TypeScript. A leaf whose shape violates §2.2 projects as
`keys: None` with a non-writable `SetTarget` and is passed through untouched on
save, rather than being normalised.

### 5.2 The setter

```rust
/// Returns the commands whose binding was stolen (cleared to None).
pub fn set_keybind(root: &mut Value, command: &str, keys: Option<Vec<i64>>)
    -> Result<Vec<String>, String>;
```

One pass, one commit:

1. Validate `keys` against §2.2 — exactly one non-modifier, modifiers unique,
   then **canonicalise the order to Ctrl → Alt → Shift → key** regardless of the
   order supplied. The caller cannot produce a non-conforming file.
2. Find every other command holding the same combination and set it to `None`.
3. Write the new value for `command`.
4. **Leave the `customCmds` timestamp untouched.** Never wrap a leaf.

On (4): the repo convention is to preserve an existing wrapper's timestamp and
mint a zero one only when the wrapper is absent — see
`autofill.rs::set_list_entries_preserves_the_timestamp_list_wrapper` and
`overview_pack.rs::apply_pack_preserves_an_existing_wrappers_timestamp`. Five
shipped editors do this and every live smoke passed, so the client does not
require a fresh stamp. Since `customCmds` is present whenever there is anything
to edit (§2.5), the minting path is unreachable here and must not be written.

Unbinding is `set_keybind(cmd, None)`. Binding a command to the combination it
already holds is a no-op that still returns `Ok(vec![])`.

The stolen-command list is returned rather than logged so the UI can name what
it took (§6).

### 5.3 Batch

```rust
Category::Keybinds => &[b"cmd", b"customCmds"],
```

Same two-step path shape `Category::Autofill` already uses for
`ui → editHistory`, so `extract_categories` / `apply_to_tree` need no change.
Copies the table wholesale, including its timestamp — which is what lets it
populate an account that has none (§2.5).

The category is added to the batch UI's aspect list as **Keybinds**, alongside
Layout / Overview / Autofill.

### 5.4 Tauri commands

```
keybinds()                              -> Keybinds
set_keybind(command, keys: Option<Vec<i64>>) -> SetKeybindResult { keybinds, stolen }
```

Account-scoped, so both read the `user` slot, like `autofill_lists` /
`set_autofill_list`.

## 6. UI

### 6.1 Placement

A new top-level **Keybinds** view button beside Overview and Autofill in
`+page.svelte`, gated identically (`openCharId !== null || slots.user` opened),
with the `active` slot deriving to `user` exactly as Autofill does. An unpaired
character gets the existing `Pair…` prompt — no new empty-state path. Ctrl+F
routes to the view's filter box, following the Layout view precedent.

### 6.2 `KeybindsView.svelte`

One filterable list, grouped per §3:

```
Modules                                       Default
  Activate High Power Slot 1    [ Q ]            —      ↺
  Activate High Power Slot 2    [ S ]            —      ↺
  Activate Mid Power Slot 1     [ Ctrl+S ]       —      ↺
  Toggle Autopilot              [ unbound ]      —      ↺
```

Clicking a chip enters *listening* state. `keydown` is captured with
`preventDefault`; **Esc** cancels, **Backspace** unbinds, anything else is
recorded. There is no confirmation dialog for a conflict — EVE steals silently,
so the app does too — and the row that lost the combination shows a transient
`unbound — Ctrl+S taken by Activate Mid Power Slot 1`, driven by the `stolen`
list from §5.2.

When `available` is false, the view shows the §2.5 explanation and points at the
Batch view as the way to populate the account.

The `↺` control and the Default column are rendered disabled until
`command-defaults.json` is non-empty (§4).

Per `eve-editor-dark-native-controls`: the filter input and any native control
added here need explicit dark background/color.

### 6.3 `keybinds.ts`

One hand-authored VK↔label table (~100 entries) serving both display and
validation:

- `keysToLabel([17, 81]) → "Ctrl+Q"`
- `eventToKeys(KeyboardEvent) → number[] | null`

Capture reads `event.keyCode`, which in WebView2 is the Windows virtual-key
code — the same value EVE stores. A code absent from the table is **rejected**
with "unsupported key" rather than written blind. This gets a `ponytail:` comment
naming the ceiling: `keyCode` is deprecated-but-universally-implemented; if it
is ever removed, the upgrade is an `event.code → VK` table against the same map.

Known limitation, documented in the view: the OS and the WebView swallow a few
combinations (Alt+F4, Alt+Tab, some F-keys), which therefore cannot be captured.
They are poor EVE bindings anyway. If this proves annoying, the fallback is a
modifier-checkbox + key-dropdown escape hatch over the same table — deliberately
not built now.

## 7. Testing

### 7.1 Unit — `keybinds.rs`

Projection of a synthetic account; `available: false` for a missing section and
for an empty dict; set, unbind, steal; order canonicalisation; rejection of a
zero-key and a two-key binding; malformed leaves projecting as unwritable and
surviving a round-trip; and the `customCmds` timestamp surviving a write
untouched.

### 7.2 Corpus gate — `crates/settings-model/tests/keybinds_corpus.rs`

Modelled on `hud_corpus.rs`, which is the test that would have caught the badge
offset bug. Over the real corpus:

1. **≥ 130 accounts project a non-empty table.** A wrong section or key reports
   0 and fails. No hand-built fixture can catch this — every `hud.rs` fixture
   agreed with the broken `FIELDS` table and all 20 tests passed.
2. Every projected binding satisfies §2.2 — one non-modifier, canonical
   modifier order.
3. No file projects a duplicate combination.
4. Round-trip: project → `set_keybind` → encode → decode leaves the file
   identical except the one leaf — the `customCmds` timestamp included.

### 7.3 TypeScript — `keybinds.test.ts`

`node --test`, zero-dep, matching the existing suites: `keysToLabel` across all
combination shapes, `eventToKeys` including the reject path, grouping and the
de-camelcase label fallback.

### 7.4 Live smoke

1. Rebind a command in the app → log in → confirm EVE honours it and does not
   revert the file.
2. Batch-copy the table onto an account with an empty `customCmds` → confirm the
   in-game keybinding screen shows the copied bindings.

Gate 2 is the one that can fail in an interesting way: a freshly minted table
carrying another account's timestamp is a shape the client has not been observed
to read.

## 8. Non-goals

- **Factory defaults data** — shape only, per §4.
- **Character-scoped keybindings** — there are none; `customCmds` is
  account-scoped and applies to every character on the account.
- **A dropdown key picker** — §6.3.
- **Conflict *warnings* across accounts** — the batch copy replaces the whole
  table, so cross-account conflicts cannot exist.
- **Editing `cmd` beyond `customCmds`** — the section holds exactly one key in
  175/175 files.
- **Import/export of a keybind pack** — the batch category covers the copy case;
  a file format is a separate decision.

## 9. Risks

| risk | mitigation |
|---|---|
| `None` semantics (unbound vs. fall back to a default) unverified | The app writes exactly what the client writes; behaviour is identical either way (§2.3) |
| A client update adds commands the label map lacks | De-camelcase fallback keeps the row readable (§3) |
| `event.keyCode` deprecation | `ponytail:` comment + upgrade path to `event.code` (§6.3) |
| Minted table with a foreign timestamp rejected by the client | Live smoke gate 2 (§7.4); precedent is encouraging — EVE accepted a freshly minted zero-timestamp `overviewProfilePresets` container |
| Some combinations uncapturable | Documented in-view; dropdown fallback deferred (§6.3) |

## 10. Corrections to existing docs

This slice must land these alongside the code:

1. **`docs/settings-field-reference.md` §5.3** — replace the "holds only what the
   user changed … not been verified in-game" inference with §2.4 of this spec.
   Also correct §10 Tier 1 item 1, which proposes the same unverified inference
   as the slice's main risk, and drops the "needs two hand-authored
   vocabularies" claim to one (command labels are harvestable; only the VK table
   is hand-authored).
2. **`docs/format-notes.md`** — new "Keybindings" section carrying §2.1 and
   §2.2, and an explicit note that `customCmds` is an exception to the
   value-wrapper convention: the stamp is on the container, the leaves are bare.
