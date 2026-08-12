//! Real-data guard for the tab containers' `(timestamp, payload)` wrapper.
//!
//! `overview_tabs`'s unit fixtures build their own zero timestamps, so they can
//! prove a wrapper is PRESENT but not that a real one SURVIVES a write — the
//! same gap `keybinds_corpus.rs` closes for `customCmds` ("the customCmds table
//! timestamp must survive the write untouched"). `tabs_mut`/`groups_mut` now
//! repair a missing wrapper, and a repair that also reset a real timestamp to
//! zero would be a different kind of damage that no unit fixture would notice.
//!
//! Skips silently when the corpus is not checked out.

mod common;

use blue_marshal::Value;
use settings_model::{delete_tab, rename_tab};

/// Files carrying a tab table. Measured: 77 real account files. Set well below
/// that so refreshing the corpus cannot fail this spuriously.
const ENOUGH_REAL: usize = 40;

/// The raw `overview → <key>` slot, before any unwrapping.
fn slot(doc: &Value, key: &[u8]) -> Option<Value> {
    let Value::Dict(top) = doc else { return None };
    let (_, ov) = top.iter().find(|(k, _)| matches!(k, Value::Bytes(b) if b.as_slice() == b"overview"))?;
    let Value::Dict(entries) = ov else { return None };
    entries
        .iter()
        .find(|(k, _)| matches!(k, Value::Bytes(b) if b.as_slice() == key))
        .map(|(_, v)| v.clone())
}

/// The leading timestamp of a `(timestamp, payload)` slot, if it has one.
fn stamp(doc: &Value, key: &[u8]) -> Option<Value> {
    match slot(doc, key)? {
        Value::Tuple(items) => items.first().cloned(),
        _ => None,
    }
}

/// Every tab index in a real file's table, ascending.
fn tab_indices(doc: &Value) -> Vec<i64> {
    let Some(slot) = slot(doc, b"tabsettings_new").or_else(|| slot(doc, b"tabsettings")) else {
        return Vec::new();
    };
    let inner = match slot {
        Value::Dict(d) => d,
        Value::Tuple(items) => match items.into_iter().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }) {
            Some(d) => d,
            None => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    let mut out: Vec<i64> = inner
        .iter()
        .filter_map(|(k, _)| match k {
            Value::Int(i) => Some(*i),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out
}

/// The first tab index in a real file's table, so the edit targets something.
fn a_tab_index(doc: &Value) -> Option<i64> {
    tab_indices(doc).first().copied()
}

#[test]
fn a_real_tab_tables_timestamp_survives_a_write() {
    if !common::real_corpus_present() {
        return;
    }
    let mut checked = 0usize;
    let mut repaired = 0usize;

    for f in common::user_files().filter(|f| !f.synthetic) {
        let Ok(raw) = blue_marshal::decode(&f.bytes) else { continue };
        // Inline first: real files key the overview container and its children
        // through Shared/Ref, which a raw walk cannot follow. Every entry point
        // in `overview_tabs` calls `inline_all` anyway, so this is the same tree
        // the edit sees.
        let doc = blue_marshal::inline(&raw);
        let Some(idx) = a_tab_index(&doc) else { continue };
        let before = stamp(&doc, b"tabsettings_new");
        let was_bare = matches!(slot(&doc, b"tabsettings_new"), Some(Value::Dict(_)));

        let mut edited = doc.clone();
        if rename_tab(&mut edited, idx, "corpus gate").is_err() {
            continue;
        }
        checked += 1;

        // Always wrapped afterwards, whatever it was before.
        let after_slot = slot(&edited, b"tabsettings_new");
        assert!(
            matches!(after_slot, Some(Value::Tuple(_))),
            "{}: tabsettings_new must be (timestamp, dict) after a write, got {:?}",
            f.name(),
            after_slot,
        );

        if was_bare {
            repaired += 1; // a bare container an older build left behind
        } else if let Some(b) = before {
            // The repair is for a MISSING wrapper. A real one must come through
            // untouched — resetting it to zero would be its own kind of damage.
            assert_eq!(
                stamp(&edited, b"tabsettings_new"),
                Some(b),
                "{}: an existing tab-table timestamp must survive the write untouched",
                f.name(),
            );
        }

        // And the edited tree must still encode.
        let bytes = blue_marshal::encode(&edited).expect("re-encodes");
        blue_marshal::decode(&bytes).expect("re-decodes");
    }

    eprintln!("{checked} real account file(s) edited, {repaired} carried a bare container");
    assert!(
        checked >= ENOUGH_REAL,
        "only {checked} real files exercised, expected at least {ENOUGH_REAL} — \
         the walker or the tab-table lookup has drifted",
    );
}

/// EVE keys the tab table densely, and a gap is not inert: the client draws a
/// blank, unnamed tab in the missing slot. Measured over this corpus, every
/// untouched account file is dense — so a delete that leaves a gap is the
/// editor inventing a shape the client never writes, which is what a user hit
/// on 2026-08-12 (nine deletes, nine phantom tabs).
///
/// The unit fixtures build their own small tables; only real data can show that
/// the invariant holds before the edit as well as after it.
#[test]
fn a_real_tab_table_is_gap_free_before_and_after_a_delete() {
    if !common::real_corpus_present() {
        return;
    }
    let mut checked = 0usize;

    for f in common::user_files().filter(|f| !f.synthetic) {
        let Ok(raw) = blue_marshal::decode(&f.bytes) else { continue };
        let doc = blue_marshal::inline(&raw);
        let before = tab_indices(&doc);
        if before.len() < 2 {
            continue; // nothing to delete, or the last-tab guard would refuse
        }
        assert!(
            before.iter().enumerate().all(|(i, &n)| n == i as i64),
            "{}: an UNTOUCHED file already has a gap in its tab table: {before:?}",
            f.name(),
        );

        let mut edited = doc.clone();
        // Delete from the middle, where a gap is possible at all.
        if delete_tab(&mut edited, before[before.len() / 2]).is_err() {
            continue;
        }
        checked += 1;
        let after = tab_indices(&edited);
        assert_eq!(
            after,
            (0..before.len() as i64 - 1).collect::<Vec<_>>(),
            "{}: a delete must leave the tab table gap-free",
            f.name(),
        );

        let bytes = blue_marshal::encode(&edited).expect("re-encodes");
        blue_marshal::decode(&bytes).expect("re-decodes");
    }

    eprintln!("{checked} real account file(s) checked for tab-table gaps");
    assert!(
        checked >= ENOUGH_REAL,
        "only {checked} real files exercised, expected at least {ENOUGH_REAL} — \
         the walker or the tab-table lookup has drifted",
    );
}
