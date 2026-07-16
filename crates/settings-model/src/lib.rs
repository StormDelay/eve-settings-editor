//! EVE settings document handling on top of the `blue-marshal` codec:
//! fidelity-checked loading, JSON tree projection, mutations, the
//! backup/verify/atomic save chain, backups, and profile discovery.
//! No EVE *semantics* live here yet (categories arrive in M2/M3).

pub mod backups;
pub mod discover;
pub mod document;
pub mod mutate;
pub mod path;
pub mod projection;
pub mod save;
mod treewalk;
pub mod windows;
pub mod overview;
pub mod autofill;

pub use backups::{list_backups, restore, BackupInfo}; // enabled in Task 7
pub use discover::{default_roots, discover, FileKind, Profile, SettingsFile}; // enabled in Task 8
pub use document::{Document, Fidelity, LoadError};
pub use mutate::{apply, Mutation, MutateError, NewValue};
pub use path::{resolve, resolve_mut, NodePath, Step};
pub use projection::{project, Node}; // enabled in Task 4
pub use save::{save, SaveError, SaveReport}; // enabled in Task 6
pub use windows::{window_layout, BoolFlag, Geom, SetTarget, StackField, WindowLayout, WindowRect};
pub use overview::{project_overview, set_column_order, set_column_visible, set_column_width, OverviewColumn, OverviewColumns, OverviewError, OverviewTab, OverviewWindow};
pub use autofill::{project_edit_history, RememberedList};

/// Kind name for error messages; mirrors projection::Node.kind.
pub(crate) fn projection_kind(v: &blue_marshal::Value) -> &'static str {
    use blue_marshal::Value;
    match v {
        Value::None => "none",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Long(_) => "long",
        Value::Float(_) => "float",
        Value::Bytes(_) => "bytes",
        Value::Str(_) => "str",
        Value::StrUcs2(_) => "str_ucs2",
        Value::StrTable(_) => "str_table",
        Value::Tuple(_) => "tuple",
        Value::List(_) => "list",
        Value::Dict(_) => "dict",
        Value::Stream(_) => "stream",
        Value::Global(_) => "global",
        Value::Instance { .. } => "instance",
        Value::Reduce { .. } => "reduce",
        Value::Shared { .. } => "shared",
        Value::Ref(_) => "ref",
    }
}
