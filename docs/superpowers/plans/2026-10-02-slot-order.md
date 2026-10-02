# Ship slot layouts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Edit EVE's per-ship HUD module arrangement (`ui -> slotOrder`, account file) in a new Racks view and over MCP, and merge every arrangement across a profile's account files from the batch view.

**Architecture:** All format knowledge goes in one new model module, `crates/settings-model/src/slot_order.rs`: projection, edits, merge, and the merge's disk write. `ops.rs` wraps the edits in the existing undo/`edit_slot` machinery. `setup.rs` wraps the merge beside `copy_files`. Tauri commands, the MCP tools and two Svelte components sit on top.

**Tech Stack:** Rust (`blue_marshal::Value` trees, Tauri commands, the in-process MCP server), Svelte 5 + TypeScript, vitest + @testing-library/svelte.

**Spec:** `docs/superpowers/specs/2026-10-02-slot-order-design.md`

## Global Constraints

- **Wire shape:** `ui -> slotOrder` = `(FILETIME, {shipItemID: [24 Int]})` in `core_user_<id>.dat` only.
- **Every list written is a permutation of flags 11–34.** Low slots are 11–18, mid 19–26, high 27–34. Nothing else may reach the file, because a malformed entry crashes the client's HUD.
- **Positions:** position `p` is rack row `p / 8` (0 top, 1 middle, 2 bottom), column `p % 8`.
- **Default order:** `27..=34, 19..=26, 11..=18`. An absent entry means the default, so removing an entry resets the ship.
- **Labels:** `H1`–`H8` = 27–34, `M1`–`M8` = 19–26, `L1`–`L8` = 11–18.
- **Minting:** a minted `slotOrder` key is `(Long([0;8]), {})`, the zero FILETIME already proven safe in-game.
- **Keys:** ship ids are matched by value. They are written with `fleet.rs`'s `id_key` (`Int` while it fits i32, else the minimal-width `Long`).
- **Edits vs merge:** `add` refuses a present ship; `set`, `remove` and `swap` refuse an absent one; `copy` adds or overwrites. Unreadable entries are projected as `order: None`, are never read by a merge, and are never overwritten by one.
- **Merge rule:** the newest file mtime wins a disagreement. Equal mtimes go to the path that sorts later. A ticked file is both merged and written. Unticked files are never read or written.
- **No real game data:** use synthetic ids only (ships `90000001…`, accounts `80000001…`), per CLAUDE.md.
- **Commits:** no AI attribution. The author is the repo's configured `StormDelay` identity.
- **Test commands:**
  - Rust: `cargo test -p settings-model` / `cargo test -p app`.
  - Frontend: `cd app && npx vitest run <file>`. Judge `npm test` by its **exit code**: it can exit 1 with every test passing, so read the summary line before concluding anything.

## Review Focus

1. **A ship id above 2³¹** (every real one is ~1e12) must round-trip as a `Long` key and match by value. Tasks 1 and 4 pin this with a `Long`-keyed fixture and a 1e12 id.
2. **Merging a file that has unreadable entries** must leave those entries byte-identical, and must neither count them nor use them as a winner. Task 2 pins this.
3. **A second merge run** must write nothing: no backups and no mtime changes. Otherwise every run rewrites every account and moves the "newest wins" clock. Task 4 pins this.
4. **An unticked file in the same folder** must not be read, written or counted, even when it is the newest. Task 4 pins this.
5. **A failed op in an MCP batch** (`add` twice, bad label) must roll the whole batch back. Task 5 pins this.

---

### Task 1: The model — projection and edits

