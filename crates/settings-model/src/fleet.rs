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

use crate::hud::{project_fields, section_dict_mut, set_field, Field, HudEntry, HudError, HudKind, HudScope};
use crate::treewalk::{
    as_dict, collect_shared, dict_inner_mut, effective, find_child, inline_all, is_bytes, section, Entries,
    SharedTable,
};

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
        colours: project_colours(user_root),
        watchlist: project_watchlist(char_root),
        palette: PALETTE.iter().map(|(n, c)| (n.to_string(), *c)).collect(),
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
}
