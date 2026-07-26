//! Codec gates over the corpus.
//!
//! Two roots. `fixtures/synthetic` is committed and therefore always present —
//! before it existed these gates returned early on a missing `testdata/`, so
//! they asserted nothing on CI or on any machine but the author's.
//! `testdata/corpus` is real client output, gitignored as personal data, and
//! runs in addition when checked out; it is the only evidence our bytes match
//! CCP's, but it is 93 % duplicate (6140 files, 413 distinct), so it is
//! deduplicated by content hash below and read once per test binary.
//!
//! The helper is inlined here rather than shared with settings-model's
//! `tests/common/mod.rs`: this crate is deliberately dependency-free, and a
//! shared fixture crate would be a dev-dependency.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

struct CorpusFile {
    path: PathBuf,
    bytes: Vec<u8>,
    /// `decode-only/` fixtures carry opcodes the decoder accepts but the encoder
    /// canonically re-emits differently (deprecated STRING/STRINGL), so they can
    /// never survive a byte-identity check.
    identity_safe: bool,
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        h ^= *byte as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut found: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    found.sort();
    for path in found {
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "dat") {
            out.push(path);
        }
    }
}

fn corpus() -> &'static [CorpusFile] {
    static FILES: OnceLock<Vec<CorpusFile>> = OnceLock::new();
    FILES.get_or_init(|| {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"));
        let synthetic = base.join("../../fixtures/synthetic");
        let mut roots = vec![synthetic.clone()];
        // `EVE_SYNTHETIC_ONLY=1` ignores the real corpus: reproduces exactly what
        // CI sees, and turns a full run into a seconds-long loop locally.
        let synthetic_only = std::env::var_os("EVE_SYNTHETIC_ONLY").is_some_and(|v| v != "0");
        if !synthetic_only {
            // `testdata/` is gitignored (personal data), so a git worktree never
            // carries it and this gate silently degrades to synthetic-only there.
            // Point `EVE_CORPUS_DIR` at the main checkout's `testdata/corpus` to
            // run it from a worktree. Deliberately no fallback to the default path
            // below when the override is set: a typo'd override must fail closed
            // (silently synthetic-only), not silently succeed against the wrong
            // directory.
            let real = if let Some(dir) = std::env::var_os("EVE_CORPUS_DIR") {
                let p = PathBuf::from(dir);
                p.is_dir().then_some(p)
            } else {
                let p = base.join("../../testdata/corpus");
                p.is_dir().then_some(p)
            };
            if let Some(real) = real {
                roots.push(real);
            }
        }
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for root in roots {
            let mut paths = Vec::new();
            walk(&root, &mut paths);
            for path in paths {
                let Ok(bytes) = std::fs::read(&path) else { continue };
                if !seen.insert((bytes.len(), fnv1a(&bytes))) {
                    continue;
                }
                let identity_safe =
                    !path.components().any(|c| c.as_os_str().to_string_lossy() == "decode-only");
                out.push(CorpusFile { path, bytes, identity_safe });
            }
        }
        assert!(
            !out.is_empty(),
            "synthetic corpus missing at {} — run `cargo run -p settings-model --bin gen_fixtures`",
            synthetic.display()
        );
        out
    })
}

/// M0 gate: full decode coverage reached in Task 9 (GLOBAL, INSTANCE,
/// REDUCE). This is now a permanent regression test — any future corpus
/// addition or decoder change must keep every file decoding cleanly.
#[test]
fn every_corpus_file_decodes() {
    let files = corpus();
    let mut failures = Vec::new();
    for f in files {
        if let Err(e) = blue_marshal::decode(&f.bytes) {
            failures.push(format!("{}: {e}", f.path.display()));
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
///
/// On the real corpus that is an independence proof — those bytes came from the
/// client. On the synthetic corpus, whose bytes this repo generated, it is a
/// golden-file regression check: encoder output for those shapes must not drift.
#[test]
fn every_corpus_file_reencodes_byte_identically() {
    let files: Vec<&CorpusFile> = corpus().iter().filter(|f| f.identity_safe).collect();
    let mut failures = Vec::new();
    for f in &files {
        let value = match blue_marshal::decode(&f.bytes) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{}: decode: {e}", f.path.display()));
                continue;
            }
        };
        match blue_marshal::encode(&value) {
            Err(e) => failures.push(format!("{}: encode: {e}", f.path.display())),
            Ok(out) if out != f.bytes => {
                let at = out
                    .iter()
                    .zip(f.bytes.iter())
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| out.len().min(f.bytes.len()));
                failures.push(format!(
                    "{}: first byte diff at {:#x} (encoded {} bytes, original {} bytes)",
                    f.path.display(),
                    at,
                    out.len(),
                    f.bytes.len()
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
/// SOURCE value (checked via `inline` against the original decode) and
/// produce a stream that encodes and wire-round-trips back to itself. The
/// byte-identical replay gate above is unchanged and still guards the read path.
#[test]
fn reshare_preserves_every_corpus_file() {
    let files = corpus();
    let mut failures = Vec::new();
    for f in files {
        let Ok(value) = blue_marshal::decode(&f.bytes) else {
            failures.push(format!("{}: decode", f.path.display()));
            continue;
        };
        let reshared = blue_marshal::reshare(&value);
        if blue_marshal::inline(&reshared) != blue_marshal::inline(&value) {
            failures.push(format!("{}: reshare changed the value", f.path.display()));
            continue;
        }
        let bytes = match blue_marshal::encode(&reshared) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!("{}: reshared encode: {e}", f.path.display()));
                continue;
            }
        };
        match blue_marshal::decode(&bytes) {
            Ok(back) if back == reshared => {}
            Ok(_) => failures.push(format!("{}: round-trip differs", f.path.display())),
            Err(e) => failures.push(format!("{}: reshared decode: {e}", f.path.display())),
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

/// The synthetic corpus is the part that is always present, so it gets an
/// explicit floor: if a fixture is deleted or the generator stops writing one,
/// the gates above would quietly shrink instead of failing.
#[test]
fn synthetic_corpus_is_complete() {
    let names: Vec<String> = corpus()
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for expected in [
        "scalars.dat",
        "containers.dat",
        "sharing.dat",
        "objects.dat",
        "odd_keys.dat",
        "deprecated_strings.dat",
        "core_char_90000001.dat",
        "core_char_90000002.dat",
        "core_char_90000003.dat",
        "core_user_80000001.dat",
        "core_user_80000002.dat",
        "core_user_80000003.dat",
        "core_user_80000004.dat",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "synthetic fixture {expected} is missing — run \
             `cargo run -p settings-model --bin gen_fixtures`"
        );
    }
}
