use std::fs;
use std::path::{Path, PathBuf};

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

/// M0 gate: full decode coverage reached in Task 9 (GLOBAL, INSTANCE,
/// REDUCE). This is now a permanent regression test — any future corpus
/// addition or decoder change must keep every file decoding cleanly.
#[test]
fn every_corpus_file_decodes() {
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

/// Codec re-share gate: for every corpus file, `reshare` must preserve the
/// value and produce a stream that encodes and round-trips. `reshare` inlines
/// internally and is a normalizer, so re-normalizing after a wire round-trip
/// must land on the identical value — that proves no value was dropped or
/// corrupted and that the emitted sharing satisfies store-before-ref. The
/// byte-identical replay gate above is unchanged and still guards the read path.
#[test]
fn reshare_preserves_every_corpus_file() {
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
        let data = fs::read(f).unwrap();
        let Ok(value) = blue_marshal::decode(&data) else {
            failures.push(format!("{}: decode", f.display()));
            continue;
        };
        let reshared = blue_marshal::reshare(&value);
        let bytes = match blue_marshal::encode(&reshared) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!("{}: reshared encode: {e}", f.display()));
                continue;
            }
        };
        match blue_marshal::decode(&bytes) {
            Ok(back) if blue_marshal::reshare(&back) == reshared => {}
            Ok(_) => failures.push(format!("{}: reshare not preserved by round-trip", f.display())),
            Err(e) => failures.push(format!("{}: reshared decode: {e}", f.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{}/{} corpus files failed the reshare gate:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
