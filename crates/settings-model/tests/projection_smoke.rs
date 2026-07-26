//! Cross-feature guard against the "reads the wrong place" bug class.
//!
//! Every unit test in this crate builds its own fixture, so a projection that
//! names the wrong section, matches the wrong `Value` variant, or fails to
//! resolve `Shared`/`Ref` passes all of them while reading **nothing** from a
//! real file. That is not hypothetical: it shipped twice — the overview state
//! read path matching bare `Bytes` (fixed in v0.13.0) and the HUD badge anchor
//! declared under `ui` instead of `notifications` (fixed after v0.15.0). Both
//! needed real-shaped data to catch.
//!
//! Two passes:
//!
//! 1. **Never panics.** Run every projection over every corpus file. Cheap, and
//!    it is the only thing that ever sees the odd shapes — `None` dict keys,
//!    tuple keys, `Int`-where-`Bool`-was-expected.
//! 2. **Reads what is there.** For each named synthetic fixture, assert the
//!    projection returns the data the generator deliberately put in it. A fixture
//!    that contains a window layout and projects zero windows is the exact
//!    signature of a wrong path.

mod common;

use settings_model::{
    extract_categories, overview_bools, project, project_edit_history, project_hud,
    project_overview, state_colors, window_layout, Category,
};

/// Pass 1 — no projection may panic on any file in either corpus, and the
/// char/user projections must be safe to run against the *wrong* file kind
/// (the app does exactly this while a character is still unpaired).
#[test]
fn no_projection_panics_on_any_corpus_file() {
    let mut scanned = 0usize;
    for f in common::corpus() {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        scanned += 1;

        let _ = project(&doc);
        let _ = window_layout(&doc);
        let _ = project_hud(&doc, None);
        let _ = project_hud(&doc, Some(&doc));
        let _ = project_edit_history(&doc);
        let _ = project_overview(&doc, None);
        let _ = project_overview(&doc, Some(&doc));
        let _ = overview_bools(&doc);
        let _ = state_colors(&doc);
        let _ = extract_categories(
            &doc,
            &[Category::Layout, Category::Autofill, Category::Overview, Category::OverviewWidths],
        );
    }
    assert!(scanned > 0, "no corpus files decoded");
    eprintln!("projected {scanned} distinct corpus file(s)");
}

