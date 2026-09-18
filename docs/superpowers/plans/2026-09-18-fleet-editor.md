# Fleet Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Fleet view that edits EVE's broadcast settings, watch-list colours and fleet-warp formation, adds watch-list characters by name or id through ESI, and a Fleet batch aspect that copies all of it between characters and accounts.

**Architecture:** A new `fleet.rs` model in `settings-model` reuses `hud.rs`'s scalar-field machinery (generalised to take its field table as a parameter) and adds two shapes of its own — a colour leaf that can be `None` and a dict keyed by character id. Four Tauri commands and one ESI reverse lookup sit in the app crate; a `FleetView.svelte` of three stacked panels renders it with the Phase-1 primitives only; `Category::Fleet(&'static FleetLeaf)` borrows the model's own key table so the batch key list and the editor's are one list.

**Tech Stack:** Rust (settings-model crate, Tauri 2 app crate, serde, reqwest blocking), Svelte 5 runes + TypeScript, vitest + @testing-library/svelte, cargo test with the corpus gate.

**Spec:** `docs/superpowers/specs/2026-09-18-fleet-editor-design.md`

## Global Constraints

- Every leaf written is `(FILETIME, value)`-wrapped; an overwrite keeps the existing FILETIME, a mint uses `Value::Long(vec![0u8; 8])`. Never a bare leaf.
- A key of a shape the module does not write is **refused**, never overwritten and never duplicated (`hud.rs`'s `Located::Unwritable` rule).
- Listen checkboxes are `Value::Int` 0/1 on the wire. Colours are three `Value::Float`s, or `Value::None` for "no colour". A watch-list key is `Value::Int` when the id fits `i32`, else a minimal-width little-endian two's-complement `Value::Long`.
- Lock order in `ops.rs` is user → char → history, everywhere.
- One Tauri command = at most one undo entry (`edit_reshared` / `edit_slot` capture history; nothing else pushes).
- Frontend: no hex literal, `rgba()`, `opacity` (other than `--o-disabled`), native-control CSS or blocking dialog in any `.svelte`/`.css` file — `app/src/lib/ui/tokens.test.ts` fails the build on any of them. Colours in `.svelte` script blocks are built with `rgbToHex(...)`, never typed as hex; the one placeholder is the `UNSET_HEX` constant, whose name whitelists its line.
- Copy standard (spec §4.4): sentence case, one verb per concept (add / remove / set), errors read "<Thing> wasn't <verbed> — <reason>", names not files, no shortcut baked into a string, British spelling ("colour").
- `npm run check` (`svelte-check --fail-on-warnings`) must stay clean; `cargo clippy --workspace --all-targets -- -D warnings` must stay clean.
- Commit after every task with the attribution trailer `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.
- Run Rust tests from the repo root: `cargo test -p settings-model` for the model crate, `cargo test -p app` for the Tauri crate (its package name is `app`; check `app/src-tauri/Cargo.toml` `[package] name` if that fails). Run frontend tests from `app/`: `npm test` (exit code is the verdict — read it, not the summary line) and `npx vitest run <pattern>` for one file.

---

### Task 1: Let `hud.rs`'s field machinery take its table as a parameter

**Files:**
- Modify: `crates/settings-model/src/hud.rs` (`Field` at :62, `FIELDS` at :72, `project_hud` at :134, `set_hud_value` at :322, `mint` at :349, `section_dict_mut` at :384)

**Interfaces:**
- Produces (all `pub(crate)`, used by Task 2):
  - `pub(crate) struct Field { name, section, key, elem, kind, default, scope }` — `Clone + Copy`, all fields `pub(crate)`.
  - `pub(crate) fn project_fields(fields: &[Field], char_root: Option<&Value>, user_root: Option<&Value>) -> Vec<HudEntry>`
  - `pub(crate) fn set_field(fields: &[Field], root: &mut Value, name: &str, text: &str) -> Result<bool, HudError>` — writes **every** row whose `name` matches; `Ok(true)` when any row was minted.
  - `pub(crate) fn section_dict_mut<'a>(root: &'a mut Value, section: &[u8]) -> Option<&'a mut Entries>`
- `project_hud` and `set_hud_value` keep their public signatures and behaviour.

- [ ] **Step 1: Run the existing HUD tests to record the baseline**

Run: `cargo test -p settings-model hud`
Expected: all pass (this is the bar the refactor must hold).

- [ ] **Step 2: Make `Field` crate-visible and copyable**

Replace the struct definition at `hud.rs:62-70` with:

```rust
/// One editable value. `elem` indexes into an `(x, y)` tuple; `None` means the
/// leaf itself. Defaults are EVE's built-in behaviour when the key is absent
/// (assumed, confirmed in the slice's live smoke).
///
/// Crate-visible because `fleet.rs` declares its own table and drives the same
/// locate/mint machinery with it — the field table is a parameter now, not a
/// module constant.
#[derive(Clone, Copy)]
pub(crate) struct Field {
    pub(crate) name: &'static str,
    pub(crate) section: &'static [u8],
    pub(crate) key: &'static [u8],
    pub(crate) elem: Option<usize>,
    pub(crate) kind: HudKind,
    pub(crate) default: &'static str,
    pub(crate) scope: HudScope,
}
```

- [ ] **Step 3: Split `project_hud` into `project_fields` + a thin wrapper**

Replace `project_hud` (`hud.rs:134-163`) with:

```rust
/// Project a field table against whichever documents are open. A `None` root
/// leaves that side's fields `Unavailable` — no account file is normal (an
/// unpaired character), and for the fleet table no character file is too.
pub(crate) fn project_fields(
    fields: &[Field],
    char_root: Option<&Value>,
    user_root: Option<&Value>,
) -> Vec<HudEntry> {
    let mut char_shared = SharedTable::new();
    if let Some(c) = char_root {
        collect_shared(c, &mut char_shared);
    }
    let mut user_shared = SharedTable::new();
    if let Some(u) = user_root {
        collect_shared(u, &mut user_shared);
    }

    fields
        .iter()
        .map(|f| {
            let (root, shared) = match f.scope {
                HudScope::Char => (char_root, &char_shared),
                HudScope::Account => (user_root, &user_shared),
            };
            let (value, set) = root.map_or((None, SetTarget::Unavailable), |r| probe(r, f, shared));
            HudEntry {
                name: f.name.to_string(),
                kind: f.kind,
                value,
                default: f.default.to_string(),
                scope: f.scope,
                set,
            }
        })
        .collect()
}

pub fn project_hud(char_root: &Value, user_root: Option<&Value>) -> Hud {
    Hud { entries: project_fields(&FIELDS, Some(char_root), user_root) }
}
```

- [ ] **Step 4: Split `set_hud_value` into `set_field` + a thin wrapper, and thread the table into `mint`**

Replace `set_hud_value` (`hud.rs:322-347`) with:

```rust
/// Write one field of `fields` by name. An existing key is overwritten in
/// place (no reshare needed — a scalar edit is not structural). An absent key
/// is minted as the `(timestamp, value)` leaf real files use, which needs
/// `inline_all` first per the house rule; the caller (`ops`) reshares
/// afterwards. Returns `true` when any row MINTED, which is the only path that
/// de-shares the document.
///
/// Every row carrying `name` is written, in table order. HUD names are unique
/// so this is one row; the fleet table has one name on two rows (the top
/// checkbox of EVE's Broadcast Settings writes two keys) and this is what
/// keeps them in step. A failure part-way is rolled back by `edit_reshared`.
pub(crate) fn set_field(
    fields: &[Field],
    root: &mut Value,
    name: &str,
    text: &str,
) -> Result<bool, HudError> {
    let rows: Vec<&Field> = fields.iter().filter(|f| f.name == name).collect();
    if rows.is_empty() {
        return Err(HudError::UnknownField(name.to_string()));
    }
    let mut minted = false;
    for f in rows {
        // Resolve the decision under an immutable borrow, then mutate.
        let located = {
            let mut shared = SharedTable::new();
            collect_shared(root, &mut shared);
            let (entries, base) = section(root, f.section, &shared).ok_or(HudError::NoSection)?;
            locate(entries, &base, f, &shared)
        };
        match located {
            Located::Writable(path, _) => {
                let m = crate::mutate::Mutation::SetScalar { path, text: text.to_string() };
                crate::mutate::apply(root, &m).map_err(|e| HudError::Parse(e.to_string()))?;
            }
            Located::Unwritable => return Err(HudError::NotEditable),
            Located::Absent => {
                mint(fields, root, f, text)?;
                minted = true;
            }
        }
    }
    Ok(minted)
}

/// Write one HUD field. See `set_field` for the contract.
pub fn set_hud_value(root: &mut Value, name: &str, text: &str) -> Result<bool, HudError> {
    set_field(&FIELDS, root, name, text)
}
```

Change `mint`'s signature and its one use of `FIELDS`:

```rust
fn mint(fields: &[Field], root: &mut Value, f: &Field, text: &str) -> Result<(), HudError> {
```

and inside it, `let sibling = FIELDS` → `let sibling = fields`.

Make `section_dict_mut` crate-visible: `fn section_dict_mut<'a>(` → `pub(crate) fn section_dict_mut<'a>(`.

- [ ] **Step 5: Run the HUD tests and clippy**

Run: `cargo test -p settings-model hud && cargo clippy -p settings-model --all-targets -- -D warnings`
Expected: every test that passed in Step 1 still passes; clippy clean. (`Field` is now unused outside `hud.rs` until Task 2 — clippy does not warn on unused `pub(crate)` items, but if it reports `dead_code` on `project_fields`/`set_field`, add `#[allow(dead_code)]` on them with the comment `// used by fleet.rs (Task 2)` and remove it in Task 2.)

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/hud.rs
git commit -m "hud: take the field table as a parameter

project_fields / set_field / section_dict_mut are crate-visible so the fleet
model can drive the same locate/mint machinery with its own table.
set_field writes every row carrying the name, which HUD (unique names) never
notices and the fleet table's two-key top checkbox needs.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: `fleet.rs` — the scalar fields

**Files:**
- Create: `crates/settings-model/src/fleet.rs`
- Modify: `crates/settings-model/src/lib.rs:20` (add `mod fleet;` after `mod hud;`) and the `pub use` block (:46)

**Interfaces:**
- Consumes: Task 1's `Field`, `project_fields`, `set_field`.
- Produces (public, re-exported from `lib.rs`):
  - `pub const BROADCAST_TYPES: [&str; 16]` in EVE's row order.
  - `pub struct Fleet { fields: Vec<HudEntry>, colours: Vec<ColourEntry>, watchlist: Vec<WatchEntry>, palette: Vec<(String, [f64; 3])>, char_open: bool, user_open: bool }` — this task fills `fields`, `char_open`, `user_open`; `colours`, `watchlist`, `palette` are empty until Tasks 3–4.
  - `pub fn project_fleet(char_root: Option<&Value>, user_root: Option<&Value>) -> Fleet`
  - `pub fn set_fleet_field(root: &mut Value, name: &str, text: &str) -> Result<bool, HudError>`
  - Field names: `listen_<Type>` (the type token verbatim, e.g. `listen_HealArmor`), `listen_show_own`, `formation`, `formation_size`, `formation_spacing`, `finder_group_only`.
  - `pub(crate) const FIELDS: [Field; 22]`, `const COLOUR_KEYS: [&[u8]; 16]` (used by Tasks 3 and 6).

- [ ] **Step 1: Write the failing tests**

Create `crates/settings-model/src/fleet.rs` with the module doc, the table, empty stubs for the types Tasks 3–4 fill, and the tests. Write the whole file now; the tests fail because nothing projects yet.

```rust
//! Read + edit projection of EVE's fleet settings: the Broadcast Settings
//! dialog (account file), the watch list's per-member colours and the
//! fleet-warp formation panel (character file). All of it lives under root
//! `ui` in one file or the other, every leaf `(FILETIME, value)`-wrapped. See
//! docs/superpowers/specs/2026-09-18-fleet-editor-design.md §2 for the corpus
//! measurements behind every default and palette float here.
//!
//! The scalars ride `hud.rs`'s field machinery — the same locate/mint decision,
//! the same "refuse a key of the wrong shape" rule. The two shapes HUD lacks, a
//! colour leaf that can be `None` and a dict keyed by character id, are here.

use blue_marshal::Value;
use serde::Serialize;

use crate::hud::{project_fields, set_field, Field, HudEntry, HudError, HudKind, HudScope};

/// The sixteen broadcast types, in the row order of EVE's Broadcast Settings
/// dialog, each with the checkbox's value when no key has been written (spec
/// §2.2). Declared once: the type list, the listen fields and the colour keys
/// all expand from it, so a type CCP adds is added here and nowhere else.
macro_rules! broadcast_types {
    ($($t:ident = $listen_default:literal),* $(,)?) => {
        pub const BROADCAST_TYPES: [&str; 16] = [$(stringify!($t)),*];
        const LISTEN_FIELDS: [Field; 16] = [$(listen(
            concat!("listen_", stringify!($t)),
            concat!("listenBroadcast_", stringify!($t)).as_bytes(),
            $listen_default,
        )),*];
        const COLOUR_KEYS: [&[u8]; 16] =
            [$(concat!("fleet_broadcastcolor_", stringify!($t)).as_bytes()),*];
    };
}

broadcast_types!(
    HealArmor = "0", HealCapacitor = "0", NeedBackup = "0", Target = "1",
    HealShield = "0", WarpTo = "1", TravelTo = "1", Event = "1", JumpTo = "1",
    AlignTo = "1", HealTarget = "1", InPosition = "1", EnemySpotted = "1",
    HoldPosition = "0", JumpBeacon = "1", Location = "1",
);

const fn listen(name: &'static str, key: &'static [u8], default: &'static str) -> Field {
    Field { name, section: b"ui", key, elem: None, kind: HudKind::Int, default, scope: HudScope::Account }
}

const fn char_field(name: &'static str, key: &'static [u8], default: &'static str) -> Field {
    Field { name, section: b"ui", key, elem: None, kind: HudKind::Int, default, scope: HudScope::Char }
}

/// The rows the type list does not generate: the top checkbox's TWO keys (spec
/// §2.3 — same name, so `set_field` writes both and `project_fleet` reports
/// the first) and the four character-side scalars.
const EXTRA_FIELDS: [Field; 6] = [
    listen("listen_show_own", b"listenBroadcast_ShowOwnBroadcasts", "0"),
    listen("listen_show_own", b"ShowOwnBroadcasts", "0"),
    char_field("formation", b"setFleetFormation", "0"),
    char_field("formation_size", b"setFleetFormationSize", "20000"),
    char_field("formation_spacing", b"setFleetFormationSpacing", "2000"),
    char_field("finder_group_only", b"fleetfinder_showGroupAndHighStandingsFleets", "1"),
];

/// Every scalar this module edits: the sixteen listen rows, then `EXTRA_FIELDS`.
pub(crate) const FIELDS: [Field; 22] = {
    let mut out = [EXTRA_FIELDS[0]; 22];
    let mut i = 0;
    while i < 16 {
        out[i] = LISTEN_FIELDS[i];
        i += 1;
    }
    let mut j = 0;
    while j < 6 {
        out[16 + j] = EXTRA_FIELDS[j];
        j += 1;
    }
    out
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColourEntry {
    pub broadcast: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WatchEntry {
    pub char_id: u64,
    pub rgb: Option<[f64; 3]>,
}

#[derive(Debug, Serialize)]
pub struct Fleet {
    /// The scalars, in `HudEntry`'s shape — `hud.rs` defined it for exactly
    /// this kind of field and the frontend already renders it.
    pub fields: Vec<HudEntry>,
    pub colours: Vec<ColourEntry>,
    pub watchlist: Vec<WatchEntry>,
    pub palette: Vec<(String, [f64; 3])>,
    pub char_open: bool,
    pub user_open: bool,
}

/// Both roots optional, unlike `project_hud`: an open account with no character
/// is a normal subject, and Broadcast settings is entirely its own.
pub fn project_fleet(char_root: Option<&Value>, user_root: Option<&Value>) -> Fleet {
    let mut fields = project_fields(&FIELDS, char_root, user_root);
    // The two show-own rows sit together in FIELDS; the first speaks for both.
    fields.dedup_by(|later, kept| later.name == kept.name);
    Fleet {
        fields,
        colours: Vec::new(),
        watchlist: Vec::new(),
        palette: Vec::new(),
        char_open: char_root.is_some(),
        user_open: user_root.is_some(),
    }
}

/// Write one scalar. See `hud::set_field` for the contract; `Ok(true)` means a
/// key was minted and the caller must reshare.
pub fn set_fleet_field(root: &mut Value, name: &str, text: &str) -> Result<bool, HudError> {
    set_field(&FIELDS, root, name, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{b, ts};
    use crate::windows::SetTarget;
    use blue_marshal::Value;

    /// (timestamp, value) — the file-wide value-wrapper convention.
    pub(super) fn wrapped(v: Value) -> Value {
        Value::Tuple(vec![ts(), v])
    }

    /// An account document with a `ui` holding the two show-own keys and one
    /// listen key; every other listen key is absent, as in a real file.
    pub(super) fn user_doc() -> Value {
        Value::Dict(vec![(
            b("ui"),
            Value::Dict(vec![
                (b("listenBroadcast_ShowOwnBroadcasts"), wrapped(Value::Int(1))),
                (b("ShowOwnBroadcasts"), wrapped(Value::Int(1))),
                (b("listenBroadcast_HealArmor"), wrapped(Value::Int(0))),
            ]),
        )])
    }

    /// A character document with the formation trio present and the finder
    /// toggle absent.
    pub(super) fn char_doc() -> Value {
        Value::Dict(vec![(
            b("ui"),
            Value::Dict(vec![
                (b("setFleetFormation"), wrapped(Value::Int(3))),
                (b("setFleetFormationSize"), wrapped(Value::Int(30000))),
                (b("setFleetFormationSpacing"), wrapped(Value::Int(5000))),
            ]),
        )])
    }

    pub(super) fn entry<'a>(f: &'a Fleet, name: &str) -> &'a HudEntry {
        f.fields.iter().find(|e| e.name == name).expect("field projected")
    }

    /// Find a key's wrapped leaf in a plain (post-inline) root's `ui` section.
    pub(super) fn ui_leaf<'a>(root: &'a Value, key: &[u8]) -> Option<&'a Value> {
        let Value::Dict(root) = root else { return None };
        let (_, ui) = root.iter().find(|(k, _)| matches!(k, Value::Bytes(v) if v.as_slice() == b"ui"))?;
        let Value::Dict(ui) = ui else { return None };
        ui.iter().find(|(k, _)| matches!(k, Value::Bytes(v) if v.as_slice() == key)).map(|(_, v)| v)
    }

    #[test]
    fn the_type_list_is_eves_row_order_and_every_type_has_a_listen_field() {
        assert_eq!(BROADCAST_TYPES[0], "HealArmor");
        assert_eq!(BROADCAST_TYPES[15], "Location");
        for t in BROADCAST_TYPES {
            let name = format!("listen_{t}");
            let f = FIELDS.iter().find(|f| f.name == name).expect("a listen field per type");
            assert_eq!(f.key, format!("listenBroadcast_{t}").as_bytes());
            assert_eq!(f.kind, HudKind::Int, "checkboxes are Int 0/1 on the wire");
            assert_eq!(f.scope, HudScope::Account);
        }
    }

    #[test]
    fn projects_present_values_and_defaults_for_absent_ones() {
        let f = project_fleet(Some(&char_doc()), Some(&user_doc()));
        assert!(f.char_open && f.user_open);
        assert_eq!(entry(&f, "listen_HealArmor").value.as_deref(), Some("0"));
        // Never written on this account: the row reads its default.
        let warp = entry(&f, "listen_WarpTo");
        assert_eq!(warp.value, None);
        assert_eq!(warp.default, "1");
        assert!(matches!(warp.set, SetTarget::Insert { .. }));
        assert_eq!(entry(&f, "formation").value.as_deref(), Some("3"));
        assert_eq!(entry(&f, "formation_size").value.as_deref(), Some("30000"));
        assert_eq!(entry(&f, "finder_group_only").default, "1");
    }

    #[test]
    fn the_defaults_are_the_corpus_defaults() {
        let f = project_fleet(None, None);
        for (name, default) in [
            ("listen_HealArmor", "0"), ("listen_HealShield", "0"), ("listen_HealCapacitor", "0"),
            ("listen_HoldPosition", "0"), ("listen_NeedBackup", "0"), ("listen_show_own", "0"),
            ("listen_Target", "1"), ("listen_InPosition", "1"), ("listen_WarpTo", "1"),
            ("formation", "0"), ("formation_size", "20000"), ("formation_spacing", "2000"),
        ] {
            assert_eq!(entry(&f, name).default, default, "{name}");
        }
    }

    #[test]
    fn the_show_own_checkbox_projects_once_but_writes_both_keys() {
        let f = project_fleet(None, Some(&user_doc()));
        assert_eq!(f.fields.iter().filter(|e| e.name == "listen_show_own").count(), 1);
        assert_eq!(entry(&f, "listen_show_own").value.as_deref(), Some("1"));

        let mut user = user_doc();
        let minted = set_fleet_field(&mut user, "listen_show_own", "0").unwrap();
        assert!(!minted, "both keys existed, so nothing was minted");
        assert_eq!(ui_leaf(&user, b"listenBroadcast_ShowOwnBroadcasts"), Some(&wrapped(Value::Int(0))));
        assert_eq!(ui_leaf(&user, b"ShowOwnBroadcasts"), Some(&wrapped(Value::Int(0))));
    }

    #[test]
    fn a_missing_side_projects_its_fields_unavailable_not_absent() {
        let f = project_fleet(Some(&char_doc()), None);
        assert!(!f.user_open);
        assert!(matches!(entry(&f, "listen_Target").set, SetTarget::Unavailable));
        assert!(matches!(entry(&f, "formation").set, SetTarget::Set { .. }));
    }

    #[test]
    fn minting_a_never_written_listen_key_writes_the_wrapped_int() {
        let mut user = user_doc();
        let minted = set_fleet_field(&mut user, "listen_WarpTo", "0").unwrap();
        assert!(minted);
        let leaf = ui_leaf(&user, b"listenBroadcast_WarpTo").expect("minted");
        let Value::Tuple(parts) = leaf else { panic!("a (timestamp, value) wrapper, never a bare leaf") };
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], ts(), "a mint carries a zero FILETIME");
        assert_eq!(parts[1], Value::Int(0), "the checkbox stays Int on the wire");
    }

    #[test]
    fn overwriting_keeps_the_leafs_timestamp_and_kind() {
        let real_ts = Value::Long(vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let mut user = Value::Dict(vec![(
            b("ui"),
            Value::Dict(vec![(b("listenBroadcast_HealArmor"), Value::Tuple(vec![real_ts.clone(), Value::Int(0)]))]),
        )]);
        assert!(!set_fleet_field(&mut user, "listen_HealArmor", "1").unwrap());
        assert_eq!(
            ui_leaf(&user, b"listenBroadcast_HealArmor"),
            Some(&Value::Tuple(vec![real_ts, Value::Int(1)]))
        );
    }

    #[test]
    fn a_key_of_the_wrong_shape_is_refused_and_untouched() {
        let mut user = Value::Dict(vec![(
            b("ui"),
            Value::Dict(vec![(b("listenBroadcast_HealArmor"), wrapped(Value::Str("yes".into())))]),
        )]);
        let before = user.clone();
        assert_eq!(set_fleet_field(&mut user, "listen_HealArmor", "1"), Err(HudError::NotEditable));
        assert_eq!(user, before);
        let f = project_fleet(None, Some(&user));
        assert!(matches!(entry(&f, "listen_HealArmor").set, SetTarget::Unavailable));
    }

    #[test]
    fn an_unknown_name_and_a_missing_section_are_errors() {
        let mut user = user_doc();
        assert!(matches!(set_fleet_field(&mut user, "listen_Nope", "1"), Err(HudError::UnknownField(_))));
        let mut bare = Value::Dict(vec![]);
        assert_eq!(set_fleet_field(&mut bare, "listen_Target", "1"), Err(HudError::NoSection));
    }

    /// Real account files store the root `ui` key as a Ref to a byte-string
    /// defined later in the stream (hud.rs). A bare `is_bytes` lookup misses it.
    #[test]
    fn a_ref_keyed_ui_section_still_resolves() {
        let doc = Value::Dict(vec![
            (Value::Ref(7), Value::Dict(vec![(b("listenBroadcast_Target"), wrapped(Value::Int(0)))])),
            (b("elsewhere"), Value::Shared { slot: 7, value: Box::new(b("ui")) }),
        ]);
        let f = project_fleet(None, Some(&doc));
        assert_eq!(entry(&f, "listen_Target").value.as_deref(), Some("0"));
    }
}
```

