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

/// M0 gate: full decode coverage reached in Task 9 (GLOBAL, INSTANCE,
/// REDUCE). This is now a permanent regression test — any future corpus
/// addition or decoder change must keep every file decoding cleanly.
#[test]
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

/// M1a gate: decode → encode must reproduce every corpus file byte-for-byte.
/// This is the strongest writer-correctness proof available without the game
/// client: any drift in opcode choice, length encoding, shared-slot order, or
/// tail-map content fails here with the first differing offset. If a future
/// client patch breaks a canonical rule, this is where it shows up.
#[test]
fn every_corpus_file_reencodes_byte_identically() {
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
        let value = match blue_marshal::decode(&data) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{}: decode: {e}", f.display()));
                continue;
            }
        };
        match blue_marshal::encode(&value) {
            Err(e) => failures.push(format!("{}: encode: {e}", f.display())),
            Ok(out) if out != data => {
                let at = out
                    .iter()
                    .zip(data.iter())
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| out.len().min(data.len()));
                failures.push(format!(
                    "{}: first byte diff at {:#x} (encoded {} bytes, original {} bytes)",
                    f.display(),
                    at,
                    out.len(),
                    data.len()
                ));
            }
            Ok(_) => {}
        }
    }
    assert!(
        failures.is_empty(),
        "{}/{} corpus files failed byte-identical re-encode:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
