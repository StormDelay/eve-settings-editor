//! Real-data guard for the keybinding table. Unit fixtures in `keybinds.rs`
//! build whatever shape the reader expects, so a wrong section or key passes
//! them all while projecting nothing from a real file — the class of bug that
//! shipped in v0.15.0 for the HUD badge offset.
//!
//! Also pins the value invariants the writer relies on (spec §2.2): rejecting a
//! two-key combo is only safe if real files never contain one.
//!
//! Skips silently when the corpus is not checked out.

mod common;

use settings_model::{project_keybinds, set_keybind, MOD_ALT, MOD_CTRL, MOD_SHIFT};

/// A wrong section or key path projects 0, which is the whole point of this
/// gate; the exact figure only has to be comfortably above that.
///
/// Measured: 97 real account files (counted separately from the synthetic
/// corpus — see `with_table` below). Note this is NOT the 132 quoted in the
/// design spec — that counted files by path within a single corpus snapshot,
/// whereas `common` deduplicates by content across every snapshot, collapsing
/// the many byte-identical account files this corpus carries. Set well below
/// 97 so refreshing the corpus cannot fail this spuriously.
const ENOUGH_REAL: usize = 80;

#[test]
fn the_keybinding_table_reads_from_real_files() {
    if !common::real_corpus_present() {
        return;
    }
    let mut with_table = 0usize;
    let mut bindings = 0usize;

    for f in common::user_files().filter(|f| !f.synthetic) {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let k = project_keybinds(Some(&doc));
        if k.available {
            with_table += 1;
            bindings += k.entries.iter().filter(|e| e.keys.is_some()).count();
        }
    }

    eprintln!("{with_table} real account file(s) with a keybinding table, {bindings} binding(s)");
    assert!(
        with_table >= ENOUGH_REAL,
        "only {with_table} real account files projected a keybinding table (expected >= {ENOUGH_REAL}); \
         the section or key path is wrong"
    );
    assert!(bindings > 1000, "expected thousands of real bindings, got {bindings}");
}

#[test]
fn every_real_binding_satisfies_the_writer_invariants() {
    let modifiers = [MOD_CTRL, MOD_ALT, MOD_SHIFT];

    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let k = project_keybinds(Some(&doc));
        for e in &k.entries {
            assert!(!e.malformed, "{}: {} projected as malformed", f.name(), e.command);
            let Some(keys) = &e.keys else { continue };

            let mods: Vec<i64> = keys.iter().copied().filter(|c| modifiers.contains(c)).collect();
            let rest: Vec<i64> = keys.iter().copied().filter(|c| !modifiers.contains(c)).collect();
            assert_eq!(rest.len(), 1, "{}: {} has {:?}, expected one non-modifier", f.name(), e.command, keys);
            assert_eq!(&keys[keys.len() - 1], &rest[0], "{}: the key must come last", f.name());

            // Canonical order is Ctrl, Alt, Shift — i.e. the modifiers appear as
            // a subsequence of MODIFIERS.
            let mut want = modifiers.iter().copied().filter(|m| mods.contains(m));
            assert!(
                mods.iter().all(|m| want.next() == Some(*m)),
                "{}: {} modifiers {:?} are not in Ctrl/Alt/Shift order",
                f.name(), e.command, mods
            );
        }
    }
}

#[test]
fn no_real_file_contains_a_duplicate_combination() {
    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let k = project_keybinds(Some(&doc));
        let mut seen: Vec<(&Vec<i64>, &str)> = Vec::new();
        for e in &k.entries {
            let Some(keys) = &e.keys else { continue };
            if let Some((_, other)) = seen.iter().find(|(s, _)| *s == keys) {
                panic!("{}: {:?} bound to both {} and {}", f.name(), keys, other, e.command);
            }
            seen.push((keys, &e.command));
        }
    }
}

/// A write against a real file must change exactly one leaf and leave the rest
/// of the document — the table timestamp included — byte-identical.
#[test]
fn a_write_against_a_real_file_changes_only_the_target_leaf() {
    let Some(f) = common::user_files().find(|f| {
        !f.synthetic
            && blue_marshal::decode(&f.bytes).map(|d| project_keybinds(Some(&d)).available).unwrap_or(false)
    }) else {
        return; // no real corpus checked out
    };
    eprintln!("round-trip target: {}", f.name());

    let doc = blue_marshal::decode(&f.bytes).expect("decodes");
    let before = project_keybinds(Some(&doc));
    let before_timestamp = customcmds_timestamp(&doc);
    let target = before
        .entries
        .iter()
        .find(|e| e.keys.is_none())
        .map(|e| e.command.clone())
        .expect("a real corpus file has unbound commands");

    let mut edited = doc.clone();
    set_keybind(&mut edited, &target, Some(vec![MOD_CTRL, 145])).expect("write succeeds");

    let after = project_keybinds(Some(&edited));
    assert_eq!(after.entries.len(), before.entries.len(), "no rows added or removed");
    for (b, a) in before.entries.iter().zip(after.entries.iter()) {
        assert_eq!(b.command, a.command, "command order preserved");
        if a.command == target {
            assert_eq!(a.keys, Some(vec![MOD_CTRL, 145]));
        } else {
            assert_eq!(a.keys, b.keys, "{} must be untouched", a.command);
        }
    }
    assert_eq!(
        customcmds_timestamp(&edited),
        before_timestamp,
        "the customCmds table timestamp must survive the write untouched"
    );

    // Re-encoding must round-trip.
    let bytes = blue_marshal::encode(&edited).expect("re-encodes");
    let redecoded = blue_marshal::decode(&bytes).expect("re-decodes");
    let round = project_keybinds(Some(&redecoded));
    assert_eq!(round.entries, after.entries, "the write survives an encode/decode cycle");
}

/// Pull the raw `cmd -> customCmds` FILETIME (tuple element 0) out of a
/// decoded document. `blue_marshal::inline` resolves any `Shared`/`Ref` on `cmd` and
/// `customCmds` first (real account files may wrap either — see the format
/// note atop `keybinds.rs`), so the walk below only has to match bare `Bytes`
/// keys.
fn customcmds_timestamp(doc: &blue_marshal::Value) -> blue_marshal::Value {
    use blue_marshal::Value;
    let plain = blue_marshal::inline(doc);
    let Value::Dict(root) = &plain else { panic!("root is a dict") };
    let (_, cmd) = root
        .iter()
        .find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"cmd"))
        .expect("cmd section");
    let Value::Dict(cmd) = cmd else { panic!("cmd is a bare dict") };
    let (_, wrapper) = cmd
        .iter()
        .find(|(k, _)| matches!(k, Value::Bytes(b) if b == b"customCmds"))
        .expect("customCmds");
    let Value::Tuple(parts) = wrapper else { panic!("customCmds is a (ts, dict) tuple") };
    parts[0].clone()
}
