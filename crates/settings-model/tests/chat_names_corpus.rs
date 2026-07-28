//! Real-data guard for chat-window names. Unit fixtures in `windows.rs` build
//! whatever shape the reader expects, so a wrong key or a missing unwrap passes
//! them all while naming nothing from a real file — the class of bug that
//! shipped in v0.15.0 for the HUD badge offset, and that shipped again here.
//!
//! Both defects this gate exists for were live simultaneously and invisible to
//! four unit tests: the join keyed on the tuple's SECOND element while the
//! window id is built from the FIRST, and it matched a bare `Value::List` while
//! every real file wraps the section as `(timestamp, list)`. Result: 0 of 11,481
//! chat windows named. Either bug alone still names 0 with the other present.
//!
//! Runs on the SYNTHETIC corpus too, deliberately — `gen_fixtures.rs` has
//! carried the correct wrapped, element-0 shape since the synthetic corpus was
//! created, so this gate would have failed from day one on a checkout with no
//! `testdata/` at all. A gate that only runs where the real corpus happens to be
//! present is a gate that does not run on CI.

mod common;

use settings_model::window_layout;

/// Names projected from the committed synthetic corpus alone. It carries one
/// `chatchannels` row (`local`), so anything above zero proves the reader
/// survives; zero means the join broke again.
const ENOUGH_SYNTHETIC: usize = 1;

/// Names projected from real files. Measured: 1,114 across 281 files carrying
/// the section. Set well below that so refreshing the corpus cannot fail this
/// spuriously — a wrong key or a missing unwrap projects 0, which is the whole
/// point, and the exact figure only has to be comfortably above it.
const ENOUGH_REAL: usize = 400;

fn named_in(files: impl Iterator<Item = &'static common::CorpusFile>) -> (usize, usize) {
    let mut chats = 0usize;
    let mut named = 0usize;
    for f in files {
        let Ok(doc) = blue_marshal::decode(&f.bytes) else { continue };
        for w in window_layout(&doc, None).windows {
            if w.id.starts_with("chatchannel_") {
                chats += 1;
                if w.name.is_some() {
                    named += 1;
                }
            }
        }
    }
    (chats, named)
}

#[test]
fn chat_windows_are_named_from_the_synthetic_corpus() {
    let (chats, named) = named_in(common::char_files().filter(|f| f.synthetic));
    eprintln!("synthetic: {chats} chat window(s), {named} named");
    assert!(
        named >= ENOUGH_SYNTHETIC,
        "named {named} chat windows from the synthetic corpus, expected at least \
         {ENOUGH_SYNTHETIC}. The reader has stopped matching the shape \
         gen_fixtures.rs writes — check the tuple element the join keys on and \
         whether `chatchannels` is still unwrapped through its (timestamp, list) \
         wrapper.",
    );
}

#[test]
fn chat_windows_are_named_from_real_files() {
    if !common::real_corpus_present() {
        return;
    }
    let (chats, named) = named_in(common::char_files().filter(|f| !f.synthetic));
    eprintln!("real: {chats} chat window(s), {named} named");
    assert!(
        named >= ENOUGH_REAL,
        "named {named} chat windows from {chats} in real files, expected at least \
         {ENOUGH_REAL}.",
    );
}
