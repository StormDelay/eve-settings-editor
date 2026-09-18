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
        #[allow(dead_code)] // wired up by Tasks 3 and 6
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
