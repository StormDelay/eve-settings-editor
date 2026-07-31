//! Real-data guard for the HUD anchor *sections*. Every unit test in `hud.rs`
//! builds its own fixture, so a `Field` that names the wrong section passes them
//! all while reading nothing from a real file — which is exactly what shipped in
//! v0.15.0 (`badge_*` declared `ui`; the key really lives under `notifications`).
//! Only real files can catch that class, so this asserts each character-scoped
//! anchor actually projects a value somewhere in the corpus.
//!
//! It also pins the one *shape* the batch copy reads rather than a value: that
//! every real character file carries a root `notifications` section, which is
//! what lets `batch.rs::absence_means_eve_default` tell a pre-HUD Layout preset
//! apart from a character sitting at EVE's defaults.
//!
//! Skips silently when the corpus is not checked out.

mod common;

use settings_model::{extract_categories, project_hud, Category};

/// Enough sightings per field to mean "the section is right", not "one odd
/// file". The synthetic corpus is curated — one deliberate fixture carrying an
/// anchor is already proof the section name matches — while the real corpus is
/// a pile of files where a single hit could be a fluke.
const ENOUGH_REAL: usize = 20;
const ENOUGH_SYNTHETIC: usize = 1;

const CHAR_FIELDS: [&str; 3] = ["ship_offset", "fighter_x", "badge_x"];

/// The account half of the same guard. `target_x` is the reason it exists: its
/// keys read back from a plain `bmdump dump` as if they sat under `windows`,
/// beside `neocomWidth`, and only an inlined dump shows them under `ui`. A
/// `Field` naming the wrong one passes every hand-built fixture in `hud.rs`
/// while reading nothing from a real account file.
const ACCOUNT_FIELDS: [&str; 3] = ["neocom_width", "ship_top", "target_x"];

/// `targetOrigin` is written on demand — 87 % of corpus account files have
/// never had the target list dragged — so it cannot clear `ENOUGH_REAL`.
/// Ten distinct real files is still far past "one odd file".
const ENOUGH_REAL_TARGET: usize = 10;

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

#[test]
fn every_account_hud_field_reads_from_a_real_file() {
    // The character root every account field ignores: `project_hud` needs one,
    // and an empty document proves these values come from the account side.
    let no_char = blue_marshal::Value::Dict(vec![]);
    let mut synthetic = [0usize; ACCOUNT_FIELDS.len()];
    let mut real = [0usize; ACCOUNT_FIELDS.len()];
    let mut scanned = 0usize;

    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        scanned += 1;
        let hud = project_hud(&no_char, Some(&doc));
        for (i, name) in ACCOUNT_FIELDS.iter().enumerate() {
            let e = hud.entries.iter().find(|e| &e.name == name).expect("field projected");
            if e.value.is_some() {
                if f.synthetic { synthetic[i] += 1 } else { real[i] += 1 }
            }
        }
    }

    for (i, name) in ACCOUNT_FIELDS.iter().enumerate() {
        assert!(
            synthetic[i] >= ENOUGH_SYNTHETIC,
            "{name} projected no value in any synthetic account fixture \
             — its `section`/`key` does not match the shape the generator writes"
        );
        if common::real_corpus_present() {
            let enough = if *name == "target_x" { ENOUGH_REAL_TARGET } else { ENOUGH_REAL };
            assert!(
                real[i] >= enough,
                "{name} projected a value in only {}/{scanned} scanned real account files \
                 — its `section`/`key` almost certainly does not match real data",
                real[i]
            );
        }
    }
}

/// The char-side removal guard rests on a shape: a source document with no root
/// `notifications` key is read as a Layout preset saved before the aspect
/// carried the HUD, and its HUD absences stop deleting the target's values.
/// That rule must never fire on a real character file, and the only reason it
/// cannot is that EVE writes the section into every one of them.
///
/// Asserted through `extract_categories` itself rather than by re-implementing
/// the key lookup: `HudBadge` comes back exactly when the guard accepts the
/// document, so this tests the real predicate on real bytes.
#[test]
fn a_real_char_file_is_never_read_as_a_pre_hud_preset() {
    let mut checked = 0usize;
    let mut missing: Vec<String> = Vec::new();
    for f in common::char_files().filter(|f| !f.synthetic) {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        checked += 1;
        if extract_categories(&doc, &[Category::HudBadge]).is_empty() {
            missing.push(f.name());
        }
    }
    if !common::real_corpus_present() {
        return; // gitignored personal data; nothing to assert here on CI
    }
    assert!(checked > 0, "the real corpus is present but held no character files");
    assert!(
        missing.is_empty(),
        "{}/{checked} real character files have no root `notifications` section, so a copy \
         FROM them would stop removing the target's HUD keys: {:?}",
        missing.len(),
        &missing[..missing.len().min(5)]
    );
}
