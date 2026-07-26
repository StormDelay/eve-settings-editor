//! A preset's documents are pruned: a `user.dat` holding only `cmd` legally has
//! no `overview` key at all. No file EVE writes looks like that, so every
//! projection is exercised here against a document missing its own section.
//! An empty projection is the contract; a panic is a bug.

use blue_marshal::Value;
use settings_model::{
    project_edit_history, project_hud, project_keybinds, project_overview, window_layout,
};

fn b(s: &str) -> Value {
    Value::Bytes(s.as_bytes().to_vec())
}

/// The extreme case: a document with nothing in it at all.
fn empty() -> Value {
    Value::Dict(vec![])
}

/// A document holding exactly one unrelated section.
fn only_windows() -> Value {
    Value::Dict(vec![(b("windows"), Value::Dict(vec![]))])
}

#[test]
fn overview_projects_empty_without_an_overview_key() {
    for doc in [empty(), only_windows()] {
        let cols = project_overview(&doc, None);
        assert!(cols.tabs.is_empty(), "no tabs");
        assert!(cols.presets.is_empty(), "no presets");
        assert!(cols.windows.is_empty(), "no overview windows");
    }
}

#[test]
fn autofill_projects_empty_without_an_edit_history() {
    for doc in [empty(), only_windows()] {
        assert!(project_edit_history(&doc).is_empty());
    }
}

#[test]
fn keybinds_report_unavailable_without_a_cmd_section() {
    for doc in [empty(), only_windows()] {
        let k = project_keybinds(Some(&doc));
        assert!(!k.available, "a document with no cmd section is not editable");
        assert!(k.entries.is_empty());
    }
}

#[test]
fn window_layout_projects_empty_without_a_windows_key() {
    let wl = window_layout(&empty(), None);
    assert!(wl.windows.is_empty());
    assert!(wl.stacks.is_empty());
}

#[test]
fn hud_projects_without_either_section() {
    // The HUD reads both documents; neither has its section here.
    let hud = project_hud(&empty(), Some(&empty()));
    // Every entry must fall back to its default rather than panicking.
    for e in &hud.entries {
        assert!(e.value.is_none(), "{} should read as unset", e.name);
    }
}
