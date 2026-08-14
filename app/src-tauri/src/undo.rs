//! The undo stack.
//!
//! All of the machinery lives here, so `ops.rs` grows by call sites and not by
//! mechanism. Three facts make this cheap, and each is load-bearing:
//!
//! 1. **Every document edit funnels through one function.** `edit_reshared` is
//!    the only place `doc.value` is written, apart from `try_edit_char` — a
//!    grep for writes to a document's value returns four lines in two
//!    functions. A snapshot taken there therefore covers every edit the app can
//!    make, *including ones written after this shipped*.
//! 2. **`Value` is a plain owned tree** with no `Rc` and no `Arc`, so a clone
//!    is genuinely independent and a dropped entry's memory is freed.
//! 3. **The cost is already paid elsewhere.** Every edit already calls
//!    `project()` over the whole document and serialises the result to JSON
//!    over IPC; one `clone()` is strictly less work than that.
//!
//! The invariants, in one place:
//!
//! - **One Tauri command produces at most one undo entry** (`Group`). This is
//!   the headline one; the next two are how it is held.
//! - `edit_reshared` is the only pusher. `try_edit_char` never pushes.
//! - Every entry holds BOTH slots. Never one — three commands write the account
//!   file and then the character file inside a single command, and an entry
//!   holding only the slot `edit_reshared` was called for would undo half of
//!   them.
//! - A push happens only after the edit succeeded, and a command that returns
//!   `Err` from a write leaves both documents and both stacks as it found them.
//! - A push clears `redo`.
//! - Slot occupancy is constant for a stack's lifetime: anything that opens or
//!   closes a slot clears the stack. That is what lets `Entry.char == None`
//!   mean "still empty" rather than "unknown", and it is phrased absolutely
//!   because the alternative is data corruption rather than a stale view —
//!   after `open_file` swaps a different character in, an older entry's char
//!   tree belongs to a DIFFERENT pilot's settings file.
//! - Locks are taken user → char → history, without exception.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;
use settings_model::{project, Document, Node};

use crate::ops::{AppState, Slot};

/// Twenty, not fifty.
///
/// The job is small: undo here is "take that back" for the toast plus a short
/// tail. "Revert everything" is already Discard, and it is one click. Memory is
/// linear in this number and a resident tree is a MULTIPLE of the file rather
/// than a fraction of it, so fifty would be a decision to change the snapshot
/// representation, made without measuring.
///
/// The failure modes are asymmetric, which is what settles it: a cap that is
/// too small costs one constant to raise and the user still has Discard, and a
/// cap that is too large costs a settings editor a few hundred megabytes on a
/// machine that is also running EVE.
pub const CAP: usize = 20;

const CHAR: usize = 0;
const USER: usize = 1;

const fn idx(slot: Slot) -> usize {
    match slot {
        Slot::Char => CHAR,
        Slot::User => USER,
    }
}

/// One document's before-state, as ENCODED BYTES.
///
/// The measurement decided this, not the argument. `05b-undo.md` §4 specified
/// a `Value` clone as the default — smallest correct code, no `Result` in
/// either direction — and set a threshold: switch if peak stack memory would
/// exceed 40 MB. Its back-of-envelope guessed a tree-to-file ratio near 10-12.
///
/// Measured over 627 real `core_*.dat` files (`snapshot_footprint`, below):
///
/// ```text
/// median file bytes  121952
/// median tree/file   20.39x
/// max    tree/file   33.72x
/// peak stack (2 x CAP x R x median pair) ~ 190 MB
/// ```
///
/// Twice the estimate, and nearly five times the budget. A settings editor
/// holding 190 MB of resident tree on a machine that is also running EVE is not
/// a trade worth making for slightly smaller code. Encoded, the same twenty
/// steps cost about 10 MB.
///
/// The cost of this form is real and bounded: `encode` returns `Result`, so a
/// snapshot that cannot be taken degrades to "undo unavailable" rather than
/// failing the user's edit (see `Capture::Unavailable`). `Fidelity::Editable`
/// means precisely `encode(decode(bytes)) == bytes` for THIS file, decided once
/// at load — so for the only documents this app will ever edit, the encode
/// cannot fail. That is what makes the fallback lossless rather than hopeful.
///
/// Undo pays one `decode` of ~100 KB per document, at human keypress speed.
pub struct Snap(Vec<u8>);

