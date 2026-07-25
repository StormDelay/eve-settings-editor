//! Real-data guard: every corpus `core_user` file must project to a pack that
//! emits YAML which parses back to the same tree. This is the quoting, unicode
//! and markup check — the corpus holds accounts that imported published packs,
//! so it carries the awkward strings a hand-written fixture would not think of.
//! Skips silently when the corpus is not checked out.

use std::path::{Path, PathBuf};
use settings_model::{emit_pack, parse_pack, read_pack};

fn user_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("corpus walk failed at {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry
            .unwrap_or_else(|e| panic!("corpus walk failed under {}: {e}", dir.display()));
        let p = entry.path();
        if p.is_dir() { user_files(&p, out); }
        else if p.file_name().map_or(false, |n| n.to_string_lossy().starts_with("core_user_")) {
            out.push(p);
        }
    }
}

#[test]
fn every_corpus_user_file_round_trips_as_a_pack() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/corpus");
    if !root.is_dir() {
        eprintln!("corpus missing at {root:?} — skipping (run tools/sync-corpus.ps1)");
        return;
    }
    let mut files = Vec::new();
    user_files(&root, &mut files);
    if files.is_empty() { eprintln!("corpus not present, skipping"); return; }

    let mut checked = 0usize;
    for path in files {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(doc) = blue_marshal::decode(&bytes) else { continue };
        let (pack, _) = read_pack(&doc);
        if pack.sections.is_empty() { continue }
        let text = emit_pack(&pack);
        let again = parse_pack(&text).unwrap_or_else(|e| panic!("{}: emitted YAML did not parse: {e:?}", path.display()));
        assert_eq!(again.sections, pack.sections, "{}: round trip changed the pack", path.display());
        checked += 1;
    }
    assert!(checked > 0, "corpus present but no user file produced a pack");
}
