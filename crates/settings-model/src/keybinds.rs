//! Read + edit projection of the keybinding table. All of it lives in
//! `core_user` under `cmd -> customCmds -> (timestamp, dict)`, mapping a command
//! name (Bytes) to either `None` (unbound) or a tuple of Windows virtual-key
//! codes. See docs/format-notes.md, "Keybindings".
//!
//! TWO FORMAT TRAPS, both corpus-verified over 12,117 bindings:
//!   1. The `(timestamp, value)` wrapper is on `customCmds`, NOT on the leaves.
//!      A leaf is a bare `Tuple(Int, ..)`. Wrapping one produces a malformed
//!      value the client ignores while keeping its stale binding.
//!   2. The root `cmd` key can be a `Ref`/`Shared` like its siblings, so the
//!      lookup resolves through `effective` rather than matching Bytes.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{as_dict, bytes_str, collect_shared, effective, find_child, SharedTable};

/// Modifier virtual-key codes, in the canonical order EVE writes them.
pub const MOD_CTRL: i64 = 17;
pub const MOD_ALT: i64 = 18;
pub const MOD_SHIFT: i64 = 16;
pub(crate) const MODIFIERS: [i64; 3] = [MOD_CTRL, MOD_ALT, MOD_SHIFT];

#[derive(Debug, PartialEq, Serialize)]
pub struct KeybindEntry {
    pub command: String,
    /// `None` = unbound. Otherwise `[17?, 18?, 16?, key]`.
    pub keys: Option<Vec<i64>>,
    /// The stored value is neither `None` nor an all-`Int` tuple. Projected as
    /// `keys: None` so the row reads honestly instead of silently blank; the
    /// raw value survives save untouched unless the user rebinds the row.
    pub malformed: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Keybinds {
    pub entries: Vec<KeybindEntry>,
    /// False when there is no account file, no `cmd -> customCmds`, or the
    /// table is empty — the last is a real state for an account that has never
    /// opened the in-game keybinding screen (spec §2.5).
    pub available: bool,
}

pub fn project_keybinds(user: Option<&Value>) -> Keybinds {
    let empty = Keybinds { entries: Vec::new(), available: false };
    let Some(user) = user else { return empty };

    let mut sh = SharedTable::new();
    collect_shared(user, &mut sh);
    let Value::Dict(root) = effective(user, &sh) else { return empty };
    let Some(cmd) = find_child(root, b"cmd", &sh).and_then(|v| as_dict(v, &sh)) else { return empty };
    let Some(table) = find_child(cmd, b"customCmds", &sh).and_then(|v| as_dict(v, &sh)) else { return empty };

    let entries: Vec<KeybindEntry> = table
        .iter()
        .filter_map(|(k, v)| {
            let command = bytes_str(effective(k, &sh))?;
            let (keys, malformed) = read_binding(effective(v, &sh), &sh);
            Some(KeybindEntry { command, keys, malformed })
        })
        .collect();

    let available = !entries.is_empty();
    Keybinds { entries, available }
}

/// Values are reported exactly as stored — no re-canonicalisation. The corpus
/// is already canonical; if a file is not, showing the truth beats lying.
fn read_binding(v: &Value, sh: &SharedTable) -> (Option<Vec<i64>>, bool) {
    match v {
        Value::None => (None, false),
        Value::Tuple(items) => {
            let mut codes = Vec::with_capacity(items.len());
            for e in items {
                match effective(e, sh) {
                    Value::Int(i) => codes.push(*i),
                    _ => return (None, true),
                }
            }
            if codes.is_empty() { (None, true) } else { (Some(codes), false) }
        }
        _ => (None, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }
    fn codes(v: &[i64]) -> Value { Value::Tuple(v.iter().map(|&n| Value::Int(n)).collect()) }

    /// root -> b"cmd" -> b"customCmds" -> (ts, { command: value }).
    /// `cmd` is a BARE dict and the leaves are BARE tuples — corpus-verified
    /// (spec §2.1). Only `customCmds` carries the timestamp.
    fn user_with_binds() -> Value {
        let table = Value::Dict(vec![
            (b("CmdActivateHighPowerSlot1"), codes(&[81])),
            (b("CmdActivateMediumPowerSlot1"), codes(&[17, 83])),
            (b("CmdToggleAutopilot"), Value::None),
            (b("CmdDronesEngage"), codes(&[18, 16, 68])),
        ]);
        let cmd = Value::Dict(vec![(b("customCmds"), Value::Tuple(vec![ts(), table]))]);
        Value::Dict(vec![(b("cmd"), cmd)])
    }

    fn entry<'a>(k: &'a Keybinds, name: &str) -> &'a KeybindEntry {
        k.entries.iter().find(|e| e.command == name).expect("command projected")
    }

