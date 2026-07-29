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

use crate::treewalk::{as_dict, child_dict_mut, collect_shared, effective, find_child, inline_all, is_bytes, text, SharedTable, Entries};

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
            let command = text(k, &sh)?;
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

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum KeybindError {
    /// No `cmd -> customCmds` in this file.
    NoTable,
    /// This client build's table has no such command (spec §2.4 — the table is
    /// the command set; the editor never mints rows).
    UnknownCommand,
    /// No non-modifier code supplied.
    NoKey,
    /// More than one non-modifier code; the corpus has none such.
    MultipleKeys,
    DuplicateModifier,
}

/// Bind `command` to `keys` (or unbind it with `None`), stealing the
/// combination from any other command that holds it — which is what EVE does,
/// and why no corpus file contains a duplicate.
///
/// Returns the commands whose binding was cleared, so the caller can say what
/// it took. Leaves the `customCmds` timestamp untouched.
pub fn set_keybind(
    user: &mut Value,
    command: &str,
    keys: Option<Vec<i64>>,
) -> Result<Vec<String>, KeybindError> {
    // Validate BEFORE mutating: a rejected write must change nothing.
    let canon = keys.map(|k| canonical(&k)).transpose()?;

    inline_all(user);
    let table = custom_cmds_mut(user).ok_or(KeybindError::NoTable)?;
    if !table.iter().any(|(k, _)| is_bytes(k, command.as_bytes())) {
        return Err(KeybindError::UnknownCommand);
    }

    let mut stolen = Vec::new();
    if let Some(c) = &canon {
        let want = Value::Tuple(c.iter().map(|&n| Value::Int(n)).collect());
        for (k, v) in table.iter_mut() {
            if is_bytes(k, command.as_bytes()) || *v != want {
                continue;
            }
            if let Value::Bytes(name) = k {
                stolen.push(String::from_utf8_lossy(name).into_owned());
            }
            *v = Value::None;
        }
    }

    let (_, slot) = table
        .iter_mut()
        .find(|(k, _)| is_bytes(k, command.as_bytes()))
        .expect("presence checked above");
    *slot = match &canon {
        Some(c) => Value::Tuple(c.iter().map(|&n| Value::Int(n)).collect()),
        None => Value::None,
    };
    Ok(stolen)
}

/// Enforce the corpus invariant and impose the canonical order: modifiers
/// Ctrl, Alt, Shift (in that order), then exactly one non-modifier code.
fn canonical(keys: &[i64]) -> Result<Vec<i64>, KeybindError> {
    let mods: Vec<i64> = keys.iter().copied().filter(|c| MODIFIERS.contains(c)).collect();
    let rest: Vec<i64> = keys.iter().copied().filter(|c| !MODIFIERS.contains(c)).collect();

    let mut seen = mods.clone();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != mods.len() {
        return Err(KeybindError::DuplicateModifier);
    }
    match rest.len() {
        0 => return Err(KeybindError::NoKey),
        1 => {}
        _ => return Err(KeybindError::MultipleKeys),
    }

    let mut out: Vec<i64> = MODIFIERS.iter().copied().filter(|m| mods.contains(m)).collect();
    out.push(rest[0]);
    Ok(out)
}

