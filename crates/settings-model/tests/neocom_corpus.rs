//! Real-data guard for the neocom projection. Real files intern the four state
//! key names and the repeated icon paths as `Shared`/`Ref`, and 11 corpus
//! buttons carry an `id` that is a `Tuple(bytes, None)` rather than plain bytes
//! — a reader that matched `Value::Bytes` directly would pass every hand-built
//! unit test in `neocom.rs` and still read nothing from a real file.
//!
//! Skips silently when the real corpus is not checked out.

mod common;

use settings_model::{neocom_reorder, project_neocom, NeocomError};

#[test]
fn every_corpus_character_file_projects_or_reports_no_bar() {
    let mut projected = 0;
    for f in common::char_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        match project_neocom(&doc) {
            Ok(bar) => {
                projected += 1;
                for b in &bar.buttons {
                    assert!(!b.id.is_empty(), "{}: a button projected with an empty id", f.path.display());
                    assert!(b.btn_type > 0, "{}: button {} projected btnType 0", f.path.display(), b.id);
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