impl Snap {
    fn of(doc: &Document) -> Option<Snap> {
        blue_marshal::encode(&doc.value).ok().map(Snap)
    }
    /// A restore that cannot decode leaves the document exactly as it stands and
    /// never panics. The outcome is then today's behaviour — the edit is not
    /// reversed — which is a degradation and not a corruption.
    fn restore(self, doc: &mut Document) {
        if let Ok(v) = blue_marshal::decode(&self.0) {
            doc.value = v;
        }
    }
}

/// The before-state of BOTH slots plus their edit counters.
pub struct Entry {
    /// `None` means "that slot was empty". Slot occupancy cannot change while a
    /// stack exists, so `None` here implies `None` now.
    char: Option<Snap>,
    user: Option<Snap>,
    /// Per-slot edit counters as they stood BEFORE this step; `[char, user]`.
    /// Restored with the trees, which is what makes the unsaved badge exact
    /// across an undo rather than merely plausible.
    counters: [u64; 2],
}

impl Entry {
    fn restore_into(
        self,
        u: &mut Option<Document>,
        c: &mut Option<Document>,
        counters: &mut [u64; 2],
    ) {
        debug_assert_eq!(
            self.char.is_some(),
            c.is_some(),
            "slot occupancy changed under a live undo stack — open_file/close_file must clear it",
        );
        if let (Some(s), Some(d)) = (self.char, c.as_mut()) {
            s.restore(d);
        }
        if let (Some(s), Some(d)) = (self.user, u.as_mut()) {
            s.restore(d);
        }
        *counters = self.counters;
    }

    /// Put ONE slot back, for the ungrouped error path: `apply_mutations` runs
    /// its whole batch against one locked doc, so a failure at mutation `k`
    /// leaves `k-1` applied without this.
    pub fn restore_slot(self, doc: &mut Document, slot: Slot) {
        let s = match slot {
            Slot::Char => self.char,
            Slot::User => self.user,
        };
        if let Some(s) = s {
            s.restore(doc);
        }
    }
}

#[derive(Default)]
pub struct History {
    undo: VecDeque<Entry>,
    /// A `Vec`, not a `VecDeque`: it is cleared by any edit and can never exceed
    /// `CAP`, because it only grows by one per undo and undo is bounded by the
    /// undo stack's own depth.
    redo: Vec<Entry>,
    /// Monotone per-slot edit counts, `[char, user]`. Bumped at both write
    /// sites.
    counters: [u64; 2],
    /// Counter values as of the last load or save of each slot. Dirty is
    /// `counters[i] != saved[i]`.
    saved: [u64; 2],
    /// `None` outside a group. `Some(false)` inside one that has not pushed its
    /// entry yet, `Some(true)` inside one that has — after which further writes
    /// in the same command bump counters but push nothing.
    group: Option<bool>,
}

/// What a write found when it looked for a before-state. Three outcomes rather
/// than an `Option`, because "this command already pushed" and "this tree could
/// not be encoded" call for opposite handling and an `Option` would collapse
/// them into the same silent no-op.
pub enum Capture {
    /// Rides the entry this command already pushed. Bump the counter, push
    /// nothing — and skip the encode as well as the push.
    Grouped,
    /// The before-state, ready to push.
    Taken(Entry),
    /// A document would not encode. The edit still happens; the STACK is
    /// dropped, because an entry silently missing from the middle of it would
    /// make the next undo jump two steps back without saying so.
    Unavailable,
}

impl History {
    pub fn capture_unless_grouped(
        &self,
        u: &Option<Document>,
        c: &Option<Document>,
    ) -> Capture {
        if self.group == Some(true) {
            return Capture::Grouped;
        }
        match self.capture(u, c) {
            Some(e) => Capture::Taken(e),
            None => Capture::Unavailable,
        }
    }

    fn capture(&self, u: &Option<Document>, c: &Option<Document>) -> Option<Entry> {
        // `?` inside the `match`, not `.map(Snap::of)`: an empty slot and an
        // unencodable one must not both arrive as `None`, because the first is
        // normal and the second must drop the stack.
        Some(Entry {
            char: match c {
                Some(d) => Some(Snap::of(d)?),
                None => None,
            },
            user: match u {
                Some(d) => Some(Snap::of(d)?),
                None => None,
            },
            counters: self.counters,
        })
    }

