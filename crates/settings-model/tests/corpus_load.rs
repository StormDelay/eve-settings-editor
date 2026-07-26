//! Every corpus file must load Editable: the fidelity baseline is the
//! byte-identity gate applied through the Document API. A regression here
//! with the blue-marshal gates green means Document::load itself broke.
//!
//! Runs over the committed synthetic corpus always, plus the real corpus when
//! it is checked out (see `common`). `decode-only/` fixtures are excluded by
//! construction: they exist precisely because they cannot re-encode identically,
//! so they load ReadOnly on purpose.

mod common;

use settings_model::{Document, Fidelity};

#[test]
fn every_corpus_file_loads_editable() {
    let files: Vec<_> = common::identity_corpus().collect();
    let mut failures = Vec::new();
    for f in &files {
        match Document::load(&f.path) {
            Ok(doc) => {
                if let Fidelity::ReadOnly { reason } = doc.fidelity {
                    failures.push(format!("{}: ReadOnly: {reason}", f.path.display()));
                }
            }
            Err(e) => failures.push(format!("{}: {e:?}", f.path.display())),
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

/// The inverse: a `decode-only` fixture must load, and must load ReadOnly.
/// Without this, a change that made those bytes round-trip would go unnoticed
/// and the exclusion above would be silently pointless.
#[test]
fn decode_only_fixtures_load_read_only() {
    let mut checked = 0;
    for f in common::corpus().iter().filter(|f| !f.identity_safe) {
        let doc = Document::load(&f.path)
            .unwrap_or_else(|e| panic!("{}: should still decode: {e:?}", f.path.display()));
        assert!(
            matches!(doc.fidelity, Fidelity::ReadOnly { .. }),
            "{}: expected ReadOnly (deprecated opcodes cannot re-encode identically)",
            f.path.display()
        );
        checked += 1;
    }
    assert!(checked > 0, "no decode-only fixtures found");
}
