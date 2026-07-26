//! Real-data guard for the HUD anchor *sections*. Every unit test in `hud.rs`
//! builds its own fixture, so a `Field` that names the wrong section passes them
//! all while reading nothing from a real file — which is exactly what shipped in
//! v0.15.0 (`badge_*` declared `ui`; the key really lives under `notifications`).
//! Only real files can catch that class, so this asserts each character-scoped
//! anchor actually projects a value somewhere in the corpus.
//!
//! Skips silently when the corpus is not checked out.

use std::path::{Path, PathBuf};

use settings_model::project_hud;

/// Enough sightings per field to mean "the section is right", not "one odd file".
const ENOUGH: usize = 20;

const CHAR_FIELDS: [&str; 3] = ["ship_offset", "fighter_x", "badge_x"];

fn char_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            char_files(&p, out);
        } else if p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("core_char_")) {
            out.push(p);
        }
    }
}

#[test]
fn every_character_hud_anchor_reads_from_a_real_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/corpus");
    if !root.is_dir() {
        eprintln!("corpus missing at {root:?} — skipping (run tools/sync-corpus.ps1)");
        return;
    }
    let mut files = Vec::new();
    char_files(&root, &mut files);
    if files.is_empty() {
        eprintln!("corpus not present, skipping");
        return;
    }

    let mut seen = [0usize; CHAR_FIELDS.len()];
    let mut scanned = 0usize;
    for path in &files {
        // Stop as soon as every field is well attested — the corpus holds
        // thousands of files and decoding them all takes minutes.
        if seen.iter().all(|n| *n >= ENOUGH) {
            break;
        }
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Ok(doc) = blue_marshal::decode(&bytes) else { continue };
        scanned += 1;
        let hud = project_hud(&doc, None);
        for (i, name) in CHAR_FIELDS.iter().enumerate() {
            let e = hud.entries.iter().find(|e| &e.name == name).expect("field projected");
            if e.value.is_some() {
                seen[i] += 1;
            }
        }
    }

    for (i, name) in CHAR_FIELDS.iter().enumerate() {
        assert!(
            seen[i] >= ENOUGH,
            "{name} projected a value in only {}/{scanned} scanned character files \
             — its `section`/`key` almost certainly does not match real data",
            seen[i]
        );
    }
}