    /// Bumps `counters[slot]` unconditionally — the dirty flag counts WRITES,
    /// not undo steps. Pushes only when a before-state was actually taken.
    pub fn push(&mut self, before: Capture, slot: Slot) {
        self.counters[idx(slot)] += 1;
        let entry = match before {
            Capture::Taken(e) => e,
            Capture::Grouped => return,
            Capture::Unavailable => {
                self.undo.clear();
                self.redo.clear();
                return;
            }
        };
        self.redo.clear();
        // The dropped entry's trees are owned with no sharing, so its memory is
        // freed here: the stack is flat at the cap, not merely bounded by it.
        if self.undo.len() == CAP {
            self.undo.pop_front();
        }
        self.undo.push_back(entry);
        if self.group.is_some() {
            self.group = Some(true);
        }
    }

    /// The counter bump alone, for `try_edit_char` — which writes the character
    /// document but must never push, because all three of its call sites run
    /// inside a step `edit_reshared` has already snapshotted.
    pub fn bump(&mut self, slot: Slot) {
        self.counters[idx(slot)] += 1;
    }

    /// Put both documents and both counters back to where the open group found
    /// them, and leave the group open but empty.
    ///
    /// No-op outside a group, and no-op inside one that has not written yet.
    /// `back()` IS this group's entry: a group pushes exactly once, so nothing
    /// can have been stacked on top of it.
    pub fn rollback_group(&mut self, u: &mut Option<Document>, c: &mut Option<Document>) {
        if self.group != Some(true) {
            return;
        }
        let entry = self.undo.pop_back().expect("a filled group has its entry");
        entry.restore_into(u, c, &mut self.counters);
        self.group = Some(false);
    }

    /// Undo one step. `false` when there is nothing to undo — which is not an
    /// error and must not reach an error surface.
    pub fn undo(&mut self, u: &mut Option<Document>, c: &mut Option<Document>) -> bool {
        let Some(entry) = self.undo.pop_back() else { return false };
        // The current state becomes the redo step. If it will not encode there
        // is no redo to offer, which is strictly better than offering one that
        // would restore nothing.
        if let Some(now) = self.capture(u, c) {
            self.redo.push(now);
        }
        entry.restore_into(u, c, &mut self.counters);
        true
    }

    pub fn redo(&mut self, u: &mut Option<Document>, c: &mut Option<Document>) -> bool {
        let Some(entry) = self.redo.pop() else { return false };
        if let Some(now) = self.capture(u, c) {
            self.undo.push_back(now);
        }
        entry.restore_into(u, c, &mut self.counters);
        true
    }

    /// Everything a slot being replaced or emptied invalidates.
    ///
    /// BOTH stacks, even though only one slot changed. An entry holds trees for
    /// both slots, so after a different character lands in the char slot an
    /// older entry's char tree belongs to another pilot's file — restoring it
    /// would silently write one pilot's window geometry into another's.
    pub fn clear_for(&mut self, slot: Slot) {
        self.undo.clear();
        self.redo.clear();
        self.counters[idx(slot)] = 0;
        self.saved[idx(slot)] = 0;
    }

    /// On a successful save only. The stack is deliberately NOT cleared: you can
    /// undo past a save, and the file is then correctly reported unsaved again,
    /// because memory now differs from disk.
    pub fn mark_saved(&mut self, slot: Slot) {
        self.saved[idx(slot)] = self.counters[idx(slot)];
    }

