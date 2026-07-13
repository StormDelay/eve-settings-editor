//! A loaded settings file: original bytes, decoded tree, and the load-time
//! fidelity baseline that gates every save.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use blue_marshal::{decode, encode, Value};
use serde::Serialize;

/// Whether the document may be saved. Decided once, at load:
/// `encode(decode(bytes))` must reproduce the on-disk bytes exactly —
/// otherwise a save would write a file that differs from what the client
/// wrote in ways the user never asked for. This is the M1a final review's
/// load-bearing recommendation: the corpus gate proves the codec, this
/// check proves *this* file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Fidelity {
    Editable,
    ReadOnly { reason: String },
}

#[derive(Debug)]
pub enum LoadError {
    Io(String),
    /// The file is not a decodable blue-marshal stream. The app shows a hex
    /// view (spec §7: never writable) — offset points at the failure.
    Decode { offset: usize, message: String },
}

#[derive(Debug)]
pub struct Document {
    pub path: PathBuf,
    pub value: Value,
    pub fidelity: Fidelity,
    /// Conflict-check reference: what the file looked like at load
    /// (mtime + length, per spec §5.3). The bytes themselves are not kept —
    /// backups are taken from disk at save time.
    pub(crate) loaded_mtime: Option<SystemTime>,
    pub(crate) loaded_len: u64,
}

impl Document {
    pub fn load(path: &Path) -> Result<Document, LoadError> {
        let bytes = fs::read(path).map_err(|e| LoadError::Io(e.to_string()))?;
        let meta = fs::metadata(path).map_err(|e| LoadError::Io(e.to_string()))?;
        let value = decode(&bytes).map_err(|e| LoadError::Decode {
            offset: e.offset,
            message: e.to_string(),
        })?;
        let fidelity = match encode(&value) {
            Ok(out) if out == bytes => Fidelity::Editable,
            Ok(out) => Fidelity::ReadOnly {
                reason: format!(
                    "re-encode differs from on-disk bytes ({} vs {} bytes) — \
                     editing disabled to avoid unintended changes",
                    out.len(),
                    bytes.len()
                ),
            },
            Err(e) => Fidelity::ReadOnly { reason: format!("re-encode failed: {e}") },
        };
        Ok(Document {
            path: path.to_path_buf(),
            value,
            fidelity,
            loaded_mtime: meta.modified().ok(),
            loaded_len: meta.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "settings-model-doc-{}-{}",
            std::process::id(),
            name
        ));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("core_char_1.dat");
        fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn canonical_file_loads_editable() {
        let bytes = encode(&Value::Dict(vec![(
            Value::Bytes(b"k".to_vec()),
            Value::Int(5),
        )]))
        .unwrap();
        let doc = Document::load(&temp_file("editable", &bytes)).unwrap();
        assert_eq!(doc.fidelity, Fidelity::Editable);
        assert_eq!(doc.loaded_len, bytes.len() as u64);
    }

    #[test]
    fn noncanonical_file_loads_read_only() {
        // A valid stream the encoder would not produce: Int 1 written as
        // INT8 (canonical form is the ONE constant). Decodes fine,
        // re-encodes shorter -> ReadOnly.
        let bytes = [0x7E, 0, 0, 0, 0, 0x06, 0x01];
        let doc = Document::load(&temp_file("readonly", &bytes)).unwrap();
        match doc.fidelity {
            Fidelity::ReadOnly { ref reason } => {
                assert!(reason.contains("re-encode differs"), "reason: {reason}")
            }
            ref other => panic!("expected ReadOnly, got {other:?}"),
        }
    }

    #[test]
    fn undecodable_file_is_a_decode_error_with_offset() {
        let bytes = [0x7E, 0, 0, 0, 0, 0x3D]; // unknown opcode at offset 5
        match Document::load(&temp_file("bad", &bytes)) {
            Err(LoadError::Decode { offset, .. }) => assert_eq!(offset, 5),
            other => panic!("expected Decode error, got {other:?}"),
        }
    }
}
