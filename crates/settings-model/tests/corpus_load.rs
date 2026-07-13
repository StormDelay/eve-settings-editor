//! Every real corpus file must load Editable: the fidelity baseline is the
//! byte-identity gate applied through the Document API. A regression here
//! with the blue-marshal gates green means Document::load itself broke.

use std::fs;
use std::path::{Path, PathBuf};

use settings_model::{Document, Fidelity};

fn collect_dat_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("corpus walk failed at {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry
            .unwrap_or_else(|e| panic!("corpus walk failed under {}: {e}", dir.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_dat_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "dat") {
            out.push(path);
        }
    }
}

#[test]
fn every_corpus_file_loads_editable() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/corpus");
    if !corpus.is_dir() {
        eprintln!("corpus missing at {corpus:?} — skipping (run tools/sync-corpus.ps1)");
        return;
    }
    let mut files = Vec::new();
    collect_dat_files(&corpus, &mut files);
    if files.is_empty() {
        eprintln!("corpus empty at {corpus:?} — skipping (run tools/sync-corpus.ps1)");
        return;
    }
    let mut failures = Vec::new();
    for f in &files {
        match Document::load(f) {
            Ok(doc) => {
                if let Fidelity::ReadOnly { reason } = doc.fidelity {
                    failures.push(format!("{}: ReadOnly: {reason}", f.display()));
                }
            }
            Err(e) => failures.push(format!("{}: {e:?}", f.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{}/{} corpus files did not load Editable:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