    pub fn dirty(&self, slot: Slot) -> bool {
        self.counters[idx(slot)] != self.saved[idx(slot)]
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn depth(&self) -> usize {
        self.undo.len()
    }
}

/// Makes the rest of this Tauri command ONE undo step: the first write inside
/// the group pushes its entry — which already holds the before-state of BOTH
/// slots — and every later write in the same command rides it.
///
/// Hold it for the whole command, as the FIRST statement, before any slot lock:
///
/// ```ignore
/// let _group = undo::group(state);   // `let _group`, never `let _`
/// ```
///
/// `let _ = undo::group(state);` drops the guard at the end of that statement
/// and the group closes before the first edit. It compiles and it is silently
/// wrong, which is the one mistake worth naming here.
///
/// `Drop` is the whole point rather than a convenience. Every one of the four
/// commands that needs this has a `?` between opening the group and finishing,
/// and a matching `open`/`close` pair would leak the flag down that path. The
/// symptom would not be a crash: the flag stays set, and every LATER command in
/// the session pushes nothing and becomes silently un-undoable.
pub struct Group<'a>(&'a Mutex<History>);

pub fn group(state: &AppState) -> Group<'_> {
    // Takes and RELEASES the history lock. It must not hold it: the next thing
    // the command does is call `edit_reshared`, which takes user → char →
    // history, and a held history lock would deadlock on the third.
    state.history.lock().unwrap().group = Some(false);
    Group(&state.history)
}

impl Drop for Group<'_> {
    fn drop(&mut self) {
        // `unwrap_or_else(into_inner)` rather than `unwrap()`: a panic inside a
        // Drop during an unwind aborts the process, and a poisoned History means
        // the app is already dead — closing the group should not turn that into
        // an abort.
        self.0.lock().unwrap_or_else(|e| e.into_inner()).group = None;
    }
}

// --- The command surface ----------------------------------------------------

#[derive(Debug, Serialize)]
pub struct UndoState {
    pub can_undo: bool,
    pub can_redo: bool,
    /// The frontend shows nothing with this; it is for the tests and for a
    /// future history list. Worth its one field: "one command, one step" is a
    /// claim about the DEPTH, and no other observation can see it.
    pub depth: usize,
}

#[derive(Debug, Serialize)]
pub struct SlotFlags {
    pub char: bool,
    pub user: bool,
}

#[derive(Debug, Serialize)]
pub struct UndoOutcome {
    /// A fresh projection per open slot. Both, always: the Raw view reads
    /// `slots[x].tree` directly and no refresh token can reach it, and two
    /// `project()` calls at human keypress speed are not worth a branch here
    /// AND a branch in the frontend.
    pub char_tree: Option<Node>,
    pub user_tree: Option<Node>,
    /// Authoritative unsaved state per slot, after the step. No frontend-only
    /// scheme can get this right: after edit → save → edit → undo the account
    /// file is clean, and after one more undo it is dirty again, and only
    /// something that knows where the save point sits can tell those apart.
    pub dirty: SlotFlags,
    pub state: UndoState,
}

fn outcome(u: &Option<Document>, c: &Option<Document>, h: &History) -> UndoOutcome {
    UndoOutcome {
        char_tree: c.as_ref().map(|d| project(&d.value)),
        user_tree: u.as_ref().map(|d| project(&d.value)),
        dirty: SlotFlags { char: h.dirty(Slot::Char), user: h.dirty(Slot::User) },
        state: UndoState { can_undo: h.can_undo(), can_redo: h.can_redo(), depth: h.depth() },
    }
}

/// `Option`, not `Result`. "Nothing to undo" is not an error and must not reach
/// an error surface; `None` serialises to `null` and the frontend says so in a
/// plain toast. There is no other failure mode — undo takes no arguments,
/// touches no disk, and restores trees it produced itself.
pub fn undo(state: &AppState) -> Option<UndoOutcome> {
    let mut u = state.user.lock().unwrap();
    let mut c = state.char.lock().unwrap();
    let mut h = state.history.lock().unwrap();
    h.undo(&mut u, &mut c).then(|| outcome(&u, &c, &h))
}

pub fn redo(state: &AppState) -> Option<UndoOutcome> {
    let mut u = state.user.lock().unwrap();
    let mut c = state.char.lock().unwrap();
    let mut h = state.history.lock().unwrap();
    h.redo(&mut u, &mut c).then(|| outcome(&u, &c, &h))
}