/// Mutable inner dict of root -> cmd -> customCmds -> (ts, dict). Assumes a
/// plain tree (post-`inline_all`), so keys are plain Bytes.
fn custom_cmds_mut(user: &mut Value) -> Option<&mut Entries> {
    let Value::Dict(root) = user else { return None };
    let cmd = child_dict_mut(root, b"cmd")?;
    child_dict_mut(cmd, b"customCmds")
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

    #[test]
    fn binds_an_unbound_command() {
        let mut user = user_with_binds();
        let stolen = set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![17, 90])).unwrap();
        assert!(stolen.is_empty());
        let k = project_keybinds(Some(&user));
        assert_eq!(entry(&k, "CmdToggleAutopilot").keys, Some(vec![17, 90]));
    }

    #[test]
    fn unbinding_writes_none() {
        let mut user = user_with_binds();
        set_keybind(&mut user, "CmdActivateHighPowerSlot1", None).unwrap();
        let k = project_keybinds(Some(&user));
        let e = entry(&k, "CmdActivateHighPowerSlot1");
        assert_eq!(e.keys, None);
        assert!(!e.malformed, "an unbound leaf is None, not junk");
    }

    /// Spec §2.2: no corpus file contains a duplicate combination, so EVE steals
    /// the key from its previous owner. The editor must do the same or it writes
    /// a file the client never produces.
    #[test]
    fn rebinding_a_taken_combo_steals_it() {
        let mut user = user_with_binds();
        let stolen = set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![81])).unwrap();
        assert_eq!(stolen, vec!["CmdActivateHighPowerSlot1"]);
        let k = project_keybinds(Some(&user));
        assert_eq!(entry(&k, "CmdToggleAutopilot").keys, Some(vec![81]));
        assert_eq!(entry(&k, "CmdActivateHighPowerSlot1").keys, None, "previous owner cleared");
    }

    #[test]
    fn rebinding_a_command_to_its_own_combo_is_a_noop() {
        let mut user = user_with_binds();
        let stolen = set_keybind(&mut user, "CmdActivateHighPowerSlot1", Some(vec![81])).unwrap();
        assert!(stolen.is_empty(), "a command never steals from itself");
        let k = project_keybinds(Some(&user));
        assert_eq!(entry(&k, "CmdActivateHighPowerSlot1").keys, Some(vec![81]));
    }

    #[test]
    fn modifier_order_is_canonicalised_to_ctrl_alt_shift() {
        let mut user = user_with_binds();
        // Supplied Shift, Alt, Ctrl, key — must be stored 17, 18, 16, key.
        set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![16, 18, 17, 68])).unwrap();
        let k = project_keybinds(Some(&user));
        assert_eq!(entry(&k, "CmdToggleAutopilot").keys, Some(vec![17, 18, 16, 68]));
    }

    #[test]
    fn canonicalisation_makes_a_reordered_combo_collide() {
        let mut user = user_with_binds();
        // CmdActivateMediumPowerSlot1 holds (17, 83). Supplying (83, 17) must
        // canonicalise to (17, 83) and therefore steal it.
        let stolen = set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![83, 17])).unwrap();
        assert_eq!(stolen, vec!["CmdActivateMediumPowerSlot1"]);
    }

    #[test]
    fn rejects_combos_that_break_the_corpus_invariant() {
        let mut user = user_with_binds();
        assert_eq!(set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![])), Err(KeybindError::NoKey));
        assert_eq!(set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![17])), Err(KeybindError::NoKey));
        assert_eq!(set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![81, 83])), Err(KeybindError::MultipleKeys));
        assert_eq!(
            set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![17, 17, 81])),
            Err(KeybindError::DuplicateModifier)
        );
        // A rejected write changes nothing.
        let k = project_keybinds(Some(&user));
        assert_eq!(entry(&k, "CmdToggleAutopilot").keys, None);
    }

    #[test]
    fn rejects_an_unknown_command_and_a_missing_table() {
        let mut user = user_with_binds();
        assert_eq!(
            set_keybind(&mut user, "CmdNotInThisClient", Some(vec![81])),
            Err(KeybindError::UnknownCommand)
        );
        // A rejected write changes nothing. The attempted bind to [81] would have stolen
        // CmdActivateHighPowerSlot1's binding if the existence check had run after stealing.
        let k = project_keybinds(Some(&user));
        assert_eq!(
            entry(&k, "CmdActivateHighPowerSlot1").keys,
            Some(vec![81]),
            "rejected write must not steal CmdActivateHighPowerSlot1's binding"
        );

        let mut bare = Value::Dict(vec![]);
        assert_eq!(set_keybind(&mut bare, "CmdAnything", None), Err(KeybindError::NoTable));
    }

    /// GLOBAL CONSTRAINT. Five shipped editors preserve an existing wrapper's
    /// timestamp and every live smoke passed on that. A leaf must stay BARE.
    #[test]
    fn a_write_preserves_the_table_timestamp_and_never_wraps_a_leaf() {
        let mut user = user_with_binds();
        set_keybind(&mut user, "CmdToggleAutopilot", Some(vec![17, 90])).unwrap();

        let Value::Dict(root) = &user else { panic!("root is a dict") };
        let (_, cmd) = root.iter().find(|(k, _)| is_bytes(k, b"cmd")).expect("cmd section");
        let Value::Dict(cmd) = cmd else { panic!("cmd is a bare dict, not wrapped") };
        let (_, wrapper) = cmd.iter().find(|(k, _)| is_bytes(k, b"customCmds")).expect("customCmds");
        let Value::Tuple(parts) = wrapper else { panic!("customCmds is a (ts, dict) tuple") };
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], ts(), "the table timestamp must survive untouched");

        let Value::Dict(table) = &parts[1] else { panic!("payload is a dict") };
        let (_, leaf) = table.iter().find(|(k, _)| is_bytes(k, b"CmdToggleAutopilot")).unwrap();
        assert_eq!(
            leaf,
            &codes(&[17, 90]),
            "the leaf is a bare code tuple — wrapping it produces the malformed value EVE ignores"
        );
    }
}
