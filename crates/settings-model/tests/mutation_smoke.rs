//! Write-path counterpart to `projection_smoke.rs`.
//!
//! Every mutation in this crate has unit tests, but they all build their own
//! fixture — a plain, fully-inlined tree with no `Shared`/`Ref` in it. Real
//! files are not like that, and the failure mode is specific: a structural edit
//! that drops a `Shared` **definition** the rest of the file still `Ref`s
//! produces a tree that projects fine, mutates fine, and then fails to encode
//! (`RefBeforeStore`) or fails the save chain's verify step. The user sees
//! "save failed" with no way to tell which edit did it.
//!
//! So every test here runs the app's actual chain — mutate, `reshare`, encode,
//! decode, bit-exact verify (`ops.rs` reshares after each edit; `save.rs` steps
//! 1-2 encode and verify) — and then re-projects to prove the edit is really
//! there after the round trip, not just in the in-memory tree.
//!
//! The `core_user_80000004.dat` fixture is the point of the exercise: every list
//! in it is reached through a `Ref` to a `Shared` defined at an unrelated
//! sibling key, which is the shape `reshare` never produces and hand-built unit
//! fixtures never have.

mod common;

use blue_marshal::Value;
use settings_model::{
    add_overview_window, add_overview_window_geometry, add_to_stack, apply_pack, apply_to_tree,
    clear_all_history, create_preset, create_preset_from_lists, create_stack, create_tab,
    delete_preset, delete_tab, emit_pack, extract_categories, fork_preset, move_tab,
    overview_bools, parse_pack, project_edit_history, project_hud, project_overview, read_pack,
    remove_overview_window, remove_overview_window_geometry, rename_preset, rename_tab,
    reorder_stack, reorder_tabs_in_window, set_column_order, set_column_visible, set_column_width,
    set_hud_value, set_list_entries, set_overview_bool, set_preset_groups, set_preset_states,
    set_state_color, set_state_list, set_tab_preset, unstack, window_layout, Category, StateList,
};

