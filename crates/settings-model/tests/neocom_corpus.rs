//! Real-data guard for the neocom projection. Real files intern the four state
//! key names and the repeated icon paths as `Shared`/`Ref`, and 11 corpus
//! buttons carry an `id` that is a `Tuple(bytes, None)` rather than plain bytes
//! — a reader that matched `Value::Bytes` directly would pass every hand-built
//! unit test in `neocom.rs` and still read nothing from a real file.
//!
//! Skips silently when the real corpus is not checked out.

mod common;

use settings_model::{neocom_reorder, project_neocom, NeocomError};

/// The raw button-list length straight out of the file, so the projection can be
/// checked for silently dropping entries (`read_button` returns `None` for
/// anything that is not an `Instance`). Inlines first — the public codec API —
/// so this needs no `Shared`/`Ref` handling of its own.
fn raw_bar_len(doc: &blue_marshal::Value) -> Option<usize> {
    use blue_marshal::Value;
    let inlined = blue_marshal::inline(doc);
    let Value::Dict(root) = &inlined else { return None };
    let key = |k: &Value, name: &[u8]| matches!(k, Value::Bytes(b) if b.as_slice() == name);
    let (_, ui) = root.iter().find(|(k, _)| key(k, b"ui"))?;
    let Value::Dict(uid) = ui else { return None };
    let (_, raw) = uid.iter().find(|(k, _)| key(k, b"neocomButtonRawData"))?;
    let payload = match raw {
        Value::Tuple(t) if t.len() == 2 => &t[1],
        other => other,
    };
    match payload {
        Value::List(l) | Value::Tuple(l) => Some(l.len()),
        _ => None,
    }
}

#[test]
fn every_corpus_character_file_projects_or_reports_no_bar() {
    let mut projected = 0;
    let mut originals = 0;
    let mut counted = 0;
    for f in common::char_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        match project_neocom(&doc) {
            Ok(bar) => {
                projected += 1;
                for b in &bar.buttons {
                    assert!(!b.id.is_empty(), "{}: a button projected with an empty id", f.path.display());
                    assert!(b.btn_type > 0, "{}: button {} projected btnType 0", f.path.display(), b.id);
                }
                // Every raw entry must become a button: `read_button` skips a
                // value that is not an `Instance`, and skipping one silently
                // would shift every later index — the commands key by index.
                if let Some(raw) = raw_bar_len(&doc) {
                    counted += 1;
                    assert_eq!(
                        bar.buttons.len(), raw,
                        "{}: projected {} buttons from a raw list of {raw}",
                        f.path.display(), bar.buttons.len(),
                    );
                }
                // Original feeds the addable set on every real character, so it
                // has to be as readable as the live bar.
                originals += bar.original.len();
                for b in &bar.original {
                    assert!(!b.id.is_empty(), "{}: an Original entry has an empty id", f.path.display());
                }
            }
            // A file with no neocom key at all is legitimate (spec §2: 154 of
            // 4,215 corpus character files have none).
            Err(NeocomError::NoBar) | Err(NeocomError::NoUi) => {}
            Err(e) => panic!("{}: neocom projection failed: {e}", f.path.display()),
        }
    }
    if common::real_corpus_present() {
        assert!(projected > 0, "the real corpus is present but nothing projected");
        assert!(originals > 0, "no Original entry read from any real file — the addable set would be catalog-only");
        // Guards the guard: if `raw_bar_len` stopped finding the list, the
        // count comparison above would silently never run.
        assert!(counted > 0, "the raw-length check never found a bar to compare against");
    }
}

#[test]
fn a_reorder_of_a_real_bar_round_trips_through_the_codec() {
    for f in common::char_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let Ok(bar) = project_neocom(&doc) else { continue };
        if bar.buttons.len() < 2 { continue }
        let mut v = doc;
        // Reverse the bar: the most disruptive permutation there is.
        let order: Vec<usize> = (0..bar.buttons.len()).rev().collect();
        neocom_reorder(&mut v, &order).expect("reorder a real bar");
        let v = blue_marshal::reshare(&v); // what the app layer does before saving
        let bytes = blue_marshal::encode(&v).expect("edited real file still encodes");
        assert_eq!(blue_marshal::decode(&bytes).unwrap(), v, "{}: reorder broke the round trip", f.path.display());

        let after = project_neocom(&v).expect("re-project after reorder");
        let before: Vec<&str> = bar.buttons.iter().map(|b| b.id.as_str()).rev().collect();
        let now: Vec<&str> = after.buttons.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(before, now, "{}: the bar did not come back reversed", f.path.display());
    }
}
