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
