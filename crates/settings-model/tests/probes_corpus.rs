//! Real-data guard for the probe formation projection. Account files intern
//! repeated keys and the root `ui` section as `Shared`/`Ref`, so a reader that
//! matched `Value::Bytes` directly would pass every hand-built unit test in
//! `probes.rs` and still read nothing from a real file.
//!
//! It also locks in the measurements the editor's design rests on (spec §2):
//! every formation holds 8 probes and one uniform range. If a future client
//! changes either, this fails loudly rather than the editor quietly writing a
//! shape nobody checked.
//!
//! Runs against the committed synthetic corpus always, and the real corpus when
//! testdata/ is checked out.

mod common;

use settings_model::{project_formations, ProbeError, DEFAULT_RANGE, MAX_PROBES};

#[test]
fn every_corpus_account_file_projects_or_reports_no_formations() {
    let mut projected = 0;
    let mut formations = 0;
    for f in common::user_files() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        match project_formations(&doc) {
            Ok(p) => {
                projected += 1;
                formations += p.formations.len();
                for form in &p.formations {
                    assert!(
                        form.id >= 0,
                        "{}: a negative id reached the projection — the -4 scratch slot is the client's, not a user formation",
                        f.path.display(),
                    );
                    assert!(
                        !form.name.is_empty(),
                        "{}: formation {} projected an empty name",
                        f.path.display(), form.id,
                    );
                    assert!(
                        !form.probes.is_empty() && form.probes.len() <= MAX_PROBES,
                        "{}: formation {} projected {} probes",
                        f.path.display(), form.id, form.probes.len(),
                    );
                }
            }
            // A file with no formations at all is legitimate: 61 of 175 corpus
            // account files have never had one saved.
            Err(ProbeError::NoFormations) | Err(ProbeError::NoUi) => {}
            Err(e) => panic!("{}: probe formation projection failed: {e}", f.path.display()),
        }
    }
    assert!(projected > 0, "no account file projected — the corpus walker found nothing");
    assert!(formations > 0, "no formation projected — the synthetic fixture should carry one");
}

#[test]
fn every_real_formation_holds_eight_probes_at_one_uniform_range() {
    // The two measurements the editor's single range field and its 1-8 probe
    // range rest on (spec §2.3, §2.4). Real corpus only: the synthetic fixture
    // is authored to these values, so asserting on it proves nothing.
    if !common::real_corpus_present() {
        return;
    }
    let mut checked = 0;
    for f in common::user_files() {
        if f.synthetic {
            continue;
        }
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        let Ok(p) = project_formations(&doc) else { continue };
        for form in &p.formations {
            checked += 1;
            assert_eq!(
                form.probes.len(), 8,
                "{}: formation {} holds {} probes, not 8 — the corpus has only ever shown 8",
                f.path.display(), form.id, form.probes.len(),
            );
            assert!(
                !form.mixed_range,
                "{}: formation {} has mixed probe ranges — no corpus formation did when this was designed",
                f.path.display(), form.id,
            );
            assert_eq!(
                form.range, DEFAULT_RANGE,
                "{}: formation {} is at {} m, not the 0.5 AU every corpus formation carries",
                f.path.display(), form.id, form.range,
            );
        }
    }
    assert!(checked > 0, "the real corpus is present but carried no formations");
}
