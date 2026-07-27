//! A preset's documents are pruned: a `user.dat` holding only `cmd` legally has
//! no `overview` key at all. No file EVE writes looks like that, so every
//! projection is exercised here against a document missing its own section.
//! An empty projection is the contract; a panic is a bug.
//!
//! A third shape closes a real gap: pruning PRESERVES a key that was already
//! present-but-empty on the source document, so a preset can legally contain
//! `{"overview": {}}` — the section key present, holding nothing. And because
//! nothing guarantees a pruned value keeps its original wire type, a section
//! key can in principle hold the wrong type too (modelled here with a bare
//! `Value::Int(1)` standing in for whatever scalar it might be). Both must
//! project empty, not panic.

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

/// A document whose `overview` key is present but an empty dict.
fn overview_section_empty() -> Value {
    Value::Dict(vec![(b("overview"), Value::Dict(vec![]))])
}

/// Same key, holding a scalar where a dict is expected.
fn overview_section_wrong_type() -> Value {
    Value::Dict(vec![(b("overview"), Value::Int(1))])
}

/// A document whose `ui -> editHistory` key is present but an empty dict.
fn edit_history_section_empty() -> Value {
    Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("editHistory"), Value::Dict(vec![]))]))])
}

/// Same key, holding a scalar where a dict is expected.
fn edit_history_section_wrong_type() -> Value {
    Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("editHistory"), Value::Int(1))]))])
}

/// A document whose `cmd -> customCmds` key is present but an empty dict.
fn cmd_section_empty() -> Value {
    Value::Dict(vec![(b("cmd"), Value::Dict(vec![(b("customCmds"), Value::Dict(vec![]))]))])
}

/// Same key, holding a scalar where a dict is expected.
fn cmd_section_wrong_type() -> Value {
    Value::Dict(vec![(b("cmd"), Value::Dict(vec![(b("customCmds"), Value::Int(1))]))])
}

/// A document whose `windows` key holds a scalar where a dict is expected.
/// (The "present but empty" shape is `only_windows()` above — the sparsest
/// legal `windows` section already looks exactly like that.)
fn windows_section_wrong_type() -> Value {
    Value::Dict(vec![(b("windows"), Value::Int(1))])
}

/// A character document whose three HUD-relevant sections (windows, ui,
/// notifications) are all present but empty dicts.
fn hud_char_sections_empty() -> Value {
    Value::Dict(vec![
        (b("windows"), Value::Dict(vec![])),
        (b("ui"), Value::Dict(vec![])),
        (b("notifications"), Value::Dict(vec![])),
    ])
}

/// Same sections, each holding a scalar where a dict is expected.
fn hud_char_sections_wrong_type() -> Value {
    Value::Dict(vec![
        (b("windows"), Value::Int(1)),
        (b("ui"), Value::Int(1)),
        (b("notifications"), Value::Int(1)),
    ])
}

/// An account document whose two HUD-relevant sections (ui, windows) are
/// present but empty dicts.
fn hud_account_sections_empty() -> Value {
    Value::Dict(vec![(b("ui"), Value::Dict(vec![])), (b("windows"), Value::Dict(vec![]))])
}

/// Same sections, each holding a scalar where a dict is expected.
fn hud_account_sections_wrong_type() -> Value {
    Value::Dict(vec![(b("ui"), Value::Int(1)), (b("windows"), Value::Int(1))])
}

#[test]
fn overview_projects_empty_without_an_overview_key() {
    for doc in [empty(), only_windows(), overview_section_empty(), overview_section_wrong_type()] {
        let cols = project_overview(&doc, None);
        assert!(cols.tabs.is_empty(), "no tabs");
        assert!(cols.presets.is_empty(), "no presets");
        assert!(cols.windows.is_empty(), "no overview windows");
    }
}

#[test]
fn autofill_projects_empty_without_an_edit_history() {
    for doc in [empty(), only_windows(), edit_history_section_empty(), edit_history_section_wrong_type()] {
        assert!(project_edit_history(&doc).is_empty());
    }
}

#[test]
fn keybinds_report_unavailable_without_a_cmd_section() {
    for doc in [empty(), only_windows(), cmd_section_empty(), cmd_section_wrong_type()] {
        let k = project_keybinds(Some(&doc));
        assert!(!k.available, "a document with no cmd section is not editable");
        assert!(k.entries.is_empty());
    }
}

#[test]
fn window_layout_projects_empty_without_a_windows_key() {
    // only_windows() (`{"windows": {}}`) is the "own section present but
    // empty" case for this projection.
    for doc in [empty(), only_windows(), windows_section_wrong_type()] {
        let wl = window_layout(&doc, None);
        assert!(wl.windows.is_empty());
        assert!(wl.stacks.is_empty());
    }
}

#[test]
fn hud_projects_without_either_section() {
    // The HUD reads both documents; each pair below leaves neither document's
    // section readable, in a different way: absent, present-but-empty, and
    // present-with-the-wrong-type.
    for (char_doc, user_doc) in [
        (empty(), empty()),
        (hud_char_sections_empty(), hud_account_sections_empty()),
        (hud_char_sections_wrong_type(), hud_account_sections_wrong_type()),
    ] {
        let hud = project_hud(&char_doc, Some(&user_doc));
        // Every entry must fall back to its default rather than panicking.
        for e in &hud.entries {
            assert!(e.value.is_none(), "{} should read as unset", e.name);
        }
    }
}
