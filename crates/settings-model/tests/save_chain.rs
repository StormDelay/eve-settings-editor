//! Full save-chain integration tests on temp directories. These verify the
//! spec §5 invariants: backup-before-write, abort-leaves-file-untouched,
//! conflict detection, and the ReadOnly refusal.

use std::fs;
use std::path::PathBuf;

use blue_marshal::{encode, Value};
use settings_model::{apply, save, Document, Mutation, SaveError, Step};

fn temp_settings_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "settings-model-save-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_canonical_file(dir: &PathBuf) -> (PathBuf, Vec<u8>) {
    let value = Value::Dict(vec![(
        Value::Bytes(b"suggestions".to_vec()),
        Value::List(vec![Value::Str("alpha".into()), Value::Str("beta".into())]),
    )]);
    let bytes = encode(&value).unwrap();
    let path = dir.join("core_user_42.dat");
    fs::write(&path, &bytes).unwrap();
    (path, bytes)
}

#[test]
fn save_backs_up_then_writes_atomically() {
    let dir = temp_settings_dir("happy");
    let (path, original) = write_canonical_file(&dir);
    let mut doc = Document::load(&path).unwrap();
    apply(&mut doc.value, &Mutation::SetScalar {
        path: vec![Step::DictValue(0), Step::List(0)],
        text: "edited".into(),
    })
    .unwrap();
    let report = save(&mut doc, false).unwrap();
    // Backup holds the ORIGINAL bytes.
    assert_eq!(fs::read(&report.backup_path).unwrap(), original);
    assert!(report.backup_path.parent().unwrap().ends_with("eve-settings-editor-backups"));
    // Target holds the new encode, which reloads Editable with the edit.
    let reloaded = Document::load(&path).unwrap();
    assert_eq!(reloaded.fidelity, settings_model::Fidelity::Editable);
    let json = serde_json::to_value(settings_model::project(&reloaded.value)).unwrap();
    assert_eq!(json["children"][0]["children"][0]["display"], "\"edited\"");
    // A second save after the first must not be a conflict (baseline refreshed).
    save(&mut doc, false).unwrap();
}

#[test]
fn conflict_detected_and_forcible() {
    let dir = temp_settings_dir("conflict");
    let (path, _) = write_canonical_file(&dir);
    let mut doc = Document::load(&path).unwrap();
    // Simulate the client rewriting the file after our load: different
    // length guarantees detection even on coarse-mtime filesystems.
    let other = encode(&Value::Dict(vec![])).unwrap();
    fs::write(&path, &other).unwrap();
    match save(&mut doc, false) {
        Err(SaveError::Conflict) => {}
        other => panic!("expected Conflict, got {other:?}"),
    }
    // Forced save proceeds — and the backup preserves the CURRENT on-disk
    // (conflicting) bytes, so nothing is ever lost.
    let report = save(&mut doc, true).unwrap();
    assert_eq!(fs::read(&report.backup_path).unwrap(), other);
}

#[test]
fn backup_failure_aborts_with_file_untouched() {
    let dir = temp_settings_dir("nobackup");
    let (path, original) = write_canonical_file(&dir);
    // Occupy the backup-dir NAME with a file, so create_dir_all fails.
    fs::write(dir.join("eve-settings-editor-backups"), b"not a dir").unwrap();
    let mut doc = Document::load(&path).unwrap();
    apply(&mut doc.value, &Mutation::SetScalar {
        path: vec![Step::DictValue(0), Step::List(0)],
        text: "edited".into(),
    })
    .unwrap();
    match save(&mut doc, false) {
        Err(SaveError::Backup(_)) => {}
        other => panic!("expected Backup error, got {other:?}"),
    }
    assert_eq!(fs::read(&path).unwrap(), original, "no backup => no write, ever");
}

#[test]
fn encode_failure_aborts_before_touching_disk() {
    let dir = temp_settings_dir("badtree");
    let (path, original) = write_canonical_file(&dir);
    let mut doc = Document::load(&path).unwrap();
    doc.value = Value::Tuple(vec![Value::Ref(1)]); // dangling ref: unencodable
    match save(&mut doc, false) {
        Err(SaveError::Encode(_)) => {}
        other => panic!("expected Encode error, got {other:?}"),
    }
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!dir.join("eve-settings-editor-backups").exists(), "no backup taken either");
}

#[test]
fn read_only_document_refuses_to_save() {
    let dir = temp_settings_dir("readonly");
    // Non-canonical stream: Int 1 as INT8 -> loads ReadOnly.
    let path = dir.join("core_char_7.dat");
    fs::write(&path, [0x7E, 0, 0, 0, 0, 0x06, 0x01]).unwrap();
    let mut doc = Document::load(&path).unwrap();
    match save(&mut doc, false) {
        Err(SaveError::ReadOnly(_)) => {}
        other => panic!("expected ReadOnly, got {other:?}"),
    }
}