fn fixture(name: &str) -> blue_marshal::Value {
    let f = common::corpus()
        .iter()
        .find(|f| f.synthetic && f.name() == name)
        .unwrap_or_else(|| panic!("synthetic fixture {name} missing — run gen_fixtures"));
    blue_marshal::decode(&f.bytes).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The modern character carries eleven windows, a two-member stack, both HUD
/// anchors and per-tab column widths. Anything reading zero of those is reading
/// the wrong path.
#[test]
fn modern_character_projects_its_whole_layout() {
    let doc = fixture("core_char_90000001.dat");

    let layout = window_layout(&doc);
    assert_eq!((layout.reference_w, layout.reference_h), (2560, 1440));
    assert!(layout.windows.len() >= 11, "windows: {}", layout.windows.len());
    assert!(
        layout.windows.iter().any(|w| w.id == "overview" && w.open),
        "the overview window should project as open"
    );
    // Stringified-tuple window ids are real ids, not junk to be dropped.
    assert!(
        layout.windows.iter().any(|w| w.id.starts_with("('corpassets'")),
        "tuple-keyed window ids should survive projection"
    );

    assert_eq!(layout.stacks.len(), 1, "one stack");
    let stack = &layout.stacks[0];
    assert_eq!(stack.container_id, "7001");
    assert_eq!(stack.members, vec!["addressbook".to_string(), "calendar".to_string()]);

    let hud = project_hud(&doc, None);
    for name in ["ship_offset", "fighter_x", "fighter_y", "badge_x", "badge_y"] {
        let e = hud.entries.iter().find(|e| e.name == name).expect("field projected");
        assert!(e.value.is_some(), "{name} projected no value from a fixture that has it");
    }

}

/// Overview columns are the one genuinely two-file projection: the tab list and
/// the visible columns come from the account, the widths and sort come from the
/// character. Pairing the two modern fixtures is the only way to exercise the
/// join — either file alone projects half of it.
#[test]
fn overview_columns_join_across_both_files() {
    let user = fixture("core_user_80000001.dat");
    let char_doc = fixture("core_char_90000001.dat");

    let ov = project_overview(&user, Some(&char_doc));
    assert_eq!(ov.tabs.len(), 8);

    // The char fixture carries widths for tabs 0 and 1 only.
    let widths: Vec<usize> = ov
        .tabs
        .iter()
        .filter(|t| t.columns.iter().any(|c| c.width.is_some()))
        .map(|t| t.index as usize)
        .collect();
    assert_eq!(widths, vec![0, 1], "widths should attach to exactly the tabs that have them");

    let tab0 = ov.tabs.iter().find(|t| t.index == 0).expect("tab 0");
    let distance = tab0.columns.iter().find(|c| c.name == "DISTANCE").expect("DISTANCE column");
    assert_eq!(distance.width, Some(63), "width must come from the character file");

    // Without the character half the same account projects tabs but no widths.
    let user_only = project_overview(&user, None);
    assert!(
        user_only.tabs.iter().all(|t| t.columns.iter().all(|c| c.width.is_none())),
        "widths must not appear without the character file"
    );
}

/// The absent-means-default character: reads must return "not set" rather than
/// inventing a zero or panicking.
#[test]
fn minimal_character_projects_absences_as_absences() {
    let doc = fixture("core_char_90000002.dat");

    let layout = window_layout(&doc);
    assert_eq!(layout.windows.len(), 1);
    assert!(layout.stacks.is_empty());
    assert_eq!((layout.reference_w, layout.reference_h), (1920, 1080));

    let hud = project_hud(&doc, None);
    for name in ["ship_offset", "fighter_x", "badge_x"] {
        let e = hud.entries.iter().find(|e| e.name == name).expect("field projected");
        assert!(e.value.is_none(), "{name} should be absent, not defaulted to a value");
    }
}

/// The legacy character stores `shipuialignleftoffset` as `Int` where the modern
/// client writes `Float`. A reader that matches only one variant reads nothing.
#[test]
fn legacy_character_survives_type_instability() {
    let doc = fixture("core_char_90000003.dat");

    let layout = window_layout(&doc);
    assert_eq!((layout.reference_w, layout.reference_h), (1920, 1080));
    assert_eq!(layout.windows.len(), 2);

    let hud = project_hud(&doc, None);
    let ship = hud.entries.iter().find(|e| e.name == "ship_offset").expect("field projected");
    assert!(ship.value.is_some(), "an Int-valued ship offset must still project");
    let badge = hud.entries.iter().find(|e| e.name == "badge_x").expect("field projected");
    assert_eq!(badge.value.as_deref(), Some("36"));
}

/// The modern account carries every account-scoped domain. This asserts the
/// overview projection reaches all of it — tabs, the lopsided window mapping,
/// presets, state lists, colours and the exposed booleans — plus autofill.
#[test]
fn modern_account_projects_its_whole_overview() {
    let doc = fixture("core_user_80000001.dat");

    let ov = project_overview(&doc, None);
    assert_eq!(ov.tabs.len(), 8, "eight tabs from tabsettings_new");
    assert_eq!(ov.windows.len(), 3, "6+1+1 window mapping");
    assert_eq!(ov.windows[0].tab_indices, vec![0, 1, 2, 3, 4, 7]);
    assert_eq!(ov.presets.len(), 4);
    assert!(
        ov.presets.iter().any(|p| p.name == "hostile" && !p.filtered_states.is_empty()),
        "preset exception lists should project"
    );
    // tabsettings_new wins over the legacy mirror and the abandoned generation.
    assert!(
        ov.tabs.iter().all(|t| t.name.trim() != "legacy" && t.name.trim() != "stale"),
        "tabs must come from tabsettings_new, not tabsettings/tabsettings2"
    );

    assert!(!ov.appearance.defaulted, "the account has customised its states");
    assert!(!ov.appearance.colors.is_empty(), "state colours should project");
    assert_eq!(overview_bools(&doc).len(), 6, "the six exposed appearance booleans");
    assert!(!state_colors(&doc).is_empty());

    let history = project_edit_history(&doc);
    assert_eq!(history.len(), 2, "two remembered-text widgets");
    assert!(history.iter().any(|l| l.entries.len() == 2));
}

/// A clean account: the overview container exists but is empty. Every read must
/// degrade to "defaulted", not to an error or a phantom tab.
#[test]
fn clean_account_projects_as_defaulted() {
    let doc = fixture("core_user_80000002.dat");

    let ov = project_overview(&doc, None);
    assert!(ov.tabs.is_empty());
    assert!(ov.presets.is_empty());
    assert!(ov.appearance.defaulted, "no state keys at all means defaulted");
    assert!(project_edit_history(&doc).is_empty());
    assert!(overview_bools(&doc).is_empty());
}

/// The legacy account has `tabsettings` and no `tabsettings_new`, and stores its
/// booleans as `Int`. Both must still read.
#[test]
fn legacy_account_reads_through_the_legacy_tab_key() {
    let doc = fixture("core_user_80000003.dat");

    let ov = project_overview(&doc, None);
    assert_eq!(ov.tabs.len(), 2, "tabs must fall back to the legacy `tabsettings` key");
    assert_eq!(ov.presets.len(), 1);

    let bools = overview_bools(&doc);
    assert!(
        bools.iter().any(|(k, v)| k == "applyToStructures" && *v),
        "an Int-valued boolean must still read as true: {bools:?}"
    );
}

/// The interning fixture is the direct regression guard for the v0.13.0 class:
/// every list is reached only through a `Ref` to a `Shared` defined at an
/// unrelated sibling key, and the preset's exception lists are bare `Ref`s with
/// no `(FILETIME, _)` wrapper. `reshare` never produces this shape, so nothing
/// else in the suite covers it.
#[test]
fn interned_account_resolves_every_indirection() {
    let doc = fixture("core_user_80000004.dat");

    let ov = project_overview(&doc, None);
    assert!(!ov.appearance.defaulted, "state lists behind Refs must be seen");
    assert!(
        !ov.appearance.background.order.is_empty(),
        "backgroundOrder2 behind a Ref projected empty"
    );
    assert!(
        !ov.appearance.background.enabled.is_empty(),
        "backgroundStates2 behind a Ref projected empty"
    );
    assert!(!ov.appearance.flag.order.is_empty(), "flagOrder2 behind a Ref projected empty");

    assert_eq!(ov.presets.len(), 1, "a Ref-keyed preset name must resolve to one preset");
    let p = &ov.presets[0];
    assert_eq!(p.name, "interned preset");
    assert_eq!(p.filtered_states, vec![36], "bare-Ref exception list projected empty");
    assert_eq!(p.always_shown_states, vec![13], "bare-Ref exception list projected empty");

    assert_eq!(ov.tabs.len(), 1);
    assert_eq!(ov.tabs[0].preset, "interned preset", "a Ref-valued tab preset must resolve");
}

/// Batch extraction must find a subtree for every category on a file that has
/// one, and none on a file that does not — the copy path silently doing nothing
/// is indistinguishable from success at the UI level.
#[test]
fn batch_extraction_finds_each_category_where_it_exists() {
    let char_doc = fixture("core_char_90000001.dat");
    let user_doc = fixture("core_user_80000001.dat");

    let from_char = extract_categories(&char_doc, &[Category::Layout, Category::OverviewWidths]);
    assert_eq!(from_char.len(), 2, "char file carries both char-scoped categories");

    let from_user = extract_categories(&user_doc, &[Category::Autofill, Category::Overview]);
    assert_eq!(from_user.len(), 2, "user file carries both account-scoped categories");

    let clean = fixture("core_user_80000002.dat");
    assert!(
        extract_categories(&clean, &[Category::Autofill]).is_empty(),
        "a file without editHistory must yield no autofill subtree"
    );
}