**Files:**
- Create: `crates/settings-model/src/slot_order.rs`
- Modify: `crates/settings-model/src/fleet.rs:235` and `:252`: make `id_of` and `id_key` `pub(crate)` (reuse, don't copy).
- Modify: `crates/settings-model/src/lib.rs`: add `mod slot_order;` and the `pub use`.

**Interfaces:**
- Produces, re-exported from `settings_model`:
  - `SlotOrder = [u8; 24]`, `DEFAULT_ORDER: SlotOrder`
  - `SlotEntry { ship_id: u64, order: Option<SlotOrder> }` (Serialize)
  - `SlotOrderError` (Serialize with `code` tag, Display)
  - `project_slot_orders(&Value) -> Vec<SlotEntry>`
  - `slot_order_add(&mut Value, u64, Option<u64>)`
  - `slot_order_set(&mut Value, u64, &[u8])`
  - `slot_order_remove(&mut Value, u64)`
  - `slot_order_swap(&mut Value, u64, u8, u8)`
  - `slot_order_copy(&mut Value, u64, &[u64])`

  The five edits all return `Result<(), SlotOrderError>`. Also `slot_label(u8) -> String` and `parse_slot(&str) -> Option<u8>`.

- [ ] **Step 1: Make the fleet id helpers crate-visible**

In `crates/settings-model/src/fleet.rs`, change `fn id_of(v: &Value) -> Option<u64>` to `pub(crate) fn id_of(...)`, and `fn id_key(id: u64) -> Value` to `pub(crate) fn id_key(...)`. Change nothing else.

- [ ] **Step 2: Write the module with its failing tests**

Create `crates/settings-model/src/slot_order.rs`:

```rust
//! The ship HUD's module arrangement, per ship: account file
//! `ui -> slotOrder`, `(FILETIME, {shipItemID: [24 inventory flags]})`.
//! Position `p` of a list is rack row `p / 8` (top, middle, bottom) and
//! column `p % 8`; the flag there is the fitted slot drawn on that button.
//! docs/superpowers/specs/2026-10-02-slot-order-design.md §2 has the client
//! code this is read from — `GetSlotOrder`, `InitDrawSlots`, `SwapSlots`.

use blue_marshal::Value;
use serde::Serialize;

use crate::fleet::{id_key, id_of};
use crate::hud::section_dict_mut;
use crate::treewalk::{
    as_dict, as_list, collect_shared, dict_inner_mut, effective, find_child, inline_all, is_bytes, section,
    Entries, SharedTable,
};

const KEY: &[u8] = b"slotOrder";

pub type SlotOrder = [u8; 24];

/// What the client draws for a ship with no entry (`GetSlotOrder`).
pub const DEFAULT_ORDER: SlotOrder = [
    27, 28, 29, 30, 31, 32, 33, 34, // high slots: the top rack
    19, 20, 21, 22, 23, 24, 25, 26, // mid slots: the middle rack
    11, 12, 13, 14, 15, 16, 17, 18, // low slots: the bottom rack
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SlotEntry {
    pub ship_id: u64,
    /// `None` when the stored list is not a permutation of the 24 slots: shown
    /// as unreadable rather than dropped, so its id cannot be added twice.
    pub order: Option<SlotOrder>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum SlotOrderError {
    NoSection,
    NotEditable,
    InvalidOrder,
    UnknownSlot { slot: u8 },
    AlreadyPresent { ship_id: u64 },
    Missing { ship_id: u64 },
    Unreadable { ship_id: u64 },
}

impl std::fmt::Display for SlotOrderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SlotOrderError::NoSection => write!(f, "This file has no section to write ship layouts into."),
            SlotOrderError::NotEditable => write!(f, "This file's ship layouts have an unexpected shape, so they were left alone."),
            SlotOrderError::InvalidOrder => write!(f, "A ship layout must hold each of the 24 high, mid and low slots exactly once."),
            SlotOrderError::UnknownSlot { slot } => write!(f, "{slot} is not a high, mid or low slot."),
            SlotOrderError::AlreadyPresent { ship_id } => write!(f, "Ship {ship_id} already has a layout."),
            SlotOrderError::Missing { ship_id } => write!(f, "Ship {ship_id} has no layout here."),
            SlotOrderError::Unreadable { ship_id } => {
                write!(f, "Ship {ship_id}'s layout is unreadable. Remove it or set a new one.")
            }
        }
    }
}

fn is_slot(f: u8) -> bool {
    (11..=34).contains(&f)
}

/// The list as a `SlotOrder`, only when it holds each of the 24 slots once.
fn check_order(order: &[u8]) -> Result<SlotOrder, SlotOrderError> {
    let arr: SlotOrder = order.try_into().map_err(|_| SlotOrderError::InvalidOrder)?;
    let mut seen = [false; 24];
    for f in arr {
        if !is_slot(f) || std::mem::replace(&mut seen[usize::from(f - 11)], true) {
            return Err(SlotOrderError::InvalidOrder);
        }
    }
    Ok(arr)
}

fn read_order<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<SlotOrder> {
    let flags: Option<Vec<u8>> = as_list(v, sh)?
        .iter()
        .map(|x| match effective(x, sh) {
            Value::Int(i) => u8::try_from(*i).ok(),
            _ => None,
        })
        .collect();
    check_order(&flags?).ok()
}

fn order_value(o: &SlotOrder) -> Value {
    Value::List(o.iter().map(|&f| Value::Int(i64::from(f))).collect())
}

/// Every ship entry, in file order.
pub fn project(user: &Value) -> Vec<SlotEntry> {
    let mut sh = SharedTable::new();
    collect_shared(user, &mut sh);
    let Some((ui, _)) = section(user, b"ui", &sh) else { return Vec::new() };
    let Some(map) = find_child(ui, KEY, &sh).and_then(|v| as_dict(v, &sh)) else { return Vec::new() };
    map.iter()
        .filter_map(|(k, v)| Some(SlotEntry { ship_id: id_of(effective(k, &sh))?, order: read_order(v, &sh) }))
        .collect()
}

fn present(user: &Value, ship_id: u64) -> bool {
    project(user).iter().any(|e| e.ship_id == ship_id)
}

fn order_of(user: &Value, ship_id: u64) -> Result<SlotOrder, SlotOrderError> {
    match project(user).into_iter().find(|e| e.ship_id == ship_id) {
        None => Err(SlotOrderError::Missing { ship_id }),
        Some(SlotEntry { order: None, .. }) => Err(SlotOrderError::Unreadable { ship_id }),
        Some(SlotEntry { order: Some(o), .. }) => Ok(o),
    }
}

/// The inlined payload map, minted as `(zero FILETIME, {})` when absent. Every
/// refusal above is decided BEFORE this runs, so a refused edit never inlines.
fn map_mut(user: &mut Value) -> Result<&mut Entries, SlotOrderError> {
    inline_all(user);
    let ui = section_dict_mut(user, b"ui").ok_or(SlotOrderError::NoSection)?;
    if !ui.iter().any(|(k, _)| is_bytes(k, KEY)) {
        ui.push((
            Value::Bytes(KEY.to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), Value::Dict(Vec::new())]),
        ));
    }
    let (_, slot) = ui.iter_mut().find(|(k, _)| is_bytes(k, KEY)).expect("just ensured");
    dict_inner_mut(slot).ok_or(SlotOrderError::NotEditable)
}

/// Overwrite `ship_id`'s list in place, or append it.
fn put(map: &mut Entries, ship_id: u64, order: &SlotOrder) {
    match map.iter_mut().find(|(k, _)| id_of(k) == Some(ship_id)) {
        Some((_, v)) => *v = order_value(order),
        None => map.push((id_key(ship_id), order_value(order))),
    }
}

/// A new ship, starting from `from`'s order or the default.
pub fn add(user: &mut Value, ship_id: u64, from: Option<u64>) -> Result<(), SlotOrderError> {
    if present(user, ship_id) {
        return Err(SlotOrderError::AlreadyPresent { ship_id });
    }
    let order = match from {
        None => DEFAULT_ORDER,
        Some(f) => order_of(user, f)?,
    };
    put(map_mut(user)?, ship_id, &order);
    Ok(())
}

/// Replace an existing ship's order. An unreadable entry may be replaced.
pub fn set(user: &mut Value, ship_id: u64, order: &[u8]) -> Result<(), SlotOrderError> {
    let order = check_order(order)?;
    if !present(user, ship_id) {
        return Err(SlotOrderError::Missing { ship_id });
    }
    put(map_mut(user)?, ship_id, &order);
    Ok(())
}

/// Drop the entry; the client falls back to `DEFAULT_ORDER`.
pub fn remove(user: &mut Value, ship_id: u64) -> Result<(), SlotOrderError> {
    if !present(user, ship_id) {
        return Err(SlotOrderError::Missing { ship_id });
    }
    map_mut(user)?.retain(|(k, _)| id_of(k) != Some(ship_id));
    Ok(())
}

/// Exchange the buttons of two slots, by flag, exactly as the client's
/// `SwapSlots(flag1, flag2)`.
pub fn swap(user: &mut Value, ship_id: u64, a: u8, b: u8) -> Result<(), SlotOrderError> {
    for slot in [a, b] {
        if !is_slot(slot) {
            return Err(SlotOrderError::UnknownSlot { slot });
        }
    }
    let mut order = order_of(user, ship_id)?;
    let at = |f: u8| order.iter().position(|&x| x == f).expect("a permutation holds every slot");
    let (ia, ib) = (at(a), at(b));
    order.swap(ia, ib);
    put(map_mut(user)?, ship_id, &order);
    Ok(())
}

/// Give every ship in `to` the order of `from`, adding or overwriting.
pub fn copy(user: &mut Value, from: u64, to: &[u64]) -> Result<(), SlotOrderError> {
    let order = order_of(user, from)?;
    let map = map_mut(user)?;
    for &t in to.iter().filter(|&&t| t != from) {
        put(map, t, &order);
    }
    Ok(())
}

/// The name players use: `H1`–`H8`, `M1`–`M8`, `L1`–`L8`.
pub fn slot_label(flag: u8) -> String {
    match flag {
        27..=34 => format!("H{}", flag - 26),
        19..=26 => format!("M{}", flag - 18),
        11..=18 => format!("L{}", flag - 10),
        _ => flag.to_string(),
    }
}

/// A label (any case) or a bare flag, to the flag.
pub fn parse_slot(s: &str) -> Option<u8> {
    let s = s.trim();
    let (base, digits) = match s.get(..1)?.to_ascii_uppercase().as_str() {
        "H" => (26, &s[1..]),
        "M" => (18, &s[1..]),
        "L" => (10, &s[1..]),
        _ => (0, s),
    };
    let n: u8 = digits.parse().ok()?;
    let flag = if base == 0 {
        n
    } else if (1..=8).contains(&n) {
        base + n
    } else {
        return None;
    };
    is_slot(flag).then_some(flag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{b, ts};

    fn wrapped(v: Value) -> Value {
        Value::Tuple(vec![ts(), v])
    }

    pub(super) const CUSTOM: SlotOrder = [
        29, 33, 34, 30, 31, 32, 28, 27, 19, 20, 21, 22, 23, 24, 25, 26, 11, 12, 13, 14, 15, 16, 17, 18,
    ];

    /// Ship 1 custom, ship 2 default under a `Long` key (matched by value),
    /// ship 3 unreadable (one flag).
    fn user_doc() -> Value {
        let map = Value::Dict(vec![
            (Value::Int(90000001), order_value(&CUSTOM)),
            (Value::Long(vec![0x82, 0x4A, 0x5D, 0x05]), order_value(&DEFAULT_ORDER)),
            (Value::Int(90000003), Value::List(vec![Value::Int(27)])),
        ]);
        Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("slotOrder"), wrapped(map))]))])
    }

    fn empty_ui() -> Value {
        Value::Dict(vec![(b("ui"), Value::Dict(vec![]))])
    }

    fn orders(v: &Value) -> Vec<(u64, Option<SlotOrder>)> {
        project(v).into_iter().map(|e| (e.ship_id, e.order)).collect()
    }

    fn map_of(v: &Value) -> &Entries {
        let Value::Dict(root) = v else { panic!("dict root") };
        let Value::Dict(ui) = &root[0].1 else { panic!("ui dict") };
        let (_, leaf) = ui.iter().find(|(k, _)| is_bytes(k, KEY)).expect("slotOrder present");
        let Value::Tuple(parts) = leaf else { panic!("wrapped") };
        let Value::Dict(d) = &parts[1] else { panic!("dict payload") };
        d
    }

    #[test]
    fn projects_in_file_order_matching_ids_by_value_and_keeping_unreadable_entries() {
        assert_eq!(
            orders(&user_doc()),
            vec![(90000001, Some(CUSTOM)), (90000002, Some(DEFAULT_ORDER)), (90000003, None)]
        );
        assert!(project(&empty_ui()).is_empty());
        assert!(project(&Value::Dict(vec![])).is_empty());
    }

    #[test]
    fn add_mints_the_map_with_a_zero_timestamp_and_starts_from_the_default() {
        let mut u = empty_ui();
        add(&mut u, 90000010, None).unwrap();
        assert_eq!(orders(&u), vec![(90000010, Some(DEFAULT_ORDER))]);
        let Value::Dict(root) = &u else { panic!() };
        let Value::Dict(ui) = &root[0].1 else { panic!() };
        let Value::Tuple(parts) = &ui[0].1 else { panic!("a (timestamp, map) wrapper") };
        assert_eq!(parts[0], Value::Long(vec![0u8; 8]), "a mint carries a zero FILETIME");
    }

    #[test]
    fn add_copies_another_ship_and_refuses_a_present_ship_or_an_unusable_source() {
        let mut u = user_doc();
        add(&mut u, 90000010, Some(90000001)).unwrap();
        assert_eq!(orders(&u)[3], (90000010, Some(CUSTOM)));
        assert_eq!(add(&mut u, 90000001, None), Err(SlotOrderError::AlreadyPresent { ship_id: 90000001 }));
        assert_eq!(add(&mut u, 90000003, None), Err(SlotOrderError::AlreadyPresent { ship_id: 90000003 }));
        assert_eq!(add(&mut u, 90000011, Some(90000003)), Err(SlotOrderError::Unreadable { ship_id: 90000003 }));
        assert_eq!(add(&mut u, 90000011, Some(90000099)), Err(SlotOrderError::Missing { ship_id: 90000099 }));
    }

    #[test]
    fn a_ship_id_above_two_to_the_31_is_written_as_a_long_and_read_back() {
        let mut u = empty_ui();
        let id = 1_024_000_000_001;
        add(&mut u, id, None).unwrap();
        assert!(matches!(map_of(&u)[0].0, Value::Long(_)));
        swap(&mut u, id, 27, 11).unwrap();
        assert_eq!(orders(&u)[0].0, id);
    }

    #[test]
    fn set_validates_the_order_and_needs_the_ship_present() {
        let mut u = user_doc();
        assert_eq!(set(&mut u, 90000001, &[27]), Err(SlotOrderError::InvalidOrder));
        let mut twice = DEFAULT_ORDER;
        twice[1] = 27;
        assert_eq!(set(&mut u, 90000001, &twice), Err(SlotOrderError::InvalidOrder));
        let mut outside = DEFAULT_ORDER;
        outside[0] = 35;
        assert_eq!(set(&mut u, 90000001, &outside), Err(SlotOrderError::InvalidOrder));
        assert_eq!(set(&mut u, 90000099, &CUSTOM), Err(SlotOrderError::Missing { ship_id: 90000099 }));
        set(&mut u, 90000002, &CUSTOM).unwrap();
        assert_eq!(orders(&u)[1], (90000002, Some(CUSTOM)));
        assert!(matches!(map_of(&u)[1].0, Value::Long(_)), "the key keeps its wire kind");
        set(&mut u, 90000003, &DEFAULT_ORDER).unwrap();
        assert_eq!(orders(&u)[2], (90000003, Some(DEFAULT_ORDER)), "an unreadable entry can be replaced");
    }

    #[test]
    fn remove_drops_only_that_ship_and_needs_it_present() {
        let mut u = user_doc();
        remove(&mut u, 90000002).unwrap();
        assert_eq!(orders(&u), vec![(90000001, Some(CUSTOM)), (90000003, None)]);
        assert_eq!(remove(&mut u, 90000002), Err(SlotOrderError::Missing { ship_id: 90000002 }));
    }

    #[test]
    fn swap_exchanges_two_slots_by_flag() {
        let mut u = user_doc();
        swap(&mut u, 90000002, 27, 19).unwrap();
        let o = orders(&u)[1].1.unwrap();
        assert_eq!((o[0], o[8]), (19, 27));
        assert_eq!(swap(&mut u, 90000002, 35, 19), Err(SlotOrderError::UnknownSlot { slot: 35 }));
        assert_eq!(swap(&mut u, 90000099, 27, 19), Err(SlotOrderError::Missing { ship_id: 90000099 }));
        assert_eq!(swap(&mut u, 90000003, 27, 19), Err(SlotOrderError::Unreadable { ship_id: 90000003 }));
    }

    #[test]
    fn copy_adds_or_overwrites_every_target_and_skips_the_source() {
        let mut u = user_doc();
        copy(&mut u, 90000001, &[90000002, 90000010, 90000001]).unwrap();
        let o = orders(&u);
        assert_eq!(o[1], (90000002, Some(CUSTOM)));
        assert_eq!(o[3], (90000010, Some(CUSTOM)));
        assert_eq!(o.len(), 4);
        let before = orders(&u);
        assert_eq!(copy(&mut u, 90000003, &[90000001]), Err(SlotOrderError::Unreadable { ship_id: 90000003 }));
        assert_eq!(orders(&u), before, "a refused copy writes nothing");
    }

    #[test]
    fn labels_and_parsing_agree() {
        for (f, l) in [(27, "H1"), (34, "H8"), (19, "M1"), (26, "M8"), (11, "L1"), (18, "L8")] {
            assert_eq!(slot_label(f), l);
            assert_eq!(parse_slot(l), Some(f));
        }
        assert_eq!(parse_slot("m8"), Some(26));
        assert_eq!(parse_slot("27"), Some(27));
        for bad in ["H9", "H0", "35", "10", "X1", "", "H"] {
            assert_eq!(parse_slot(bad), None, "{bad}");
        }
    }

    #[test]
    fn an_edited_document_round_trips_through_the_codec() {
        let mut u = user_doc();
        add(&mut u, 90000010, None).unwrap();
        u = blue_marshal::reshare(&u);
        let bytes = blue_marshal::encode(&u).unwrap();
        assert_eq!(orders(&blue_marshal::decode(&bytes).unwrap()), orders(&u));
    }
}
```

- [ ] **Step 3: Register the module**

In `crates/settings-model/src/lib.rs`, add `mod slot_order;` after `mod fleet;`. After the `pub use fleet::{…};` block, add:

```rust
pub use slot_order::{
    add as slot_order_add, copy as slot_order_copy, parse_slot, project as project_slot_orders,
    remove as slot_order_remove, set as slot_order_set, slot_label, swap as slot_order_swap, SlotEntry,
    SlotOrder, SlotOrderError, DEFAULT_ORDER,
};
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p settings-model slot_order`
Expected: all 10 tests PASS. If a test fails, fix the module, not the test. The tests encode the spec.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/slot_order.rs crates/settings-model/src/lib.rs crates/settings-model/src/fleet.rs
git commit -m "slot_order: project and edit the ship HUD's per-ship slot order"
```

---

### Task 2: The model — merge and its disk write

**Files:**
- Modify: `crates/settings-model/src/slot_order.rs`
- Modify: `crates/settings-model/src/lib.rs` (extend the `pub use slot_order::{…}`)

**Interfaces:**
- Consumes: Task 1's `project`, `map_mut`, `put`.
- Produces:
  - `SlotConflict { ship_id: u64, winner: usize, losers: Vec<usize> }`, where the indices refer to the merge input.
  - `SlotMerge { orders: Vec<(u64, SlotOrder)>, conflicts: Vec<SlotConflict> }`, with `orders` sorted by ship id.
  - `slot_order_merge(&[(&Value, u64)]) -> SlotMerge`
  - `slot_order_merge_diff(&Value, &[(u64, SlotOrder)]) -> (usize, usize)`, meaning (gained, changed).
  - `slot_order_write_merge(&Path, &[(u64, SlotOrder)]) -> Result<SaveReport, String>`

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `slot_order.rs`:

```rust
    /// One map of Int-keyed ships; `None` writes an unreadable one-flag list.
    fn doc_with(entries: &[(i64, Option<SlotOrder>)]) -> Value {
        let map = Value::Dict(
            entries
                .iter()
                .map(|(id, o)| (Value::Int(*id), o.map_or(Value::List(vec![Value::Int(27)]), |o| order_value(&o))))
                .collect(),
        );
        Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("slotOrder"), wrapped(map))]))])
    }

    const OTHER: SlotOrder = [
        19, 28, 29, 30, 31, 32, 33, 34, 27, 20, 21, 22, 23, 24, 25, 26, 11, 12, 13, 14, 15, 16, 17, 18,
    ];

    #[test]
    fn merge_takes_the_union_and_the_newest_file_wins_a_disagreement() {
        let a = doc_with(&[(90000001, Some(DEFAULT_ORDER)), (90000002, Some(CUSTOM))]);
        let b_ = doc_with(&[(90000002, Some(OTHER)), (90000003, Some(DEFAULT_ORDER))]);
        let c = doc_with(&[(90000002, Some(CUSTOM))]);
        let m = merge(&[(&a, 100), (&b_, 200), (&c, 50)]);
        assert_eq!(
            m.orders,
            vec![(90000001, DEFAULT_ORDER), (90000002, OTHER), (90000003, DEFAULT_ORDER)]
        );
        assert_eq!(m.conflicts, vec![SlotConflict { ship_id: 90000002, winner: 1, losers: vec![0, 2] }]);
    }

    #[test]
    fn equal_times_go_to_the_later_file() {
        let a = doc_with(&[(90000002, Some(CUSTOM))]);
        let b_ = doc_with(&[(90000002, Some(OTHER))]);
        assert_eq!(merge(&[(&a, 100), (&b_, 100)]).orders, vec![(90000002, OTHER)]);
        assert_eq!(merge(&[(&b_, 100), (&a, 100)]).orders, vec![(90000002, CUSTOM)]);
    }

    #[test]
    fn unreadable_entries_never_take_part_and_are_never_overwritten() {
        let a = doc_with(&[(90000003, None), (90000001, Some(CUSTOM))]);
        let b_ = doc_with(&[(90000003, Some(DEFAULT_ORDER))]);
        let m = merge(&[(&a, 500), (&b_, 100)]);
        assert_eq!(m.orders, vec![(90000001, CUSTOM), (90000003, DEFAULT_ORDER)]);
        assert!(m.conflicts.is_empty());
        assert_eq!(merge_diff(&a, &m.orders), (0, 0), "its unreadable ship is neither gained nor changed");
        let mut a2 = a.clone();
        apply_merge(&mut a2, &m.orders).unwrap();
        assert_eq!(orders(&a2), orders(&a), "the unreadable entry is left exactly as it was");
    }

    #[test]
    fn the_diff_counts_gained_and_changed_ships_and_apply_makes_it_zero() {
        let a = doc_with(&[(90000001, Some(DEFAULT_ORDER)), (90000002, Some(CUSTOM))]);
        let b_ = doc_with(&[(90000002, Some(OTHER)), (90000003, Some(DEFAULT_ORDER))]);
        let m = merge(&[(&a, 100), (&b_, 200)]);
        assert_eq!(merge_diff(&a, &m.orders), (1, 1));
        assert_eq!(merge_diff(&b_, &m.orders), (1, 0));
        let mut a2 = a.clone();
        apply_merge(&mut a2, &m.orders).unwrap();
        assert_eq!(merge_diff(&a2, &m.orders), (0, 0));
    }

    #[test]
    fn write_merge_backs_up_and_saves_the_merged_file() {
        let dir = std::env::temp_dir().join(format!("slot-order-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("core_user_80000001.dat");
        std::fs::write(&path, blue_marshal::encode(&doc_with(&[(90000001, Some(CUSTOM))])).unwrap()).unwrap();
        let rep = write_merge(&path, &[(90000001, CUSTOM), (90000002, OTHER)]).unwrap();
        assert!(rep.backup_path.exists());
        let back = blue_marshal::decode(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(orders(&back), vec![(90000001, Some(CUSTOM)), (90000002, Some(OTHER))]);
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p settings-model slot_order`
Expected: compile errors. `merge`, `SlotConflict`, `merge_diff`, `apply_merge` and `write_merge` are undefined.

- [ ] **Step 3: Implement**

Add to `slot_order.rs`, above `#[cfg(test)]`. Add these imports at the top: `use std::collections::{BTreeMap, HashMap};`, `use std::path::Path;`, `use crate::document::{Document, LoadError};` and `use crate::save::{save, SaveReport};`.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct SlotConflict {
    pub ship_id: u64,
    /// Index into the merge input of the file whose order was kept.
    pub winner: usize,
    /// Indices of every file holding a different order for this ship.
    pub losers: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlotMerge {
    /// Every readable ship across the input, sorted by id.
    pub orders: Vec<(u64, SlotOrder)>,
    pub conflicts: Vec<SlotConflict>,
}

/// The union of every readable entry in `files` (`(document, modified)`).
/// Where files disagree the newest `modified` wins; equal times go to the
/// later index, so a caller that sorts its input by path breaks ties the same
/// way every run. Unreadable entries never take part.
pub fn merge(files: &[(&Value, u64)]) -> SlotMerge {
    let readable: Vec<Vec<(u64, SlotOrder)>> = files
        .iter()
        .map(|(v, _)| project(v).into_iter().filter_map(|e| Some((e.ship_id, e.order?))).collect())
        .collect();
    let mut by_age: Vec<usize> = (0..files.len()).collect();
    by_age.sort_by_key(|&i| (files[i].1, i));
    let mut kept: BTreeMap<u64, (SlotOrder, usize)> = BTreeMap::new();
    for &i in &by_age {
        for &(id, o) in &readable[i] {
            kept.insert(id, (o, i));
        }
    }
    let conflicts = kept
        .iter()
        .filter_map(|(&ship_id, &(o, winner))| {
            let losers: Vec<usize> = (0..files.len())
                .filter(|&i| readable[i].iter().any(|&(id, p)| id == ship_id && p != o))
                .collect();
            (!losers.is_empty()).then_some(SlotConflict { ship_id, winner, losers })
        })
        .collect();
    SlotMerge { orders: kept.into_iter().map(|(id, (o, _))| (id, o)).collect(), conflicts }
}

/// What writing `orders` would do to this file: (ships it gains, ships whose
/// order changes). An unreadable entry is neither; the merge leaves it alone.
pub fn merge_diff(user: &Value, orders: &[(u64, SlotOrder)]) -> (usize, usize) {
    let have: HashMap<u64, Option<SlotOrder>> = project(user).into_iter().map(|e| (e.ship_id, e.order)).collect();
    orders.iter().fold((0, 0), |(gained, changed), (id, o)| match have.get(id) {
        None => (gained + 1, changed),
        Some(Some(p)) if p != o => (gained, changed + 1),
        _ => (gained, changed),
    })
}

/// Make every readable or absent entry equal `orders`; unreadable ones stay.
fn apply_merge(user: &mut Value, orders: &[(u64, SlotOrder)]) -> Result<(), SlotOrderError> {
    let unreadable: Vec<u64> = project(user).into_iter().filter(|e| e.order.is_none()).map(|e| e.ship_id).collect();
    let map = map_mut(user)?;
    for (id, o) in orders.iter().filter(|(id, _)| !unreadable.contains(id)) {
        put(map, *id, o);
    }
    Ok(())
}

/// Load `path`, apply the merge, reshare, and run the full save chain (backup
/// first). `force_conflict = true` as in `batch::apply_categories_to`: the
/// file is loaded fresh in this call, so there is no stale copy to guard.
pub fn write_merge(path: &Path, orders: &[(u64, SlotOrder)]) -> Result<SaveReport, String> {
    let mut doc = Document::load(path).map_err(|e| match e {
        LoadError::Io(m) => format!("Io: {m}"),
        LoadError::Decode { message, .. } => format!("Decode: {message}"),
    })?;
    apply_merge(&mut doc.value, orders).map_err(|e| e.to_string())?;
    doc.value = blue_marshal::reshare(&doc.value);
    save(&mut doc, true).map_err(|e| format!("{e:?}"))
}
```

In `lib.rs`, extend the slot_order re-export with `merge as slot_order_merge, merge_diff as slot_order_merge_diff, write_merge as slot_order_write_merge, SlotConflict, SlotMerge`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p settings-model slot_order`
Expected: all 15 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/slot_order.rs crates/settings-model/src/lib.rs
git commit -m "slot_order: merge ship layouts across files, newest file wins"
```

---

### Task 3: App commands for the editor

**Files:**
- Modify: `app/src-tauri/src/ops.rs`: imports (the `use settings_model::{…}` at the top), new functions after `set_watchlist_colour` (~line 526), and a test after `watch_list_writes_land_in_the_character_file`.
- Modify: `app/src-tauri/src/lib.rs`: six `#[tauri::command]`s after `set_watchlist_colour` (~line 586), registered in `generate_handler!` after the fleet line (~line 781).
- Modify: `app/src/lib/api.ts`: the `SlotEntry` type and six calls after `setWatchlistColour` (~line 570).

**Interfaces:**
- Consumes: Task 1's re-exports.
- Produces:
  - `ops::slot_orders(&AppState) -> Result<Vec<SlotEntry>, ErrDto>`
  - `ops::add_slot_order(&AppState, u64, Option<u64>)`
  - `ops::set_slot_order(&AppState, u64, &[u8])`
  - `ops::remove_slot_order(&AppState, u64)`
  - `ops::swap_slot_order(&AppState, u64, u8, u8)`
  - `ops::copy_slot_order(&AppState, u64, &[u64])`

  The five edit functions all return `Result<Vec<SlotEntry>, ErrDto>`. Error codes come from `SlotOrderError`'s tag: `already_present`, `missing`, `unreadable`, `invalid_order`, `unknown_slot`, `no_section`, `not_editable`.
- Tauri: `slot_orders`, `slot_order_add {shipId, from}`, `slot_order_set {shipId, order}`, `slot_order_remove {shipId}`, `slot_order_swap {shipId, a, b}`, `slot_order_copy {from, to}`.
- TS: `api.slotOrders()`, `api.addSlotOrder(shipId, from | null)`, `api.setSlotOrder(shipId, order)`, `api.removeSlotOrder(shipId)`, `api.swapSlots(shipId, a, b)`, `api.copySlotOrder(from, to)`. Each resolves to `SlotEntry[]`.

- [ ] **Step 1: Write the failing ops test**

In `ops.rs`'s `mod tests`, after `watch_list_writes_land_in_the_character_file`:

```rust
    #[test]
    fn slot_order_edits_land_in_the_account_file_one_undo_step_each() {
        let state = AppState::new();
        assert_eq!(slot_orders(&state).unwrap_err().code, "no_document");
        let path = temp_file("slot-user", &fleet_doc_bytes());
        open_file(&state, Slot::User, &path.to_string_lossy()).expect("open");
        assert!(slot_orders(&state).unwrap().is_empty());

        let s = add_slot_order(&state, 90000001, None).expect("add");
        assert_eq!(s[0].order, Some(settings_model::DEFAULT_ORDER));
        let s = swap_slot_order(&state, 90000001, 27, 19).expect("swap");
        assert_eq!(s[0].order.unwrap()[0], 19);
        let s = copy_slot_order(&state, 90000001, &[90000002]).expect("copy");
        assert_eq!(s.len(), 2);
        let s = remove_slot_order(&state, 90000002).expect("remove");
        assert_eq!(s.len(), 1);

        assert_eq!(add_slot_order(&state, 90000001, None).unwrap_err().code, "already_present");
        assert_eq!(set_slot_order(&state, 90000001, &[27]).unwrap_err().code, "invalid_order");
        assert_eq!(remove_slot_order(&state, 90000009).unwrap_err().code, "missing");
        roundtrips(&state, Slot::User);
        assert_eq!(undo::undo_state(&state).depth, 4, "four edits, four steps; refusals add none");
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p app slot_order_edits`
Expected: compile errors, because `slot_orders` and the other new functions are undefined.

- [ ] **Step 3: Implement the ops**

Add to the `use settings_model::{…}` list in `ops.rs`: `project_slot_orders, slot_order_add, slot_order_copy, slot_order_remove, slot_order_set, slot_order_swap, SlotEntry,`. After `set_watchlist_colour`:

```rust
/// Every ship's HUD slot order in the open account file (spec 2026-10-02 §3.2).
pub fn slot_orders(state: &AppState) -> Result<Vec<SlotEntry>, ErrDto> {
    let guard = state.user.lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| no_document(Slot::User))?;
    Ok(project_slot_orders(&doc.value))
}

pub fn add_slot_order(state: &AppState, ship_id: u64, from: Option<u64>) -> Result<Vec<SlotEntry>, ErrDto> {
    edit_slot(state, Slot::User, |v| slot_order_add(v, ship_id, from), |e| coded_err("slot_order", e))?;
    slot_orders(state)
}

pub fn set_slot_order(state: &AppState, ship_id: u64, order: &[u8]) -> Result<Vec<SlotEntry>, ErrDto> {
    edit_slot(state, Slot::User, |v| slot_order_set(v, ship_id, order), |e| coded_err("slot_order", e))?;
    slot_orders(state)
}

pub fn remove_slot_order(state: &AppState, ship_id: u64) -> Result<Vec<SlotEntry>, ErrDto> {
    edit_slot(state, Slot::User, |v| slot_order_remove(v, ship_id), |e| coded_err("slot_order", e))?;
    slot_orders(state)
}

pub fn swap_slot_order(state: &AppState, ship_id: u64, a: u8, b: u8) -> Result<Vec<SlotEntry>, ErrDto> {
    edit_slot(state, Slot::User, |v| slot_order_swap(v, ship_id, a, b), |e| coded_err("slot_order", e))?;
    slot_orders(state)
}

pub fn copy_slot_order(state: &AppState, from: u64, to: &[u64]) -> Result<Vec<SlotEntry>, ErrDto> {
    edit_slot(state, Slot::User, |v| slot_order_copy(v, from, to), |e| coded_err("slot_order", e))?;
    slot_orders(state)
}
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p app slot_order_edits`
Expected: PASS. If the undo depth is not 4, read `edit_reshared` before changing the assertion: a refused edit must not leave a history entry.

- [ ] **Step 5: Add the Tauri commands**

In `lib.rs`, after the `set_watchlist_colour` command:

```rust
#[tauri::command]
fn slot_orders(state: tauri::State<'_, AppState>) -> Result<Vec<settings_model::SlotEntry>, ErrDto> {
    ops::slot_orders(&state)
}
#[tauri::command]
fn slot_order_add(state: tauri::State<'_, AppState>, ship_id: u64, from: Option<u64>) -> Result<Vec<settings_model::SlotEntry>, ErrDto> {
    ops::add_slot_order(&state, ship_id, from)
}
#[tauri::command]
fn slot_order_set(state: tauri::State<'_, AppState>, ship_id: u64, order: Vec<u8>) -> Result<Vec<settings_model::SlotEntry>, ErrDto> {
    ops::set_slot_order(&state, ship_id, &order)
}
#[tauri::command]
fn slot_order_remove(state: tauri::State<'_, AppState>, ship_id: u64) -> Result<Vec<settings_model::SlotEntry>, ErrDto> {
    ops::remove_slot_order(&state, ship_id)
}
#[tauri::command]
fn slot_order_swap(state: tauri::State<'_, AppState>, ship_id: u64, a: u8, b: u8) -> Result<Vec<settings_model::SlotEntry>, ErrDto> {
    ops::swap_slot_order(&state, ship_id, a, b)
}
#[tauri::command]
fn slot_order_copy(state: tauri::State<'_, AppState>, from: u64, to: Vec<u64>) -> Result<Vec<settings_model::SlotEntry>, ErrDto> {
    ops::copy_slot_order(&state, from, &to)
}
```

In `generate_handler![…]`, after `fleet_settings, set_fleet_field, set_fleet_colour, set_watchlist_colour,`, add the line:
`slot_orders, slot_order_add, slot_order_set, slot_order_remove, slot_order_swap, slot_order_copy,`

- [ ] **Step 6: Add the API calls**

In `api.ts`, next to the other exported interfaces (e.g. after `BatchTargetResult`):

```ts
/** One ship's HUD arrangement: 24 inventory flags, position p drawn at rack
 *  row p/8 (top, middle, bottom), column p%8. `null`: the stored list is not
 *  a readable arrangement. Ship ids are item ids (~1e12), safe as numbers. */
export interface SlotEntry {
  ship_id: number;
  order: number[] | null;
}
```

In the `api` object, after `setWatchlistColour`:

```ts
  slotOrders: () => invoke<SlotEntry[]>("slot_orders"),
  addSlotOrder: (shipId: number, from: number | null) =>
    invoke<SlotEntry[]>("slot_order_add", { shipId, from }),
  setSlotOrder: (shipId: number, order: number[]) =>
    invoke<SlotEntry[]>("slot_order_set", { shipId, order }),
  removeSlotOrder: (shipId: number) => invoke<SlotEntry[]>("slot_order_remove", { shipId }),
  swapSlots: (shipId: number, a: number, b: number) =>
    invoke<SlotEntry[]>("slot_order_swap", { shipId, a, b }),
  copySlotOrder: (from: number, to: number[]) => invoke<SlotEntry[]>("slot_order_copy", { from, to }),
```

- [ ] **Step 7: Build and commit**

Run: `cargo build -p app`. It can take minutes, so run it in the background (memory: a silent long build gets an agent killed). Then run `cd app && npx svelte-check --threshold error`, or the repo's `npm run check` if it exists.
Expected: both exit 0.

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "slot_order: app commands for the per-ship slot order editor"
```

---

### Task 4: App commands for the profile merge

**Files:**
- Modify: `app/src-tauri/src/setup.rs`: types and functions after `err_result` (~line 659), tests in `mod tests`.
- Modify: `app/src-tauri/src/lib.rs`: two commands after `copy_files`, registered next to `copy_files`.
- Modify: `app/src/lib/api.ts`: three types and two calls after `copyFiles`.

**Interfaces:**
- Consumes: Task 2's `slot_order_merge`, `slot_order_merge_diff` and `slot_order_write_merge`. Also `discover`, `FileKind`, `TargetResult`, `ok_result` and `err_result`, all already in `setup.rs`.
- Produces:
  - `setup::MergePlan { files: Vec<MergeFile>, conflicts: Vec<MergeConflict> }`
  - `MergeFile { path, account_id: Option<u64>, gained, changed, error: Option<String> }`
  - `MergeConflict { ship_id, kept_from: String, overridden: Vec<String> }`
  - `setup::slot_order_merge_preview(&[PathBuf], &[String]) -> MergePlan`
  - `setup::slot_order_merge_apply(&[PathBuf], &[String]) -> Vec<TargetResult>`. A file with nothing to change reports `ok: true, backup_path: None`.
- Tauri: `slot_order_merge_preview {files}`, `slot_order_merge_apply {files}`.
- TS: `api.slotOrderMergePreview(files)` and `api.slotOrderMergeApply(files)`, with types `SlotMergePlan`, `SlotMergeFile`, `SlotMergeConflict`.

- [ ] **Step 1: Write the failing test**

In `setup.rs`'s `mod tests`:

```rust
    fn merge_root(tag: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("slot-merge-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let prof = base.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        fs::create_dir_all(&prof).unwrap();
        (base, prof)
    }

    fn slot_doc(entries: &[(i64, [u8; 24])]) -> Vec<u8> {
        let map = Value::Dict(
            entries
                .iter()
                .map(|(id, o)| (Value::Int(*id), Value::List(o.iter().map(|&f| Value::Int(f.into())).collect())))
                .collect(),
        );
        let ui = Value::Dict(vec![(b("slotOrder"), Value::Tuple(vec![Value::Long(vec![0; 8]), map]))]);
        encode(&Value::Dict(vec![(b("ui"), ui)])).unwrap()
    }

    fn write_at(path: &Path, bytes: &[u8], secs: u64) {
        fs::write(path, bytes).unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
    }

    fn slot_orders_in(path: &Path) -> Vec<(u64, Option<[u8; 24]>)> {
        let v = blue_marshal::decode(&fs::read(path).unwrap()).unwrap();
        let mut o: Vec<_> = settings_model::project_slot_orders(&v).into_iter().map(|e| (e.ship_id, e.order)).collect();
        o.sort();
        o
    }

    #[test]
    fn slot_order_merge_previews_then_writes_the_union_into_every_ticked_account_file() {
        let (root, prof) = merge_root("union");
        let d = settings_model::DEFAULT_ORDER;
        let mut custom = d;
        custom.swap(0, 8);
        let big: i64 = 1_024_000_000_001; // a real-sized item id: a Long key on the wire
        let a = prof.join("core_user_80000001.dat");
        let bb = prof.join("core_user_80000002.dat");
        let c = prof.join("core_user_80000003.dat");
        let ch = prof.join("core_char_90000101.dat");
        write_at(&a, &slot_doc(&[(90000001, d), (90000002, d)]), 1_000);
        write_at(&bb, &slot_doc(&[(90000002, custom), (big, d)]), 2_000);
        write_at(&c, &slot_doc(&[(90000002, d), (90000009, d)]), 3_000); // newest, but never ticked
        fs::write(&ch, slot_doc(&[])).unwrap();
        let roots = vec![root];
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let ticked = vec![s(&a), s(&bb), s(&ch)];

        let plan = slot_order_merge_preview(&roots, &ticked);
        let row = |p: &Path| plan.files.iter().find(|f| Path::new(&f.path) == p).unwrap();
        assert_eq!((row(&a).gained, row(&a).changed), (1, 1));
        assert_eq!((row(&bb).gained, row(&bb).changed), (1, 0));
        assert_eq!(row(&a).account_id, Some(80000001));
        assert_eq!(row(&ch).error.as_deref(), Some("Not an account settings file."));
        assert!(plan.files.iter().all(|f| Path::new(&f.path) != c), "an unticked file is not in the plan");
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].ship_id, 90000002);
        assert_eq!(Path::new(&plan.conflicts[0].kept_from), bb.as_path(), "newest TICKED file wins");

        let c_before = fs::read(&c).unwrap();
        let results = slot_order_merge_apply(&roots, &ticked);
        let res = |p: &Path| results.iter().find(|r| Path::new(&r.path) == p).unwrap();
        for p in [&a, &bb] {
            assert!(res(p).ok && res(p).backup_path.is_some(), "{:?}", res(p));
            assert_eq!(
                slot_orders_in(p),
                vec![(90000001, Some(d)), (90000002, Some(custom)), (big as u64, Some(d))]
            );
        }
        assert!(!res(&ch).ok);
        assert_eq!(fs::read(&c).unwrap(), c_before, "an unticked file is never written");

        let mtime = |p: &Path| fs::metadata(p).unwrap().modified().unwrap();
        let before = (mtime(&a), mtime(&bb));
        let again = slot_order_merge_apply(&roots, &ticked[..2]);
        assert!(again.iter().all(|r| r.ok && r.backup_path.is_none()), "{again:?}");
        assert_eq!((mtime(&a), mtime(&bb)), before, "a second run writes nothing");
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p app slot_order_merge`
Expected: compile errors, because `slot_order_merge_preview` is undefined.

- [ ] **Step 3: Implement**

Add `slot_order_merge, slot_order_merge_diff, slot_order_write_merge, SlotMerge` to `setup.rs`'s `use settings_model::{…}`. After `err_result`:

```rust
/// One ticked account file in a slot-order merge plan.
#[derive(Debug, Serialize)]
pub struct MergeFile {
    pub path: String,
    pub account_id: Option<u64>,
    /// Ships this file does not have yet.
    pub gained: usize,
    /// Ships whose order here differs from the merged one.
    pub changed: usize,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MergeConflict {
    pub ship_id: u64,
    pub kept_from: String,
    pub overridden: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MergePlan {
    pub files: Vec<MergeFile>,
    pub conflicts: Vec<MergeConflict>,
}

struct MergeSource {
    path: String,
    account_id: Option<u64>,
    modified: u64,
    doc: Result<Value, String>,
}

/// The ticked files, sorted by path (so equal mtimes break ties the same way
/// every run), each checked against discovery: only a real ACCOUNT file is
/// read, as `copy_files` checks kinds. Paths come from the frontend.
fn merge_sources(roots: &[PathBuf], files: &[String]) -> Vec<MergeSource> {
    let known: HashMap<PathBuf, (Option<u64>, u64)> = discover(roots)
        .into_iter()
        .flat_map(|p| p.files)
        .filter(|f| f.kind == FileKind::User)
        .map(|f| (f.path, (f.id, f.modified_unix.unwrap_or(0))))
        .collect();
    let mut paths = files.to_vec();
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|path| match known.get(Path::new(&path)) {
            None => MergeSource { path, account_id: None, modified: 0, doc: Err("Not an account settings file.".into()) },
            Some(&(account_id, modified)) => {
                let doc = fs::read(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|b| blue_marshal::decode(&b).map_err(|e| e.to_string()));
                MergeSource { path, account_id, modified, doc }
            }
        })
        .collect()
}

/// The merge over the readable sources, plus the map from merge-input index
/// back to `sources` index.
fn merge_of(sources: &[MergeSource]) -> (SlotMerge, Vec<usize>) {
    let readable: Vec<usize> = (0..sources.len()).filter(|&i| sources[i].doc.is_ok()).collect();
    let inputs: Vec<(&Value, u64)> = readable
        .iter()
        .map(|&i| (sources[i].doc.as_ref().expect("filtered to Ok"), sources[i].modified))
        .collect();
    (slot_order_merge(&inputs), readable)
}

/// Plan a slot-order merge across the ticked account files, writing nothing.
pub fn slot_order_merge_preview(roots: &[PathBuf], files: &[String]) -> MergePlan {
    let sources = merge_sources(roots, files);
    let (m, readable) = merge_of(&sources);
    MergePlan {
        files: sources
            .iter()
            .map(|s| {
                let (gained, changed) = s.doc.as_ref().map_or((0, 0), |d| slot_order_merge_diff(d, &m.orders));
                MergeFile { path: s.path.clone(), account_id: s.account_id, gained, changed, error: s.doc.as_ref().err().cloned() }
            })
            .collect(),
        conflicts: m
            .conflicts
            .iter()
            .map(|c| MergeConflict {
                ship_id: c.ship_id,
                kept_from: sources[readable[c.winner]].path.clone(),
                overridden: c.losers.iter().map(|&i| sources[readable[i]].path.clone()).collect(),
            })
            .collect(),
    }
}

/// Write the merge into every ticked account file that changes, each backed up
/// first. Recomputed from disk, never from an earlier preview. A file with
/// nothing to change is not touched: `ok` with no backup.
pub fn slot_order_merge_apply(roots: &[PathBuf], files: &[String]) -> Vec<TargetResult> {
    let sources = merge_sources(roots, files);
    let (m, _) = merge_of(&sources);
    sources
        .iter()
        .map(|s| match &s.doc {
            Err(e) => err_result(&s.path, e.clone()),
            Ok(d) if slot_order_merge_diff(d, &m.orders) == (0, 0) => {
                TargetResult { path: s.path.clone(), ok: true, backup_path: None, error: None }
            }
            Ok(_) => slot_order_write_merge(Path::new(&s.path), &m.orders)
                .map(|r| ok_result(&s.path, r.backup_path.to_string_lossy().into_owned()))
                .unwrap_or_else(|e| err_result(&s.path, e)),
        })
        .collect()
}
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p app slot_order_merge`
Expected: PASS. If `discover` doesn't find the temp profile, compare with `mcp.rs`'s `temp_profile()` (~line 3846), which uses the same `<root>/c_eve_sharedcache_tq_tranquility/settings_Default` layout.

- [ ] **Step 5: Commands and API**

In `lib.rs`, after `copy_files`:

```rust
#[tauri::command]
fn slot_order_merge_preview(files: Vec<String>) -> setup::MergePlan {
    setup::slot_order_merge_preview(&settings_model::default_roots(), &files)
}
#[tauri::command]
fn slot_order_merge_apply(files: Vec<String>) -> Vec<setup::TargetResult> {
    setup::slot_order_merge_apply(&settings_model::default_roots(), &files)
}
```

Register them: change `setup_preview, setup_apply, copy_files,` to `setup_preview, setup_apply, copy_files, slot_order_merge_preview, slot_order_merge_apply,`.

In `api.ts`, after `BatchTargetResult`:

```ts
export interface SlotMergeFile {
  path: string;
  account_id: number | null;
  gained: number;
  changed: number;
  error: string | null;
}
export interface SlotMergeConflict {
  ship_id: number;
  kept_from: string;
  overridden: string[];
}
export interface SlotMergePlan {
  files: SlotMergeFile[];
  conflicts: SlotMergeConflict[];
}
```

In the `api` object, after `copyFiles`:

```ts
  slotOrderMergePreview: (files: string[]) =>
    invoke<SlotMergePlan>("slot_order_merge_preview", { files }),
  slotOrderMergeApply: (files: string[]) =>
    invoke<BatchTargetResult[]>("slot_order_merge_apply", { files }),
```

- [ ] **Step 6: Build and commit**

Run: `cargo build -p app` in the background, then the svelte check. Expected: both exit 0.

```bash
git add app/src-tauri/src/setup.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "slot_order: merge ship layouts across a profile's account files"
```

---

### Task 5: MCP tools

**Files:**
- Modify: `app/src-tauri/src/mcp.rs`:
  - view and op helpers after `fleet_op` (~line 451);
  - `ToolDef`s after `lookup_character` (~line 961);
  - dispatch after `"lookup_character"` (~line 1285);
  - `merge_files` in `impl EveMcp` next to `fleet_get` (~line 606);
  - `written_paths` (~line 1666);
  - `WORKSPACE_FREE` (~line 1700);
  - `TOPICS` (line 276) and the `eve_guide` description (line 665);
  - tests.
- Modify: `app/src-tauri/src/mcp_primer.md`: the line-19 list, plus a new `## racks` section before `## copy`.

**Interfaces:**
- Consumes: Task 3's ops and Task 4's setup functions. Also `settings_model::{slot_label, parse_slot, SlotEntry, FileKind}`.
- Produces the tools `slot_order_get`, `slot_order_edit`, `slot_order_merge_preview` and `slot_order_merge_apply`, plus the `eve_guide` topic `racks`.

- [ ] **Step 1: Write the failing tests**

In `mcp.rs`'s `mod tests`, after the fleet tests:

```rust
    #[test]
    fn slot_order_edit_adds_swaps_copies_and_removes_in_one_undo_step_and_survives_save() {
        let (s, upath) = open_user(&empty_ui_bytes());
        let v = s.call("slot_order_edit", &args(json!({ "ops": [
            { "op": "add", "ship_id": 90000001 },
            { "op": "swap", "ship_id": 90000001, "a": "H1", "b": "M1" },
            { "op": "copy", "from": 90000001, "to": [90000002] },
            { "op": "add", "ship_id": 90000003, "from": 90000001 },
            { "op": "remove", "ship_id": 90000003 }
        ]}))).unwrap();
        assert_no_paths(&v, "slot_order_edit");
        let ships = v["ships"].as_array().unwrap();
        assert_eq!(ships.len(), 2);
        assert_eq!(ships[0]["rows"]["top"][0], "M1");
        assert_eq!(ships[0]["rows"]["middle"][0], "H1");
        assert_eq!(ships[0]["rows"]["bottom"].as_array().unwrap().len(), 8);
        assert_eq!(ships[1]["ship_id"], 90000002);
        assert_eq!(ships[1]["rows"], ships[0]["rows"]);
        assert_eq!(undo::undo_state(&s.state()).depth, 1, "one batch, one undo step");

        s.call("save", &Args::new()).unwrap();
        s.call("open", &open_args(&upath, None)).unwrap();
        let v = s.call("slot_order_get", &Args::new()).unwrap();
        assert_eq!(v["ships"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn slot_order_edit_rolls_back_a_batch_on_a_refused_op_or_a_bad_slot() {
        let (s, _) = open_user(&empty_ui_bytes());
        let e = s.call("slot_order_edit", &args(json!({ "ops": [
            { "op": "add", "ship_id": 90000001 }, { "op": "add", "ship_id": 90000001 }
        ]}))).unwrap_err();
        assert_eq!(e["code"], "already_present");
        assert_eq!(e["op_index"], 1);
        assert_eq!(s.call("slot_order_get", &Args::new()).unwrap()["ships"], json!([]), "rolled back");

        s.call("slot_order_edit", &args(json!({ "ops": [{ "op": "add", "ship_id": 90000001 }] }))).unwrap();
        let e = s.call("slot_order_edit", &args(json!({ "ops": [
            { "op": "swap", "ship_id": 90000001, "a": "H9", "b": "M1" }
        ]}))).unwrap_err();
        assert_eq!(e["code"], "bad_arguments");
        let e = s.call("slot_order_edit", &args(json!({ "ops": [
            { "op": "set", "ship_id": 90000001, "order": ["H1"] }
        ]}))).unwrap_err();
        assert_eq!(e["code"], "invalid_order");
    }

    #[test]
    fn slot_order_merge_previews_then_writes_immediately_and_defaults_to_account_files() {
        let (s, prof) = copy_server();
        let order: Vec<BmValue> = settings_model::DEFAULT_ORDER.iter().map(|&f| BmValue::Int(f.into())).collect();
        let doc = |id: i64| {
            let map = BmValue::Dict(vec![(BmValue::Int(id), BmValue::List(order.clone()))]);
            let ui = BmValue::Dict(vec![(b("slotOrder"), BmValue::Tuple(vec![BmValue::Long(vec![0; 8]), map]))]);
            encode(&BmValue::Dict(vec![(b("ui"), ui)])).unwrap()
        };
        std::fs::write(prof.join("core_user_500.dat"), doc(90000001)).unwrap();
        std::fs::write(prof.join("core_user_600.dat"), doc(90000002)).unwrap();
        let files = json!([
            prof.join("core_user_500.dat").to_string_lossy(),
            prof.join("core_user_600.dat").to_string_lossy()
        ]);
        let plan = s.call("slot_order_merge_preview", &args(json!({ "files": files }))).unwrap();
        assert!(plan["files"].as_array().unwrap().iter().all(|f| f["gained"] == 1), "{plan}");
        let r = s.call("slot_order_merge_apply", &args(json!({ "files": files }))).unwrap();
        assert!(r.as_array().unwrap().iter().all(|r| r["ok"] == true && !r["backup_path"].is_null()), "{r}");
        let again = s.call("slot_order_merge_preview", &args(json!({ "files": files }))).unwrap();
        assert!(again["files"].as_array().unwrap().iter().all(|f| f["gained"] == 0 && f["changed"] == 0), "{again}");

        let all = s.call("slot_order_merge_preview", &Args::new()).unwrap();
        let paths: Vec<&str> = all["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
        assert!(!paths.is_empty() && paths.iter().all(|p| p.contains("core_user_")), "{all}");
    }
```

Also add `"slot_order_get"` to the tool list in `every_get_result_carries_no_paths` (~line 3245).

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p app slot_order_`
Expected: the new MCP tests FAIL with `unknown_tool`. The ops and setup tests from Tasks 3 and 4 still pass.

- [ ] **Step 3: Implement the helpers and dispatch**

After `fleet_op`:

```rust
/// Ships with their racks as labels, top/middle/bottom, so the model never
/// needs the flag table.
fn slot_order_view(ships: &[settings_model::SlotEntry]) -> Value {
    let row = |o: &[u8]| o.iter().map(|&f| settings_model::slot_label(f)).collect::<Vec<_>>();
    json!({ "ships": ships.iter().map(|s| match &s.order {
        Some(o) => json!({ "ship_id": s.ship_id, "rows": { "top": row(&o[..8]), "middle": row(&o[8..16]), "bottom": row(&o[16..]) } }),
        None => json!({ "ship_id": s.ship_id, "unreadable": true }),
    }).collect::<Vec<_>>() })
}

fn parse_slot_arg(s: &str, key: &str) -> Result<u8, Value> {
    settings_model::parse_slot(s)
        .ok_or_else(|| err("bad_arguments", format!("`{key}`: {s:?} is not a slot. Use H1–H8, M1–M8, L1–L8 or a flag 11–34.")))
}

fn slot_order_op(state: &AppState, a: &Args) -> Result<(), Value> {
    let op: String = req(a, "op")?;
    match op.as_str() {
        "add" => ops::add_slot_order(state, req(a, "ship_id")?, opt(a, "from")?),
        "remove" => ops::remove_slot_order(state, req(a, "ship_id")?),
        "set" => {
            let order = req::<Vec<String>>(a, "order")?
                .iter()
                .map(|s| parse_slot_arg(s, "order"))
                .collect::<Result<Vec<u8>, Value>>()?;
            ops::set_slot_order(state, req(a, "ship_id")?, &order)
        }
        "swap" => {
            let (x, y) = (parse_slot_arg(&req::<String>(a, "a")?, "a")?, parse_slot_arg(&req::<String>(a, "b")?, "b")?);
            ops::swap_slot_order(state, req(a, "ship_id")?, x, y)
        }
        "copy" => ops::copy_slot_order(state, req(a, "from")?, &req::<Vec<u64>>(a, "to")?),
        _ => return Err(unknown_op(&op)),
    }
    .map(drop)
    .map_err(fail)
}
```

In `impl EveMcp`, next to `fleet_get`:

```rust
    fn slot_order_get(&self) -> ToolResult {
        Ok(slot_order_view(&ops::slot_orders(&self.state()).map_err(fail)?))
    }

    /// `files`, or every account file of the first profile folder (the one
    /// `list_characters` shows first).
    fn merge_files(&self, args: &Args) -> Result<Vec<String>, Value> {
        if let Some(f) = opt::<Vec<String>>(args, "files")? {
            return Ok(f);
        }
        let profiles = discover(&self.roots);
        let p = profiles.first().ok_or_else(|| err("no_profile", "No EVE settings folder was found."))?;
        Ok(p.files
            .iter()
            .filter(|f| f.kind == settings_model::FileKind::User)
            .map(|f| f.path.to_string_lossy().into_owned())
            .collect())
    }
```

In the dispatch `match`, after `"lookup_character" => {…}`:

```rust
            "slot_order_get" => self.slot_order_get(),
            "slot_order_edit" => self.batch(args, slot_order_op, |s| s.slot_order_get()),
            "slot_order_merge_preview" => ok(setup::slot_order_merge_preview(&self.roots, &self.merge_files(args)?)),
            "slot_order_merge_apply" => ok(setup::slot_order_merge_apply(&self.roots, &self.merge_files(args)?)),
```

In `written_paths`, change `"copy_apply" | "copy_files" => {` to `"copy_apply" | "copy_files" | "slot_order_merge_apply" => {`. Add `.filter(|r| !r["backup_path"].is_null())` after `.filter(|r| r["ok"] == true)`, because a merge result with no backup wrote nothing. A successful copy always has a backup, so copy behaviour is unchanged.

Add `"slot_order_merge_preview", "slot_order_merge_apply"` to `WORKSPACE_FREE`.

- [ ] **Step 4: Tool definitions**

After the `lookup_character` `ToolDef`:

```rust
        ToolDef {
            name: "slot_order_get",
            description: "The ship HUD's module arrangement per ship, from the account file: ships [{ship_id, rows: {top, middle, bottom}}], each row 8 slot labels in screen order (H1–H8 high, M1–M8 mid, L1–L8 low slots). A ship not listed uses EVE's default (top H1–H8, middle M1–M8, bottom L1–L8). ship_id is the ship's item id; the files hold no ship name. An entry in an unexpected shape is {ship_id, unreadable: true}. Needs the account file open. Unsure: eve_guide racks.",
            schema: || obj(json!({}), &[]),
        },
        ToolDef {
            name: "slot_order_edit",
            description: "Edit ship HUD arrangements as a batch (one undo step; first failure rolls back). Slots are strings: a label (H1–H8, M1–M8, L1–L8) or an inventory flag (\"27\"). Ops: add {ship_id, from?} (a new ship, starting from ship `from`'s arrangement or EVE's default; refused if the ship is already listed); remove {ship_id} (drop its arrangement, so EVE falls back to the default; refused if not listed); set {ship_id, order} (replace a listed ship's arrangement: 24 slots, top row left to right, then middle, then bottom, each slot exactly once); swap {ship_id, a, b} (exchange two slots' buttons, as dragging one onto the other in game); copy {from, to} (give every ship in `to` the arrangement of `from`, adding or overwriting). Returns slot_order_get's shape. Nothing reaches disk until save.",
            schema: || obj(op_item(&["add", "remove", "set", "swap", "copy"], json!({
                "ship_id": { "type": "integer" }, "from": { "type": "integer" },
                "order": { "type": "array", "items": { "type": "string" }, "minItems": 24, "maxItems": 24 },
                "a": { "type": "string" }, "b": { "type": "string" },
                "to": { "type": "array", "items": { "type": "integer" }, "minItems": 1 }
            })), &["ops"]),
        },
        ToolDef {
            name: "slot_order_merge_preview",
            description: "Plan merging ship HUD arrangements across account files without changing anything: every ship arranged in any of `files` ends up in all of them; where files disagree on a ship, the most recently modified file wins. Returns {files: [{path, account_id, gained, changed, error}], conflicts: [{ship_id, kept_from, overridden}]}. files: account file paths (core_user_*.dat, list_characters' user_file); omitted, every account file in the first profile folder. ALWAYS call this before slot_order_merge_apply and show the user the plan.",
            schema: || obj(json!({ "files": { "type": "array", "items": { "type": "string" } } }), &[]),
        },
        ToolDef {
            name: "slot_order_merge_apply",
            description: "Perform the merge slot_order_merge_preview planned, with the same arguments. WRITES TO DISK IMMEDIATELY: each file that changes is backed up, then written; returns [{path, ok, backup_path, error}] — backup_path null with ok true means that file needed no change. Only for accounts whose characters are all logged out; reopen a file afterwards if it was open here.",
            schema: || obj(json!({ "files": { "type": "array", "items": { "type": "string" } } }), &[]),
        },
```

- [ ] **Step 5: The guide**

In `mcp.rs`:
- Change `TOPICS` to `[&str; 9]` and add `"racks"` before `"copy"`.
- In the `eve_guide` description, add `racks (ship HUD slot arrangements, labels, merging across accounts), ` before `copy (`.

In `mcp_primer.md`:
- On line 19, change `**chat** splits (`chat_get`/`chat_set_splits`) — account file` to `**chat** splits (`chat_get`/`chat_set_splits`) and **ship HUD racks** (`slot_order_get`/`slot_order_edit`) — account file`.
- In the same sentence, change `**copy settings** (`copy_preview`, then `copy_apply`; `copy_files` for a whole file)` to `**copy settings** (`copy_preview`, then `copy_apply`; `copy_files` for a whole file), **merging ship racks** across account files (`slot_order_merge_preview`, then `slot_order_merge_apply`)`.
- Before `## copy`, add:

```markdown
## racks

EVE remembers how the modules on the ship HUD are arranged per ship: per hull, by the ship's item id, not per ship type. It is the account file's `slotOrder`, shared by every character on the account. Each ship is 24 buttons in three rows: top, middle (drawn half a button to the right), bottom, 8 each. Each button shows one fitted slot: H1–H8 high slots (inventory flags 27–34), M1–M8 mid (19–26), L1–L8 low (11–18). A ship with no entry uses the default: high slots on top, mid in the middle, low at the bottom. Dragging a module in game swaps two slots; `slot_order_edit` swap does the same. Removing a ship's entry resets it to the default. The files never name a ship; recognise one by its id or its arrangement. `slot_order_merge_preview` / `slot_order_merge_apply` give every ship arranged on any of the chosen accounts to all of them, the most recently modified file winning where they disagree. They write immediately, like `copy_apply`.
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p app mcp`
Expected: PASS, including `schema_invariants_hold_for_every_tool`, the primer/topics tests and `every_get_result_carries_no_paths`. If a primer test pins the topic count or section order, update it to the new nine-topic list. Never delete the check.

- [ ] **Step 7: Commit**

```bash
git add app/src-tauri/src/mcp.rs app/src-tauri/src/mcp_primer.md
git commit -m "mcp: ship HUD slot order tools and the racks guide"
```

---

### Task 6: The Racks view

**Files:**
- Create: `app/src/lib/RacksView.svelte`
- Create: `app/src/lib/RacksView.spec.ts`
- Modify: `app/src/lib/views.ts`: add `"racks"` to `View`, and `{ id: "racks", label: "Racks" }` after Fleet in `VIEWS`.
- Modify: `app/src/lib/keymap.ts:82-83`: `"7": "go.racks"`, `"8": "go.raw"`.
- Modify: `app/src/routes/+page.svelte`: import, `ACCOUNT_SCOPED` (line 124), and a render branch after the fleet branch (~line 693).
- Modify these tests, whose pinned view lists gain Racks:
  - `app/src/lib/keymap.spec.ts:48-56`
  - `app/src/lib/ViewTabs.spec.ts:33-34`
  - `app/src/routes/page.spec.ts:490`
  - `app/src/lib/commands.spec.ts:33` (add `"Racks"` to `PROPER`)

**Interfaces:**
- Consumes: Task 3's `api.slotOrders`, `addSlotOrder`, `removeSlotOrder`, `swapSlots`, `copySlotOrder` and `SlotEntry`.
- Produces: `RacksView` with props `{ userOpen: boolean; userId?: number | null; refreshToken?: number; onUserDirty: () => void }`.

- [ ] **Step 1: Write the failing spec**

Create `app/src/lib/RacksView.spec.ts`:

```ts
// Component test: run with `npm test` (vitest + jsdom).
import { describe, expect, test } from "vitest";
import { render, fireEvent, screen, waitFor, within } from "@testing-library/svelte";
import RacksView from "$lib/RacksView.svelte";
import { calls } from "$lib/test/setup";
import type { SlotEntry } from "$lib/api";

const DEFAULT = [27, 28, 29, 30, 31, 32, 33, 34, 19, 20, 21, 22, 23, 24, 25, 26, 11, 12, 13, 14, 15, 16, 17, 18];
const SHIPS: SlotEntry[] = [
  { ship_id: 90000001, order: DEFAULT },
  { ship_id: 90000002, order: null },
];

function mount(ships: SlotEntry[] = SHIPS, dirty: () => void = () => {}) {
  for (const cmd of ["slot_orders", "slot_order_add", "slot_order_remove", "slot_order_swap", "slot_order_copy"]) {
    calls.stub(cmd, ships);
  }
  return render(RacksView, { userOpen: true, userId: 1, onUserDirty: dirty });
}

const rack = (name: string) => screen.getByRole("group", { name });

describe("the racks view", () => {
  test("lists every ship and draws the selected one's three racks by label", async () => {
    mount();
    await fireEvent.click(await screen.findByText("90000001"));
    expect(within(rack("Top rack")).getAllByRole("button").map((b) => b.textContent)).toEqual(
      ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8"]);
    expect(within(rack("Middle rack")).getAllByRole("button")[0].textContent).toBe("M1");
    expect(within(rack("Bottom rack")).getAllByRole("button")[7].textContent).toBe("L8");
  });

  test("clicking two slots swaps them by flag and marks the account dirty", async () => {
    let dirty = 0;
    mount(SHIPS, () => dirty++);
    await fireEvent.click(await screen.findByText("90000001"));
    await fireEvent.click(within(rack("Top rack")).getByText("H1"));
    await fireEvent.click(within(rack("Middle rack")).getByText("M1"));
    await waitFor(() => expect(calls.of("slot_order_swap")).toHaveLength(1));
    expect(calls.of("slot_order_swap")[0].args).toEqual({ shipId: 90000001, a: 27, b: 19 });
    expect(dirty).toBe(1);
  });

  test("dropping one slot on another swaps them", async () => {
    mount();
    await fireEvent.click(await screen.findByText("90000001"));
    const data = new Map<string, string>();
    const dataTransfer = { setData: (k: string, v: string) => data.set(k, v), getData: (k: string) => data.get(k) ?? "" };
    await fireEvent.dragStart(within(rack("Top rack")).getByText("H2"), { dataTransfer });
    await fireEvent.drop(within(rack("Bottom rack")).getByText("L1"), { dataTransfer });
    await waitFor(() => expect(calls.of("slot_order_swap")[0]?.args).toEqual({ shipId: 90000001, a: 28, b: 11 }));
  });

  test("an unreadable ship says so and can only be removed", async () => {
    mount();
    await fireEvent.click(await screen.findByText(/90000002/));
    expect(screen.getByText(/shape the editor doesn't read/)).toBeTruthy();
    expect(screen.queryByRole("group", { name: "Top rack" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Remove ship" }));
    await waitFor(() => expect(calls.of("slot_order_remove")[0]?.args).toEqual({ shipId: 90000002 }));
  });

  test("add starts from the selected ship and refuses an id already listed", async () => {
    mount();
    await fireEvent.click(await screen.findByText("90000001"));
    const id = screen.getByLabelText("Ship id") as HTMLInputElement;
    await fireEvent.input(id, { target: { value: "90000001" } });
    expect((screen.getByRole("button", { name: "Add ship" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.input(id, { target: { value: "90000005" } });
    await fireEvent.click(screen.getByRole("button", { name: "Add ship" }));
    await waitFor(() => expect(calls.of("slot_order_add")[0]?.args).toEqual({ shipId: 90000005, from: 90000001 }));
  });

  test("copy sends the ticked ships", async () => {
    mount([...SHIPS, { ship_id: 90000003, order: DEFAULT }]);
    await fireEvent.click(await screen.findByText("90000001"));
    await fireEvent.click(screen.getByLabelText("Copy to 90000003"));
    await fireEvent.click(screen.getByRole("button", { name: "Copy layout" }));
    await waitFor(() => expect(calls.of("slot_order_copy")[0]?.args).toEqual({ from: 90000001, to: [90000003] }));
  });

  test("a refused write shows the backend's sentence", async () => {
    mount();
    calls.stub("slot_order_swap", () => Promise.reject({ code: "missing", message: "Ship 90000001 has no layout here." }));
    await fireEvent.click(await screen.findByText("90000001"));
    await fireEvent.click(within(rack("Top rack")).getByText("H1"));
    await fireEvent.click(within(rack("Top rack")).getByText("H2"));
    expect(await screen.findByText(/has no layout here/)).toBeTruthy();
  });
});
```

The rejection stub relies on how `calls.stub` handles a function value; read `app/src/lib/test/setup.ts:60` first. If a returned rejected promise doesn't propagate as a rejection, follow the pattern the existing specs use for errors (grep `Promise.reject` in `*.spec.ts`).

- [ ] **Step 2: Run it to see it fail**

Run: `cd app && npx vitest run src/lib/RacksView.spec.ts`
Expected: FAIL, because `RacksView.svelte` doesn't exist.

- [ ] **Step 3: Write the component**

Create `app/src/lib/RacksView.svelte`:

```svelte
<script lang="ts">
  import { api, errMessage, errText, type SlotEntry } from "./api";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";
  import SearchField from "./ui/SearchField.svelte";

  let { userOpen, userId = null, refreshToken = 0, onUserDirty }: {
    userOpen: boolean;
    userId?: number | null;
    /** Bumped by every save, open, discard, restore and undo (see FleetView). */
    refreshToken?: number;
    onUserDirty: () => void;
  } = $props();

  /** Inventory flag → the name players use: high H1–H8, mid M1–M8, low L1–L8. */
  const slotLabel = (f: number) => (f >= 27 ? `H${f - 26}` : f >= 19 ? `M${f - 18}` : `L${f - 10}`);
  const rackOf = (f: number) => (f >= 27 ? "high" : f >= 19 ? "mid" : "low");
  // Position p is row p/8: the client's InitDrawSlots, spec §2.2.
  const ROWS = [
    { name: "Top rack", from: 0 },
    { name: "Middle rack", from: 8 },
    { name: "Bottom rack", from: 16 },
  ];

  let ships = $state<SlotEntry[] | null>(null);
  let loadError = $state<string | null>(null);
  let actionError = $state<{ text: string; detail: string } | null>(null);
  let selectedId = $state<number | null>(null);
  let filter = $state("");
  /** The slot clicked first, waiting for the slot to swap it with. */
  let picked = $state<number | null>(null);
  let newId = $state("");
  let copyTo = $state<Set<number>>(new Set());

  async function reload() {
    if (!userOpen) { ships = null; return; }
    loadError = null;
    try { ships = await api.slotOrders(); } catch (e) { loadError = errMessage(e); }
  }
  $effect(() => { void userOpen; void userId; void refreshToken; reload(); });

  const shown = $derived((ships ?? []).filter((s) => String(s.ship_id).includes(filter.trim())));
  const current = $derived(ships?.find((s) => s.ship_id === selectedId) ?? null);
  // A different ship starts with nothing picked and nothing ticked.
  $effect(() => { void selectedId; picked = null; copyTo = new Set(); });

  async function write(subject: string, fn: () => Promise<SlotEntry[]>): Promise<boolean> {
    actionError = null;
    try {
      ships = await fn();
      onUserDirty();
      return true;
    } catch (e) {
      actionError = { text: `${subject} — ${errText(e)}`, detail: errMessage(e) };
      return false;
    }
  }

  function swap(a: number, b: number) {
    picked = null;
    const ship = current;
    if (ship && a !== b) void write("Those slots weren't swapped", () => api.swapSlots(ship.ship_id, a, b));
  }
  function pick(flag: number) {
    if (picked === null) picked = flag;
    else swap(picked, flag);
  }

  const parsedNewId = $derived(/^\d+$/.test(newId.trim()) ? Number(newId.trim()) : null);
  const newIdTaken = $derived(parsedNewId !== null && !!ships?.some((s) => s.ship_id === parsedNewId));
  async function add() {
    if (parsedNewId === null || newIdTaken) return;
    const id = parsedNewId;
    const from = current?.order ? current.ship_id : null;
    if (await write(`Ship ${id} wasn't added`, () => api.addSlotOrder(id, from))) {
      selectedId = id;
      newId = "";
    }
  }
  async function remove() {
    if (!current) return;
    const id = current.ship_id;
    if (await write(`Ship ${id} wasn't removed`, () => api.removeSlotOrder(id))) selectedId = null;
  }
  function toggleCopy(id: number) {
    const next = new Set(copyTo);
    next.has(id) ? next.delete(id) : next.add(id);
    copyTo = next;
  }
  async function copy() {
    if (!current || copyTo.size === 0) return;
    const from = current.ship_id;
    if (await write("That layout wasn't copied", () => api.copySlotOrder(from, [...copyTo]))) copyTo = new Set();
  }
</script>

<div class="racks">
  {#if !userOpen}
    <EmptyState title="Open an account file to edit its ship layouts." />
  {:else if loadError}
    <InlineMessage variant="error">{loadError}</InlineMessage>
  {:else if ships}
    <div class="cols">
      <div class="list">
        <SearchField verb="filter" nouns="ship ids" bind:value={filter} count={shown.length} total={ships.length} />
        {#if ships.length === 0}
          <EmptyState
            title="No ship has a saved layout yet."
            description="EVE saves one the first time you drag a module on a ship's HUD." />
        {/if}
        {#each shown as s (s.ship_id)}
          <ListRow selected={s.ship_id === selectedId} onclick={() => (selectedId = s.ship_id)}>
            {s.ship_id}{#if !s.order}<span class="muted"> · unreadable</span>{/if}
          </ListRow>
        {/each}
        <div class="add">
          <Field kind="text" label="Ship id" layout="column" bind:value={newId} />
          <Button
            size="sm"
            disabled={parsedNewId === null || newIdTaken}
            disabledReason={newIdTaken ? "That ship is already listed" : "Type a ship's item id"}
            onclick={add}>Add ship</Button>
        </div>
      </div>

      <div class="editor">
        {#if !current}
          <EmptyState title="Pick a ship to see its racks." />
        {:else}
          <h3>Ship {current.ship_id}</h3>
          {#if current.order}
            {@const order = current.order}
            {#each ROWS as row, r}
              <div class="row" class:middle={r === 1} role="group" aria-label={row.name}>
                {#each order.slice(row.from, row.from + 8) as flag (flag)}
                  <button
                    type="button"
                    class="slot {rackOf(flag)}"
                    class:picked={picked === flag}
                    aria-pressed={picked === flag}
                    draggable="true"
                    ondragstart={(e) => e.dataTransfer?.setData("text/plain", String(flag))}
                    ondragover={(e) => e.preventDefault()}
                    ondrop={(e) => {
                      e.preventDefault();
                      const from = Number(e.dataTransfer?.getData("text/plain"));
                      if (from) swap(from, flag);
                    }}
                    onclick={() => pick(flag)}>{slotLabel(flag)}</button>
                {/each}
              </div>
            {/each}
            <p class="muted">Click two slots, or drag one onto another, to swap them, as you would on the HUD in game.</p>
          {:else}
            <InlineMessage variant="warn">This ship's layout is in a shape the editor doesn't read. Remove it to give the ship EVE's default layout.</InlineMessage>
          {/if}
          <div class="actions">
            <Button variant="danger" size="sm" onclick={remove}>Remove ship</Button>
          </div>
          {#if current.order && ships.length > 1}
            <div class="copy">
              <div class="head">Copy this layout to</div>
              {#each shown.filter((s) => s.ship_id !== current.ship_id) as s (s.ship_id)}
                <Field
                  kind="checkbox"
                  label={`Copy to ${s.ship_id}`}
                  value={copyTo.has(s.ship_id)}
                  onchange={() => toggleCopy(s.ship_id)} />
              {/each}
              <Button
                size="sm"
                disabled={copyTo.size === 0}
                disabledReason="Tick at least one ship"
                onclick={copy}>Copy layout</Button>
            </div>
          {/if}
        {/if}
        {#if actionError}
          <InlineMessage variant="error" detail={actionError.detail}>{actionError.text}</InlineMessage>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .racks { max-width: 64rem; }
  .cols { display: grid; grid-template-columns: minmax(12rem, 18rem) 1fr; gap: var(--s4); }
  .list, .editor { display: flex; flex-direction: column; gap: var(--s2); min-width: 0; }
  .add { display: flex; gap: var(--s2); align-items: end; }
  .row { display: flex; gap: var(--s1); }
  /* The client draws the middle rack half a button to the right (grid x 1.5). */
  .row.middle { padding-left: calc((2.75rem + var(--s1)) / 2); }
  .slot {
    width: 2.75rem; height: 2.75rem; border-radius: 50%;
    border: 1px solid var(--border); background: var(--surface-2); color: var(--text);
    font: inherit; cursor: grab;
  }
  .slot.high { border-color: var(--accent); }
  .slot.low { border-style: dashed; }
  .slot.picked { outline: 2px solid var(--accent); outline-offset: 2px; }
  .actions, .copy { display: flex; flex-direction: column; gap: var(--s1); align-items: start; }
  .head { font-weight: 600; }
  .muted { color: var(--text-muted); }
  @media (max-width: 40rem) { .cols { grid-template-columns: 1fr; } }
</style>
```

Check the token names against `app/src/app.css` before you commit. Use the existing tokens for border, surface and text; if `--surface-2` or `--border` is spelled differently there, use the existing name. Never add a hex literal, per `docs/ui-redesign/`. Memory note: check alignment by eye. Automated checks are blind to it.

- [ ] **Step 4: Run the spec**

Run: `cd app && npx vitest run src/lib/RacksView.spec.ts`
Expected: all 7 PASS.

- [ ] **Step 5: Wire it into the shell**

- `views.ts`:
  - The `View` union becomes `"layout" | "overview" | "autofill" | "keybinds" | "probes" | "fleet" | "racks" | "raw"`.
  - Add `{ id: "racks", label: "Racks" }` after the Fleet entry in `VIEWS`.
  - Update the header comment's view count, which says "six".
  - `viewAvailable` needs no change: the generic "Open a character or an account file." rule applies, as for Fleet.
- `keymap.ts`: `"7": "go.racks"` and `"8": "go.raw"`.
- `+page.svelte`:
  - Add `import RacksView from "$lib/RacksView.svelte";` next to the FleetView import.
  - `ACCOUNT_SCOPED` becomes `["overview", "autofill", "keybinds", "probes", "fleet", "racks"]`.
  - After the fleet branch, add:

```svelte
      {:else if view === "racks"}
        <div class="scroll">
          <RacksView
            userOpen={subject.slots.user?.status === "opened"}
            refreshToken={subject.savedAt}
            userId={subject.userId}
            onUserDirty={() => { subject.dirty.user = true; noteEdit(); }} />
        </div>
```

- Pinned lists:
  - `keymap.spec.ts`: add `["7", "go.racks"]`, and change the raw row to `["8", "go.raw"]`. The ids array gets `"go.racks"` before `"go.raw"`.
  - `ViewTabs.spec.ts:33`: `"Racks"` goes before `"Raw"`. Add it to the loop list on line 34 too.
  - `page.spec.ts:490`: `"Racks"` goes before `"Raw"`.
  - `commands.spec.ts:33`: add `"Racks"` to `PROPER`.

- [ ] **Step 6: Run the frontend suite**

Run: `cd app && npm test`
Expected: the summary shows 0 failed. Judge by the failure count in the summary, not the exit code alone (see Global Constraints).

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/RacksView.svelte app/src/lib/RacksView.spec.ts app/src/lib/views.ts app/src/lib/keymap.ts app/src/routes/+page.svelte app/src/lib/keymap.spec.ts app/src/lib/ViewTabs.spec.ts app/src/routes/page.spec.ts app/src/lib/commands.spec.ts
git commit -m "racks: a view to edit each ship's HUD slot layout"
```

---

### Task 7: The batch view's merge source

**Files:**
- Modify: `app/src/lib/BatchView.svelte`
- Modify: `app/src/lib/BatchView.spec.ts`

**Interfaces:**
- Consumes: Task 4's `api.slotOrderMergePreview`, `api.slotOrderMergeApply` and `SlotMergePlan`.
- Produces: a fourth radio, "Ship slot layouts, merged across the profile" (`sourceKind === "racks"`).

- [ ] **Step 1: Write the failing spec**

Append to `BatchView.spec.ts`:

```ts
describe("merging ship slot layouts", () => {
  const MERGE_PLAN = {
    files: [
      { path: `${DIR}/core_user_80000001.dat`, account_id: 80000001, gained: 2, changed: 0, error: null },
      { path: `${DIR}/core_user_80000002.dat`, account_id: 80000002, gained: 0, changed: 0, error: null },
    ],
    conflicts: [{ ship_id: 90000501, kept_from: `${DIR}/core_user_80000002.dat`, overridden: [`${DIR}/core_user_80000001.dat`] }],
  };

  async function mountRacks() {
    calls.stub("slot_order_merge_preview", MERGE_PLAN);
    calls.stub("slot_order_merge_apply", [
      { path: `${DIR}/core_user_80000001.dat`, ok: true, backup_path: "b", error: null },
      { path: `${DIR}/core_user_80000002.dat`, ok: true, backup_path: null, error: null },
    ]);
    await mount();
    await fireEvent.click(screen.getByLabelText("Ship slot layouts, merged across the profile"));
  }

  const accountBox = (id: number) =>
    rowIn("Account files", `core_user_${id}.dat`).querySelector("input")! as HTMLInputElement;

  test("lists only the profile's account files, all ticked, and previews the merge", async () => {
    await mountRacks();
    await waitFor(() => expect(accountBox(80000001).checked).toBe(true));
    expect(accountBox(80000002).checked).toBe(true);
    expect(() => rowIn("Account files", "core_char_")).toThrow();
    await waitFor(() => expect(calls.of("slot_order_merge_preview").length).toBeGreaterThan(0));
    expect(await screen.findByText(/gains 2 ships/)).toBeTruthy();
    expect(screen.getByText(/no change/)).toBeTruthy();
    expect(screen.getByText(/90000501/)).toBeTruthy();
  });

  test("an unticked file is left out of the merge and the write", async () => {
    await mountRacks();
    await waitFor(() => expect(accountBox(80000002).checked).toBe(true));
    await fireEvent.click(accountBox(80000002));
    await waitFor(() =>
      expect(calls.of("slot_order_merge_preview").at(-1)!.args).toEqual({ files: [`${DIR}/core_user_80000001.dat`] }));
    await fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await waitFor(() =>
      expect(calls.of("slot_order_merge_apply")[0]?.args).toEqual({ files: [`${DIR}/core_user_80000001.dat`] }));
  });
});
```

- [ ] **Step 2: Run it to see it fail**

Run: `cd app && npx vitest run src/lib/BatchView.spec.ts`
Expected: the new tests FAIL, because there is no "Ship slot layouts, merged across the profile" radio.

- [ ] **Step 3: Implement**

In `BatchView.svelte`:

1. Imports: add `type SlotMergePlan` to the `./api` import.
2. Source kind: `let sourceKind = $state<"character" | "preset" | "file" | "racks">("character");`. After `const fileMode = …`, add `const racksMode = $derived(sourceKind === "racks");`.
3. The profile's account files, which are both the merge's sources and its targets:

```ts
  // The rack merge reads and writes the same ticked list: a ticked account file
  // contributes its ships and receives everyone's; an unticked one is untouched.
  // Its own folder only — the merge is a per-profile operation.
  const accountFiles = $derived(
    profiles
      .filter((p) => p.dir === folder)
      .flatMap((p) => p.files.filter((f) => f.kind === "user").map((f) => ({ ...f, dir: p.dir })))
      .sort(byResolvedName),
  );
  // Default: every account file ticked. Re-ticks when the folder changes or
  // discovery lands, which is when the list itself changes.
  $effect(() => {
    if (racksMode) selectedTargets = new Set(accountFiles.map((f) => f.path));
  });
```

   This effect must be declared **after** the existing reset effect (the one that clears `selectedTargets` on a `sourceKind` change), so it runs after it in the same flush.
4. `candidates`: in front of the `fileMode ? …` chain, add `racksMode ? accountFiles : …`.
5. `targetDisabled`: change the rule to `!fileMode && !racksMode && anyAccountAspect && …`.
6. `effectiveTargets`: change `fileMode ? [...selectedTargets]` to `fileMode || racksMode ? [...selectedTargets]`.
7. `willWrite`: change `fileMode ? effectiveTargets` to `fileMode || racksMode ? effectiveTargets`.
8. The merge plan:

```ts
  let mergePlan = $state<SlotMergePlan | null>(null);
  let mergeSeq = 0;
  $effect(() => {
    const files = effectiveTargets;
    if (!racksMode || files.length === 0) { mergePlan = null; return; }
    const seq = ++mergeSeq;
    api.slotOrderMergePreview(files)
      .then((p) => { if (seq === mergeSeq) mergePlan = p; })
      .catch(() => { if (seq === mergeSeq) mergePlan = null; });
  });
  const mergeWrites = $derived(mergePlan?.files.filter((f) => !f.error && f.gained + f.changed > 0).length ?? 0);
  const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;
```

   In the existing preview effect, change `if (fileMode || !src …` to `if (fileMode || racksMode || !src …`.
9. `canApply`: put `racksMode ? mergeWrites > 0 && !busy : …` in front of the `fileMode ? …` chain.
10. `apply()`: add a first branch:

```ts
      if (racksMode) {
        results = await api.slotOrderMergeApply(effectiveTargets);
      } else if (fileMode) {
```

    Change the `onApplied` filter to `results.filter((r) => r.ok && r.backup_path !== null)`. A merge row with no backup wrote nothing, and every successful copy has a backup, so the copy flows are unaffected.
11. Markup:
    - **Radio:** after the "A file, copied as-is" radio, add `<Field kind="radio" name="sourceKind" bind:value={sourceKind} radioValue="racks" label="Ship slot layouts, merged across the profile" />`.
    - **Explanation:** in the source `{#if}` chain, add a `{:else if sourceKind === "racks"}` branch before the final `{:else}`:

```svelte
    {:else if sourceKind === "racks"}
      <InlineMessage>
        Every ship arranged on any ticked account ends up on all of them. Where two accounts arrange
        the same ship differently, the most recently saved account file wins.
      </InlineMessage>
```

    - **Content gate:** the `{#if fileMode ? !!sourceFile : !!batchSource}` gate becomes `{#if racksMode || (fileMode ? !!sourceFile : !!batchSource)}`.
    - **Aspects:** the "What to copy" section's `{#if !fileMode}` becomes `{#if !fileMode && !racksMode}`.
    - **Targets head:** the text becomes `{racksMode ? "Account files" : fileMode ? "Copy onto" : "Target characters"}`. Wrap the "Show other folders" checkbox in `{#if !racksMode}`.
    - **Empty state:** the title becomes `racksMode ? "No account files in this profile." : fileMode ? … : …`.
    - **Preview:** before `{#if fileMode}`, add a racks branch, then change `{#if fileMode}` to `{:else if fileMode}`:

```svelte
    {#if racksMode}
      {#if mergePlan}
        <section class="preview">
          <p>Will write {mergeWrites} file(s) — each is backed up first.</p>
          {#each mergePlan.files as f}
            <p class:muted={!f.error && f.gained + f.changed === 0}>
              {f.account_id !== null ? `Account ${accountLabel(f.account_id)}` : fileName(f.path)}:
              {#if f.error}{f.error}
              {:else if f.gained + f.changed === 0}no change
              {:else}gains {f.gained} ship{f.gained === 1 ? "" : "s"}, {f.changed} changed{/if}
            </p>
          {/each}
          {#each mergePlan.conflicts as c}
            <InlineMessage variant="warn">⚠ Ship {c.ship_id}: kept the layout from {fileName(c.kept_from)} (newest), replacing it in {c.overridden.map(fileName).join(", ")}.</InlineMessage>
          {/each}
        </section>
      {/if}
    {:else if fileMode}
```

    - **Button:** the label becomes `{busy ? (racksMode ? "Merging…" : "Copying…") : racksMode ? "Merge" : "Copy"}`. The disabled reason becomes `racksMode ? "Nothing to merge — every ticked account already has every ship" : …` when not busy.
    - **Subtitle:** `racksMode ? "Merge ship slot layouts across accounts" : fileMode ? … : …`.

- [ ] **Step 4: Run the spec**

Run: `cd app && npx vitest run src/lib/BatchView.spec.ts`
Expected: every test PASS, the old ones included. They pin the character, preset and file flows that this task must not change.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/BatchView.svelte app/src/lib/BatchView.spec.ts
git commit -m "batch: merge ship slot layouts across a profile's account files"
```

---

### Task 8: Docs, full verification, in-game check

**Files:**
- Modify: `docs/format-notes.md`: a new `### Ship slot order (2026-10-02)` section after the fleet section (~line 1784 onward, at the end of the file).
- Modify: `docs/settings-field-reference.md:550`: point `slotOrder` at it.

- [ ] **Step 1: format-notes section**

Append to `docs/format-notes.md`:

```markdown
### Ship slot order (2026-10-02)

`ui -> slotOrder`, account file, `(FILETIME, {shipItemID: [24 Int]})`. Read from
the client code (`eve/client/script/ui/inflight/shipHud/__init__.py`,
`slotsContainer.py`, decoded from `code.ccp`) and measured over 157 account
files (21,720 entries, every one 24 flags from 11–34):

- The key is `session.shipid`, the ship's item id, so a layout belongs to one
  hull, not a ship type. Ids are ~1e12, so they are `Long` keys on the wire.
- The value is a permutation of the 24 rack flags: low 11–18, mid 19–26, high
  27–34. `InitDrawSlots` reads `myOrder[r * 8 + i]`, so position `p` is row
  `p // 8` (grid `[[1.0, 0.0], [1.5, 1.0], [1.0, 2.0]]`: top, middle staggered
  half a button, bottom) and column `p % 8`.
- `GetSlotOrder` falls back to `[Hi0..7, Med0..7, Lo0..7]` when the ship has
  no entry. `SwapSlots(flag1, flag2)` is the only writer and nothing ever
  deletes an entry, so the map grows with every ship ever rearranged.
- A short list raises in `InitDrawSlots` and a missing flag raises in
  `SwapSlots`. The editor writes full permutations only.

Edited by `crates/settings-model/src/slot_order.rs`; spec
`docs/superpowers/specs/2026-10-02-slot-order-design.md`.
```

In `settings-field-reference.md:550`, change `` `slotOrder`, `` to `` `slotOrder` (per-ship HUD slot layout — format-notes "Ship slot order"), ``.

- [ ] **Step 2: Full verification**

Run, judging each by its exit code and summary line:
- `cargo test --workspace`. Run it in the background; it is slow.
- `cd app && npm test`. Read the summary line: 0 failed.
- `cargo clippy --workspace -- -D warnings`, if the repo's CI runs clippy (check `.github/workflows`).

Expected: all green. Fix anything red before going on.

- [ ] **Step 3: In the app (starting-the-app skill)**

Use the `starting-the-app` skill to launch the app, then:
- open an account file and switch to Racks;
- pick a ship, swap two slots and check the racks redraw;
- undo, then save;
- open Copy settings and choose "Ship slot layouts, merged across the profile". Read the preview and **don't apply against real files** without the user.

Take a screenshot with PrintWindow (memory note). Check the middle rack's stagger and the button alignment by eye.

- [ ] **Step 4: Commit**

```bash
git add docs/format-notes.md docs/settings-field-reference.md
git commit -m "docs: ship slot order format notes"
```

- [ ] **Step 5: Hand the in-game check to the user**

The definition of done includes an in-game check, which only the user can run:
1. With every character on account A logged out, rearrange a ship's HUD on account A in game. Log out.
2. Merge the profile in the app.
3. Board the same ship on account B and check that its HUD shows the same arrangement.

Report the result. Don't claim it passed without the user's word.