    #[test]
    fn projects_every_command_in_file_order() {
        let k = project_keybinds(Some(&user_with_binds()));
        assert!(k.available);
        assert_eq!(k.entries.len(), 4);
        assert_eq!(k.entries[0].command, "CmdActivateHighPowerSlot1");
        assert_eq!(k.entries[3].command, "CmdDronesEngage");
    }

    #[test]
    fn projects_bound_and_unbound_values() {
        let k = project_keybinds(Some(&user_with_binds()));
        assert_eq!(entry(&k, "CmdActivateHighPowerSlot1").keys, Some(vec![81]));
        assert_eq!(entry(&k, "CmdActivateMediumPowerSlot1").keys, Some(vec![17, 83]));
        assert_eq!(entry(&k, "CmdDronesEngage").keys, Some(vec![18, 16, 68]));
        let unbound = entry(&k, "CmdToggleAutopilot");
        assert_eq!(unbound.keys, None, "None is unbound, not malformed");
        assert!(!unbound.malformed);
    }

    #[test]
    fn no_file_and_no_section_are_unavailable() {
        assert!(!project_keybinds(None).available);
        assert!(!project_keybinds(Some(&Value::Dict(vec![]))).available);
    }

    /// Spec §2.5: a live account that never opened the keybinding screen has
    /// the section but an EMPTY table. That drives the view's empty state.
    #[test]
    fn an_empty_table_is_unavailable() {
        let cmd = Value::Dict(vec![(
            b("customCmds"),
            Value::Tuple(vec![ts(), Value::Dict(vec![])]),
        )]);
        let user = Value::Dict(vec![(b("cmd"), cmd)]);
        let k = project_keybinds(Some(&user));
        assert!(!k.available);
        assert!(k.entries.is_empty());
    }

    /// Real account files Ref/Shared their repeated root keys — the trap that
    /// made the overview state-colour read path project nothing (spec §2.1).
    #[test]
    fn resolves_ref_and_shared_keys_and_values() {
        let table = Value::Dict(vec![
            (Value::Shared { slot: 3, value: Box::new(b("CmdApproachItem")) }, codes(&[65])),
            (b("CmdWarpToItem"), Value::Shared { slot: 4, value: Box::new(codes(&[83])) }),
            (Value::Ref(3), Value::Ref(4)),
        ]);
        let cmd = Value::Dict(vec![(b("customCmds"), Value::Tuple(vec![ts(), table]))]);
        let user = Value::Dict(vec![(Value::Shared { slot: 9, value: Box::new(b("cmd")) }, cmd)]);
        let k = project_keybinds(Some(&user));
        assert!(k.available, "a Ref-keyed section must still resolve");
        assert_eq!(entry(&k, "CmdApproachItem").keys, Some(vec![65]));
        assert_eq!(entry(&k, "CmdWarpToItem").keys, Some(vec![83]));
    }

    #[test]
    fn an_unrecognised_value_is_malformed_not_silently_blank() {
        let table = Value::Dict(vec![
            (b("CmdWeird"), Value::Str("Q".into())),
            (b("CmdEmptyTuple"), Value::Tuple(vec![])),
        ]);
        let cmd = Value::Dict(vec![(b("customCmds"), Value::Tuple(vec![ts(), table]))]);
        let k = project_keybinds(Some(&Value::Dict(vec![(b("cmd"), cmd)])));
        assert!(entry(&k, "CmdWeird").malformed);
        assert_eq!(entry(&k, "CmdWeird").keys, None);
        assert!(entry(&k, "CmdEmptyTuple").malformed);
    }
}