`HudKind` and `HudScope` need `PartialEq` for the first test's `assert_eq!` — both already derive it (`hud.rs:21-31`). `HudError` derives `PartialEq` (`hud.rs:285`).

- [ ] **Step 2: Register the module and run the tests to see them fail**

In `crates/settings-model/src/lib.rs`, after `mod hud;` add `mod fleet;`, and in the `pub use` block add:

```rust
pub use fleet::{project_fleet, set_fleet_field, ColourEntry, Fleet, WatchEntry, BROADCAST_TYPES};
```

Run: `cargo test -p settings-model fleet`
Expected: compiles (the file above is complete) and every test passes — this task's implementation IS the file. If a test fails, the table or `project_fields` is wrong; fix before moving on. The failing-test step here is the compile error you get if you register the module before writing the file, which is fine to skip.

- [ ] **Step 3: Clippy**

Run: `cargo clippy -p settings-model --all-targets -- -D warnings`
Expected: clean. If clippy flags the `while` loops in `FIELDS`, the lint name will say what it wants; `#[allow(clippy::needless_range_loop)]` is NOT the answer (they are `while`, not `for`) — read the lint and adjust.

- [ ] **Step 4: Commit**

```bash
git add crates/settings-model/src/fleet.rs crates/settings-model/src/lib.rs
git commit -m "fleet: project and write the broadcast, formation and finder scalars

The sixteen broadcast types are declared once; the listen fields and colour
keys expand from the list. Defaults are the corpus's (spec §2.2), and the
top checkbox's two keys are two rows with one name.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: `fleet.rs` — the broadcast colour leaves and the palette

**Files:**
- Modify: `crates/settings-model/src/fleet.rs`, `crates/settings-model/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub enum Colour { Absent, Cleared, Set { rgb: [f64; 3] }, Unreadable }` serialised as `{"state": "absent"}` / `{"state": "set", "rgb": [..]}` etc.
  - `pub struct ColourEntry { broadcast: String, state: Colour, default: Option<[f64; 3]> }` (replaces Task 2's stub).
  - `pub const PALETTE: [(&str, [f64; 3]); 9]`, `pub fn default_colour(broadcast: &str) -> Option<[f64; 3]>`.
  - `pub fn set_broadcast_colour(user: &mut Value, broadcast: &str, rgb: Option<[f64; 3]>) -> Result<(), FleetError>` — `None` writes `Value::None`.
  - `pub enum FleetError { UnknownBroadcast(String), NoSection, NotEditable }` — `Serialize` tagged `code`, `Display`.
  - `Fleet.colours` and `Fleet.palette` are now filled by `project_fleet`.

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `fleet.rs`:

```rust
    fn rgb(r: f64, g: f64, b_: f64) -> Value {
        Value::Tuple(vec![Value::Float(r), Value::Float(g), Value::Float(b_)])
    }

    fn user_with_colours() -> Value {
        Value::Dict(vec![(
            b("ui"),
            Value::Dict(vec![
                (b("fleet_broadcastcolor_HealArmor"), wrapped(rgb(0.1, 0.6, 0.1))),
                (b("fleet_broadcastcolor_Location"), wrapped(Value::None)),
                (b("fleet_broadcastcolor_WarpTo"), wrapped(Value::Int(4))),
            ]),
        )])
    }

    fn colour<'a>(f: &'a Fleet, broadcast: &str) -> &'a ColourEntry {
        f.colours.iter().find(|c| c.broadcast == broadcast).expect("every type is projected")
    }

    #[test]
    fn colours_are_projected_for_every_type_in_row_order_with_three_states() {
        let f = project_fleet(None, Some(&user_with_colours()));
        assert_eq!(f.colours.len(), 16);
        assert_eq!(f.colours[0].broadcast, "HealArmor");
        assert_eq!(f.colours[15].broadcast, "Location");
        assert_eq!(colour(&f, "HealArmor").state, Colour::Set { rgb: [0.1, 0.6, 0.1] });
        assert_eq!(colour(&f, "Location").state, Colour::Cleared);
        assert_eq!(colour(&f, "Target").state, Colour::Absent);
        assert_eq!(colour(&f, "WarpTo").state, Colour::Unreadable);
    }

    #[test]
    fn the_four_default_colours_ride_the_projection_and_the_rest_default_to_none() {
        let f = project_fleet(None, None);
        assert_eq!(colour(&f, "HealArmor").default, Some([0.1, 0.6, 0.1]));
        assert_eq!(colour(&f, "HealCapacitor").default, Some([1.0, 0.7, 0.0]));
        assert_eq!(colour(&f, "HealShield").default, Some([0.2, 0.5, 1.0]));
        assert_eq!(colour(&f, "Target").default, Some([0.75, 0.0, 0.0]));
        assert_eq!(colour(&f, "NeedBackup").default, None);
        // No account file: every state is Absent, never an error.
        assert!(f.colours.iter().all(|c| c.state == Colour::Absent));
    }

    #[test]
    fn the_palette_is_eves_nine_swatches_with_teal_from_the_live_capture() {
        let f = project_fleet(None, None);
        assert_eq!(f.palette.len(), 9);
        assert_eq!(f.palette[0], ("yellow".to_string(), [1.0, 0.7, 0.0]));
        assert_eq!(f.palette[4], ("teal".to_string(), [0.0, 0.63, 0.57]));
        assert_eq!(f.palette[8], ("white".to_string(), [0.7, 0.7, 0.7]));
    }

    #[test]
    fn setting_a_colour_overwrites_in_place_and_keeps_the_timestamp() {
        let real_ts = Value::Long(vec![9, 9, 9, 9, 9, 9, 9, 9]);
        let mut user = Value::Dict(vec![(
            b("ui"),
            Value::Dict(vec![(b("fleet_broadcastcolor_Target"), Value::Tuple(vec![real_ts.clone(), rgb(0.75, 0.0, 0.0)]))]),
        )]);
        set_broadcast_colour(&mut user, "Target", Some([0.2, 0.5, 1.0])).unwrap();
        assert_eq!(
            ui_leaf(&user, b"fleet_broadcastcolor_Target"),
            Some(&Value::Tuple(vec![real_ts, rgb(0.2, 0.5, 1.0)]))
        );
    }

    #[test]
    fn clearing_writes_none_without_removing_the_key() {
        let mut user = user_with_colours();
        set_broadcast_colour(&mut user, "HealArmor", None).unwrap();
        assert_eq!(ui_leaf(&user, b"fleet_broadcastcolor_HealArmor"), Some(&wrapped(Value::None)));
        let f = project_fleet(None, Some(&user));
        assert_eq!(colour(&f, "HealArmor").state, Colour::Cleared);
    }

    #[test]
    fn setting_an_absent_colour_mints_the_wrapped_leaf() {
        let mut user = user_with_colours();
        set_broadcast_colour(&mut user, "JumpTo", Some([0.0, 0.63, 0.57])).unwrap();
        assert_eq!(ui_leaf(&user, b"fleet_broadcastcolor_JumpTo"), Some(&wrapped(rgb(0.0, 0.63, 0.57))));
        // A cleared, absent type: writing None still mints, so the client reads
        // an explicit "no colour" rather than falling back to its default.
        set_broadcast_colour(&mut user, "Target", None).unwrap();
        assert_eq!(ui_leaf(&user, b"fleet_broadcastcolor_Target"), Some(&wrapped(Value::None)));
    }

    #[test]
    fn an_unreadable_colour_is_refused_not_overwritten() {
        let mut user = user_with_colours();
        let before = user.clone();
        assert_eq!(set_broadcast_colour(&mut user, "WarpTo", Some([0.0, 0.0, 0.0])), Err(FleetError::NotEditable));
        assert_eq!(user, before);
    }

    #[test]
    fn an_unknown_type_and_a_missing_section_are_errors() {
        let mut user = user_with_colours();
        assert_eq!(
            set_broadcast_colour(&mut user, "Nope", None),
            Err(FleetError::UnknownBroadcast("Nope".into()))
        );
        let mut bare = Value::Dict(vec![]);
        assert_eq!(set_broadcast_colour(&mut bare, "Target", None), Err(FleetError::NoSection));
    }

    /// A stored colour written as Int (the client is not type-stable across
    /// generations — `hud.rs`'s Float-as-Int finding) still reads.
    #[test]
    fn an_int_component_reads_as_a_float() {
        let user = Value::Dict(vec![(
            b("ui"),
            Value::Dict(vec![(
                b("fleet_broadcastcolor_InPosition"),
                wrapped(Value::Tuple(vec![Value::Int(0), Value::Int(0), Value::Int(0)])),
            )]),
        )]);
        let f = project_fleet(None, Some(&user));
        assert_eq!(colour(&f, "InPosition").state, Colour::Set { rgb: [0.0, 0.0, 0.0] });
    }
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p settings-model fleet`
Expected: compile errors — `Colour`, `set_broadcast_colour`, `FleetError`, `ColourEntry::state` do not exist.

- [ ] **Step 3: Implement**

In `fleet.rs`, extend the imports:

```rust
use crate::hud::{project_fields, section_dict_mut, set_field, Field, HudEntry, HudError, HudKind, HudScope};
use crate::treewalk::{collect_shared, effective, find_child, inline_all, is_bytes, section, Entries, SharedTable};
```

Replace the `ColourEntry` stub with, and add after it:

```rust
/// EVE's "Select Color" swatches, left to right, top row then bottom (spec
/// §2.5). Names are labels for the datalist and tooltips, never written to a
/// file — unlike `overview_pack::PALETTE`, whose names are pack vocabulary.
pub const PALETTE: [(&str, [f64; 3]); 9] = [
    ("yellow", [1.0, 0.7, 0.0]),
    ("orange", [1.0, 0.35, 0.0]),
    ("red", [0.75, 0.0, 0.0]),
    ("green", [0.1, 0.6, 0.1]),
    ("teal", [0.0, 0.63, 0.57]),
    ("blue", [0.2, 0.5, 1.0]),
    ("darkBlue", [0.0, 0.15, 0.6]),
    ("black", [0.0, 0.0, 0.0]),
    ("white", [0.7, 0.7, 0.7]),
];

/// The colour EVE shows for a type whose key was never written — the four the
/// client itself stores on first open of the dialog, identical in every
/// corpus account (spec §2.2). Everything else defaults to no colour.
pub fn default_colour(broadcast: &str) -> Option<[f64; 3]> {
    match broadcast {
        "HealArmor" => Some([0.1, 0.6, 0.1]),
        "HealCapacitor" => Some([1.0, 0.7, 0.0]),
        "HealShield" => Some([0.2, 0.5, 1.0]),
        "Target" => Some([0.75, 0.0, 0.0]),
        _ => None,
    }
}

/// The three wire states of a colour leaf (spec §2.4), plus the one this
/// module refuses to touch.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Colour {
    /// No key: the type shows `ColourEntry::default`.
    Absent,
    /// `(ts, None)` — EVE's ✕. Captured live 2026-09-18 on `Location`.
    Cleared,
    Set { rgb: [f64; 3] },
    /// A key of a shape this module does not write; refused on write rather
    /// than overwritten (the `hud.rs` rule).
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColourEntry {
    pub broadcast: String,
    pub state: Colour,
    pub default: Option<[f64; 3]>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum FleetError {
    UnknownBroadcast(String),
    /// No `ui` section to write into. Real character and account files always
    /// have one.
    NoSection,
    /// The key holds a shape this module does not write; overwriting it would
    /// change its type.
    NotEditable,
}

impl std::fmt::Display for FleetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FleetError::UnknownBroadcast(t) => write!(f, "Unknown broadcast type {t:?}."),
            FleetError::NoSection => write!(f, "This file has no section to write this value into."),
            FleetError::NotEditable => {
                write!(f, "This value has an unexpected type here and cannot be edited safely.")
            }
        }
    }
}