pub fn undo_state(state: &AppState) -> UndoState {
    let h = state.history.lock().unwrap();
    UndoState { can_undo: h.can_undo(), can_redo: h.can_redo(), depth: h.depth() }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the measurement walks a raw tree; the stack itself holds bytes.
    use blue_marshal::Value;

    /// `ops.rs` split into (name, body) pairs. Walks lines, starts a body at
    /// each `fn `/`pub fn `, and ends it at the next one.
    fn fns_of(src: &str) -> Vec<(&str, String)> {
        let mut out: Vec<(&str, String)> = Vec::new();
        for line in src.lines() {
            let t = line.trim_start();
            let after_fn = t.strip_prefix("pub fn ").or_else(|| t.strip_prefix("fn "));
            if let Some(rest) = after_fn {
                let name = rest.split(['(', '<', ' ']).next().unwrap_or("");
                out.push((name, String::new()));
            } else if let Some(last) = out.last_mut() {
                last.1.push_str(line);
                last.1.push('\n');
            }
        }
        out
    }

    /// Whether `line` calls `name` — as a whole identifier, not a substring.
    ///
    /// The boundary check earns its keep immediately: without it the helper
    /// `ts(` matches inside `set_chat_splits(`, and the tripwire fires on a
    /// one-write command. A guard that cries wolf gets deleted, which costs the
    /// release it was written for.
    fn calls(line: &str, name: &str) -> bool {
        let pat = format!("{name}(");
        let mut from = 0;
        while let Some(i) = line[from..].find(&pat) {
            let at = from + i;
            let before = line[..at].chars().next_back();
            if !matches!(before, Some(c) if c.is_alphanumeric() || c == '_') {
                return true;
            }
            from = at + 1;
        }
        false
    }

    /// Non-comment lines in `body` that call any of `names`, ignoring `self_`.
    ///
    /// The `self_` exclusion is the second half of the same lesson: several
    /// commands are thin wrappers that share a name with the `settings_model`
    /// function they call — `ops::set_chat_splits` calls
    /// `settings_model::set_chat_splits` — so without it a one-write command
    /// counts its own model call as a second write.
    fn writes(body: &str, names: &[&str], self_: &str) -> usize {
        body.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .filter(|l| names.iter().any(|n| *n != self_ && calls(l, n)))
            .count()
    }

    /// `ops.rs` WITHOUT its test module.
    ///
    /// The spec derives this list "over the non-test half of ops.rs", and the
    /// half matters: the test module's own fixtures are named `ts`, `bb` and
    /// `geom`, and letting the closure pull those into the writer set turns
    /// every fixture builder into a "multi-write command".
    fn ops_src() -> &'static str {
        const OPS_FULL: &str = include_str!("ops.rs");
        match OPS_FULL.find("#[cfg(test)]") {
            Some(i) => &OPS_FULL[..i],
            None => OPS_FULL,
        }
    }

    /// TRIPWIRE A — guards the *a snapshot exists* precondition.
    ///
    /// `try_edit_char` pushes nothing, so a new caller without an
    /// `edit_slot`/`edit_reshared` earlier in the SAME command produces a
    /// char-side change no `Ctrl+Z` can reach.
    ///
    /// This is not hypothetical. The proposal said `try_edit_char` had two call
    /// sites; it has three, and the third arrived with a later PR without anyone
    /// noticing that a rule existed.
    #[test]
    fn try_edit_char_callers_are_snapshotted() {
        const KNOWN: &[&str] = &["tab_delete", "overview_window_add", "overview_window_remove"];
        let fns = fns_of(ops_src());
        let found: Vec<&str> = fns
            .iter()
            .filter(|(name, body)| *name != "try_edit_char" && body.contains("try_edit_char(state,"))
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            found, KNOWN,
            "try_edit_char gained or lost a caller. It does NOT push an undo entry — confirm the \
             new caller runs edit_slot/edit_reshared FIRST in the same command, add an `undo \
             reverts both files` test for it, then update KNOWN."
        );
    }

    /// TRIPWIRE B — guards the *one command, one entry* count.
    ///
    /// A cannot catch B's bug and B cannot catch A's: A pushes zero entries
    /// where one is needed, B pushes two. `group` suppresses pushes AFTER the
    /// first, and A's bug is that there is no first.
    #[test]
    fn multi_write_commands_open_a_group() {
        let fns = fns_of(ops_src());
        // Seed with the three primitives, then close over their callers, so the
        // thin per-view helpers (`edit_user_tabs`, `edit_char_neocom`, …) count
        // as writes and a seventh added tomorrow is picked up without editing a
        // list. A hand-maintained list is exactly what failed in tripwire A.
        let mut writers: Vec<&str> = vec!["edit_reshared", "edit_slot", "try_edit_char"];
        loop {
            let grown: Vec<&str> = fns
                .iter()
                .filter(|(n, b)| !writers.contains(n) && writes(b, &writers, n) > 0)
                .map(|(n, _)| *n)
                .collect();
            if grown.is_empty() {
                break;
            }
            writers.extend(grown);
        }
        for (name, body) in &fns {
            assert!(
                writes(body, &writers, name) < 2 || body.contains("undo::group(state)"),
                "{name} writes to a document more than once without \
                 `let _group = undo::group(state);` on its first line. One Tauri command is one \
                 undo step: add the guard, then add a test that ONE undo reverts all of it."
            );
        }
    }

    /// The measurement §4.3 asks for, `#[ignore]`d so it never runs in CI.
    ///
    /// ```text
    /// cargo test -p eve-settings-editor --lib undo::tests::snapshot_footprint -- --ignored --nocapture
    /// ```
    ///
    /// Point `EVE_CORPUS` at a directory of real `core_*.dat` files. It counts
    /// `capacity()` rather than `len()` and includes the inline
    /// `size_of::<Value>()` for every node, because that is what the allocator
    /// is actually holding; it under-counts only per-allocation malloc headers,
    /// which biases the answer towards "the clone is fine" — that is, AGAINST
    /// changing anything.
    #[test]
    #[ignore = "measurement, not a gate; needs EVE_CORPUS"]
    fn snapshot_footprint() {
        fn heap(v: &Value) -> usize {
            use Value::*;
            std::mem::size_of::<Value>()
                + match v {
                    Long(b) | Bytes(b) | Global(b) => b.capacity(),
                    Str(s) | StrUcs2(s) => s.capacity(),
                    Tuple(xs) | List(xs) => {
                        xs.capacity() * std::mem::size_of::<Value>()
                            + xs.iter().map(heap).sum::<usize>()
                    }
                    Dict(kv) => {
                        kv.capacity() * 2 * std::mem::size_of::<Value>()
                            + kv.iter().map(|(k, x)| heap(k) + heap(x)).sum::<usize>()
                    }
                    Stream(b) | Shared { value: b, .. } => heap(b),
                    Instance { class, state } => heap(class) + heap(state),
                    Reduce { ctor, items, pairs } => {
                        heap(ctor)
                            + items.iter().map(heap).sum::<usize>()
                            + pairs.iter().map(|(k, x)| heap(k) + heap(x)).sum::<usize>()
                    }
                    _ => 0,
                }
        }

        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(rd) = std::fs::read_dir(dir) else { return };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("core_") && n.ends_with(".dat"))
                {
                    out.push(p);
                }
            }
        }

        let root = std::env::var("EVE_CORPUS").expect("set EVE_CORPUS to a directory of .dat files");
        let mut paths = Vec::new();
        walk(std::path::Path::new(&root), &mut paths);
        assert!(!paths.is_empty(), "no core_*.dat under {root}");

        let mut ratios: Vec<f64> = Vec::new();
        let mut file_bytes: Vec<usize> = Vec::new();
        for path in &paths {
            let Ok(bytes) = std::fs::read(path) else { continue };
            let Ok(doc) = Document::load(path) else { continue };
            let tree = heap(&doc.value);
            if bytes.is_empty() {
                continue;
            }
            ratios.push(tree as f64 / bytes.len() as f64);
            file_bytes.push(bytes.len());
        }
        ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
        file_bytes.sort_unstable();
        let median = ratios[ratios.len() / 2];
        let max = *ratios.last().unwrap();
        let median_bytes = file_bytes[file_bytes.len() / 2];
        println!("files              {}", ratios.len());
        println!("median file bytes  {median_bytes}");
        println!("median tree/file   {median:.2}x");
        println!("max    tree/file   {max:.2}x");
        println!(
            "peak stack (2 x CAP x R x median pair) ~ {:.0} MB",
            2.0 * CAP as f64 * median * (median_bytes * 2) as f64 / 1_048_576.0
        );
    }
}
