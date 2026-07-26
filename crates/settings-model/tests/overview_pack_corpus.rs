//! Real-data guard: every corpus `core_user` file must project to a pack that
//! emits YAML which parses back to the same tree. This is the quoting, unicode
//! and markup check — the corpus holds accounts that imported published packs,
//! so it carries the awkward strings a hand-written fixture would not think of.
//! Runs over the committed synthetic corpus always, plus the real corpus when
//! it is checked out.
//!
//! It also closes the loop past `parse_pack`: `apply_pack` (re-importing the
//! pack this file's own account would export) is exercised against every real
//! file here, asserting the round trip loses nothing AND that no tab index
//! ends up mapped into two overview windows at once — the permanent guard on
//! the C1 corpus finding (real multi-window accounts duplicating an index).

mod common;

use settings_model::{apply_pack, emit_pack, parse_pack, project_overview, read_pack};

#[test]
fn every_corpus_user_file_round_trips_as_a_pack() {
    let mut checked = 0usize;
    for f in common::user_files() {
        let path = &f.path;
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let (pack, _) = read_pack(&doc);
        if pack.sections.is_empty() { continue }
        let text = emit_pack(&pack);
        let again = parse_pack(&text).unwrap_or_else(|e| panic!("{}: emitted YAML did not parse: {e:?}", path.display()));
        assert_eq!(again.sections, pack.sections, "{}: round trip changed the pack", path.display());

        // Close the loop: apply the re-parsed pack back onto a clone of the
        // account and read it again. Nothing the pack carries should be lost.
        let mut doc2 = doc.clone();
        apply_pack(&mut doc2, &again).unwrap_or_else(|e| panic!("{}: apply_pack failed: {e:?}", path.display()));
        let (back, _) = read_pack(&doc2);
        assert_eq!(back.sections, pack.sections, "{}: export then re-import lost data", path.display());

        // C1 guard: no tab index may end up mapped into more than one overview
        // window after the re-import.
        let oc = project_overview(&doc2, None);
        let mut seen = Vec::new();
        for w in &oc.windows {
            for idx in &w.tab_indices {
                assert!(!seen.contains(idx), "{}: tab {idx} ended up in two overview windows after re-import", path.display());
                seen.push(*idx);
            }
        }

        checked += 1;
    }
    assert!(checked > 0, "no user file produced a pack");
    eprintln!("checked {checked} distinct user file(s)");
}