fn num(v: &Value, sh: &SharedTable) -> Option<f64> {
    match effective(v, sh) {
        Value::Float(f) => Some(*f),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

/// Three numbers, or `None` for anything else.
fn rgb_of(v: &Value, sh: &SharedTable) -> Option<[f64; 3]> {
    let Value::Tuple(items) = effective(v, sh) else { return None };
    let [r, g, b] = items.as_slice() else { return None };
    Some([num(r, sh)?, num(g, sh)?, num(b, sh)?])
}

fn rgb_value([r, g, b]: [f64; 3]) -> Value {
    Value::Tuple(vec![Value::Float(r), Value::Float(g), Value::Float(b)])
}

fn read_colour(ui: &Entries, key: &[u8], sh: &SharedTable) -> Colour {
    let Some(v) = find_child(ui, key, sh) else { return Colour::Absent };
    let Value::Tuple(w) = v else { return Colour::Unreadable };
    if w.len() != 2 {
        return Colour::Unreadable;
    }
    match effective(&w[1], sh) {
        Value::None => Colour::Cleared,
        inner => rgb_of(inner, sh).map_or(Colour::Unreadable, |rgb| Colour::Set { rgb }),
    }
}

fn project_colours(user_root: Option<&Value>) -> Vec<ColourEntry> {
    let mut sh = SharedTable::new();
    if let Some(u) = user_root {
        collect_shared(u, &mut sh);
    }
    let ui = user_root.and_then(|u| section(u, b"ui", &sh)).map(|(entries, _)| entries);
    BROADCAST_TYPES
        .iter()
        .zip(COLOUR_KEYS)
        .map(|(t, key)| ColourEntry {
            broadcast: t.to_string(),
            state: ui.map_or(Colour::Absent, |ui| read_colour(ui, key, &sh)),
            default: default_colour(t),
        })
        .collect()
}

/// Write one broadcast colour: `Some` → the three floats, `None` → `Value::None`
/// (EVE's ✕). An absent key is minted; a key of another shape is refused. The
/// caller reshares (this inlines first, so it is always a structural edit).
pub fn set_broadcast_colour(
    user: &mut Value,
    broadcast: &str,
    rgb: Option<[f64; 3]>,
) -> Result<(), FleetError> {
    let Some(i) = BROADCAST_TYPES.iter().position(|t| *t == broadcast) else {
        return Err(FleetError::UnknownBroadcast(broadcast.to_string()));
    };
    let key = COLOUR_KEYS[i];
    let value = rgb.map_or(Value::None, rgb_value);
    inline_all(user);
    let ui = section_dict_mut(user, b"ui").ok_or(FleetError::NoSection)?;
    let flat = SharedTable::new();
    match ui.iter_mut().find(|(k, _)| is_bytes(k, key)) {
        Some((_, slot)) => match slot {
            Value::Tuple(w) if w.len() == 2 && (w[1] == Value::None || rgb_of(&w[1], &flat).is_some()) => {
                w[1] = value;
            }
            _ => return Err(FleetError::NotEditable),
        },
        None => ui.push((Value::Bytes(key.to_vec()), Value::Tuple(vec![Value::Long(vec![0u8; 8]), value]))),
    }
    Ok(())
}
```

In `project_fleet`, replace `colours: Vec::new()` with `colours: project_colours(user_root)` and `palette: Vec::new()` with `palette: PALETTE.iter().map(|(n, c)| (n.to_string(), *c)).collect()`.

In `lib.rs`, extend the export line:

```rust
pub use fleet::{
    default_colour, project_fleet, set_broadcast_colour, set_fleet_field, Colour, ColourEntry, Fleet,
    FleetError, WatchEntry, BROADCAST_TYPES, PALETTE,
};
```

- [ ] **Step 4: Run the tests and clippy**

Run: `cargo test -p settings-model fleet && cargo clippy -p settings-model --all-targets -- -D warnings`
Expected: all pass, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/fleet.rs crates/settings-model/src/lib.rs
git commit -m "fleet: the sixteen broadcast colour leaves and EVE's nine-swatch palette

Absent, cleared (None — EVE's ✕, captured live) and set are three states the
projection keeps apart; a leaf of any other shape is refused, not overwritten.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: `fleet.rs` — the watch-list map and the batch leaf table

**Files:**
- Modify: `crates/settings-model/src/fleet.rs`, `crates/settings-model/src/lib.rs`

**Interfaces:**
- Produces:
  - `Fleet.watchlist: Vec<WatchEntry { char_id: u64, rgb: Option<[f64; 3]> }>` in file order.
  - `pub fn set_watchlist_colour(char_root: &mut Value, id: u64, rgb: Option<[f64; 3]>) -> Result<(), FleetError>` — `None` removes the entry; mints the map when absent.
  - `pub struct FleetLeaf { scope: HudScope, path: [&'static [u8]; 2] }` (`Copy + Eq + Debug`) and `pub static FLEET_LEAVES: [FleetLeaf; 39]` — the 22 scalar rows, the 16 colour keys, the watch-list map (used by Task 6).

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module:

```rust
    fn char_with_watchlist() -> Value {
        let map = Value::Dict(vec![
            (Value::Int(1001131163), rgb(0.2, 0.5, 1.0)),
            (Value::Int(1694010657), rgb(1.0, 0.7, 0.0)),
            (Value::Int(90000001), Value::Str("blue".into())),
        ]);
        Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("fleet_watchlistcolors"), wrapped(map))]))])
    }

    fn watch_map(root: &Value) -> &Entries {
        let leaf = ui_leaf(root, b"fleet_watchlistcolors").expect("map present");
        let Value::Tuple(parts) = leaf else { panic!("wrapped") };
        let Value::Dict(d) = &parts[1] else { panic!("dict payload") };
        d
    }

    #[test]
    fn the_watch_list_projects_in_file_order_and_keeps_unreadable_entries() {
        let f = project_fleet(Some(&char_with_watchlist()), None);
        assert_eq!(
            f.watchlist,
            vec![
                WatchEntry { char_id: 1001131163, rgb: Some([0.2, 0.5, 1.0]) },
                WatchEntry { char_id: 1694010657, rgb: Some([1.0, 0.7, 0.0]) },
                WatchEntry { char_id: 90000001, rgb: None },
            ]
        );
        assert!(project_fleet(None, None).watchlist.is_empty());
        assert!(project_fleet(Some(&char_doc()), None).watchlist.is_empty(), "no map is an empty list");
    }

    #[test]
    fn recolouring_overwrites_the_entry_in_place() {
        let mut c = char_with_watchlist();
        set_watchlist_colour(&mut c, 1001131163, Some([0.75, 0.0, 0.0])).unwrap();
        let map = watch_map(&c);
        assert_eq!(map.len(), 3);
        assert_eq!(map[0], (Value::Int(1001131163), rgb(0.75, 0.0, 0.0)));
    }

    #[test]
    fn adding_appends_an_int_key_and_removing_retains_the_rest() {
        let mut c = char_with_watchlist();
        set_watchlist_colour(&mut c, 2117000000, Some([0.2, 0.5, 1.0])).unwrap();
        assert_eq!(watch_map(&c)[3], (Value::Int(2117000000), rgb(0.2, 0.5, 1.0)));
        set_watchlist_colour(&mut c, 1694010657, None).unwrap();
        let ids: Vec<&Value> = watch_map(&c).iter().map(|(k, _)| k).collect();
        assert_eq!(ids, vec![&Value::Int(1001131163), &Value::Int(90000001), &Value::Int(2117000000)]);
        // The unreadable entry can be removed too — that is the only edit it offers.
        set_watchlist_colour(&mut c, 90000001, None).unwrap();
        assert_eq!(watch_map(&c).len(), 2);
    }

    #[test]
    fn the_map_is_minted_on_first_add_and_not_on_a_remove() {
        let mut c = char_doc();
        set_watchlist_colour(&mut c, 5, None).unwrap();
        assert!(ui_leaf(&c, b"fleet_watchlistcolors").is_none(), "nothing stored, nothing to clear");
        set_watchlist_colour(&mut c, 5, Some([0.0, 0.0, 0.0])).unwrap();
        let leaf = ui_leaf(&c, b"fleet_watchlistcolors").expect("minted");
        let Value::Tuple(parts) = leaf else { panic!("wrapped") };
        assert_eq!(parts[0], ts(), "a mint carries a zero FILETIME");
        assert_eq!(watch_map(&c), &vec![(Value::Int(5), rgb(0.0, 0.0, 0.0))]);
    }

    /// Ids above i32 are written as the client writes ids: a minimal-width
    /// little-endian two's-complement Long (spec §2.6). Read back either way.
    #[test]
    fn an_id_above_the_int_range_is_a_minimal_long_and_reads_back() {
        let mut c = char_doc();
        let id: u64 = 3_000_000_000; // 0xB2D05E00 — top bit of byte 4 set, so 5 bytes
        set_watchlist_colour(&mut c, id, Some([0.2, 0.5, 1.0])).unwrap();
        assert_eq!(watch_map(&c)[0].0, Value::Long(vec![0x00, 0x5E, 0xD0, 0xB2, 0x00]));
        let f = project_fleet(Some(&c), None);
        assert_eq!(f.watchlist[0].char_id, id);
        // And a Long the client happened to write for a small value matches by value.
        let mut d = Value::Dict(vec![(
            b("ui"),
            Value::Dict(vec![(b("fleet_watchlistcolors"), wrapped(Value::Dict(vec![(Value::Long(vec![7]), rgb(0.0, 0.0, 0.0))])))]),
        )]);
        assert_eq!(project_fleet(Some(&d), None).watchlist[0].char_id, 7);
        set_watchlist_colour(&mut d, 7, None).unwrap();
        assert!(watch_map(&d).is_empty());
    }

    #[test]
    fn the_leaf_table_covers_every_key_once() {
        assert_eq!(FLEET_LEAVES.len(), 39);
        let chars = FLEET_LEAVES.iter().filter(|l| l.scope == HudScope::Char).count();
        assert_eq!(chars, 5, "formation trio, finder toggle, watch-list map");
        let mut keys: Vec<&[u8]> = FLEET_LEAVES.iter().map(|l| l.path[1]).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 39, "no key appears twice");
        assert!(FLEET_LEAVES.iter().all(|l| l.path[0] == b"ui"));
        assert!(FLEET_LEAVES.iter().any(|l| l.path[1] == b"ShowOwnBroadcasts"));
        assert!(FLEET_LEAVES.iter().any(|l| l.path[1] == b"fleet_broadcastcolor_Location"));
        assert!(FLEET_LEAVES.iter().any(|l| l.path[1] == b"fleet_watchlistcolors" && l.scope == HudScope::Char));
    }
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p settings-model fleet`
Expected: compile errors — `set_watchlist_colour`, `FLEET_LEAVES` missing.

- [ ] **Step 3: Implement**

Extend the treewalk import: add `as_dict` and `dict_inner_mut`:

```rust
use crate::treewalk::{as_dict, collect_shared, dict_inner_mut, effective, find_child, inline_all, is_bytes, section, Entries, SharedTable};
```

Add after `set_broadcast_colour`:

```rust
const WATCHLIST_KEY: &[u8] = b"fleet_watchlistcolors";

/// A character id however the client stored it: `Int` today, `Long` once ids
/// pass 2³¹ (spec §2.6). Negative or oversized values are not ids.
fn id_of(v: &Value) -> Option<u64> {
    match v {
        Value::Int(i) => u64::try_from(*i).ok(),
        Value::Long(bytes) if !bytes.is_empty() && bytes.len() <= 8 && bytes[bytes.len() - 1] & 0x80 == 0 => {
            let mut buf = [0u8; 8];
            buf[..bytes.len()].copy_from_slice(bytes);
            Some(u64::from_le_bytes(buf))
        }
        _ => None,
    }
}

/// The key the client would write for `id`: `Int` while it fits, else the
/// minimal-width little-endian two's-complement `Long` (2,321 of 2,322
/// id-sized Longs in a sampled file are minimal-width).
// ponytail: no corpus file has a watch-listed id above 2³¹ yet, so the Long arm
// is unit-tested only — the first real file with one is the test that matters.
fn id_key(id: u64) -> Value {
    if let Ok(i) = i32::try_from(id) {
        return Value::Int(i64::from(i));
    }
    let mut bytes = id.to_le_bytes().to_vec();
    while bytes.len() > 1 && bytes[bytes.len() - 1] == 0 {
        bytes.pop();
    }
    if bytes[bytes.len() - 1] & 0x80 != 0 {
        bytes.push(0); // keep the sign bit clear: a positive two's-complement
    }
    Value::Long(bytes)
}

fn project_watchlist(char_root: Option<&Value>) -> Vec<WatchEntry> {
    let Some(c) = char_root else { return Vec::new() };
    let mut sh = SharedTable::new();
    collect_shared(c, &mut sh);
    let Some((ui, _)) = section(c, b"ui", &sh) else { return Vec::new() };
    let Some(map) = find_child(ui, WATCHLIST_KEY, &sh).and_then(|v| as_dict(v, &sh)) else { return Vec::new() };
    map.iter()
        .filter_map(|(k, v)| Some(WatchEntry { char_id: id_of(effective(k, &sh))?, rgb: rgb_of(v, &sh) }))
        .collect()
}

/// Set (`Some`) or remove (`None`) one character's watch-list colour. The map
/// is minted as `(zero FILETIME, {})` when absent and something is being set,
/// as `set_state_color` mints `stateColors`. Matches an existing entry by id
/// VALUE, whichever wire kind the client used for the key. The caller
/// reshares.
pub fn set_watchlist_colour(
    char_root: &mut Value,
    id: u64,
    rgb: Option<[f64; 3]>,
) -> Result<(), FleetError> {
    inline_all(char_root);
    let ui = section_dict_mut(char_root, b"ui").ok_or(FleetError::NoSection)?;
    if !ui.iter().any(|(k, _)| is_bytes(k, WATCHLIST_KEY)) {
        if rgb.is_none() {
            return Ok(()); // nothing stored, nothing to clear
        }
        ui.push((
            Value::Bytes(WATCHLIST_KEY.to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(Vec::new())]),
        ));
    }
    let (_, slot) = ui.iter_mut().find(|(k, _)| is_bytes(k, WATCHLIST_KEY)).expect("just checked");
    let entries = dict_inner_mut(slot).ok_or(FleetError::NotEditable)?;
    match rgb {
        None => entries.retain(|(k, _)| id_of(k) != Some(id)),
        Some(c) => {
            let val = rgb_value(c);
            match entries.iter_mut().find(|(k, _)| id_of(k) == Some(id)) {
                Some((_, v)) => *v = val,
                None => entries.push((id_key(id), val)),
            }
        }
    }
    Ok(())
}

/// One key this module owns, as a batch-copy leaf. `batch::Category::Fleet`
/// borrows these, so the batch key list and the editor's are one list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FleetLeaf {
    pub scope: HudScope,
    /// `[section, key]` — every fleet key is one level under `ui`.
    pub path: [&'static [u8]; 2],
}

/// The 22 scalar rows, the 16 colour keys and the watch-list map. A `static`
/// rather than a `const` so `&FLEET_LEAVES[i]` is `'static`, which is what
/// `Category::Fleet` holds.
pub static FLEET_LEAVES: [FleetLeaf; 39] = {
    let mut out = [FleetLeaf { scope: HudScope::Char, path: [b"ui", WATCHLIST_KEY] }; 39];
    let mut i = 0;
    while i < 22 {
        out[i] = FleetLeaf { scope: FIELDS[i].scope, path: [FIELDS[i].section, FIELDS[i].key] };
        i += 1;
    }
    let mut t = 0;
    while t < 16 {
        out[22 + t] = FleetLeaf { scope: HudScope::Account, path: [b"ui", COLOUR_KEYS[t]] };
        t += 1;
    }
    out // index 38 keeps the watch-list initialiser
};
```

In `project_fleet`, replace `watchlist: Vec::new()` with `watchlist: project_watchlist(char_root)`.

In `lib.rs`, add `set_watchlist_colour`, `FleetLeaf`, `FLEET_LEAVES` to the `fleet::` export.

- [ ] **Step 4: Run the tests and clippy**

Run: `cargo test -p settings-model fleet && cargo clippy -p settings-model --all-targets -- -D warnings`
Expected: all pass, clippy clean. (`HudScope` must be `Copy` for the `static` initialiser — it already derives `Clone, Copy` at `hud.rs:21`.)

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/fleet.rs crates/settings-model/src/lib.rs
git commit -m "fleet: the watch-list colour map, and the leaf table batch copy borrows

Entries match by id value whichever wire kind the key has; a new id above
i32 is written as the client's minimal-width Long. FLEET_LEAVES lists every
key once so the batch category cannot drift from the editor.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: The corpus gate and the synthetic fixtures

**Files:**
- Create: `crates/settings-model/tests/fleet_corpus.rs`
- Modify: `crates/settings-model/src/bin/gen_fixtures.rs` (`char_modern`'s `ui` block near :349, `user_modern`'s `ui` block near :705)
- Regenerate: `fixtures/synthetic/profile/settings_Default/*.dat`

**Interfaces:**
- Consumes: `project_fleet`, `Colour`, `common::char_files()` / `user_files()` / `real_corpus_present()` from `tests/common/mod.rs`.

- [ ] **Step 1: Write the gate**

Create `crates/settings-model/tests/fleet_corpus.rs`:

```rust
//! Real-data guard for the fleet keys. Every unit test in `fleet.rs` builds its
//! own fixture, so a table naming the wrong section or key passes them all
//! while reading nothing from a real file — the class of fault `hud_corpus.rs`
//! exists for. This asserts each fleet field, each colour type and the watch
//! list actually project a value somewhere in the corpus.
//!
//! The twelve colour types beyond the client's four are carried by exactly two
//! real files (the 2026-09-18 captures on the owner's B1 account), so their bar
//! is one file, not twenty. Skips the real half silently when the corpus is not
//! checked out; the synthetic half always runs.

mod common;

use settings_model::{project_fleet, Colour, BROADCAST_TYPES};

const ENOUGH_REAL: usize = 20;
const ENOUGH_REAL_RARE: usize = 1;
const ENOUGH_SYNTHETIC: usize = 1;

/// The seven listen keys every account carries, plus the four the client
/// stores itself — the ones that can clear a twenty-file bar.
const COMMON_LISTEN: [&str; 7] = [
    "listen_HealArmor", "listen_HealShield", "listen_HealCapacitor", "listen_Target",
    "listen_HoldPosition", "listen_InPosition", "listen_NeedBackup",
];
const COMMON_COLOURS: [&str; 4] = ["HealArmor", "HealCapacitor", "HealShield", "Target"];
const CHAR_FIELDS: [&str; 4] = ["formation", "formation_size", "formation_spacing", "finder_group_only"];

fn bar(common: bool) -> usize {
    if common { ENOUGH_REAL } else { ENOUGH_REAL_RARE }
}

#[test]
fn every_account_fleet_key_reads_from_a_real_file() {
    let listen: Vec<String> = BROADCAST_TYPES.iter().map(|t| format!("listen_{t}")).collect();
    let mut listen_syn = vec![0usize; 16];
    let mut listen_real = vec![0usize; 16];
    let mut colour_syn = vec![0usize; 16];
    let mut colour_real = vec![0usize; 16];
    let mut scanned = 0usize;

    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        scanned += 1;
        let fleet = project_fleet(None, Some(&doc));
        for (i, name) in listen.iter().enumerate() {
            let e = fleet.fields.iter().find(|e| &e.name == name).expect("field projected");
            if e.value.is_some() {
                if f.synthetic { listen_syn[i] += 1 } else { listen_real[i] += 1 }
            }
        }
        for (i, c) in fleet.colours.iter().enumerate() {
            if matches!(c.state, Colour::Set { .. } | Colour::Cleared) {
                if f.synthetic { colour_syn[i] += 1 } else { colour_real[i] += 1 }
            }
        }
    }

    for (i, name) in listen.iter().enumerate() {
        assert!(listen_syn[i] >= ENOUGH_SYNTHETIC, "{name} projected no value in any synthetic account fixture");
        if common::real_corpus_present() {
            let need = bar(COMMON_LISTEN.contains(&name.as_str()));
            assert!(listen_real[i] >= need, "{name} projected a value in only {}/{scanned} real account files", listen_real[i]);
        }
    }
    for (i, t) in BROADCAST_TYPES.iter().enumerate() {
        assert!(colour_syn[i] >= ENOUGH_SYNTHETIC, "colour {t} read from no synthetic account fixture");
        if common::real_corpus_present() {
            let need = bar(COMMON_COLOURS.contains(t));
            assert!(colour_real[i] >= need, "colour {t} read from only {}/{scanned} real account files", colour_real[i]);
        }
    }
}

#[test]
fn every_character_fleet_key_reads_from_a_real_file() {
    let mut field_syn = [0usize; CHAR_FIELDS.len()];
    let mut field_real = [0usize; CHAR_FIELDS.len()];
    let mut watch_syn = 0usize;
    let mut watch_real = 0usize;
    let mut scanned = 0usize;

    for f in common::char_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        scanned += 1;
        let fleet = project_fleet(Some(&doc), None);
        for (i, name) in CHAR_FIELDS.iter().enumerate() {
            let e = fleet.fields.iter().find(|e| &e.name == name).expect("field projected");
            if e.value.is_some() {
                if f.synthetic { field_syn[i] += 1 } else { field_real[i] += 1 }
            }
        }
        if !fleet.watchlist.is_empty() {
            if f.synthetic { watch_syn += 1 } else { watch_real += 1 }
        }
    }

    for (i, name) in CHAR_FIELDS.iter().enumerate() {
        assert!(field_syn[i] >= ENOUGH_SYNTHETIC, "{name} projected no value in any synthetic character fixture");
        if common::real_corpus_present() {
            assert!(field_real[i] >= ENOUGH_REAL, "{name} projected a value in only {}/{scanned} real character files", field_real[i]);
        }
    }
    assert!(watch_syn >= ENOUGH_SYNTHETIC, "no synthetic character fixture carries a watch list");
    if common::real_corpus_present() {
        assert!(watch_real >= ENOUGH_REAL, "a watch list read from only {watch_real}/{scanned} real character files");
    }
}
```

- [ ] **Step 2: Run the gate to see the synthetic half fail**

Run: `cargo test -p settings-model --test fleet_corpus`
Expected: FAIL — "listen_HealArmor projected no value in any synthetic account fixture" (the generator writes no fleet keys yet). If the real corpus is present the real half may already pass; the synthetic assertion comes first and fails regardless.

- [ ] **Step 3: Add fleet keys to the generator**

In `gen_fixtures.rs`, inside `char_modern()`'s `ui` dict (the block that starts `(b("fightersDetachedPosition"), …)` near :351), add after the `fightersDetachedPosition` line:

```rust
                // Fleet: the formation trio, the finder toggle and a two-member
                // watch list keyed by character id (spec 2026-09-18 §2.1).
                (b("setFleetFormation"), w(i(0))),
                (b("setFleetFormationSize"), w(i(20000))),
                (b("setFleetFormationSpacing"), w(i(2000))),
                (b("fleetfinder_showGroupAndHighStandingsFleets"), w(i(1))),
                (
                    b("fleet_watchlistcolors"),
                    w(dict(vec![
                        (i(1001131163), tup(vec![f(0.2), f(0.5), f(1.0)])),
                        (i(1694010657), tup(vec![f(1.0), f(0.7), f(0.0)])),
                    ])),
                ),
```

Inside `user_modern()`'s `ui` dict (near :705, after the `displayFighterUI` line), add:

```rust
        // Fleet broadcast settings: every listen key (the client writes seven
        // by itself and the rest only when toggled) and every colour key, one
        // of them cleared to None the way EVE's ✕ writes it.
        (b("listenBroadcast_ShowOwnBroadcasts"), w(i(1))),
        (b("ShowOwnBroadcasts"), w(i(1))),
        (b("listenBroadcast_HealArmor"), w(i(0))),
        (b("listenBroadcast_HealCapacitor"), w(i(0))),
        (b("listenBroadcast_NeedBackup"), w(i(0))),
        (b("listenBroadcast_Target"), w(i(1))),
        (b("listenBroadcast_HealShield"), w(i(0))),
        (b("listenBroadcast_WarpTo"), w(i(1))),
        (b("listenBroadcast_TravelTo"), w(i(1))),
        (b("listenBroadcast_Event"), w(i(1))),
        (b("listenBroadcast_JumpTo"), w(i(1))),
        (b("listenBroadcast_AlignTo"), w(i(1))),
        (b("listenBroadcast_HealTarget"), w(i(1))),
        (b("listenBroadcast_InPosition"), w(i(1))),
        (b("listenBroadcast_EnemySpotted"), w(i(1))),
        (b("listenBroadcast_HoldPosition"), w(i(0))),
        (b("listenBroadcast_JumpBeacon"), w(i(1))),
        (b("listenBroadcast_Location"), w(i(1))),
        (b("fleet_broadcastcolor_HealArmor"), w(tup(vec![f(0.1), f(0.6), f(0.1)]))),
        (b("fleet_broadcastcolor_HealCapacitor"), w(tup(vec![f(1.0), f(0.7), f(0.0)]))),
        (b("fleet_broadcastcolor_NeedBackup"), w(tup(vec![f(1.0), f(0.7), f(0.0)]))),
        (b("fleet_broadcastcolor_Target"), w(tup(vec![f(0.75), f(0.0), f(0.0)]))),
        (b("fleet_broadcastcolor_HealShield"), w(tup(vec![f(0.2), f(0.5), f(1.0)]))),
        (b("fleet_broadcastcolor_WarpTo"), w(tup(vec![f(1.0), f(0.35), f(0.0)]))),
        (b("fleet_broadcastcolor_TravelTo"), w(tup(vec![f(0.75), f(0.0), f(0.0)]))),
        (b("fleet_broadcastcolor_Event"), w(tup(vec![f(0.1), f(0.6), f(0.1)]))),
        (b("fleet_broadcastcolor_JumpTo"), w(tup(vec![f(0.0), f(0.63), f(0.57)]))),
        (b("fleet_broadcastcolor_AlignTo"), w(tup(vec![f(0.2), f(0.5), f(1.0)]))),
        (b("fleet_broadcastcolor_HealTarget"), w(tup(vec![f(0.0), f(0.15), f(0.6)]))),
        (b("fleet_broadcastcolor_InPosition"), w(tup(vec![f(0.0), f(0.0), f(0.0)]))),
        (b("fleet_broadcastcolor_EnemySpotted"), w(tup(vec![f(0.7), f(0.7), f(0.7)]))),
        (b("fleet_broadcastcolor_HoldPosition"), w(tup(vec![f(1.0), f(0.7), f(0.0)]))),
        (b("fleet_broadcastcolor_JumpBeacon"), w(tup(vec![f(1.0), f(0.35), f(0.0)]))),
        (b("fleet_broadcastcolor_Location"), w(Value::None)),
```

`i`, `f`, `w`, `tup`, `dict`, `b` are the generator's own helpers (`gen_fixtures.rs:32-60`).

- [ ] **Step 4: Regenerate the synthetic corpus and run every gate**

Run: `cargo run -p settings-model --bin gen_fixtures && cargo test -p settings-model`
Expected: the generator prints the seven profile files; the whole crate passes, including `fleet_corpus`, `hud_corpus` and every other `*_corpus`/`*_realshape` gate (they read the same fixtures — a regression there means the added keys collided with something a gate counts, which they should not, but this is where it would show).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/tests/fleet_corpus.rs crates/settings-model/src/bin/gen_fixtures.rs fixtures/synthetic
git commit -m "fleet: corpus gate for every key, and fleet keys in the synthetic fixtures

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: `Category::Fleet`, `Aspect::Fleet`, and preset detection

**Files:**
- Modify: `crates/settings-model/src/batch.rs:10,17-62,64-95,117-131` (the enum, `key_path`, `absent_means_default`)
- Modify: `app/src-tauri/src/setup.rs:24-31` (`Aspect`), `:69-105` (`aspect_writes`)
- Modify: `app/src-tauri/src/presets.rs:325-345` (`derive_aspects`)

**Interfaces:**
- Consumes: `settings_model::{FleetLeaf, FLEET_LEAVES, HudScope}`.
- Produces: `Category::Fleet(&'static FleetLeaf)`; `Aspect::Fleet` serialised `"fleet"`; `derive_aspects` reports `Aspect::Fleet` when any fleet key is present on either document.

- [ ] **Step 1: Write the failing tests**

In `crates/settings-model/src/batch.rs`'s `tests` module, add:

```rust
    #[test]
    fn a_fleet_category_is_one_leaf_under_ui_and_absence_means_default() {
        let leaf = settings_model_fleet_leaf(b"fleet_watchlistcolors");
        let cat = Category::Fleet(leaf);
        assert_eq!(cat.key_path(), &[b"ui".as_slice(), b"fleet_watchlistcolors".as_slice()]);
        assert!(cat.absent_means_default(), "a copy makes the target match key-for-key, like the HUD leaves");
        // Extract → apply carries the map, and a source without it deletes the target's.
        let map = Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(5), Value::Tuple(vec![Value::Float(0.2), Value::Float(0.5), Value::Float(1.0)]))])]);
        let source = Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("fleet_watchlistcolors"), map.clone())]))]);
        let mut target = Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("keep"), Value::Int(1))]))]);
        apply_to_tree(&mut target, &extract_categories(&source, &[cat]));
        let Value::Dict(root) = &target else { panic!("dict") };
        let (_, ui) = root.iter().find(|(k, _)| is_bytes(k, b"ui")).unwrap();
        let Value::Dict(ui) = ui else { panic!("dict") };
        assert!(ui.iter().any(|(k, _)| is_bytes(k, b"fleet_watchlistcolors")));
        assert!(ui.iter().any(|(k, _)| is_bytes(k, b"keep")), "siblings survive");

        let bare_source = Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("other"), Value::Int(0))]))]);
        let extracted = extract_categories(&bare_source, &[cat]);
        assert_eq!(extracted, vec![(cat, None)], "absent on the source = EVE's default = delete on the target");
    }

    fn settings_model_fleet_leaf(key: &[u8]) -> &'static crate::fleet::FleetLeaf {
        crate::fleet::FLEET_LEAVES.iter().find(|l| l.path[1] == key).expect("a fleet leaf")
    }
```

In `app/src-tauri/src/setup.rs`'s `tests` module (near the `layout_carries_the_whole_hud_across_both_files` test at :726), add:

```rust
    #[test]
    fn fleet_carries_every_leaf_on_its_own_side() {
        let w = aspect_writes(&[Aspect::Fleet]);
        assert_eq!(w.char_categories.len(), 5, "formation trio, finder toggle, watch-list map");
        assert_eq!(w.account_categories.len(), 34, "18 listen rows and 16 colours");
        assert!(w.writes_account() && w.writes_char());
        assert!(!w.copies_char_geometry(), "no window geometry moves");
        for cat in w.char_categories.iter().chain(&w.account_categories) {
            let Category::Fleet(leaf) = cat else { panic!("only fleet leaves") };
            assert_eq!(leaf.path[0], b"ui");
        }
        let side = |key: &[u8]| {
            let leaf = settings_model::FLEET_LEAVES.iter().find(|l| l.path[1] == key).expect("leaf");
            (w.char_categories.contains(&Category::Fleet(leaf)), w.account_categories.contains(&Category::Fleet(leaf)))
        };
        assert_eq!(side(b"fleet_watchlistcolors"), (true, false));
        assert_eq!(side(b"listenBroadcast_Target"), (false, true));
        assert_eq!(side(b"fleet_broadcastcolor_Location"), (false, true));
    }
```

In `app/src-tauri/src/presets.rs`'s `tests` module, add:

```rust
    #[test]
    fn a_preset_holding_only_a_watch_list_map_reports_fleet() {
        let map = Value::Tuple(vec![ts(), Value::Dict(vec![])]);
        let char_doc = Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("fleet_watchlistcolors"), map)]))]);
        assert_eq!(derive_aspects(&char_doc, &Value::Dict(vec![]), false), vec![Aspect::Fleet]);
        // And an account side alone is enough too.
        let user_doc = Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("listenBroadcast_Target"), Value::Tuple(vec![ts(), Value::Int(1)]))]))]);
        assert_eq!(derive_aspects(&Value::Dict(vec![]), &user_doc, false), vec![Aspect::Fleet]);
        // The existing fixtures hold no fleet key, so they must not report it.
        assert!(!derive_aspects(&char_doc(), &user_doc(), false).contains(&Aspect::Fleet));
    }
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p settings-model batch && cargo test -p app aspect && cargo test -p app preset`
Expected: compile errors — `Category::Fleet`, `Aspect::Fleet` do not exist.

- [ ] **Step 3: Implement the category**

In `batch.rs`:

Change the import at :10 to `use serde::Serialize;` only if anything else in the file derives it — check with `grep -n "Serialize\|Deserialize" crates/settings-model/src/batch.rs`; if the `Category` derive was the only user, delete the `use serde::…` line entirely (clippy `-D warnings` fails on an unused import).

Change the derive at :17 to `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` (nothing serialises a `Category`: no frontend type, no manifest, no `Serialize` struct carries one — this is what makes a data-carrying variant possible).

Add the variant at the end of the enum (after `HudTargetAlign,`):

```rust
    /// One fleet key. The leaf table is `fleet::FLEET_LEAVES` — the editor's own
    /// key list — so a key added there is a category here with no second list
    /// to keep in step. Every leaf is one level under `ui`, on the side its
    /// `scope` names.
    Fleet(&'static crate::fleet::FleetLeaf),
```

Add the `key_path` arm (after `Category::HudTargetAlign => …`):

```rust
            Category::Fleet(leaf) => &leaf.path,
```

Add `| Category::Fleet(_)` to the `matches!` in `absent_means_default` (after `| Category::HudTargetAlign`), and extend its doc comment with one line: `/// The fleet leaves take the same rule: a copy makes the two characters match key-for-key, which includes removing a target's watch-list map when the source has none.`

- [ ] **Step 4: Implement the aspect**

In `setup.rs`, add `Fleet,` to `Aspect` after `ProbeFormations,`. In `aspect_writes`, add before `Aspect::Everything => unreachable!(…)`:

```rust
            Aspect::Fleet => {
                // Both sides: broadcast settings are per account, the watch
                // list and formation per character. The leaf table is the
                // model's own, so nothing here names a key.
                for leaf in settings_model::FLEET_LEAVES.iter() {
                    match leaf.scope {
                        settings_model::HudScope::Char => char_categories.push(Category::Fleet(leaf)),
                        settings_model::HudScope::Account => account_categories.push(Category::Fleet(leaf)),
                    }
                }
            }
```

In `presets.rs`, in `derive_aspects`, add before `if full {`:

```rust
    let fleet_present = settings_model::FLEET_LEAVES.iter().any(|leaf| {
        let doc = match leaf.scope {
            settings_model::HudScope::Char => char_doc,
            settings_model::HudScope::Account => user_doc,
        };
        has_category(doc, Category::Fleet(leaf))
    });
    if fleet_present {
        out.push(Aspect::Fleet);
    }
```

- [ ] **Step 5: Run the tests and clippy across the workspace**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all green. Two places may need a `match` arm for the new variant — the compiler names them; add `Aspect::Fleet` wherever `Aspect` is matched exhaustively (there is one such match in `aspect_writes`; the `has_category` helper takes any `Category`).

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/batch.rs app/src-tauri/src/setup.rs app/src-tauri/src/presets.rs
git commit -m "batch: a Fleet aspect, one category per fleet leaf

Category::Fleet borrows FLEET_LEAVES so the batch key list is the editor's.
Absence means EVE's default, the HUD-leaf rule.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: The four Tauri commands

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (imports at :26; new functions after `set_hud_field` at :441; tests after `hud_without_a_character_file_is_an_error` at :1921)
- Modify: `app/src-tauri/src/lib.rs` (commands after `set_hud_value` at :505; registration at :692)

**Interfaces:**
- Consumes: `settings_model::{project_fleet, set_fleet_field, set_broadcast_colour, set_watchlist_colour, Fleet, HudScope}`.
- Produces commands `fleet_settings`, `set_fleet_field { name, text }`, `set_fleet_colour { broadcast, rgb }`, `set_watchlist_colour { char_id, rgb }` — each returns `Fleet`.

- [ ] **Step 1: Write the failing tests**

In `ops.rs`'s `tests` module, after `hud_without_a_character_file_is_an_error`:

```rust
    /// root -> { b"ui": {} } — the section present, no fleet keys.
    fn fleet_doc_bytes() -> Vec<u8> {
        let doc = blue_marshal::Value::Dict(vec![(
            blue_marshal::Value::Bytes(b"ui".to_vec()),
            blue_marshal::Value::Dict(vec![]),
        )]);
        blue_marshal::encode(&doc).expect("encode fixture")
    }

    fn roundtrips(state: &AppState, slot: Slot) {
        let guard = state.doc(slot).lock().unwrap();
        let doc = guard.as_ref().expect("open");
        let bytes = blue_marshal::encode(&doc.value).expect("encode");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), doc.value, "reshare ran cleanly");
    }

    #[test]
    fn fleet_needs_at_least_one_file_and_projects_with_only_an_account() {
        let state = AppState::new();
        assert!(fleet_settings(&state).is_err());
        let path = temp_file("fleet-user", &fleet_doc_bytes());
        open_file(&state, Slot::User, &path.to_string_lossy()).expect("open");
        let f = fleet_settings(&state).expect("project");
        assert!(f.user_open && !f.char_open);
        assert_eq!(f.colours.len(), 16);
    }

    #[test]
    fn fleet_field_and_colour_writes_land_in_the_account_file() {
        let state = AppState::new();
        let path = temp_file("fleet-user2", &fleet_doc_bytes());
        open_file(&state, Slot::User, &path.to_string_lossy()).expect("open");

        let f = set_fleet_field(&state, "listen_show_own", "1").expect("set");
        let e = f.fields.iter().find(|e| e.name == "listen_show_own").expect("entry");
        assert_eq!(e.value.as_deref(), Some("1"));

        let f = set_fleet_colour(&state, "Target", Some([0.2, 0.5, 1.0])).expect("set");
        let c = f.colours.iter().find(|c| c.broadcast == "Target").expect("entry");
        assert_eq!(c.state, settings_model::Colour::Set { rgb: [0.2, 0.5, 1.0] });
        let f = set_fleet_colour(&state, "Target", None).expect("clear");
        assert_eq!(f.colours.iter().find(|c| c.broadcast == "Target").unwrap().state, settings_model::Colour::Cleared);
        roundtrips(&state, Slot::User);

        // A character-side field with no character open is a no_document error.
        assert_eq!(set_fleet_field(&state, "formation", "1").unwrap_err().code, "no_document");
    }

    #[test]
    fn watch_list_writes_land_in_the_character_file() {
        let state = AppState::new();
        let path = temp_file("fleet-char", &fleet_doc_bytes());
        open_file(&state, Slot::Char, &path.to_string_lossy()).expect("open");

        let f = set_watchlist_colour(&state, 1001131163, Some([1.0, 0.7, 0.0])).expect("add");
        assert_eq!(f.watchlist.len(), 1);
        assert_eq!(f.watchlist[0].char_id, 1001131163);
        let f = set_watchlist_colour(&state, 1001131163, None).expect("remove");
        assert!(f.watchlist.is_empty());
        roundtrips(&state, Slot::Char);
        assert_eq!(set_fleet_colour(&state, "Target", None).unwrap_err().code, "no_document");
    }
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p app fleet`
Expected: compile errors — the four functions do not exist.

- [ ] **Step 3: Implement the ops**

In `ops.rs`, extend the `settings_model` import at :26 to include `project_fleet, set_broadcast_colour, set_fleet_field as model_set_fleet_field, set_watchlist_colour as model_set_watchlist_colour, Fleet` (aliases because the ops functions take the same names). Add after `set_hud_field`:

```rust
/// Project the fleet settings: both documents optional, an error only when
/// neither is open. Lock user before char, the file's order.
pub fn fleet_settings(state: &AppState) -> Result<Fleet, ErrDto> {
    let uguard = state.user.lock().unwrap();
    let cguard = state.char.lock().unwrap();
    let user = uguard.as_ref().map(|d| &d.value);
    let char_root = cguard.as_ref().map(|d| &d.value);
    if user.is_none() && char_root.is_none() {
        return Err(ErrDto::new("no_document", "no file open"));
    }
    Ok(project_fleet(char_root, user))
}

/// Write one fleet scalar into whichever document its scope names. The
/// projection is the single source of truth for the side, as `set_hud_field`.
pub fn set_fleet_field(state: &AppState, name: &str, text: &str) -> Result<Fleet, ErrDto> {
    let scope = {
        let f = fleet_settings(state)?;
        f.fields
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.scope)
            .ok_or_else(|| ErrDto::new("fleet", format!("unknown field {name:?}")))?
    };
    let slot = match scope {
        HudScope::Char => Slot::Char,
        HudScope::Account => Slot::User,
    };
    // Only a mint de-shares the document — the model's answer IS the reshare decision.
    edit_reshared(
        state,
        slot,
        |v| model_set_fleet_field(v, name, text).map(|minted| ((), minted)),
        |e| coded_err("fleet", e),
    )?;
    fleet_settings(state)
}

/// Set (`Some`) or clear (`None`) one broadcast colour in the account file.
pub fn set_fleet_colour(state: &AppState, broadcast: &str, rgb: Option<[f64; 3]>) -> Result<Fleet, ErrDto> {
    edit_slot(state, Slot::User, |v| set_broadcast_colour(v, broadcast, rgb), |e| coded_err("fleet", e))?;
    fleet_settings(state)
}

/// Set (`Some`) or remove (`None`) one character's watch-list colour in the
/// character file.
pub fn set_watchlist_colour(state: &AppState, char_id: u64, rgb: Option<[f64; 3]>) -> Result<Fleet, ErrDto> {
    edit_slot(state, Slot::Char, |v| model_set_watchlist_colour(v, char_id, rgb), |e| coded_err("fleet", e))?;
    fleet_settings(state)
}
```

In `lib.rs`, after `set_hud_value`:

```rust
#[tauri::command]
fn fleet_settings(state: tauri::State<'_, AppState>) -> Result<settings_model::Fleet, ErrDto> {
    ops::fleet_settings(&state)
}
#[tauri::command]
fn set_fleet_field(
    state: tauri::State<'_, AppState>,
    name: String,
    text: String,
) -> Result<settings_model::Fleet, ErrDto> {
    ops::set_fleet_field(&state, &name, &text)
}
#[tauri::command]
fn set_fleet_colour(
    state: tauri::State<'_, AppState>,
    broadcast: String,
    rgb: Option<[f64; 3]>,
) -> Result<settings_model::Fleet, ErrDto> {
    ops::set_fleet_colour(&state, &broadcast, rgb)
}
#[tauri::command]
fn set_watchlist_colour(
    state: tauri::State<'_, AppState>,
    char_id: u64,
    rgb: Option<[f64; 3]>,
) -> Result<settings_model::Fleet, ErrDto> {
    ops::set_watchlist_colour(&state, char_id, rgb)
}
```

and add `fleet_settings, set_fleet_field, set_fleet_colour, set_watchlist_colour,` to the `generate_handler!` list after `hud_layout, set_hud_value,`.

- [ ] **Step 4: Run the tests and clippy**

Run: `cargo test -p app fleet && cargo clippy -p app --all-targets -- -D warnings`
Expected: pass, clean.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs
git commit -m "fleet: the four commands — project, set a scalar, set a colour, set a watch-list colour

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Character lookup by name or id

**Files:**
- Modify: `app/src-tauri/src/names.rs` (types after `EsiName` at :60; functions after `resolve_blocking` at :216; tests at the end)
- Modify: `app/src-tauri/src/lib.rs` (command after `refresh_character_names` near :155; registration)

**Interfaces:**
- Produces: `names::Found { id: u64, name: String }` (`Serialize`), `names::lookup_blocking(dir, query) -> Result<Option<Found>, FetchError>`, command `lookup_character { query } -> Option<Found>` (rejects with code `esi` on transport failure).

- [ ] **Step 1: Write the failing tests**

Append to `names.rs`'s `tests` module:

```rust
    fn character(name: &str) -> ResolvedName {
        ResolvedName { name: name.into(), category: "character".into() }
    }
    fn no_names(_: &[u64]) -> Result<Vec<EsiName>, FetchError> {
        panic!("the names endpoint must not be asked")
    }
    fn no_ids(_: &str) -> Result<Option<Found>, FetchError> {
        panic!("the ids endpoint must not be asked")
    }

    #[test]
    fn lookup_by_name_hits_the_cache_case_insensitively_without_a_call() {
        let dir = temp_dir("lookup-cache");
        let mut cache = Cache::new();
        cache.insert(96821229, character("Holy Storm"));
        save_cache(&dir, &cache).unwrap();
        let found = lookup_with(&dir, "holy storm", no_names, no_ids).unwrap();
        assert_eq!(found, Some(Found { id: 96821229, name: "Holy Storm".into() }));
    }

    #[test]
    fn lookup_by_name_asks_esi_on_a_miss_and_persists_the_answer() {
        let dir = temp_dir("lookup-miss");
        let found = lookup_with(&dir, "Farm Delay", no_names, |q| {
            assert_eq!(q, "Farm Delay");
            Ok(Some(Found { id: 2117000000, name: "Farm Delay".into() }))
        })
        .unwrap();
        assert_eq!(found.as_ref().map(|f| f.id), Some(2117000000));
        // Both directions are cached from now on.
        assert_eq!(load_cache(&dir).get(&2117000000), Some(&character("Farm Delay")));
        let again = lookup_with(&dir, "farm delay", no_names, no_ids).unwrap();
        assert_eq!(again.map(|f| f.id), Some(2117000000));
    }

    #[test]
    fn lookup_by_name_returns_none_when_esi_knows_nobody() {
        let dir = temp_dir("lookup-none");
        assert_eq!(lookup_with(&dir, "Nobody Here", no_names, |_| Ok(None)).unwrap(), None);
        assert!(load_cache(&dir).is_empty(), "a miss is not cached");
    }

    #[test]
    fn lookup_by_id_uses_the_names_endpoint_and_only_accepts_a_character() {
        let dir = temp_dir("lookup-id");
        let found = lookup_with(&dir, " 96821229 ", |ids| {
            assert_eq!(ids, &[96821229]);
            Ok(vec![EsiName { category: "character".into(), id: 96821229, name: "Holy Storm".into() }])
        }, no_ids)
        .unwrap();
        assert_eq!(found, Some(Found { id: 96821229, name: "Holy Storm".into() }));
        // Cached: the second ask makes no call.
        assert_eq!(lookup_with(&dir, "96821229", no_names, no_ids).unwrap().map(|f| f.name), Some("Holy Storm".into()));
        // A corporation id resolves to a name ESI is happy with, but it is not a character.
        let corp = lookup_with(&dir, "98000001", |_| Ok(vec![EsiName { category: "corporation".into(), id: 98000001, name: "Corp".into() }]), no_ids).unwrap();
        assert_eq!(corp, None);
    }

    #[test]
    fn a_transport_failure_is_an_error_not_a_miss() {
        let dir = temp_dir("lookup-offline");
        assert!(lookup_with(&dir, "Someone", no_names, |_| Err(FetchError("offline".into()))).is_err());
        assert!(lookup_with(&dir, "12345", |_| Err(FetchError("offline".into())), no_ids).is_err());
    }

    #[test]
    fn parse_ids_reads_the_characters_array_and_tolerates_its_absence() {
        let body = br#"{"characters":[{"id":96821229,"name":"Holy Storm"}],"corporations":[{"id":1,"name":"x"}]}"#;
        assert_eq!(parse_ids(body).unwrap(), Some(Found { id: 96821229, name: "Holy Storm".into() }));
        assert_eq!(parse_ids(br#"{}"#).unwrap(), None);
        assert!(parse_ids(b"not json").is_err());
    }
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p app names::tests::lookup`
Expected: compile errors — `Found`, `lookup_with`, `parse_ids` missing.

- [ ] **Step 3: Implement**

In `names.rs`, after the `EsiName` struct:

```rust
/// A character the lookup found — by name or by id — for the watch list's
/// "Add a character" row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Found {
    pub id: u64,
    pub name: String,
}

/// The `characters` half of an ESI `/universe/ids` body. ESI omits a category
/// with no hits, so the field defaults.
#[derive(Debug, Deserialize)]
struct EsiIds {
    #[serde(default)]
    characters: Vec<EsiIdEntry>,
}

#[derive(Debug, Deserialize)]
struct EsiIdEntry {
    id: u64,
    name: String,
}

const ESI_IDS_URL: &str = "https://esi.evetech.net/latest/universe/ids/";

fn parse_ids(bytes: &[u8]) -> Result<Option<Found>, FetchError> {
    let ids: EsiIds = serde_json::from_slice(bytes).map_err(|e| FetchError(e.to_string()))?;
    Ok(ids.characters.into_iter().next().map(|e| Found { id: e.id, name: e.name }))
}

/// One POST of a single name to ESI `/universe/ids`. An exact, case-insensitive
/// match on ESI's side; `Ok(None)` when no character has that name.
fn post_ids(client: &reqwest::blocking::Client, name: &str) -> Result<Option<Found>, FetchError> {
    let resp = client
        .post(ESI_IDS_URL)
        .header(reqwest::header::USER_AGENT, "eve-settings-editor")
        .json(&[name])
        .send()
        .map_err(|e| FetchError(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(FetchError(format!("ESI status {}", resp.status())));
    }
    let bytes = resp.bytes().map_err(|e| FetchError(e.to_string()))?;
    parse_ids(&bytes)
}
```

After `resolve_blocking`:

```rust
/// Find a character by name or id, cache first. A numeric query goes through
/// the names endpoint (cache, then `fetch_names`) and counts only if ESI calls
/// it a character; a name is looked up in the cache case-insensitively, then
/// through `fetch_ids`, and a hit is written into the same cache so both
/// directions are answered without a call from then on. `Err` is a transport
/// failure — distinct from `Ok(None)`, "no such character" — so the UI can say
/// which. Both fetchers are injected so this unit-tests without the network.
fn lookup_with<N, I>(dir: &Path, query: &str, fetch_names: N, fetch_ids: I) -> Result<Option<Found>, FetchError>
where
    N: FnOnce(&[u64]) -> Result<Vec<EsiName>, FetchError>,
    I: FnOnce(&str) -> Result<Option<Found>, FetchError>,
{
    let q = query.trim();
    let mut cache = load_cache(dir);
    let as_found = |id: u64, n: &ResolvedName| {
        (n.category == "character").then(|| Found { id, name: n.name.clone() })
    };

    if let Ok(id) = q.parse::<u64>() {
        if let Some(n) = cache.get(&id) {
            return Ok(as_found(id, n));
        }
        let fetched = fetch_names(&[id])?;
        apply_fetch(&mut cache, fetched);
        let _ = save_cache(dir, &cache);
        return Ok(cache.get(&id).and_then(|n| as_found(id, n)));
    }

    if let Some((id, n)) = cache.iter().find(|(_, n)| n.category == "character" && n.name.eq_ignore_ascii_case(q)) {
        return Ok(Some(Found { id: *id, name: n.name.clone() }));
    }
    let hit = fetch_ids(q)?;
    if let Some(f) = &hit {
        cache.insert(f.id, ResolvedName { name: f.name.clone(), category: "character".into() });
        let _ = save_cache(dir, &cache);
    }
    Ok(hit)
}

/// Production wiring: `lookup_with` over the real ESI client. Blocking — call
/// it from a worker thread.
pub fn lookup_blocking(dir: &Path, query: &str) -> Result<Option<Found>, FetchError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| FetchError(e.to_string()))?;
    lookup_with(dir, query, esi_fetch, |q| post_ids(&client, q))
}
```

In `lib.rs`, after `refresh_character_names`:

```rust
#[tauri::command]
async fn lookup_character(app: tauri::AppHandle, query: String) -> Result<Option<names::Found>, ErrDto> {
    let dir = app_dir(&app);
    tauri::async_runtime::spawn_blocking(move || names::lookup_blocking(&dir, &query))
        .await
        .map_err(|e| ErrDto::new("esi", e.to_string()))?
        .map_err(|e| ErrDto::new("esi", e.0))
}
```

and register `lookup_character` after `refresh_character_names` in `generate_handler!`. `ErrDto::new` is `pub(crate)` (`ops.rs:92`) — reachable from `lib.rs`.

- [ ] **Step 4: Run the tests and clippy**

Run: `cargo test -p app names && cargo clippy -p app --all-targets -- -D warnings`
Expected: pass, clean.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/names.rs app/src-tauri/src/lib.rs
git commit -m "names: look a character up by name or id, through the same cache

A name miss asks ESI /universe/ids and writes the hit into names-cache.json,
so the id→name and name→id directions are both cached from then on.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Frontend contracts — `api.ts`, `colour.ts`, `fleet.ts`

**Files:**
- Modify: `app/src/lib/api.ts` (types after `Hud` at :171; `Aspect` at :408; api entries after `setHudValue` at :483)
- Create: `app/src/lib/colour.ts`, `app/src/lib/colour.test.ts`, `app/src/lib/fleet.ts`, `app/src/lib/fleet.test.ts`
- Modify: `app/src/lib/OverviewAppearanceTab.svelte:118-128` (`setColor` uses `snapToPalette`)

**Interfaces:**
- Produces:
  - `api.ts`: `Rgb`, `Colour`, `ColourEntry`, `WatchEntry`, `Fleet`, `FoundCharacter`; `api.fleet()`, `api.setFleetField(name, text)`, `api.setFleetColour(broadcast, rgb | null)`, `api.setWatchlistColour(charId, rgb | null)`, `api.lookupCharacter(query)`; `Aspect` gains `"fleet"`.
  - `colour.ts`: `rgbToHex(rgb: Rgb): string`, `hexToRgb(hex: string): Rgb`, `snapToPalette<C extends number[]>(hex, palette: [string, C][]): C | undefined`, `UNSET_HEX`.
  - `fleet.ts`: `BROADCASTS: { type: string; label: string }[]` (sixteen, EVE's order), `SHOW_OWN = { field: "listen_show_own", label: "Always show my own broadcasts" }`, `listenField(type)`.

- [ ] **Step 1: Write the failing pure-module tests**

Create `app/src/lib/colour.test.ts`:

```ts
// Pure-module tests: plain data in, plain data out, no DOM. See test/README.md.
import { check, eq } from "./test/check.ts";
import { hexToRgb, rgbToHex, snapToPalette } from "./colour.ts";

check("floats to hex", rgbToHex([0.75, 0, 0]) === "#bf0000");
check("hex to floats", eq(hexToRgb("#010203"), [1 / 255, 2 / 255, 3 / 255]));

const PALETTE: [string, [number, number, number]][] = [
  ["red", [0.75, 0.0, 0.0]],
  ["blue", [0.2, 0.5, 1.0]],
];
// #bf0000 inverts to 0.74901…, which is not the 0.75 EVE stores; the palette's
// own floats are what the file must get.
check("a palette hex snaps to the exact floats", eq(snapToPalette("#bf0000", PALETTE), [0.75, 0.0, 0.0]));
check("an off-palette hex does not snap", snapToPalette("#010203", PALETTE) === undefined);
const RGBA: [string, [number, number, number, number]][] = [["red", [0.75, 0.0, 0.0, 1.0]]];
check("four-float palettes snap too", eq(snapToPalette("#bf0000", RGBA), [0.75, 0.0, 0.0, 1.0]));
```

Create `app/src/lib/fleet.test.ts`:

```ts
// Pure-module tests. See test/README.md.
import { check } from "./test/check.ts";
import { BROADCASTS, SHOW_OWN, listenField } from "./fleet.ts";

check("sixteen types in EVE's row order", BROADCASTS.length === 16 && BROADCASTS[0].type === "HealArmor" && BROADCASTS[15].type === "Location");
check("a listen field is the type token verbatim", listenField("HealArmor") === "listen_HealArmor");
check("the top checkbox has its own field", SHOW_OWN.field === "listen_show_own");
// R1: sentence case — one leading capital, nothing capitalised after it.
check(
  "labels are sentence case",
  [SHOW_OWN.label, ...BROADCASTS.map((b) => b.label)].every(
    (l) => l[0] === l[0].toUpperCase() && l.slice(1).split(" ").every((w) => w === "" || w[0] === w[0].toLowerCase()),
  ),
);
check("no label says Broadcast:", BROADCASTS.every((b) => !b.label.startsWith("Broadcast")));
```

- [ ] **Step 2: Run them to see them fail**

Run (from `app/`): `npx vitest run colour fleet.test`
Expected: FAIL — the modules do not exist.

- [ ] **Step 3: Write `colour.ts`**

```ts
// EVE stores colours as 0..1 floats; <input type="color"> speaks #rrggbb.
// The three-float helpers live here beside the one rule both colour editors
// share: a hex inverts to n/255 — #bf0000 gives 0.74901…, not the 0.75 EVE
// stores for red — so picking a palette colour off a swatch would write
// something the client never wrote. When the hex IS a palette colour's hex,
// write that entry's exact floats instead. Nothing visible changes: both
// render as the same #rrggbb.
import { hexToRgba, rgbaToHex } from "./states";

export type Rgb = [number, number, number];

/** The swatch shown for "no colour" — a mid grey, dimmed by the caller. The
 *  name whitelists this line in the hex guard (`ui/tokens.test.ts`). */
export const UNSET_HEX = "#808080";

export function rgbToHex(rgb: Rgb): string {
  return rgbaToHex([rgb[0], rgb[1], rgb[2], 1]);
}

export function hexToRgb(hex: string): Rgb {
  const [r, g, b] = hexToRgba(hex, 1);
  return [r, g, b];
}

/** The palette entry whose hex is `hex`, as its exact floats, or undefined. */
export function snapToPalette<C extends number[]>(hex: string, palette: [string, C][]): C | undefined {
  return palette.find(([, c]) => rgbaToHex([c[0], c[1], c[2], 1]) === hex)?.[1];
}
```

- [ ] **Step 4: Write `fleet.ts`**

```ts
// EVE's Broadcast Settings dialog, row by row: the internal type token (the
// suffix of `listenBroadcast_<Type>` and `fleet_broadcastcolor_<Type>`) and
// the label, sentence-cased per the copy standard. "Broadcast:" is dropped —
// every row bar one would carry it. The order is EVE's (spec §2.2).
export const BROADCASTS: { type: string; label: string }[] = [
  { type: "HealArmor", label: "Need armor" },
  { type: "HealCapacitor", label: "Need capacitor" },
  { type: "NeedBackup", label: "Need backup" },
  { type: "Target", label: "Target" },
  { type: "HealShield", label: "Need shield" },
  { type: "WarpTo", label: "Warp to" },
  { type: "TravelTo", label: "Travel to" },
  { type: "Event", label: "Fleet event" },
  { type: "JumpTo", label: "Jump to" },
  { type: "AlignTo", label: "Align to" },
  { type: "HealTarget", label: "Repair target" },
  { type: "InPosition", label: "In position at" },
  { type: "EnemySpotted", label: "Spotted an enemy" },
  { type: "HoldPosition", label: "Request that the fleet hold position" },
  { type: "JumpBeacon", label: "Jump to beacon" },
  { type: "Location", label: "At location" },
];

/** The dialog's top checkbox. One field; the backend writes its two keys. */
export const SHOW_OWN = { field: "listen_show_own", label: "Always show my own broadcasts" };

/** The projection's field name for a type's checkbox. */
export const listenField = (type: string): string => `listen_${type}`;
```

- [ ] **Step 5: Add the API types and calls**

In `api.ts`, after the `Hud` interface (:171):

```ts
export type Rgb = [number, number, number];
/** A broadcast colour's three wire states, plus the one the backend refuses to
 *  touch. `absent` shows the type's default; `cleared` is EVE's ✕. */
export type Colour =
  | { state: "absent" }
  | { state: "cleared" }
  | { state: "set"; rgb: Rgb }
  | { state: "unreadable" };
export interface ColourEntry {
  broadcast: string;
  state: Colour;
  /** What EVE shows when no key is stored — a colour for four types, none for the rest. */
  default: Rgb | null;
}
export interface WatchEntry {
  char_id: number;
  /** null when the stored value is not three numbers — shown as unreadable, removable only. */
  rgb: Rgb | null;
}
export interface Fleet {
  /** The scalars, in HudEntry's shape: `listen_<Type>`, `listen_show_own`,
   *  `formation`, `formation_size`, `formation_spacing`, `finder_group_only`. */
  fields: HudEntry[];
  /** Sixteen entries, EVE's row order. */
  colours: ColourEntry[];
  watchlist: WatchEntry[];
  /** EVE's nine swatches, `[name, rgb]`. */
  palette: [string, Rgb][];
  char_open: boolean;
  user_open: boolean;
}
export interface FoundCharacter {
  id: number;
  name: string;
}
```

Change `Aspect` (:408) to:

```ts
export type Aspect = "layout" | "overview" | "autofill" | "keybinds" | "probe_formations" | "fleet" | "everything";
```

After `setHudValue` (:483) add:

```ts
  fleet: () => invoke<Fleet>("fleet_settings"),
  setFleetField: (name: string, text: string) =>
    invoke<Fleet>("set_fleet_field", { name, text }),
  setFleetColour: (broadcast: string, rgb: Rgb | null) =>
    invoke<Fleet>("set_fleet_colour", { broadcast, rgb }),
  setWatchlistColour: (charId: number, rgb: Rgb | null) =>
    invoke<Fleet>("set_watchlist_colour", { charId, rgb }),
  /** `null` when ESI knows no character by that name or id; rejects with code
   *  `esi` when ESI could not be reached — the two read differently to the user. */
  lookupCharacter: (query: string) =>
    invoke<FoundCharacter | null>("lookup_character", { query }),
```

- [ ] **Step 6: Use `snapToPalette` in the appearance tab**

In `OverviewAppearanceTab.svelte`, add `import { snapToPalette } from "./colour";` beside the other imports, and replace the body of `setColor` (:118-128) with:

```ts
  function setColor(id: number, hex: string) {
    const alpha = colors.get(id)?.[3] ?? 1;
    // See colour.ts for why a palette hex must write the palette's exact floats.
    const exact = snapToPalette(hex, palette);
    const rgba: Rgba = exact ? [exact[0], exact[1], exact[2], alpha] : hexToRgba(hex, alpha);
    return edit(() => api.overviewSetStateColor(isBg ? "background" : "flag", id, rgba));
  }
```

- [ ] **Step 7: Run the pure tests, the IPC contract, the appearance spec, and svelte-check**

Run (from `app/`): `npx vitest run colour fleet.test ipc OverviewAppearanceTab && npm run check`
Expected: all pass — `ipc.test.ts` now sees five new commands on both sides with matching argument names (`charId` ↔ `char_id`); the appearance tab's colour tests still pass; check clean. If `ipc.test.ts` complains that `lookup_character` is unreachable or an argument mismatches, the command name or `{ query }` key is misspelled on one side.

- [ ] **Step 8: Commit**

```bash
git add app/src/lib/api.ts app/src/lib/colour.ts app/src/lib/colour.test.ts app/src/lib/fleet.ts app/src/lib/fleet.test.ts app/src/lib/OverviewAppearanceTab.svelte
git commit -m "frontend: fleet API types and calls, the shared palette snap, EVE's broadcast rows

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: The Fleet tab in the shell, and the Fleet aspect in copy and presets

**Files:**
- Modify: `app/src/lib/views.ts:14,26-33`, `app/src/lib/keymap.ts:81-82`, `app/src/lib/aspects.ts`, `app/src/lib/presetLibrary.svelte.ts:12-19`, `app/src/lib/BatchView.svelte:105`
- Modify specs: `app/src/lib/ViewTabs.spec.ts:33`, `app/src/lib/keymap.spec.ts:47,53-54`, `app/src/lib/commands.spec.ts:33`, `app/src/lib/presetLibrary.test.ts`

**Interfaces:**
- Produces: `View` includes `"fleet"`; tab strip order `Layout · Overview · Autofill · Keybinds · Probes · Fleet · Raw`; chord `6` → `go.fleet`, `7` → `go.raw`; `ASPECT_LABELS` and `LABELS` carry `fleet`.

- [ ] **Step 1: Update the specs first (they are the failing tests)**

`ViewTabs.spec.ts:33`:

```ts
    expect(names()).toEqual(["Layout", "Overview", "Autofill", "Keybinds", "Probes", "Fleet", "Raw"]);
```

`keymap.spec.ts`: change the table row `["6", "go.raw"]` to `["7", "go.raw"]` and add `["6", "go.fleet"]`; change the digits test to:

```ts
  test("the view digits are the tab strip's own order", () => {
    const ids = ["1", "2", "3", "4", "5", "6", "7"].map((k) => commandFor(chord(k))!.id);
    expect(ids).toEqual(["go.layout", "go.overview", "go.autofill", "go.keybinds", "go.probes", "go.fleet", "go.raw"]);
  });
```

`commands.spec.ts:33`: add `"Fleet"` to the `PROPER` set (it is a tab name, capitalised in "Go to Fleet" like the others).

`presetLibrary.test.ts`: add after the probe formations line:

```ts
check("fleet label", aspectLabel("fleet") === "Fleet");
```

- [ ] **Step 2: Run them to see them fail**

Run (from `app/`): `npx vitest run ViewTabs keymap commands presetLibrary`
Expected: FAIL on the four changed assertions (and a TypeScript complaint that `"fleet"` is not an `Aspect`/`View` until Step 3).

- [ ] **Step 3: Implement**

`views.ts`:

```ts
export type View = "layout" | "overview" | "autofill" | "keybinds" | "probes" | "fleet" | "raw";
```

and in `VIEWS`, insert `{ id: "fleet", label: "Fleet" },` between Probes and Raw. `viewAvailable` needs no change: Fleet is the shared "open a character or an account file" rule, which is the function's fall-through.

`keymap.ts` CHORDS: `"6": "go.fleet", "7": "go.raw",`.

`aspects.ts`: insert before the `everything` entry:

```ts
  { key: "fleet", label: "Fleet (broadcast settings, watch-list colours, formation)" },
```

`presetLibrary.svelte.ts` `LABELS`: add `fleet: "Fleet",` before `everything`.

`BatchView.svelte:105`: the character-source list becomes `["layout", "overview", "autofill", "keybinds", "probe_formations", "fleet", "everything"]`.

- [ ] **Step 4: Run the shell specs and svelte-check**

Run (from `app/`): `npx vitest run ViewTabs keymap commands presetLibrary BatchView PresetGroup page && npm run check`
Expected: all pass, check clean. `commands.spec.ts`'s "no accelerator is bound twice" and "every accelerator is in this platform's form" pass because `goCommand` derives the digit from `VIEWS`' index.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/views.ts app/src/lib/keymap.ts app/src/lib/aspects.ts app/src/lib/presetLibrary.svelte.ts app/src/lib/BatchView.svelte app/src/lib/ViewTabs.spec.ts app/src/lib/keymap.spec.ts app/src/lib/commands.spec.ts app/src/lib/presetLibrary.test.ts
git commit -m "shell: a Fleet tab before Raw, and a Fleet aspect in copy and presets

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: `FleetView.svelte` — the Broadcast settings panel

**Files:**
- Create: `app/src/lib/FleetView.svelte`, `app/src/lib/FleetView.spec.ts`

**Interfaces:**
- Consumes: Task 9's `api.fleet`, `api.setFleetField`, `api.setFleetColour`, `BROADCASTS`, `SHOW_OWN`, `listenField`, `colour.ts`; primitives `Panel`, `PanelHeader`, `Chip`, `Field`, `Button`, `EmptyState`, `InlineMessage`.
- Produces: `FleetView` props `{ charOpen, userOpen, userId?, charId?, refreshToken?, onUserDirty, onCharDirty, onShowAccounts? }`. Tasks 12–13 add the other two panels to this same file.

- [ ] **Step 1: Write the failing spec**

Create `app/src/lib/FleetView.spec.ts`:

```ts
// Component test: run with `npm run test:ui` (vitest + jsdom).
import { describe, expect, test } from "vitest";
import { render, fireEvent, screen, waitFor, within } from "@testing-library/svelte";
import FleetView from "$lib/FleetView.svelte";
import { calls } from "$lib/test/setup";
import { toasts } from "$lib/ui/toasts.svelte";
import type { ColourEntry, Fleet, HudEntry } from "$lib/api";
import { BROADCASTS } from "$lib/fleet";

const noop = () => {};

const field = (name: string, value: string | null, scope: "char" | "account" = "account", def = "1"): HudEntry => ({
  name, kind: "int", value, default: def, scope,
  set: value === null ? { how: "insert", parent: [], key: { kind: "int", text: "0" } as never } : { how: "set", path: [] },
});

export const FLEET: Fleet = {
  fields: [
    field("listen_show_own", "1", "account", "0"),
    field("listen_HealArmor", "0", "account", "0"),
    field("listen_Target", "1"),
    field("listen_WarpTo", null),
    field("formation", "0", "char", "0"),
    field("formation_size", "20000", "char", "20000"),
    field("formation_spacing", null, "char", "2000"),
    field("finder_group_only", "1", "char"),
  ],
  // Every type, absent with no default, then the five states under test.
  colours: BROADCASTS.map((b): ColourEntry => ({ broadcast: b.type, state: { state: "absent" }, default: null })).map((c) =>
    ({
      HealArmor: { ...c, state: { state: "set", rgb: [0.1, 0.6, 0.1] }, default: [0.1, 0.6, 0.1] },
      Target: { ...c, default: [0.75, 0.0, 0.0] },
      Location: { ...c, state: { state: "cleared" } },
      JumpTo: { ...c, state: { state: "unreadable" } },
    } as Record<string, ColourEntry>)[c.broadcast] ?? c,
  ),
  watchlist: [
    { char_id: 1001131163, rgb: [0.2, 0.5, 1.0] },
    { char_id: 90000001, rgb: null },
  ],
  palette: [
    ["yellow", [1.0, 0.7, 0.0]], ["red", [0.75, 0.0, 0.0]], ["blue", [0.2, 0.5, 1.0]],
  ],
  char_open: true,
  user_open: true,
};

export function mount(over: Partial<Fleet> = {}, props: Partial<{ charOpen: boolean; userOpen: boolean }> = {}) {
  const fleet = { ...FLEET, ...over };
  calls.stub("fleet_settings", fleet);
  calls.stub("set_fleet_field", fleet);
  calls.stub("set_fleet_colour", fleet);
  calls.stub("set_watchlist_colour", fleet);
  return render(FleetView, {
    charOpen: true, userOpen: true, userId: 1, charId: 2,
    onUserDirty: noop, onCharDirty: noop, ...props,
  });
}

const panel = async (title: string) => (await screen.findByRole("heading", { name: title })).closest("section")!;

describe("the broadcast panel", () => {
  test("renders the top checkbox and every type in EVE's order, checked from the file", async () => {
    mount();
    const p = await panel("Broadcast settings");
    const boxes = within(p).getAllByRole("checkbox") as HTMLInputElement[];
    expect(boxes.length).toBe(17);
    expect(within(p).getByLabelText("Always show my own broadcasts")).toBeTruthy();
    expect((within(p).getByLabelText("Need armor") as HTMLInputElement).checked).toBe(false);
    expect((within(p).getByLabelText("Target") as HTMLInputElement).checked).toBe(true);
    // Never written: the default (1) is what shows.
    expect((within(p).getByLabelText("Warp to") as HTMLInputElement).checked).toBe(true);
    expect(within(p).getByText("account file")).toBeTruthy();
  });

  test("a toggle writes 1 or 0 to the type's field and marks the account dirty", async () => {
    let dirty = 0;
    calls.stub("fleet_settings", FLEET);
    calls.stub("set_fleet_field", FLEET);
    render(FleetView, { charOpen: true, userOpen: true, onUserDirty: () => dirty++, onCharDirty: noop });
    const p = await panel("Broadcast settings");
    await fireEvent.click(within(p).getByLabelText("Need armor"));
    await waitFor(() => expect(calls.of("set_fleet_field").length).toBe(1));
    expect(calls.only("set_fleet_field").args).toEqual({ name: "listen_HealArmor", text: "1" });
    await waitFor(() => expect(dirty).toBe(1));
  });

  test("the top checkbox writes its own field", async () => {
    mount();
    const p = await panel("Broadcast settings");
    await fireEvent.click(within(p).getByLabelText("Always show my own broadcasts"));
    await waitFor(() => expect(calls.only("set_fleet_field").args).toEqual({ name: "listen_show_own", text: "0" }));
  });

  test("swatches show the stored colour, the default for an absent one, and a picked palette hex writes exact floats", async () => {
    mount();
    const p = await panel("Broadcast settings");
    const armor = within(p).getByLabelText("Colour for Need armor") as HTMLInputElement;
    expect(armor.value).toBe("#1a991a"); // 0.1 * 255 rounds to 26
    const target = within(p).getByLabelText("Colour for Target") as HTMLInputElement;
    expect(target.value).toBe("#bf0000");
    await fireEvent.change(target, { target: { value: "#3380ff" } });
    await waitFor(() => expect(calls.of("set_fleet_colour").length).toBe(1));
    expect(calls.only("set_fleet_colour").args).toEqual({ broadcast: "Target", rgb: [0.2, 0.5, 1.0] });
  });

  test("✕ writes null, and is disabled once there is no colour", async () => {
    mount();
    const p = await panel("Broadcast settings");
    const clearOn = (label: string) =>
      within(within(p).getByLabelText(label).closest(".row") as HTMLElement).getByTitle("No colour") as HTMLButtonElement;
    // Set, and absent-with-a-default, can still be cleared; cleared, and
    // absent-with-no-default, cannot — there is nothing left for ✕ to do.
    expect(clearOn("Need armor").disabled).toBe(false);
    expect(clearOn("Target").disabled).toBe(false);
    expect(clearOn("At location").disabled).toBe(true);
    expect(clearOn("Warp to").disabled).toBe(true);
    expect(clearOn("Warp to").title).toBe("Already no colour");
    await fireEvent.click(clearOn("Need armor"));
    await waitFor(() => expect(calls.of("set_fleet_colour").length).toBe(1));
    expect(calls.only("set_fleet_colour").args).toEqual({ broadcast: "HealArmor", rgb: null });
  });

  test("an unreadable colour disables its swatch with a reason", async () => {
    mount();
    const p = await panel("Broadcast settings");
    const jump = within(p).getByLabelText("Colour for Jump to") as HTMLInputElement;
    expect(jump.disabled).toBe(true);
    expect(jump.title).toMatch(/unexpected type/);
  });

  test("a refused write reports on the panel in the error grammar", async () => {
    mount();
    calls.stub("set_fleet_field", () => Promise.reject({ code: "not_editable", message: "This value has an unexpected type here and cannot be edited safely." }));
    const p = await panel("Broadcast settings");
    await fireEvent.click(within(p).getByLabelText("Need armor"));
    const msg = await within(p).findByRole("alert");
    expect(msg.textContent).toMatch(/^That broadcast setting wasn't changed — This value has an unexpected type/);
    expect(msg.textContent).not.toMatch(/\[not_editable\]/);
  });

  test("no account file: an empty state with the pairing action", async () => {
    let shown = 0;
    calls.stub("fleet_settings", { ...FLEET, user_open: false, colours: [], fields: FLEET.fields.filter((f) => f.scope === "char") });
    render(FleetView, { charOpen: true, userOpen: false, onUserDirty: noop, onCharDirty: noop, onShowAccounts: () => shown++ });
    const p = await panel("Broadcast settings");
    expect(within(p).getByText("No account paired")).toBeTruthy();
    await fireEvent.click(within(p).getByRole("button", { name: "Pair this character…" }));
    expect(shown).toBe(1);
  });

  test("reloads when refreshToken changes", async () => {
    const { rerender } = mount();
    await panel("Broadcast settings");
    await waitFor(() => expect(calls.of("fleet_settings").length).toBe(1));
    await rerender({ charOpen: true, userOpen: true, userId: 1, charId: 2, refreshToken: 1, onUserDirty: noop, onCharDirty: noop });
    await waitFor(() => expect(calls.of("fleet_settings").length).toBe(2));
  });
});
```

(`FLEET` and `mount` are exported so Tasks 12–13 can append to this file and reuse them; keep them in this file.)

- [ ] **Step 2: Run the spec to see it fail**

Run (from `app/`): `npx vitest run FleetView`
Expected: FAIL — `FleetView.svelte` does not exist.

- [ ] **Step 3: Write the component**

Create `app/src/lib/FleetView.svelte`:

```svelte
<script lang="ts">
  import { api, errMessage, errText, type Fleet, type HudEntry, type Rgb } from "./api";
  import { BROADCASTS, SHOW_OWN, listenField } from "./fleet";
  import { hexToRgb, rgbToHex, snapToPalette, UNSET_HEX } from "./colour";
  import { resolveNames } from "./names.svelte";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import Panel from "./ui/Panel.svelte";
  import PanelHeader from "./ui/PanelHeader.svelte";

  let {
    charOpen, userOpen, userId = null, charId = null, refreshToken = 0,
    onUserDirty, onCharDirty, onShowAccounts = () => {},
  }: {
    charOpen: boolean;
    userOpen: boolean;
    userId?: number | null;
    charId?: number | null;
    /** Bumped by every save, open, discard, backup restore and undo — the only
     *  signal that the documents changed underneath this view. */
    refreshToken?: number;
    onUserDirty: () => void;
    onCharDirty: () => void;
    onShowAccounts?: () => void;
  } = $props();

  let fleet = $state<Fleet | null>(null);
  let loadError = $state<string | null>(null);
  // One live message per panel — the panel is the control group that owns the
  // failure, and each handler nulls its own before it starts (05 §3.1).
  type Msg = { text: string; detail: string };
  type PanelId = "broadcast" | "watch" | "formation";
  let broadcastError = $state<Msg | null>(null);
  let watchError = $state<Msg | null>(null);
  let formationError = $state<Msg | null>(null);
  function setError(panel: PanelId, m: Msg | null) {
    if (panel === "broadcast") broadcastError = m;
    else if (panel === "watch") watchError = m;
    else formationError = m;
  }

  async function reload() {
    if (!charOpen && !userOpen) { fleet = null; return; }
    loadError = null;
    try {
      fleet = await api.fleet();
      void resolveNames(fleet.watchlist.map((w) => w.char_id));
    } catch (e) { loadError = errMessage(e); }
  }
  $effect(() => { void charOpen; void userOpen; void userId; void charId; void refreshToken; reload(); });

  const palette = $derived(fleet?.palette ?? []);
  const field = (name: string): HudEntry | undefined => fleet?.fields.find((f) => f.name === name);
  /** The stored value, or EVE's default when the key is absent. */
  const shown = (name: string): string => { const e = field(name); return e?.value ?? e?.default ?? ""; };
  const unavailable = (name: string): boolean => field(name)?.set.how === "unavailable";
  const NOT_EDITABLE = "This value has an unexpected type here";

  const colourEntry = (broadcast: string) => fleet?.colours.find((c) => c.broadcast === broadcast);
  /** What the swatch shows: the stored colour, else the type's default, else nothing. */
  function swatchRgb(broadcast: string): Rgb | null {
    const c = colourEntry(broadcast);
    if (!c) return null;
    if (c.state.state === "set") return c.state.rgb;
    if (c.state.state === "absent") return c.default;
    return null;
  }
  /** Whether ✕ has anything left to do. */
  function noColour(broadcast: string): boolean {
    const c = colourEntry(broadcast);
    return !c || c.state.state === "cleared" || (c.state.state === "absent" && c.default === null);
  }

  /** Run one write, replace the projection, mark the right file dirty; on
   *  failure put the R4 sentence on the panel that owns the control. */
  async function write(panel: PanelId, subject: string, fn: () => Promise<Fleet>, dirty: () => void): Promise<boolean> {
    setError(panel, null);
    try {
      fleet = await fn();
      dirty();
      return true;
    } catch (e) {
      setError(panel, { text: `${subject} — ${errText(e)}`, detail: errMessage(e) });
      return false;
    }
  }

  const setBroadcastField = (name: string, on: boolean) =>
    write("broadcast", "That broadcast setting wasn't changed", () => api.setFleetField(name, on ? "1" : "0"), onUserDirty);
  const pickColour = (broadcast: string, hex: string) =>
    write("broadcast", "That colour wasn't changed", () => api.setFleetColour(broadcast, snapToPalette(hex, palette) ?? hexToRgb(hex)), onUserDirty);
  const clearColour = (broadcast: string) =>
    write("broadcast", "That colour wasn't changed", () => api.setFleetColour(broadcast, null), onUserDirty);

  const checked = (e: Event) => (e.currentTarget as HTMLInputElement).checked;
  const picked = (e: Event) => (e.currentTarget as HTMLInputElement).value;
</script>

<div class="fleet">
  {#if loadError}
    <InlineMessage variant="error">{loadError}</InlineMessage>
  {/if}
  <!-- The palette as suggestions in the native colour picker: a hint, not a
       constraint. A picked palette colour is snapped to EVE's exact floats. -->
  <datalist id="fleet-palette">
    {#each palette as [name, c] (name)}<option value={rgbToHex(c)}></option>{/each}
  </datalist>

  <Panel class="broadcasts">
    <PanelHeader title="Broadcast settings" subtitle="Which broadcasts you receive, and the colour each shows in">
      {#snippet actions()}<Chip size="sm">account file</Chip>{/snippet}
    </PanelHeader>
    {#if !userOpen}
      <EmptyState title="No account paired" description="Broadcast settings live in the account file.">
        {#snippet action()}<Button onclick={onShowAccounts}>Pair this character…</Button>{/snippet}
      </EmptyState>
    {:else if fleet}
      {#if broadcastError}
        <InlineMessage variant="error" detail={broadcastError.detail}>{broadcastError.text}</InlineMessage>
      {/if}
      <div class="rows">
        <!-- EVE's own layout: checkbox, label, then the swatch. Field's label
             wraps the checkbox, so the whole caption is the hit target. -->
        <div class="row">
          <Field kind="checkbox" label={SHOW_OWN.label} value={shown(SHOW_OWN.field) === "1"}
            disabled={unavailable(SHOW_OWN.field)} disabledReason={NOT_EDITABLE}
            onchange={(e) => setBroadcastField(SHOW_OWN.field, checked(e))} />
        </div>
        {#each BROADCASTS as b (b.type)}
          {@const name = listenField(b.type)}
          {@const rgb = swatchRgb(b.type)}
          {@const unreadable = colourEntry(b.type)?.state.state === "unreadable"}
          <div class="row">
            <Field kind="checkbox" label={b.label} value={shown(name) === "1"}
              disabled={unavailable(name)} disabledReason={NOT_EDITABLE}
              onchange={(e) => setBroadcastField(name, checked(e))} />
            <span class="colour">
              <!-- An unset swatch shows a placeholder and takes the one disabled
                   treatment, so "no colour" and "black" cannot be confused —
                   the appearance tab's convention. -->
              <Field kind="color" list="fleet-palette" controlClass={rgb ? "" : "unset"}
                value={rgb ? rgbToHex(rgb) : UNSET_HEX}
                ariaLabel="Colour for {b.label}" title={rgb ? undefined : "No colour"}
                disabled={unreadable} disabledReason={NOT_EDITABLE}
                onchange={(e) => pickColour(b.type, picked(e))} />
              <Button variant="ghost" size="sm" iconOnly title="No colour"
                disabled={noColour(b.type)} disabledReason="Already no colour"
                onclick={() => clearColour(b.type)}>✕</Button>
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </Panel>
</div>

<style>
  .fleet { display: flex; flex-direction: column; gap: var(--s4); max-width: 56rem; }
  .rows { display: flex; flex-direction: column; }
  .row { display: flex; align-items: center; gap: var(--s2); padding: var(--s1) 0; }
  .row :global(.box) { flex: 1; }
  .colour { display: flex; align-items: center; gap: var(--s1); }
  /* The one sanctioned opacity: a placeholder swatch is not content. */
  .colour :global(.unset) { opacity: var(--o-disabled); }
</style>
```

`Field`'s checkbox-with-label renders `<label class="box">` — `.row :global(.box)` is that. `Field` passes unknown props (`list`, `title`) through `...rest` onto the control (`ui/Field.svelte:39,181-184`), which is how the appearance tab sets `list="eve-palette"` today. Task 12 adds its own imports (`ListRow`, `toast`, `undoAction`, `names`, `FoundCharacter`) when it uses them.

- [ ] **Step 4: Run the spec and svelte-check**

Run (from `app/`): `npx vitest run FleetView && npm run check`
Expected: the broadcast-panel tests pass; check clean; `tokens.test.ts` passes (no literal in the `.svelte`; `UNSET_HEX` lives in `colour.ts`).

Run: `npx vitest run tokens`
Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/FleetView.svelte app/src/lib/FleetView.spec.ts
git commit -m "Fleet view: the Broadcast settings panel

Seventeen rows in EVE's order — checkbox, label, swatch, ✕ — on the account
file, with one live error per panel in the R4 grammar.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 12: `FleetView.svelte` — the Watch list colours panel

**Files:**
- Modify: `app/src/lib/FleetView.svelte`, `app/src/lib/FleetView.spec.ts`

**Interfaces:**
- Consumes: `api.setWatchlistColour`, `api.lookupCharacter`, `names` store, `resolveNames`, `toast`, `undoAction`, `ListRow`.

- [ ] **Step 1: Append the failing tests**

Append to `FleetView.spec.ts`:

```ts
describe("the watch list panel", () => {
  test("lists entries with the resolved name, the id, a swatch and a remove button", async () => {
    calls.stub("resolve_character_names", { "1001131163": { name: "Farm Delay", category: "character" } });
    mount();
    const p = await panel("Watch list colours");
    const row = (await within(p).findByText("Farm Delay")).closest("li")!;
    expect(within(row).getByText("1001131163")).toBeTruthy();
    expect((within(row).getByLabelText("Colour for Farm Delay") as HTMLInputElement).value).toBe("#3380ff");
    expect(within(p).getByText("character file")).toBeTruthy();
    // The unresolved id renders bare, and its unreadable colour is marked and disabled.
    const bare = within(p).getByText("90000001").closest("li")!;
    expect(within(bare).getByText("unreadable")).toBeTruthy();
    expect((within(bare).getByLabelText("Colour for 90000001") as HTMLInputElement).disabled).toBe(true);
  });

  test("recolour writes exact palette floats; remove writes null", async () => {
    mount();
    const p = await panel("Watch list colours");
    const swatch = (await within(p).findByLabelText("Colour for 1001131163")) as HTMLInputElement;
    await fireEvent.change(swatch, { target: { value: "#bf0000" } });
    await waitFor(() => expect(calls.of("set_watchlist_colour").length).toBe(1));
    expect(calls.only("set_watchlist_colour").args).toEqual({ charId: 1001131163, rgb: [0.75, 0.0, 0.0] });
    const row = swatch.closest("li")!;
    await fireEvent.click(within(row).getByTitle("Remove from the list"));
    await waitFor(() => expect(calls.of("set_watchlist_colour").length).toBe(2));
    expect(calls.of("set_watchlist_colour")[1].args).toEqual({ charId: 1001131163, rgb: null });
  });

  test("add looks the name up, writes the picked colour, clears the box and toasts", async () => {
    toasts.splice(0, toasts.length);
    calls.stub("lookup_character", { id: 2117000000, name: "New Pilot" });
    mount();
    const p = await panel("Watch list colours");
    const box = within(p).getByLabelText("Add a character") as HTMLInputElement;
    const add = within(p).getByRole("button", { name: "Add" }) as HTMLButtonElement;
    expect(add.disabled).toBe(true);
    await fireEvent.input(box, { target: { value: "new pilot" } });
    expect(add.disabled).toBe(false);
    await fireEvent.click(add);
    await waitFor(() => expect(calls.only("lookup_character").args).toEqual({ query: "new pilot" }));
    await waitFor(() => expect(calls.of("set_watchlist_colour").length).toBe(1));
    // Blue is the default swatch (863 of 1,382 corpus entries).
    expect(calls.only("set_watchlist_colour").args).toEqual({ charId: 2117000000, rgb: [0.2, 0.5, 1.0] });
    await waitFor(() => expect(box.value).toBe(""));
    // ToastHost lives in +layout, so the component test reads the store.
    expect(toasts.map((t) => t.message)).toContain("Added New Pilot");
  });

  test("add: no such character, unreachable ESI, and already listed each say so and write nothing", async () => {
    mount();
    const p = await panel("Watch list colours");
    const box = within(p).getByLabelText("Add a character");
    const submit = async (q: string) => {
      await fireEvent.input(box, { target: { value: q } });
      await fireEvent.click(within(p).getByRole("button", { name: "Add" }));
    };

    calls.stub("lookup_character", null);
    await submit("Nobody");
    expect((await within(p).findByRole("alert")).textContent).toBe("No character called Nobody");

    calls.stub("lookup_character", () => Promise.reject({ code: "esi", message: "ESI status 502" }));
    await submit("Someone");
    await waitFor(() => expect(within(p).getByRole("alert").textContent).toBe("Someone wasn't looked up — couldn't reach ESI"));

    calls.stub("lookup_character", { id: 1001131163, name: "Farm Delay" });
    await submit("Farm Delay");
    await waitFor(() => expect(within(p).getByRole("alert").textContent).toBe("Farm Delay is already in the list"));
    calls.never("set_watchlist_colour");
  });

  test("an empty list has an empty state and the add row; no character file has neither", async () => {
    mount({ watchlist: [] });
    const p = await panel("Watch list colours");
    expect(within(p).getByText("No watch-list colours")).toBeTruthy();
    expect(within(p).getByLabelText("Add a character")).toBeTruthy();
  });

  test("no character file: the panel says so without an action", async () => {
    calls.stub("fleet_settings", { ...FLEET, char_open: false, watchlist: [] });
    render(FleetView, { charOpen: false, userOpen: true, onUserDirty: noop, onCharDirty: noop });
    const p = await panel("Watch list colours");
    expect(within(p).getByText("No character open")).toBeTruthy();
    expect(within(p).queryByLabelText("Add a character")).toBeNull();
  });
});
```

- [ ] **Step 2: Run to see them fail**

Run (from `app/`): `npx vitest run FleetView`
Expected: the new describe fails — no "Watch list colours" heading.

- [ ] **Step 3: Implement**

In `FleetView.svelte`'s script, extend the imports:

```ts
  import { api, errMessage, errText, type Fleet, type FoundCharacter, type HudEntry, type Rgb } from "./api";
  import { names, resolveNames } from "./names.svelte";
  import { toast } from "./ui/toasts.svelte";
  import { undoAction } from "./undo.svelte";
  import ListRow from "./ui/ListRow.svelte";
```

(replacing the narrower `api` and `names.svelte` imports Task 11 wrote). Then, after `const picked = …`, add:

```ts
  // --- watch list ---------------------------------------------------------
  const nameOf = (id: number): string => names[id]?.name ?? String(id);
  let addQuery = $state("");
  // Blue is what 863 of the corpus's 1,382 entries chose; the other 519 are all
  // one other colour, so it is the right starting swatch.
  let addHex = $state(rgbToHex([0.2, 0.5, 1.0]));
  let adding = $state(false);

  const recolour = (id: number, hex: string) =>
    write("watch", `${nameOf(id)}'s colour wasn't changed`, () => api.setWatchlistColour(id, snapToPalette(hex, palette) ?? hexToRgb(hex)), onCharDirty);
  const remove = (id: number) =>
    write("watch", `${nameOf(id)} wasn't removed`, () => api.setWatchlistColour(id, null), onCharDirty);

  async function add(ev: SubmitEvent) {
    ev.preventDefault();
    const q = addQuery.trim();
    if (!q || adding) return;
    watchError = null;
    adding = true;
    try {
      let found: FoundCharacter | null;
      try { found = await api.lookupCharacter(q); }
      catch (e) { watchError = { text: `${q} wasn't looked up — couldn't reach ESI`, detail: errMessage(e) }; return; }
      if (!found) { watchError = { text: `No character called ${q}`, detail: "" }; return; }
      const id = found.id;
      if (fleet?.watchlist.some((w) => w.char_id === id)) { watchError = { text: `${found.name} is already in the list`, detail: "" }; return; }
      const rgb = snapToPalette(addHex, palette) ?? hexToRgb(addHex);
      const ok = await write("watch", `${found.name} wasn't added`, () => api.setWatchlistColour(id, rgb), onCharDirty);
      if (ok) {
        void resolveNames([id]);
        addQuery = "";
        // The new row can land below the fold, so the success is said out loud.
        toast(`Added ${found.name}`, { action: undoAction() });
      }
    } finally { adding = false; }
  }
```

In the markup, after the broadcasts `</Panel>`, add:

```svelte
  <Panel class="watchlist">
    <PanelHeader title="Watch list colours" subtitle="The colour a fleet-mate shows in your watch list">
      {#snippet actions()}<Chip size="sm">character file</Chip>{/snippet}
    </PanelHeader>
    {#if !charOpen}
      <EmptyState title="No character open" description="Watch-list colours live in the character file." />
    {:else if fleet}
      {#if watchError}
        <InlineMessage variant="error" detail={watchError.detail || undefined}>{watchError.text}</InlineMessage>
      {/if}
      {#if fleet.watchlist.length === 0}
        <EmptyState title="No watch-list colours"
          description="Colours you set on watch-list members in-game appear here. Add one below to colour a character before you next fleet with them." />
      {:else}
        <ul class="watch-list">
          {#each fleet.watchlist as w (w.char_id)}
            {@const label = nameOf(w.char_id)}
            <li>
              <ListRow title={String(w.char_id)}>
                {#snippet leading()}
                  <Field kind="color" list="fleet-palette" controlClass={w.rgb ? "" : "unset"}
                    value={w.rgb ? rgbToHex(w.rgb) : UNSET_HEX} ariaLabel="Colour for {label}"
                    disabled={w.rgb === null} disabledReason={NOT_EDITABLE}
                    onchange={(e) => recolour(w.char_id, picked(e))} />
                {/snippet}
                <span class="label">{label}</span>
                {#snippet trailing()}
                  {#if w.rgb === null}<Chip tone="warn" size="sm">unreadable</Chip>{/if}
                  {#if label !== String(w.char_id)}<span class="meta">{w.char_id}</span>{/if}
                  <Button variant="ghost" size="sm" iconOnly title="Remove from the list"
                    onclick={() => remove(w.char_id)}>✕</Button>
                {/snippet}
              </ListRow>
            </li>
          {/each}
        </ul>
      {/if}
      <form class="add" onsubmit={add}>
        <Field kind="text" label="Add a character" placeholder="Name or ID" width="18rem" bind:value={addQuery} />
        <Field kind="color" list="fleet-palette" ariaLabel="Colour for the new entry" bind:value={addHex} />
        <Button variant="primary" type="submit" disabled={addQuery.trim() === "" || adding}
          disabledReason={adding ? "Looking the character up…" : "Type a character name or ID"}>Add</Button>
      </form>
    {/if}
  </Panel>
```

Add to `<style>`:

```css
  .watch-list { list-style: none; margin: 0; padding: 0; max-width: 32rem; }
  .watch-list .label { flex: 1; }
  .meta { color: var(--text-muted); font-size: var(--t-caption); }
  .add { display: flex; align-items: flex-end; gap: var(--s2); margin-top: var(--s3); }
```

`ListRow` renders `children` as the label slot with `min-width: 0` truncation; `leading`/`trailing` are the primitive's snippets (`ui/ListRow.svelte`). The `label !== String(w.char_id)` guard keeps the id from printing twice for an unresolved entry.

- [ ] **Step 4: Run the spec and check**

Run (from `app/`): `npx vitest run FleetView tokens && npm run check`
Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/FleetView.svelte app/src/lib/FleetView.spec.ts
git commit -m "Fleet view: watch-list colours, with add-by-name-or-id through ESI

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 13: `FleetView.svelte` — the Formation panel, and mounting the view

**Files:**
- Modify: `app/src/lib/FleetView.svelte`, `app/src/lib/FleetView.spec.ts`
- Modify: `app/src/routes/+page.svelte` (imports :9-15; `ACCOUNT_SCOPED` :119; the `{:else if view === "probes"}` block :649-656)

- [ ] **Step 1: Append the failing tests**

Append to `FleetView.spec.ts`:

```ts
describe("the formation panel", () => {
  test("renders alongside the other two, each with its file chip", async () => {
    mount();
    await panel("Broadcast settings");
    await panel("Watch list colours");
    const p = await panel("Formation");
    expect(within(p).getByText("character file")).toBeTruthy();
    // And with no account file, the character panels still render.
    calls.stub("fleet_settings", { ...FLEET, user_open: false });
    render(FleetView, { charOpen: true, userOpen: false, onUserDirty: noop, onCharDirty: noop });
    expect((await screen.findAllByRole("heading", { name: "Formation" })).length).toBe(2);
  });

  test("shows the three numbers and the finder toggle, defaults where absent", async () => {
    mount();
    const p = await panel("Formation");
    expect((within(p).getByLabelText("Formation") as HTMLInputElement).value).toBe("0");
    expect((within(p).getByLabelText("Size") as HTMLInputElement).value).toBe("20000");
    expect((within(p).getByLabelText("Spacing") as HTMLInputElement).value).toBe("2000");
    expect((within(p).getByLabelText("Show only my corp, alliance and high-standing fleets") as HTMLInputElement).checked).toBe(true);
    expect(within(p).getByText("character file")).toBeTruthy();
    expect(within(p).getByText(/Saved fleet setups live on CCP's servers/)).toBeTruthy();
  });

  test("a number commits rounded on change and the toggle writes 1/0", async () => {
    mount();
    const p = await panel("Formation");
    const size = within(p).getByLabelText("Size") as HTMLInputElement;
    await fireEvent.change(size, { target: { value: "30000.6" } });
    await waitFor(() => expect(calls.only("set_fleet_field").args).toEqual({ name: "formation_size", text: "30001" }));
    await fireEvent.click(within(p).getByLabelText("Show only my corp, alliance and high-standing fleets"));
    await waitFor(() => expect(calls.of("set_fleet_field").length).toBe(2));
    expect(calls.of("set_fleet_field")[1].args).toEqual({ name: "finder_group_only", text: "0" });
  });

  test("a refused number edit reports on this panel and puts the field back", async () => {
    mount();
    calls.stub("set_fleet_field", () => Promise.reject({ code: "not_editable", message: "nope" }));
    const p = await panel("Formation");
    const size = within(p).getByLabelText("Size") as HTMLInputElement;
    await fireEvent.change(size, { target: { value: "1" } });
    expect((await within(p).findByRole("alert")).textContent).toBe("That formation setting wasn't changed — nope");
    await waitFor(() => expect(size.value).toBe("20000"));
  });
});
```

- [ ] **Step 2: Run to see them fail**

Run (from `app/`): `npx vitest run FleetView`
Expected: the formation describe fails — no "Formation" heading.

- [ ] **Step 3: Implement the panel**

In the script, after the watch-list block, add:

```ts
  // --- formation ----------------------------------------------------------
  const setFormationField = (name: string, text: string) =>
    write("formation", "That formation setting wasn't changed", () => api.setFleetField(name, text), onCharDirty);

  // Int fields: round before writing, and put the input back in step with the
  // model whether or not the write landed — Svelte only patches `value` when
  // the expression changes, so a refused edit would otherwise sit on screen
  // beside a value that is not it (HudPanel's discipline).
  const numberEdit = (name: string) => async (ev: Event) => {
    const el = ev.target as HTMLInputElement;
    const text = el.value;
    if (text.trim() !== "" && Number.isFinite(Number(text))) {
      await setFormationField(name, String(Math.round(Number(text))));
    }
    el.value = shown(name);
  };
  const NUMBERS: { name: string; label: string; step: number }[] = [
    { name: "formation", label: "Formation", step: 1 },
    { name: "formation_size", label: "Size", step: 100 },
    { name: "formation_spacing", label: "Spacing", step: 100 },
  ];
```

In the markup, after the watch-list `</Panel>`:

```svelte
  <Panel class="formation">
    <PanelHeader title="Formation" subtitle="Fleet-warp formation, and the fleet finder">
      {#snippet actions()}<Chip size="sm">character file</Chip>{/snippet}
    </PanelHeader>
    {#if !charOpen}
      <EmptyState title="No character open" description="Formation settings live in the character file." />
    {:else if fleet}
      {#if formationError}
        <InlineMessage variant="error" detail={formationError.detail}>{formationError.text}</InlineMessage>
      {/if}
      <div class="rows">
        {#each NUMBERS as n (n.name)}
          <div class="row">
            <Field kind="number" label={n.label} min={0} step={n.step} width="8rem"
              value={shown(n.name)} disabled={unavailable(n.name)} disabledReason={NOT_EDITABLE}
              onchange={numberEdit(n.name)} />
            {#if n.name !== "formation"}<span class="meta">m</span>{/if}
          </div>
        {/each}
        <div class="row">
          <Field kind="checkbox" label="Show only my corp, alliance and high-standing fleets"
            value={shown("finder_group_only") === "1"}
            disabled={unavailable("finder_group_only")} disabledReason={NOT_EDITABLE}
            onchange={(e) => setFormationField("finder_group_only", checked(e) ? "1" : "0")} />
        </div>
      </div>
      <p class="meta">Saved fleet setups live on CCP's servers and can't be edited here.</p>
    {/if}
  </Panel>
```

- [ ] **Step 4: Mount the view in the page**

In `+page.svelte`: add `import FleetView from "$lib/FleetView.svelte";` after the `ProbeFormationsView` import; change `ACCOUNT_SCOPED` to `["overview", "autofill", "keybinds", "probes", "fleet"]`; and after the `{:else if view === "probes"}` block (before the `{:else}` that renders Raw) add:

```svelte
      {:else if view === "fleet"}
        <div class="scroll">
          <FleetView
            charOpen={subject.slots.char?.status === "opened"}
            userOpen={subject.slots.user?.status === "opened"}
            refreshToken={subject.savedAt}
            userId={subject.userId}
            charId={subject.charId}
            onShowAccounts={() => (sheet = "accounts")}
            onUserDirty={() => { subject.dirty.user = true; noteEdit(); }}
            onCharDirty={() => { subject.dirty.char = true; noteEdit(); }} />
        </div>
```

- [ ] **Step 5: Run every frontend test and the check**

Run (from `app/`): `npm test; echo "exit $?"` then `npm run check`
Expected: exit 0 (read the exit code — the summary line can say "passed" on a non-zero exit), check clean. `page.spec.ts` and `Sidebar.spec.ts` still pass: the tab strip gained a member but their assertions go through `VIEWS`.

- [ ] **Step 6: Run the app and look at it**

Use the `starting-the-app` skill (`.claude/skills/starting-the-app/`) to launch `npm run tauri dev`, open a character that has an account paired, press `Ctrl+6`, and check: three panels with a file chip each; the seventeen broadcast rows in EVE's order; a swatch pick lands in-app without a reload; the watch list names resolve; `Ctrl+7` reaches Raw. Fix anything that differs from the spec's §4 before committing.

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/FleetView.svelte app/src/lib/FleetView.spec.ts app/src/routes/+page.svelte
git commit -m "Fleet view: the Formation panel, and the tab in the shell

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 14: Docs, changelog, and the live-pass items

**Files:**
- Modify: `docs/settings-field-reference.md` (the `**Fleet.**` paragraphs at :246-251 and :688-691), `docs/format-notes.md` (append a section), `docs/live-verification-plan.md` (§8 item index at :586), `CHANGELOG.md` (`[Unreleased]`)

- [ ] **Step 1: Field reference**

Replace the character-file `**Fleet.**` paragraph (`:246-251`) with:

```markdown
**Fleet.** Modelled by the app's Fleet view (`fleet.rs`): `setFleetFormation`
(280) `Int`, `setFleetFormationSize` (280) `Int`, `setFleetFormationSpacing`
(278) `Int`, `fleet_watchlistcolors` (177) dict `{<charID>: (r, g, b)}` — all
keys `Int` so far, `Long` once ids pass 2³¹,
`fleetfinder_showGroupAndHighStandingsFleets` (292) `Int`. Not modelled:
`fleetAdvert_lastAdvert` (314) dict or `Instance(utillib.KeyVal)`,
`fleetAdvert_lastAdvertAdvancedOptions` (236) dict of 8 fields,
`fleetReconnect` (367) `None` or `Tuple(Long, Long)`, `fleetWathlistMemberInfo`
(always `{}`).
```

Replace the account-file `**Fleet.**` paragraph (`:688-691`) with:

```markdown
**Fleet.** Modelled by the app's Fleet view (`fleet.rs`):
`listenBroadcast_<Type>` (`Int` 0/1) for sixteen types — the client writes
seven itself and the rest only when toggled — plus
`listenBroadcast_ShowOwnBroadcasts` and a bare `ShowOwnBroadcasts` written
together; `fleet_broadcastcolor_<Type>` for the same sixteen, `(r, g, b)` or
`None` for EVE's ✕. The type tokens, defaults and palette floats are in the
fleet editor spec §2. Not modelled (window state): `fleetHistoryFilter`,
`fleetFinderBroadcastsVisible`, `fleetfinder_{scope,range,standing}Filter`,
`updateOnBossChange`, `hideInfo`, `public`, `publicgood`, `corp`, `alliance`.
```

- [ ] **Step 2: Format notes**

Append to `docs/format-notes.md`:

```markdown
### Fleet broadcast settings, watch-list colours and formation (2026-09-18)

Everything sits under root `ui`, every leaf `(FILETIME, value)`-wrapped;
account file for the Broadcast Settings dialog, character file for the watch
list and the formation panel. Measured over 513 distinct files plus two live
captures on B1 — see `docs/superpowers/specs/2026-09-18-fleet-editor-design.md`
§2 for the tables. Four things the next reader needs:

- **A `listenBroadcast_<Type>` key is written only when its checkbox is
  toggled.** Nine of the sixteen types have no key in any file; their default
  is ticked. The seven every account carries default to the value they hold
  in ~93 % of accounts (five off, two on).
- **The top checkbox writes two keys** — `listenBroadcast_ShowOwnBroadcasts`
  and a bare `ShowOwnBroadcasts` — same instant, same value. Write both.
- **EVE's ✕ (no colour) writes `(ts, None)`**; the key is not removed. Absent
  means the type's default, which is a colour for four types and none for the
  rest.
- **Opening the dialog re-stamps every existing broadcast key** with one
  FILETIME. A capture diff that shows 25 stamps moving is one dialog open, not
  25 edits.

The picker is nine swatches: yellow `1.0,0.7,0.0`, orange `1.0,0.35,0.0`, red
`0.75,0,0`, green `0.1,0.6,0.1`, teal `0.0,0.63,0.57`, blue `0.2,0.5,1.0`,
darkBlue `0.0,0.15,0.6`, black, white `0.7,0.7,0.7` — the same picker on a
watch-list member. Saved fleet setups ("Form fleet with setup") are not in the
files: the owner's names appear only in the account's `editHistory` for the
"Store fleet setup" dialog, and two of the eight appear nowhere while the
client still lists them.
```

- [ ] **Step 3: Live-verification items**

Append rows to the §8 item index table in `docs/live-verification-plan.md`:

```markdown
| F1 | A watch-list entry the editor minted shows its colour in-game | 0.37.0 | A / A1 | fleet spec §6 |
| F2 | A recoloured broadcast and a `listenBroadcast_*` key the editor minted for a never-keyed type are honoured | 0.37.0 | A / B1 | fleet spec §6 |
| F3 | A `Long` watch-list key is honoured, if a character above 2³¹ can be found | 0.37.0 | A / A1 | fleet spec §2.6 |
```

- [ ] **Step 4: Changelog**

Under `## [Unreleased]` in `CHANGELOG.md`:

```markdown
### Added
- **A Fleet tab.** Edit EVE's broadcast settings — which broadcasts you receive and the colour each shows in — your watch list's per-character colours, and the fleet-warp formation, with EVE's own nine-colour palette.
- **Add a watch-list colour for any character by name or ID**, looked up through ESI.
- **Fleet is a copy aspect.** Copy settings and presets can carry one character's broadcast settings, watch-list colours and formation onto others.
```

- [ ] **Step 5: Commit**

```bash
git add docs/settings-field-reference.md docs/format-notes.md docs/live-verification-plan.md CHANGELOG.md
git commit -m "docs: the fleet keys are modelled; format findings and live-pass items

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 6: Final verification across the workspace**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings` and, from `app/`, `npm test; echo "exit $?"` and `npm run check`.
Expected: everything green, exit 0, check clean. Then hand the branch to `superpowers:finishing-a-development-branch`.
