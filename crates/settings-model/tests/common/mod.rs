//! Shared corpus walker for the gates in this directory.
//!
//! Two roots, both optional-but-not-really:
//!
//! - `fixtures/synthetic` is committed, so it is ALWAYS present. Before it
//!   existed every gate here silently returned on a missing `testdata/`, which
//!   meant they asserted nothing on CI or on any machine but the author's.
//! - `testdata/corpus` is real client output, is personal data, and is
//!   gitignored. When present it still runs — it is the only evidence our bytes
//!   match CCP's — but it is heavily duplicated: 6140 files across eleven
//!   snapshots collapse to 413 distinct by content. Decoding the other 93 % over
//!   and over cost minutes per run and proved nothing, so files are deduplicated
//!   by content hash here, once, and shared across the gates in a binary.
//!
//! blue-marshal keeps its own copy of this logic inline: it is deliberately
//! dependency-free, and a shared fixture crate would be a dev-dependency.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub struct CorpusFile {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    /// False for `decode-only/` fixtures: they carry opcodes the decoder accepts
    /// but the encoder canonically re-emits differently (deprecated
    /// STRING/STRINGL), so they can never survive a byte-identity check.
    pub identity_safe: bool,
    pub synthetic: bool,
}

impl CorpusFile {
    pub fn name(&self) -> String {
        self.path.file_name().unwrap_or_default().to_string_lossy().into_owned()
    }
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

fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

pub fn synthetic_root() -> PathBuf {
    crate_root().join("../../fixtures/synthetic")
}

/// Set `EVE_SYNTHETIC_ONLY=1` to ignore the real corpus: reproduces exactly what
/// CI sees, and turns a full `cargo test` into a seconds-long loop locally.
pub fn synthetic_only() -> bool {
    std::env::var_os("EVE_SYNTHETIC_ONLY").is_some_and(|v| v != "0")
}

pub fn real_root() -> Option<PathBuf> {
    if synthetic_only() {
        return None;
    }
    let p = crate_root().join("../../testdata/corpus");
    p.is_dir().then_some(p)
}

pub fn real_corpus_present() -> bool {
    real_root().is_some()
}

/// Every distinct corpus file, synthetic first. Read and deduplicated once per
/// test binary.
pub fn corpus() -> &'static [CorpusFile] {
    static FILES: OnceLock<Vec<CorpusFile>> = OnceLock::new();
    FILES.get_or_init(|| {
        let mut roots = vec![(synthetic_root(), true)];
        if let Some(real) = real_root() {
            roots.push((real, false));
        }
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for (root, synthetic) in roots {
            let mut paths = Vec::new();
            walk(&root, &mut paths);
            for path in paths {
                let Ok(bytes) = std::fs::read(&path) else { continue };
                if !seen.insert((bytes.len(), fnv1a(&bytes))) {
                    continue;
                }
                let identity_safe = !path
                    .components()
                    .any(|c| c.as_os_str().to_string_lossy() == "decode-only");
                out.push(CorpusFile { path, bytes, identity_safe, synthetic });
            }
        }
        assert!(
            out.iter().any(|f| f.synthetic),
            "synthetic corpus missing at {} — run `cargo run -p settings-model --bin gen_fixtures`",
            synthetic_root().display()
        );
        out
    })
}

/// Files whose bytes must survive `encode(decode(bytes)) == bytes`.
pub fn identity_corpus() -> impl Iterator<Item = &'static CorpusFile> {
    corpus().iter().filter(|f| f.identity_safe)
}

pub fn char_files() -> impl Iterator<Item = &'static CorpusFile> {
    corpus().iter().filter(|f| f.name().starts_with("core_char_"))
}

pub fn user_files() -> impl Iterator<Item = &'static CorpusFile> {
    corpus().iter().filter(|f| f.name().starts_with("core_user_"))
}