fn fixture(name: &str) -> Value {
    let f = common::corpus()
        .iter()
        .find(|f| f.synthetic && f.name() == name)
        .unwrap_or_else(|| panic!("synthetic fixture {name} missing — run gen_fixtures"));
    blue_marshal::decode(&f.bytes).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The app's real post-edit chain. `ops.rs` reshares the document after every
/// mutation; `save.rs` then encodes it and verifies by decoding its own output
/// and comparing bit-exactly, refusing the write on a mismatch. Returns the tree
/// as it would come back off disk, so assertions run against the *reloaded*
/// value rather than the in-memory one.
#[track_caller]
fn saved(v: &Value) -> Value {
    let reshared = blue_marshal::reshare(v);
    assert!(
        blue_marshal::inline(&reshared).bits_eq(&blue_marshal::inline(v)),
        "reshare changed the edited value"
    );
    let bytes = blue_marshal::encode(&reshared)
        .unwrap_or_else(|e| panic!("edited tree does not encode — save would fail: {e}"));
    let back = blue_marshal::decode(&bytes)
        .unwrap_or_else(|e| panic!("encoded bytes do not decode — save would fail: {e}"));
    assert!(
        back.bits_eq(&reshared),
        "save-chain verify would reject this write: decode(encode(x)) != x"
    );
    back
}

const MODERN_CHAR: &str = "core_char_90000001.dat";
const MINIMAL_CHAR: &str = "core_char_90000002.dat";
const LEGACY_CHAR: &str = "core_char_90000003.dat";
const MODERN_USER: &str = "core_user_80000001.dat";
const CLEAN_USER: &str = "core_user_80000002.dat";
const LEGACY_USER: &str = "core_user_80000003.dat";
const INTERNED_USER: &str = "core_user_80000004.dat";

fn hud_value(v: &Value, name: &str) -> Option<String> {
    project_hud(v, None).entries.into_iter().find(|e| e.name == name).and_then(|e| e.value)
}

// ------------------------------------------------------------------ HUD

#[test]
fn hud_edits_survive_the_save_chain() {
    let mut v = fixture(MODERN_CHAR);
    set_hud_value(&mut v, "ship_offset", "-150").expect("set ship offset");
    set_hud_value(&mut v, "badge_x", "1000").expect("set badge x");
    let back = saved(&v);
    assert_eq!(hud_value(&back, "ship_offset").as_deref(), Some("-150"));
    assert_eq!(hud_value(&back, "badge_x").as_deref(), Some("1000"));
    // The sibling element of the same tuple must be untouched.
    assert_eq!(hud_value(&back, "badge_y").as_deref(), Some("131"));
}

/// Minting a key EVE has never written is its own path: the file has no
/// `shipuialignleftoffset` and no `notification_badge_offset` at all, so the
/// write has to create the leaf (and its `notifications` section) from nothing.
#[test]
fn hud_edits_mint_absent_keys() {
    let mut v = fixture(MINIMAL_CHAR);
    assert!(hud_value(&v, "ship_offset").is_none(), "fixture should start absent");

    set_hud_value(&mut v, "ship_offset", "-42").expect("mint ship offset");
    set_hud_value(&mut v, "badge_x", "17").expect("mint badge x");
    let back = saved(&v);
    assert_eq!(hud_value(&back, "ship_offset").as_deref(), Some("-42"));
    assert_eq!(hud_value(&back, "badge_x").as_deref(), Some("17"));
}

/// The legacy character stores the offset as `Int`. Reading it is fixed; writing
/// over it must also work rather than leaving the old value in place.
#[test]
fn hud_edit_overwrites_an_int_valued_float_field() {
    let mut v = fixture(LEGACY_CHAR);
    assert_eq!(hud_value(&v, "ship_offset").as_deref(), Some("0"));
    set_hud_value(&mut v, "ship_offset", "-88").expect("overwrite int-valued offset");
    let back = saved(&v);
    assert_eq!(hud_value(&back, "ship_offset").as_deref(), Some("-88"));
}

// --------------------------------------------------------------- stacks

#[test]
fn stack_edits_survive_the_save_chain() {
    // Unstack one member of the two-member stack.
    let mut v = fixture(MODERN_CHAR);
    unstack(&mut v, "calendar").expect("unstack");
    let back = saved(&v);
    let layout = window_layout(&back, None);
    assert!(
        layout.stacks.iter().all(|s| !s.members.contains(&"calendar".to_string())),
        "calendar should no longer be a stack member"
    );
    assert!(
        layout.windows.iter().any(|w| w.id == "calendar"),
        "an unstacked window must still exist as a free window"
    );

    // Add a free window to the existing stack, then reorder it to the front.
    let mut v = fixture(MODERN_CHAR);
    add_to_stack(&mut v, "market", "7001").expect("add to stack");
    reorder_stack(
        &mut v,
        "7001",
        &["market".to_string(), "addressbook".to_string(), "calendar".to_string()],
    )
    .expect("reorder stack");
    let back = saved(&v);
    let stack = window_layout(&back, None)
        .stacks
        .into_iter()
        .find(|s| s.container_id == "7001")
        .expect("stack still present");
    assert_eq!(stack.members, vec!["market", "addressbook", "calendar"]);
}

/// Minting a stack container is the structural edit with the most moving parts:
/// a free numeric id, a new rect, and entries in five sibling flag dicts.
#[test]
fn creating_a_stack_mints_a_free_container_that_round_trips() {
    let mut v = fixture(MODERN_CHAR);
    let before: Vec<String> =
        window_layout(&v, None).windows.into_iter().map(|w| w.id).collect();

    let container = create_stack(&mut v, "market", "fitting").expect("create stack");
    assert!(!before.contains(&container), "minted id {container} was already in use");

    let back = saved(&v);
    let layout = window_layout(&back, None);
    let stack = layout
        .stacks
        .iter()
        .find(|s| s.container_id == container)
        .expect("new stack projects after the round trip");
    assert_eq!(stack.members, vec!["market", "fitting"]);
    // The pre-existing stack must be untouched.
    assert!(layout.stacks.iter().any(|s| s.container_id == "7001"));
}

// ------------------------------------------------------ overview columns

#[test]
fn column_edits_survive_the_save_chain_across_both_files() {
    let mut user = fixture(MODERN_USER);
    let mut char_tree = fixture(MODERN_CHAR);

    set_column_visible(&mut user, 0, "TRANSVERSALVELOCITY", true).expect("show column");
    set_column_visible(&mut user, 0, "TYPE", false).expect("hide column");
    set_column_order(
        &mut user,
        0,
        &["NAME".to_string(), "ICON".to_string(), "DISTANCE".to_string()],
    )
    .expect("reorder columns");
    set_column_width(&mut char_tree, 0, "DISTANCE", 99).expect("set width");

    let user = saved(&user);
    let char_tree = saved(&char_tree);

    let ov = project_overview(&user, Some(&char_tree));
    let tab0 = ov.tabs.iter().find(|t| t.index == 0).expect("tab 0");
    let visible: Vec<&str> =
        tab0.columns.iter().filter(|c| c.visible).map(|c| c.name.as_str()).collect();
    assert!(visible.contains(&"TRANSVERSALVELOCITY"), "added column not visible: {visible:?}");
    assert!(!visible.contains(&"TYPE"), "hidden column still visible: {visible:?}");

    let order: Vec<&str> = tab0.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(&order[..3], &["NAME", "ICON", "DISTANCE"], "order not applied: {order:?}");

    let distance = tab0.columns.iter().find(|c| c.name == "DISTANCE").expect("DISTANCE");
    assert_eq!(distance.width, Some(99));
}

// --------------------------------------------------------- overview tabs

#[test]
fn tab_edits_survive_the_save_chain() {
    let mut v = fixture(MODERN_USER);
    let new_idx = create_tab(&mut v, 0, "  new  ", Some(0)).expect("create tab");
    rename_tab(&mut v, 1, "  renamed  ").expect("rename tab");
    set_tab_preset(&mut v, 2, "structures").expect("retarget tab");

    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert_eq!(ov.tabs.len(), 9, "one tab added");
    assert!(ov.tabs.iter().any(|t| t.index == new_idx && t.name.trim() == "new"));
    assert_eq!(ov.tabs.iter().find(|t| t.index == 1).unwrap().name.trim(), "renamed");
    assert_eq!(ov.tabs.iter().find(|t| t.index == 2).unwrap().preset, "structures");
    assert!(
        ov.windows[0].tab_indices.contains(&new_idx),
        "a tab created in window 0 must be mapped into window 0"
    );
}

#[test]
fn tab_move_and_reorder_survive_the_save_chain() {
    let mut v = fixture(MODERN_USER);
    // Window 0 holds tabs [0,1,2,3,4,7] named ["*","2","3","4","5","8"];
    // window 1 holds [5] ("6"); window 2 holds [6] ("7").
    move_tab(&mut v, 7, 0, 1, 0).expect("move tab 7 to the front of window 1");
    reorder_tabs_in_window(&mut v, 0, &[4, 3, 2, 1, 0]).expect("reorder window 0");

    let back = saved(&v);
    let ov = project_overview(&back, None);
    // Order is asserted through the NAMES, not the indices: EVE draws a window's
    // tabs in ascending index, so a reorder renumbers the table and the strip
    // stays ascending. Asserting the strip permutation would pass on exactly the
    // file that shows the old order in game.
    let shown = |w: usize| -> Vec<String> {
        ov.windows[w].tab_indices.iter()
            .map(|i| ov.tabs.iter().find(|t| t.index == *i).expect("mapped tab exists").name.trim().to_string())
            .collect()
    };
    assert_eq!(shown(1), vec!["8", "6"], "moved tab lands at the requested position");
    assert_eq!(shown(0), vec!["5", "4", "3", "2", "*"], "window 0 in the requested order");
    assert_eq!(shown(2), vec!["7"], "an untouched window keeps its tab");
    assert!(ov.windows.iter().all(|w| w.tab_indices.windows(2).all(|p| p[0] < p[1])),
        "every strip stays ascending, as the client writes them");

    // Every tab must still be mapped into exactly one window.
    let mut seen: Vec<i64> = ov.windows.iter().flat_map(|w| w.tab_indices.clone()).collect();
    seen.sort_unstable();
    let mut unique = seen.clone();
    unique.dedup();
    assert_eq!(seen, unique, "a tab ended up in two windows: {seen:?}");
    assert_eq!(seen.len(), ov.tabs.len(), "every tab must be mapped exactly once");
}

#[test]
fn tab_deletion_survives_the_save_chain() {
    let mut v = fixture(MODERN_USER);
    let gone = project_overview(&v, None)
        .tabs
        .iter()
        .find(|t| t.index == 5)
        .expect("fixture has a tab 5")
        .name
        .clone();

    delete_tab(&mut v, 5).expect("delete tab");
    let back = saved(&v);
    let ov = project_overview(&back, None);

    assert_eq!(ov.tabs.len(), 7);
    assert!(!ov.tabs.iter().any(|t| t.name == gone), "the deleted tab is gone: {gone}");
    // Gap-free afterwards — EVE draws a blank, unnamed tab in any gap, so index 5
    // is now the tab that used to be 6, not an absence. Checking indices rather
    // than "5 is unmapped" is the difference the renumbering makes.
    let mut idx: Vec<i64> = ov.tabs.iter().map(|t| t.index).collect();
    idx.sort_unstable();
    assert_eq!(idx, (0..7).collect::<Vec<_>>(), "no gap where tab 5 was");

    let mut mapped: Vec<i64> = ov.windows.iter().flat_map(|w| w.tab_indices.clone()).collect();
    mapped.sort_unstable();
    assert_eq!(mapped, idx, "every remaining tab still mapped into exactly one window");
}

/// Adding an overview window touches both files: the mapping in the account, the
/// geometry in every paired character.
#[test]
fn overview_window_add_and_remove_survive_across_both_files() {
    let mut user = fixture(MODERN_USER);
    let mut char_tree = fixture(MODERN_CHAR);

    let idx = add_overview_window(&mut user, "  extra  ", Some(0)).expect("add window");
    add_overview_window_geometry(&mut char_tree, idx);

    let user_back = saved(&user);
    let char_back = saved(&char_tree);
    let ov = project_overview(&user_back, Some(&char_back));
    assert_eq!(ov.windows.len(), 4, "a fourth overview window");
    assert!(!ov.windows[idx].tab_indices.is_empty(), "the new window must own its tab");
    assert!(
        window_layout(&char_back, None).windows.iter().any(|w| w.id == format!("overview_{idx}")),
        "the new window needs geometry in the character file"
    );

    // And back out again.
    let mut user = user_back;
    let mut char_tree = char_back;
    remove_overview_window(&mut user, idx).expect("remove window");
    remove_overview_window_geometry(&mut char_tree, idx);
    let user_back = saved(&user);
    let char_back = saved(&char_tree);
    assert_eq!(project_overview(&user_back, Some(&char_back)).windows.len(), 3);
    assert!(
        !window_layout(&char_back, None).windows.iter().any(|w| w.id == format!("overview_{idx}")),
        "removing the window must remove its geometry too"
    );
}

/// The legacy account has no `tabsettings_new`. The first structural edit has to
/// migrate the key, which is a rename of a dict key on a real-shaped tree.
#[test]
fn editing_a_legacy_account_migrates_the_tab_key_and_round_trips() {
    let mut v = fixture(LEGACY_USER);
    rename_tab(&mut v, 0, "  migrated  ").expect("rename through the legacy key");
    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert_eq!(ov.tabs.len(), 2, "no tab lost in the migration");
    assert_eq!(ov.tabs.iter().find(|t| t.index == 0).unwrap().name.trim(), "migrated");
}

// ------------------------------------------------------------- presets

#[test]
fn preset_edits_survive_the_save_chain() {
    let mut v = fixture(MODERN_USER);
    create_preset(&mut v, "hostile", "hostile copy").expect("copy preset");
    set_preset_groups(&mut v, "hostile copy", &[6, 7, 11, 65]).expect("set groups");
    set_preset_states(&mut v, "hostile copy", &[36, 37], &[13]).expect("set exceptions");
    rename_preset(&mut v, "friendly", "allies").expect("rename preset");

    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert_eq!(ov.presets.len(), 5);

    let copy = ov.presets.iter().find(|p| p.name == "hostile copy").expect("copy present");
    assert_eq!(copy.groups, vec![6, 7, 11, 65]);
    assert_eq!(copy.filtered_states, vec![36, 37]);
    assert_eq!(copy.always_shown_states, vec![13]);

    assert!(ov.presets.iter().any(|p| p.name == "allies"));
    assert!(!ov.presets.iter().any(|p| p.name == "friendly"));
    // Renaming must retarget every tab that referenced the old name.
    assert!(
        ov.tabs.iter().all(|t| t.preset != "friendly"),
        "a tab still points at the old preset name"
    );
}

#[test]
fn preset_deletion_reassigns_tabs_and_round_trips() {
    let mut v = fixture(MODERN_USER);
    let users: Vec<i64> = project_overview(&v, None)
        .tabs
        .iter()
        .filter(|t| t.preset == "friendly")
        .map(|t| t.index)
        .collect();
    assert!(!users.is_empty(), "fixture should have tabs using this preset");

    delete_preset(&mut v, "friendly").expect("delete preset");
    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert_eq!(ov.presets.len(), 3);
    for idx in users {
        let tab = ov.tabs.iter().find(|t| t.index == idx).expect("tab survives");
        assert_ne!(tab.preset, "friendly", "tab {idx} was left pointing at a deleted preset");
        assert!(
            ov.presets.iter().any(|p| p.name == tab.preset),
            "tab {idx} was reassigned to a preset that does not exist: {}",
            tab.preset
        );
    }
}

/// A clean account has no `overviewProfilePresets` container at all, so the
/// first preset write has to mint it — the zero-timestamp mint the
/// default-profile support relies on.
#[test]
fn creating_a_preset_on_a_clean_account_mints_the_container() {
    let mut v = fixture(CLEAN_USER);
    assert!(project_overview(&v, None).presets.is_empty());

    create_preset_from_lists(&mut v, "minted", &[1, 5, 11], &[36], &[13]).expect("mint preset");
    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert_eq!(ov.presets.len(), 1);
    assert_eq!(ov.presets[0].name, "minted");
    assert_eq!(ov.presets[0].groups, vec![1, 5, 11]);
}

#[test]
fn forking_a_default_preset_round_trips() {
    let mut v = fixture(MODERN_USER);
    fork_preset(&mut v, 0, "forked", &[1, 2, 3], &[], &[]).expect("fork onto tab 0");
    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert!(ov.presets.iter().any(|p| p.name == "forked" && p.groups == vec![1, 2, 3]));
    assert_eq!(
        ov.tabs.iter().find(|t| t.index == 0).unwrap().preset,
        "forked",
        "forking must point the tab at the new copy"
    );
}

// ------------------------------------------------ states, colours, bools

#[test]
fn state_edits_survive_the_save_chain() {
    let mut v = fixture(MODERN_USER);
    set_state_list(&mut v, StateList::Background, &[9, 13, 44]).expect("set enabled states");
    set_state_list(&mut v, StateList::BackgroundOrder, &[44, 13, 9, 68]).expect("set order");
    set_state_color(&mut v, "background", 13, Some([0.1, 0.2, 0.3, 1.0])).expect("set colour");
    // `None` means "remove the override and fall back to EVE's default" — it
    // must not be conflated with writing black.
    set_state_color(&mut v, "background", 44, None).expect("reset colour");
    set_state_color(&mut v, "flag", 13, Some([0.0, 0.0, 0.0, 1.0])).expect("set colortag colour");
    set_overview_bool(&mut v, "hideCorpTicker", true).expect("set bool");

    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert_eq!(ov.appearance.background.enabled, vec![9, 13, 44]);
    assert_eq!(ov.appearance.background.order, vec![44, 13, 9, 68]);

    let colors = ov.appearance.colors;
    let thirteen = colors.iter().find(|(id, _)| *id == 13).expect("colour 13 set");
    assert_eq!(thirteen.1, [0.1, 0.2, 0.3, 1.0]);
    assert!(
        !colors.iter().any(|(id, _)| *id == 44),
        "resetting a colour must remove the entry, not write one"
    );
    // The colortag surface survives the round trip on its own key, and the
    // background entry for the same state id is untouched by it.
    assert_eq!(ov.appearance.flag_colors, vec![(13, [0.0, 0.0, 0.0, 1.0])]);

    assert!(overview_bools(&back).iter().any(|(k, v)| k == "hideCorpTicker" && *v));
}

/// A clean account carries none of the four state keys; the first edit has to
/// materialise just the one it touched and leave the others absent.
#[test]
fn first_state_edit_on_a_clean_account_materialises_one_key() {
    let mut v = fixture(CLEAN_USER);
    assert!(project_overview(&v, None).appearance.defaulted);

    set_state_list(&mut v, StateList::Flag, &[13, 44]).expect("materialise flagStates2");
    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert_eq!(ov.appearance.flag.enabled, vec![13, 44]);
    assert!(
        ov.appearance.background.enabled.is_empty(),
        "editing one state list must not materialise the others"
    );
}

/// Writing a boolean the account stored as `Int` must land as a value the
/// projection reads back, whichever wire kind it chooses.
#[test]
fn overwriting_an_int_valued_bool_round_trips() {
    let mut v = fixture(LEGACY_USER);
    assert!(overview_bools(&v).iter().any(|(k, v)| k == "applyToStructures" && *v));
    set_overview_bool(&mut v, "applyToStructures", false).expect("flip int-valued bool");
    let back = saved(&v);
    assert!(
        overview_bools(&back).iter().any(|(k, v)| k == "applyToStructures" && !*v),
        "flipped value did not read back: {:?}",
        overview_bools(&back)
    );
}

// ------------------------------------------------------------- autofill

#[test]
fn autofill_edits_survive_the_save_chain() {
    let mut v = fixture(MODERN_USER);
    let widget = project_edit_history(&v)[0].widget.clone();
    set_list_entries(&mut v, &widget, &["only one".to_string()]).expect("replace list");

    let back = saved(&v);
    let lists = project_edit_history(&back);
    let edited = lists.iter().find(|l| l.widget == widget).expect("widget survives");
    assert_eq!(edited.entries, vec!["only one".to_string()]);
    assert_eq!(lists.len(), 2, "the other widget's list must be untouched");
}

#[test]
fn clearing_all_history_survives_the_save_chain() {
    let mut v = fixture(MODERN_USER);
    clear_all_history(&mut v).expect("clear");
    let back = saved(&v);
    // "Clear all" empties every list but keeps the widget keys — the client
    // rewrites them anyway, and dropping them would be a structural edit.
    let lists = project_edit_history(&back);
    assert_eq!(lists.len(), 2, "widget keys are kept");
    assert!(lists.iter().all(|l| l.entries.is_empty()), "every list must be emptied: {lists:?}");
    // Clearing autofill must not disturb the rest of the account.
    assert_eq!(project_overview(&back, None).tabs.len(), 8);
}

// ------------------------------------------------------ packs and batch

#[test]
fn applying_a_pack_onto_a_clean_account_survives_the_save_chain() {
    let source = fixture(MODERN_USER);
    let (pack, _) = read_pack(&source);
    assert!(!pack.sections.is_empty(), "the modern account should export a pack");
    let reparsed = parse_pack(&emit_pack(&pack)).expect("emitted pack parses");

    let mut target = fixture(CLEAN_USER);
    apply_pack(&mut target, &reparsed).expect("apply pack");
    let back = saved(&target);

    let (roundtripped, _) = read_pack(&back);
    assert_eq!(
        roundtripped.sections, pack.sections,
        "importing a pack then re-exporting it lost data"
    );
    // No tab may end up in two windows — the C1 guard, on the mint path.
    let ov = project_overview(&back, None);
    let mut seen: Vec<i64> = ov.windows.iter().flat_map(|w| w.tab_indices.clone()).collect();
    seen.sort_unstable();
    let mut unique = seen.clone();
    unique.dedup();
    assert_eq!(seen, unique, "a tab landed in two windows after import: {seen:?}");
}

#[test]
fn batch_category_copy_survives_the_save_chain() {
    let source_char = fixture(MODERN_CHAR);
    let source_user = fixture(MODERN_USER);

    let mut target_char = fixture(MINIMAL_CHAR);
    apply_to_tree(
        &mut target_char,
        &extract_categories(&source_char, &[Category::Layout, Category::OverviewWidths]),
    );
    let target_char = saved(&target_char);
    let layout = window_layout(&target_char, None);
    assert!(layout.windows.len() >= 11, "layout did not copy: {}", layout.windows.len());
    assert_eq!(layout.stacks.len(), 1, "stacks must come along with the layout");

    let mut target_user = fixture(CLEAN_USER);
    apply_to_tree(
        &mut target_user,
        &extract_categories(&source_user, &[Category::Overview, Category::Autofill]),
    );
    let target_user = saved(&target_user);
    assert_eq!(project_overview(&target_user, None).tabs.len(), 8);
    assert_eq!(project_edit_history(&target_user).len(), 2);
}

// ------------------------------------------------------- the hard case

/// The whole reason this file exists. Every list in this account is reached only
/// through a `Ref` to a `Shared` defined at an unrelated sibling key, and the
/// preset's exception lists are bare `Ref`s with no `(FILETIME, _)` wrapper.
/// A structural edit that replaces one of those lists without first inlining the
/// sharing destroys a definition other `Ref`s still point at, and the file stops
/// encoding. Nothing else in the suite has this shape.
#[test]
fn edits_on_an_interned_account_do_not_break_its_sharing() {
    // A state list whose current value is shared with a second key.
    let mut v = fixture(INTERNED_USER);
    set_state_list(&mut v, StateList::Background, &[1, 2, 3]).expect("replace a shared list");
    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert_eq!(ov.appearance.background.enabled, vec![1, 2, 3]);
    assert!(
        !ov.appearance.flag.enabled.is_empty(),
        "flagStates2 shared the same definition and must survive the edit"
    );

    // A preset whose NAME is a Ref and whose exception lists are bare Refs.
    let mut v = fixture(INTERNED_USER);
    set_preset_states(&mut v, "interned preset", &[9], &[10, 11]).expect("replace bare-Ref lists");
    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert_eq!(ov.presets[0].filtered_states, vec![9]);
    assert_eq!(ov.presets[0].always_shown_states, vec![10, 11]);
    assert_eq!(
        ov.tabs[0].preset, "interned preset",
        "the tab's Ref-valued preset name must still resolve"
    );

    // Renaming the Ref-keyed preset: the key, the tab reference and the
    // `activeOverviewPreset` value are all the same shared byte string.
    let mut v = fixture(INTERNED_USER);
    rename_preset(&mut v, "interned preset", "renamed").expect("rename a Ref-keyed preset");
    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert_eq!(ov.presets.len(), 1);
    assert_eq!(ov.presets[0].name, "renamed");
    assert_eq!(ov.tabs[0].preset, "renamed", "the tab reference must be retargeted");

    // A structural tab edit on the same file.
    let mut v = fixture(INTERNED_USER);
    let idx = create_tab(&mut v, 0, "  added  ", Some(0)).expect("create tab");
    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert_eq!(ov.tabs.len(), 2);
    assert!(ov.tabs.iter().any(|t| t.index == idx && t.name.trim() == "added"));
}

/// Edits stack: applying several mutations to one document before saving is what
/// the app actually does between saves, and each one reshares. A definition
/// dropped by edit 2 that edit 1 introduced only shows up this way.
#[test]
fn several_edits_in_a_row_still_save() {
    let mut v = fixture(MODERN_USER);
    create_preset(&mut v, "hostile", "temp").expect("create");
    set_preset_groups(&mut v, "temp", &[1, 2]).expect("groups");
    let idx = create_tab(&mut v, 1, "  t  ", None).expect("tab");
    set_tab_preset(&mut v, idx, "temp").expect("point tab at it");
    set_state_list(&mut v, StateList::Flag, &[13]).expect("states");
    set_overview_bool(&mut v, "useSmallText", true).expect("bool");
    rename_preset(&mut v, "temp", "kept").expect("rename");
    delete_preset(&mut v, "basic travel").expect("delete another");

    let back = saved(&v);
    let ov = project_overview(&back, None);
    assert!(ov.presets.iter().any(|p| p.name == "kept" && p.groups == vec![1, 2]));
    assert!(!ov.presets.iter().any(|p| p.name == "basic travel"));
    assert_eq!(ov.tabs.iter().find(|t| t.index == idx).unwrap().preset, "kept");
    assert_eq!(ov.appearance.flag.enabled, vec![13]);
    assert!(overview_bools(&back).iter().any(|(k, v)| k == "useSmallText" && *v));
    for tab in &ov.tabs {
        assert!(
            ov.presets.iter().any(|p| p.name == tab.preset),
            "tab {} points at a preset that no longer exists: {}",
            tab.index,
            tab.preset
        );
    }
}
