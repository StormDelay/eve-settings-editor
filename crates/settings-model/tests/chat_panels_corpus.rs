//! Real-data guard for the chat-split SECTION. Every unit test in `chat.rs`
//! builds its own fixture, so naming the wrong section passes them all while
//! projecting nothing from a real account file — the v0.15.0 `badge_*` class of
//! bug. Only real files catch it.
//!
//! Skips silently when the corpus is not checked out.

mod common;

use settings_model::project_chat;

/// Enough sightings to mean "the section is right", not "one odd file". Set
/// well below the real fractions documented in format-notes.md ("Chat window
/// splits": 86/184 account files carry a member-list width, 121/184 an input
/// height), so the guard holds regardless of how many files a given corpus
/// checkout has.
const ENOUGH: usize = 5;

#[test]
fn chat_splits_read_from_real_account_files() {
    if !common::real_corpus_present() {
        return;
    }
    let mut with_width = 0usize;
    let mut with_input = 0usize;
    for f in common::user_files() {
        if with_width >= ENOUGH && with_input >= ENOUGH {
            break;
        }
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        for p in project_chat(&doc) {
            assert!(
                p.window_id.starts_with("chatchannel_"),
                "projected a non-chat window id: {}",
                p.window_id,
            );
            if p.userlist_width.is_some() {
                with_width += 1;
            }
            if p.input_height.is_some() {
                with_input += 1;
            }
        }
    }
    assert!(with_width >= ENOUGH, "only {with_width} member-list widths read from the real corpus");
    assert!(with_input >= ENOUGH, "only {with_input} input heights read from the real corpus");
}
