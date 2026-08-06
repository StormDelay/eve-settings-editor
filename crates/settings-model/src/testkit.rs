//! Fixture builders shared by every module's unit tests.
//!
//! `b` and `ts` were declared privately in 23 and 22 test modules respectively
//! — `overview.rs` alone held eight copies of each, one per nested test module.
//! They are one definition here so a change to what a fixture byte-string or
//! wrapper timestamp looks like is a change in one place.
//!
//! `#[cfg(test)]` in lib.rs, so this costs nothing in a release build and is
//! invisible to the integration tests in `tests/` (which compile the crate
//! without `cfg(test)` and build their fixtures from real corpus files).

use blue_marshal::Value;

/// A byte-string, the shape EVE stores nearly every dict key and name in.
pub(crate) fn b(s: &str) -> Value {
    Value::Bytes(s.as_bytes().to_vec())
}

/// A stand-in FILETIME timestamp — the first half of the `(timestamp, payload)`
/// wrapper every container in these files carries. Real files hold a real one;
/// nothing under test reads it.
pub(crate) fn ts() -> Value {
    Value::Long(vec![0u8; 8])
}
