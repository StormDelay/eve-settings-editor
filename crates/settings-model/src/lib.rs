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
pub mod batch;
mod stacks;
mod overview_tabs;
mod overview_presets;
pub mod overview_states;

pub use backups::{list_backups, restore, BackupInfo}; // enabled in Task 7
pub use discover::{default_roots, discover, file_kind, FileKind, Profile, SettingsFile}; // enabled in Task 8
pub use document::{Document, Fidelity, LoadError};
pub use mutate::{apply, Mutation, MutateError, NewValue};
pub use path::{resolve, resolve_mut, NodePath, Step};
pub use projection::{project, Node}; // enabled in Task 4
pub use save::{save, SaveError, SaveReport}; // enabled in Task 6
pub use windows::{window_layout, BoolFlag, Geom, SetTarget, Stack, StackRef, StackRole, WindowLayout, WindowRect};
pub use overview::{project_overview, set_column_order, set_column_visible, set_column_width, OverviewColumn, OverviewColumns, OverviewError, OverviewTab, OverviewWindow};
pub use autofill::{clear_all_history, project_edit_history, set_list_entries, AutofillError, RememberedList};
pub use batch::{apply_categories_to, apply_to_tree, extract_categories, full_copy_to, Category};
pub use stacks::{add_to_stack, create_stack, reorder_stack, unstack, StackError};
pub use overview_tabs::{
    add_overview_window, add_overview_window_geometry, create_tab, delete_tab, move_tab,
    remove_overview_window, remove_overview_window_geometry, rename_tab, reorder_tabs_in_window,
    set_tab_preset, OverviewTabError,
};
pub use overview_presets::{create_preset, create_preset_from_lists, delete_preset, fork_preset, rename_preset, set_preset_groups};
pub use overview_states::{set_state_list, StateList};

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
