//! EVE settings document handling on top of the `blue-marshal` codec:
//! fidelity-checked loading, JSON tree projection, mutations, the
//! backup/verify/atomic save chain, backups, and profile discovery.
//! No EVE *semantics* live here yet (categories arrive in M2/M3).

// Every module is private: the crate's whole public surface is the `pub use`
// list below, and it was already the only way in — a sweep found exactly one
// module-path use anywhere in the workspace, for a name this list re-exports.
// Leaving them `pub` published a second path to every item that nothing called
// and no test covered.
mod backups;
mod discover;
mod document;
mod mutate;
mod path;
mod projection;
mod save;
mod treewalk;
mod windows;
mod hud;
mod chat;
mod neocom;
mod probes;
mod overview;
mod autofill;
mod batch;
mod keybinds;
mod stacks;
mod overview_tabs;
mod overview_presets;
mod overview_states;
mod overview_pack;
mod probe_pack;

#[cfg(test)]
mod testkit;

pub use backups::{list_backups, restore, BackupInfo};
pub use discover::{default_roots, discover, file_kind, FileKind, Profile, SettingsFile};
pub use document::{Document, Fidelity, LoadError};
pub use mutate::{apply, Mutation, MutateError, NewValue};
pub use path::{resolve, resolve_mut, NodePath, Step};
pub use projection::{project, Node};
pub use save::{save, SaveError, SaveReport};
pub use windows::{window_layout, BoolFlag, Geom, SetTarget, Stack, StackRef, StackRole, WindowLayout, WindowRect};
pub use hud::{project_hud, set_hud_value, Hud, HudEntry, HudError, HudKind, HudScope};
pub use chat::{project_chat, set_chat_splits, ChatError, ChatPanel};
pub use neocom::{add as neocom_add, project_neocom, remove as neocom_remove, reorder as neocom_reorder, reset as neocom_reset, NeocomBar, NeocomButton, NeocomError};
pub use probes::{
    check_formation, next_free_id, next_id as next_formation_id, project_formations,
    remove_formation, set_formation, Formation, Formations, ProbeError, DEFAULT_RANGE, MAX_PROBES,
};
pub use overview::{copy_tab_columns, copy_tab_widths, project_overview, set_column_order, set_column_visible, set_column_width, Appearance, OverviewColumn, OverviewColumns, OverviewError, OverviewTab, OverviewWindow, StateSurface};
pub use autofill::{clear_all_history, project_edit_history, set_list_entries, AutofillError, RememberedList};
pub use keybinds::{project_keybinds, set_keybind, KeybindEntry, KeybindError, Keybinds, MOD_ALT, MOD_CTRL, MOD_SHIFT};
pub use batch::{apply_categories_to, apply_to_tree, extract_categories, full_copy_to, Category};
pub use stacks::{add_to_stack, create_stack, delete_orphan_frames, reorder_stack, unstack, StackError};
pub use overview_tabs::{
    add_overview_window, add_overview_window_geometry, create_tab, create_window_mapping,
    delete_tab, move_tab, remove_overview_window, remove_overview_window_geometry, rename_tab,
    reorder_tabs_in_window, set_tab_preset, OverviewTabError,
};
pub use overview_presets::{create_preset, create_preset_from_lists, delete_preset, fork_preset, rename_preset, set_preset_groups, set_preset_states};
pub use overview_states::{
    overview_bools, set_overview_bool, set_state_color, set_state_list, state_colors, StateList,
    OVERVIEW_BOOLS,
};
// The pack node type is exported as `PackNode`: `projection::Node` already
// claims the bare name at the crate root (see the `pub use projection::{project,
// Node}` line above). Inside `overview_pack.rs` it stays plain `Node`; only the
// external name is aliased.
pub use overview_pack::{apply_pack, emit_pack, parse_pack, read_pack, Node as PackNode, Pack, PackError, PackReport};
pub use probe_pack::{emit_formations, parse_formations, unique_name, FormationSpec};

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
