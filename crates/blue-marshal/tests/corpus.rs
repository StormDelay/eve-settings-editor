use std::fs;
use std::path::{Path, PathBuf};

fn collect_dat_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_dat_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "dat") {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "M0 gate: un-ignore when Task 9 reaches full corpus coverage"]
fn every_corpus_file_decodes() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/corpus");
    let mut files = Vec::new();
    collect_dat_files(&corpus, &mut files);
    if files.is_empty() {
        eprintln!("corpus empty at {corpus:?} — skipping (run tools/sync-corpus.ps1)");
        return;
    }
    let mut failures = Vec::new();
    for f in &files {
        let data = fs::read(f).unwrap();
        if let Err(e) = blue_marshal::decode(&data) {
            failures.push(format!("{}: {e}", f.display()));
        }
    }
    assert!(
        failures.is_empty(),
        "{}/{} corpus files failed to decode:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
