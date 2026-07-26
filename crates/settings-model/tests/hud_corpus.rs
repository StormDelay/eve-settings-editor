//! Real-data guard for the HUD anchor *sections*. Every unit test in `hud.rs`
//! builds its own fixture, so a `Field` that names the wrong section passes them
//! all while reading nothing from a real file — which is exactly what shipped in
//! v0.15.0 (`badge_*` declared `ui`; the key really lives under `notifications`).
//! Only real files can catch that class, so this asserts each character-scoped
//! anchor actually projects a value somewhere in the corpus.
//!
//! Skips silently when the corpus is not checked out.

mod common;

use settings_model::project_hud;

/// Enough sightings per field to mean "the section is right", not "one odd
/// file". The synthetic corpus is curated — one deliberate fixture carrying an
/// anchor is already proof the section name matches — while the real corpus is
/// a pile of files where a single hit could be a fluke.
const ENOUGH_REAL: usize = 20;
const ENOUGH_SYNTHETIC: usize = 1;

const CHAR_FIELDS: [&str; 3] = ["ship_offset", "fighter_x", "badge_x"];

#[test]
fn every_character_hud_anchor_reads_from_a_real_file() {
    let mut synthetic = [0usize; CHAR_FIELDS.len()];
    let mut real = [0usize; CHAR_FIELDS.len()];
    let mut scanned = 0usize;

    for f in common::char_files() {
        // Stop once every field is well attested in whichever roots are
        // present — the real corpus holds hundreds of distinct files.
        let done_real = !common::real_corpus_present() || real.iter().all(|n| *n >= ENOUGH_REAL);
        if synthetic.iter().all(|n| *n >= ENOUGH_SYNTHETIC) && done_real {
            break;
        }
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        scanned += 1;
        let hud = project_hud(&doc, None);
        for (i, name) in CHAR_FIELDS.iter().enumerate() {
            let e = hud.entries.iter().find(|e| &e.name == name).expect("field projected");
            if e.value.is_some() {
                if f.synthetic { synthetic[i] += 1 } else { real[i] += 1 }
            }
        }
    }

    for (i, name) in CHAR_FIELDS.iter().enumerate() {
        assert!(
            synthetic[i] >= ENOUGH_SYNTHETIC,
            "{name} projected no value in any synthetic character fixture              — its `section`/`key` does not match the shape the generator writes"
        );
        if common::real_corpus_present() {
            assert!(
                real[i] >= ENOUGH_REAL,
                "{name} projected a value in only {}/{scanned} scanned real character files                  — its `section`/`key` almost certainly does not match real data",
                real[i]
            );
        }
    }
}
