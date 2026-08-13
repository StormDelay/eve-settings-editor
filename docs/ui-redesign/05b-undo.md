# Phase 5b — Undo

**Status:** plan. Nothing here is implemented.
**Depends on:** Phase 5 for the toast component only. Nothing depends on this.
**Changes behaviour:** yes.

---

## 1 Goal, and why it is separable

The app has no undo. Every mutating command writes the in-memory document and
the only way back is `discardChanges()` (`app/src/routes/+page.svelte:219-237`),
which re-reads **both** open files from disk and so reverses every in-memory
edit at once. That is an all-or-nothing escape. This phase adds a per-step one.

The goal is exactly this and nothing more:

> One `Ctrl+Z` reverts the last user action — including one that spanned both
> the character and the account file, and one that the backend performed as two
> writes — and every open view shows the reverted state without the user doing
> anything.

It is separable because Phase 5's argument does not rest on it. Phase 5 removes
58 `message()` modals and corrects the false *"This can't be undone."* copy on
the overview-tab delete; the claim it replaces that copy with is *"Discard
reverses this"*, and Discard already does, today, with no new code. Undo makes
that promise finer-grained. If 5b never ships, Phase 5 is still true.

It is also separable in the other direction: nothing in this spec needs Phase 1
tokens, Phase 2's shell, Phase 3's sheets or Phase 4's inspector. The only
Phase 5 dependency is the `Toast` primitive, and §9 gives the fallback if 5b
lands first.

**What this is not.** Not a history browser, not a "revert to save point", not
an undo for anything that touches disk. §8 draws that line and defends it.

---

## 2 The architectural findings, with evidence

Five facts make this cheap. Each was re-read for this spec; four matched the
proposal and **one did not** — see 2.2, which is the single most important
paragraph in this document.

### 2.1 Every document edit funnels through one function

`edit_reshared` (`app/src-tauri/src/ops.rs:106-122`) locks the slot, refuses a
`Fidelity::ReadOnly` document, runs the caller's closure against `doc.value`,
and reshares when the closure reports the shape changed. Its own doc comment
states the invariant it exists to hold
(`app/src-tauri/src/ops.rs:102-105`):

> The read-only guard lives here and in `save_document`'s sibling checks only:
> every mutating command in this file routes through this function or
> `edit_slot`, so a new one cannot ship without the guard.

And `edit_slot` (`app/src-tauri/src/ops.rs:127-134`) is a three-line delegation
— `edit_reshared(state, slot, |v| edit(v).map(|t| (t, true)), err)` at
`ops.rs:133`. So "through `edit_reshared` or `edit_slot`" is one function.

Grepping the whole `app/src-tauri/src` tree for writes to a document's `value`
returns four lines in two functions:

| Line | Function |
| --- | --- |
| `ops.rs:117` | `edit_reshared` — `edit(&mut doc.value)` |
| `ops.rs:119` | `edit_reshared` — `doc.value = blue_marshal::reshare(&doc.value)` |
| `ops.rs:641` | `try_edit_char` — `edit(&mut doc.value)` |
| `ops.rs:642` | `try_edit_char` — `doc.value = blue_marshal::reshare(&doc.value)` |

`setup.rs`, the other module sharing `AppState`, reads both documents
(`app/src-tauri/src/setup.rs:660-668`, `&d.value` twice) and never writes one.
`presets.rs` does not import `AppState` at all.

Whole-slot replacement is separate and matters for lifecycle (§5.3), not for
push points: `open_file` at `ops.rs:173`, `close_file` at `ops.rs:190-192`, and
`restore_backup`, which re-opens through `open_file` at `ops.rs:349`.

**A snapshot taken in `edit_reshared` therefore covers every edit the app can
make, including ones written after this ships.**

### 2.2 `try_edit_char` has THREE call sites, not two — and the third was added after the proposal

The proposal, and the brief for this spec, say `try_edit_char`
(`ops.rs:637-645`) has exactly two call sites. It has three:

| Line | Command | Preceded in the same command by |
| --- | --- | --- |
| `ops.rs:525` | `tab_delete` | `edit_slot(Slot::User, delete_tab)` at `ops.rs:519-524` |
| `ops.rs:615` | `overview_window_add` | `edit_slot(Slot::User, add_overview_window)` at `ops.rs:609-614` |
| `ops.rs:628` | `overview_window_remove` | `edit_slot(Slot::User, remove_overview_window)` at `ops.rs:622-627` |

`tab_delete` arrived with the overview-editor work on the current branch
(`d2ae9b4`, "Overview editor: bulk category select, column copy, tab-name
markup"), after the proposal was written. Its char-side write carries the
surviving tabs' per-tab settings onto their new indices
(`remap_tab_scoped_settings`, `ops.rs:525`).

The invariant survives — all three run their `try_edit_char` **after** an
`edit_slot` on the user document, inside one Tauri command, so all three are
already inside a step that has been snapshotted. But the hazard the brief asked
me to guard against as hypothetical is **not hypothetical**: a third caller was
added without anyone noticing that a rule existed. §3.4 specs the two tripwires,
and they are a hard requirement rather than a nicety.

### 2.3 A second two-slot shape the proposal does not mention

`overview_copy_columns` (`ops.rs:463-489`) writes both documents through **two
separate `edit_slot` calls** — user at `ops.rs:473-478` for order and
visibility, char at `ops.rs:481-486` for widths. Unlike the `try_edit_char`
pattern, both halves go through `edit_reshared`, so both would push. One user
action would become **two** undo steps, and the first `Ctrl+Z` would leave the
copy half-reverted. §3.5 rules that out and specs the grouping that prevents it.

It is also the only command in the file that can fail *after* it has already
written: order and visibility land, then the width half meets a read-only or
absent character file and returns `Err`. The frontend's catch
(`OverviewColumnsTab.svelte:79-85`) skips both `onUserDirty()` and
`onCharDirty()`, so today the half that landed is not even marked unsaved — it
is silently dropped at the next save. §3.7 deletes the half-write rather than
patching the flag.

### 2.4 Cost is not the risk, but the numbers are worse than the proposal's

`blue_marshal::Value` (`crates/blue-marshal/src/value.rs:3-58`) derives `Clone`
at line 3 and is a plain owned tree — `Vec<u8>`, `String`, `Vec<Value>`,
`Vec<(Value, Value)>`, `Box<Value>`. No `Rc`, no `Arc`. A snapshot is a deep
clone, and the clone is genuinely independent.

The proposal's argument that this is affordable holds: `apply_mutation` and
`apply_mutations` already call `project()` on every edit (`ops.rs:200`,
`ops.rs:217`), which walks the whole document eagerly and serialises the result
to the frontend as JSON over IPC. One `clone()` is strictly less work than one
`project()` plus a JSON round trip.

What the proposal understates is **resident memory at depth**, and §4 is where
that gets settled with a measurement rather than an argument.

### 2.5 State lives in two mutexes, and `Fidelity::Editable` is a round-trip guarantee

`AppState` (`ops.rs:35-39`) is three mutexes: `char`, `user`, `capture`. Slots
are `Copy` (`ops.rs:53-58`) and resolved by `AppState::doc` (`ops.rs:45-50`).

`Fidelity::Editable` (`crates/settings-model/src/document.rs:17-22`) is decided
once, at load, by `Document::load` (`document.rs:45-71`): it re-encodes the
freshly decoded tree and compares to the on-disk bytes
(`document.rs:52-63`). `Editable` means, precisely,
`encode(decode(bytes)) == bytes` for **this** file. That is what makes the
encoded-bytes fallback in §4 lossless rather than hopeful.

---

## 3 Stack design

### 3.1 The entry

One entry captures the before-state of **both** slots, so a step spanning two
documents undoes as one action.

```rust
/// The before-state of BOTH slots plus their edit counters. Both, always:
/// `overview_window_add`, `overview_window_remove` and `tab_delete` each write
/// the account file and then the character file inside one command (§2.2), and
/// an entry that held only the slot `edit_reshared` was called for would undo
/// half of them.
struct Entry {
    /// `None` means "that slot was empty". By §5.3 a slot cannot become
    /// occupied while a stack exists, so `None` here implies `None` now.
    char: Option<Snap>,
    user: Option<Snap>,
    /// Per-slot edit counters as they stood BEFORE this step; [char, user].
    /// Restored with the trees. §7.2 explains why this is what makes the
    /// unsaved badge exact.
    counters: [u64; 2],
}

/// One document's before-state. A newtype over the representation so §4's
/// fallback is a two-function change and no caller cares.
struct Snap(Value);          // primary
// struct Snap(Vec<u8>);     // fallback: encode(&doc.value)
```

```rust
pub struct History {
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
    /// Monotone per-slot edit counts, [char, user]. Bumped at both write sites.
    counters: [u64; 2],
    /// Counter values as of the last load or save of each slot. Dirty is
    /// `counters[i] != saved[i]`.
    saved: [u64; 2],
    /// `None` outside a group. `Some(false)` inside one that has not pushed its
    /// entry yet, `Some(true)` inside one that has — after which further writes
    /// in the same command bump counters but push nothing (§3.5). Set by
    /// `Group::new`, cleared by its `Drop`.
    group: Option<bool>,
}
```

`History` goes in `AppState` as a fourth field, `history: Mutex<History>`
(`ops.rs:35-39`, `ops.rs:42-44`). It is a new module, `app/src-tauri/src/undo.rs`,
so `ops.rs` grows by the push call and not by the machinery.

### 3.2 Push points

**`edit_reshared` pushes. Nothing else does.**

The rewritten body, with the guards ordered so a refused edit costs no clone:

```rust
fn edit_reshared<T, E>(state, slot, edit, err) -> Result<T, ErrDto> {
    // Lock order is user -> char -> history, everywhere. See §10.
    let mut u = state.user.lock().unwrap();
    let mut c = state.char.lock().unwrap();
    let mut h = state.history.lock().unwrap();

    // Guards first: a no_document or read_only refusal must not pay for a clone.
    // Both roll the open group back before returning, so a command that fails on
    // its SECOND write leaves nothing behind (§3.7).
    {
        let doc = match slot { Slot::User => u.as_ref(), Slot::Char => c.as_ref() };
        let Some(doc) = doc else {
            h.rollback_group(&mut u, &mut c);
            return Err(no_document(slot));
        };
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            // Built BEFORE the rollback call so `doc`'s borrow has ended by the
            // time the `&mut`s go in. NLL, not luck — keep the two statements.
            let e = ErrDto::new("read_only", reason.clone());
            h.rollback_group(&mut u, &mut c);
            return Err(e);
        }
    }

    // `Some` = clone both open trees. `None` = this command already pushed its
    // one entry (§3.5), so the clone is skipped as well as the push. Deciding
    // here and pushing below is not a TOCTOU: the history lock is held across
    // the closure, so no other command can open or fill a group in between.
    let before = h.capture_unless_grouped(&u, &c);

    let doc = match slot { Slot::User => u.as_mut(), Slot::Char => c.as_mut() }
        .expect("checked above");
    match edit(&mut doc.value) {
        Ok((out, changed_shape)) => {
            if changed_shape {
                doc.value = blue_marshal::reshare(&doc.value);
            }
            h.push(before, slot);    // bumps counters[slot]; pushes only if Some
            Ok(out)
        }
        Err(e) => {
            // §3.7. Ungrouped: put back the tree cloned a moment ago, because
            // `apply_mutations` can fail with half a batch applied. Grouped:
            // `before` is None and there is nothing to put back HERE — the
            // group's entry holds the state from before the whole command, and
            // rolling that back subsumes this call's half-applied edit.
            if let Some(b) = before { b.restore_slot(doc, slot); }
            h.rollback_group(&mut u, &mut c);
            Err(err(e))
        }
    }
}
```

Four properties of that ordering are load-bearing:

- **A failed edit pushes nothing.** The `Err` arm never reaches `h.push`, so a
  refused mutation leaves the stack exactly as it was. Pinned by a test (§12).
- **A failed edit also leaves no half-write.** That is §3.7, and it is what
  makes `apply_mutations` atomic.
- **A refused document costs nothing.** The `no_document` and read-only guards
  run before `capture_unless_grouped`.
- **The clone is of the *pre*-edit tree**, which is either straight from
  `Document::load` or left by a previous successful `edit_reshared` — in both
  cases a tree that encodes. That is what the §4 fallback relies on.

The three `History` methods this needs, all of them short enough to state here:

```rust
/// `None` once the open group has pushed: no entry AND no clone.
fn capture_unless_grouped(&self, u: &..., c: &...) -> Option<Entry> {
    (self.group != Some(true)).then(|| self.capture(u, c))
}

/// Bumps `counters[slot]` unconditionally — the dirty flag counts writes, not
/// undo steps (§7.2). Pushes only when `before` is `Some`.
fn push(&mut self, before: Option<Entry>, slot: Slot) {
    self.counters[slot as usize] += 1;
    let Some(entry) = before else { return };
    self.redo.clear();
    if self.undo.len() == CAP { self.undo.pop_front(); }
    self.undo.push_back(entry);
    if self.group.is_some() { self.group = Some(true); }
}

/// Put both documents and both counters back to where the open group found
/// them, and leave the group open but empty. No-op outside a group, and no-op
/// inside one that has not written yet. `back()` IS this group's entry: a group
/// pushes exactly once, so nothing can have been stacked on top of it.
fn rollback_group(&mut self, u: &mut Option<Document>, c: &mut Option<Document>) {
    if self.group != Some(true) { return; }
    let entry = self.undo.pop_back().expect("a filled group has its entry");
    entry.restore_into(u, c, &mut self.counters);
    self.group = Some(false);
}
```

`edit_slot` needs no change at all: it delegates (`ops.rs:133`), so it inherits
the push.

### 3.3 `try_edit_char` deliberately never pushes

`try_edit_char` (`ops.rs:637-645`) bumps `counters[char]` and pushes **nothing**,
because all three of its call sites (§2.2) run inside a step `edit_reshared`
has already snapshotted — and that snapshot already holds the char tree,
because §3.1 captures both slots unconditionally.

If it also pushed, a single "Add overview window" would need two `Ctrl+Z`
presses, the second of which would restore the account file to a state the
character file was never paired with.

The group flag from §3.5 would suppress that second push anyway, since all three
call sites are inside a group. It is still not made a pusher, and §3.5's
"rejected alternatives" says why: it would have to take the `user` lock and run
a capture path that no shipped command ever reaches.

Its doc comment gains the invariant, stated as a rule and not a description:

```rust
/// A char-slot edit that is allowed to do nothing: skipped when no character is
/// open or it is read-only, and it cannot fail.
///
/// # Undo invariant — read this before adding a caller
///
/// This function does NOT push an undo entry. Every call site MUST already have
/// run an `edit_slot`/`edit_reshared` earlier in the SAME Tauri command, whose
/// entry captured both slots (see `undo::Entry`), and that command MUST hold an
/// `undo::group` guard. Today that is `tab_delete` (line ~525),
/// `overview_window_add` (~615) and `overview_window_remove` (~628).
///
/// A caller that breaks this rule produces a char-side change no `Ctrl+Z` can
/// reach. `undo::tests::try_edit_char_callers_are_snapshotted` fails if the set
/// of callers changes; if you are reading this because that test failed, add
/// your command to its list only after confirming it snapshots first.
```

### 3.4 The two tripwires

There are two ways a future command can break undo, they fail differently, and
neither test catches the other's bug:

| | Guards | Fails when |
| --- | --- | --- |
| **A** `try_edit_char_callers_are_snapshotted` | the *snapshot exists* precondition | a new command calls `try_edit_char`, which pushes nothing, without an `edit_reshared` earlier in the same command — so the char edit is unreachable by any `Ctrl+Z` |
| **B** `multi_write_commands_open_a_group` | the *one command, one entry* count | a new command writes twice without an `undo::group` guard — so one action costs two presses |

A pushes zero entries where one is needed; B pushes two where one is needed.
§3.5's guard fixes B and does nothing for A: `open_group` suppresses pushes
*after the first*, and A's bug is that there is no first. That is why the
tripwire survives Decision A rather than being retired by it.

Both read this module's own source — deliberately, and not as runtime asserts:
the bug is a NEW command forgetting a rule, which no existing test exercises.
Both share one ten-line splitter, `fns_of(src) -> Vec<(name, body)>`, which
walks lines, starts a body at each `fn ` / `pub fn ` and ends it at the next.

```rust
#[test]
fn try_edit_char_callers_are_snapshotted() {
    const KNOWN: &[&str] = &["tab_delete", "overview_window_add", "overview_window_remove"];
    let found: Vec<&str> = fns_of(include_str!("ops.rs")).iter()
        .filter(|(_, body)| body.contains("try_edit_char(state,"))
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(
        found, KNOWN,
        "try_edit_char gained or lost a caller. It does NOT push an undo entry — \
         confirm the new caller runs edit_slot/edit_reshared FIRST in the same \
         command, add an `undo reverts both files` test for it, then update KNOWN."
    );
}

#[test]
fn multi_write_commands_open_a_group() {
    let fns = fns_of(include_str!("ops.rs"));
    // Seed with the three primitives, then close over their callers, so the six
    // thin per-view helpers (`edit_user_tabs`, `edit_char_neocom`, …) count as
    // writes and a seventh added tomorrow is picked up without editing a list.
    let mut writers: Vec<&str> = vec!["edit_reshared", "edit_slot", "try_edit_char"];
    loop {
        let grown: Vec<&str> = fns.iter()
            .filter(|(n, b)| !writers.contains(n) && writes(b, &writers) > 0)
            .map(|(n, _)| *n).collect();
        if grown.is_empty() { break }
        writers.extend(grown);
    }
    for (name, body) in &fns {
        assert!(
            writes(body, &writers) < 2 || body.contains("undo::group(state)"),
            "{name} writes to a document more than once without \
             `let _group = undo::group(state);` on its first line. One Tauri \
             command is one undo step (spec 05b §3.5): add the guard, then add a \
             test that ONE Ctrl+Z reverts all of it."
        );
    }
}

/// Non-comment lines in `body` that call any of `names`.
fn writes(body: &str, names: &[&str]) -> usize {
    body.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .filter(|l| names.iter().any(|n| l.contains(&format!("{n}("))))
        .count()
}
```

Zero runtime cost, and both fail with the reason rather than with a number.
They are tripwires, not proofs: A cannot check that the preceding call really
snapshots, and B cannot check that the guard is on the *first* line rather than
after a slot lock. The behavioural tests in §12 do that, one per command.

**Rejected alternatives.** A `debug_assert!` inside `try_edit_char` that the
newest entry exists is weak — it passes for any command that is not the first
of a session. A proof token returned by `edit_reshared` would be exact but
changes twenty-odd call sites for one rule.

### 3.5 One Tauri command is one undo step

An earlier draft accepted that `overview_copy_columns` costs two `Ctrl+Z`
presses, on the grounds that the two steps map onto the two checkboxes the user
ticked. That is rejected. The rule, and the reasoning behind it, is the repo
owner's:

> "It should be one press: you are undoing one action, it would feel wrong to
> have to `Ctrl+Z` multiple times to go back on a single action."

So the invariant is not stated per call site. It is stated once, over the unit
the user actually acted in:

> **One Tauri command produces at most one undo entry.**

That is stronger than "`try_edit_char` doesn't push" and simpler to check,
because "did the user press one button?" has an answer and "is this call site
inside somebody else's snapshot?" needs an audit every time the file changes.

#### The guard

```rust
/// Makes the rest of this Tauri command ONE undo step: the first write inside
/// the group pushes its entry — which already holds the before-state of BOTH
/// slots (§3.1) — and every later write in the same command rides it.
///
/// Hold it for the whole command, as the FIRST statement, before any slot lock:
///
///     let _group = undo::group(state);   // `let _group`, never `let _`
///
/// `let _ = undo::group(state);` drops the guard at the end of that statement
/// and the group closes before the first edit. It compiles and it is silently
/// wrong, which is the one mistake worth naming in the doc comment.
pub struct Group<'a>(&'a Mutex<History>);

pub fn group(state: &AppState) -> Group<'_> {
    // Takes and RELEASES the history lock. It must not hold it: the next thing
    // the command does is call `edit_reshared`, which takes user -> char ->
    // history, and a held history lock would deadlock on the third.
    state.history.lock().unwrap().group = Some(false);
    Group(&state.history)
}

impl Drop for Group<'_> {
    fn drop(&mut self) {
        // `unwrap_or_else(into_inner)` rather than `unwrap()`: a panic inside a
        // Drop during an unwind aborts the process, and a poisoned History
        // means the app is already dead — closing the group is the last thing
        // that should turn that into an abort.
        self.0.lock().unwrap_or_else(|e| e.into_inner()).group = None;
    }
}
```

`Drop` is the whole point. Every one of the four commands has a `?` between
opening the group and finishing — `tab_delete` at `ops.rs:524`,
`overview_copy_columns` at `:478` and `:486` — and a matching
`open_group()` / `close_group()` pair would leak the flag down that path. The
symptom is not a crash: the flag stays set, and every *later* command in the
session pushes nothing and becomes silently un-undoable. Pinned by a test (§12).

#### The lock story

The guard holds **no lock between construction and drop** — it holds a
`&Mutex<History>` and takes the lock twice, briefly, at each end. So:

- It cannot deadlock against `edit_reshared`, which takes `user → char →
  history` and needs the history lock free when it gets there.
- Taking `history` alone is consistent with §10's order: skipping levels is
  safe, reordering is not, and there is no level below `history` to invert
  against.
- `Drop` runs at the end of the command body, after the tail re-projection
  (`overview_columns(state)`, `window_layout(state, …)`) has already released
  its own slot locks inside itself. No slot lock is ever held while the group
  closes.

The rule that keeps this true is the one in the doc comment: **acquire the guard
before any slot lock, as the first statement of the command.**

#### Which commands need it — the definitive list

Every `edit_slot` / `edit_reshared` / `try_edit_char` call site in
`app/src-tauri/src` was re-read for this list. Four commands write more than
once; **no others do, directly or transitively.**

| Command | Writes | Grouping does |
| --- | --- | --- |
| `overview_copy_columns` (`ops.rs:463-489`) | `edit_slot(User)` `:473`, `edit_slot(Char)` `:481` | **the work.** Two entries become one, and the second write skips its clone |
| `tab_delete` (`ops.rs:518-527`) | `edit_slot(User)` `:519`, `try_edit_char` `:525` | nothing today; see below |
| `overview_window_add` (`ops.rs:608-617`) | `edit_slot(User)` `:609`, `try_edit_char` `:615` | nothing today; see below |
| `overview_window_remove` (`ops.rs:621-630`) | `edit_slot(User)` `:622`, `try_edit_char` `:628` | nothing today; see below |

How that was established, since it is the claim everything else rests on:

- The nine functions that can write are `edit_reshared`, its delegate
  `edit_slot` (`ops.rs:133`), `try_edit_char`, and the six thin per-view
  wrappers around `edit_slot` — `edit_user_overview` (`:425-431`),
  `edit_user_tabs` (`:492-498`), `edit_user_autofill` (`:730-736`),
  `edit_char_stacks` (`:775-781`), `edit_char_neocom` (`:814-820`),
  `edit_user_probes` (`:847-853`). Each wrapper contains exactly one
  `edit_slot`, and no command calls the same wrapper twice.
- The two batch-shaped commands write once each and are already one step:
  `apply_mutations` (`ops.rs:209-221`) runs its whole `for` loop *inside* one
  closure, and `add_probe_formations` (`ops.rs:923-960`) does the same.
- The near misses all turn out to be reads: `set_hud_field` (`:312-338`) and
  `set_probe_formation` (`:857-882`) call a projection before their single
  write; `set_keybind_cmd` (`:758-772`), `pack_import` (`:694-704`) and every
  `edit_user_tabs` caller call one after it.
- Outside `ops.rs`: `setup.rs` reads both documents and writes neither
  (`setup.rs:660-668`), `presets.rs` never imports `AppState`, and the only
  `lib.rs` command that composes two backend calls is
  `settings_preset_create` (`lib.rs:515-516`) — `setup::preset_save` plus a
  directory listing, neither of which touches an open document.

`tab_delete` and the two window commands carry the guard even though it
currently suppresses nothing, and that is deliberate for two reasons. It makes
the guard the *declaration* "this command writes more than once", which is what
lets §3.4's tripwire B be a general rule with no hand-maintained list — and a
list is exactly what failed in §2.2. And it is the only thing standing between
these three and a two-press regression if `try_edit_char` is ever changed. Three
lines for both.

#### Rejected alternative — make `try_edit_char` push

With a group flag in hand, the tidier-looking option is to delete §3.3's special
case: let `try_edit_char` push like everything else and let the group deduplicate
it. That also retires tripwire A, since a standalone caller would then produce a
real entry.

It is not taken. `try_edit_char` would have to take the `user` lock it does not
need and run a capture it discards, because **all three** of its call sites are
inside a group — the push path would be dead at every shipped call site, exercised
by no test, and correct only for a caller that does not exist. An untested branch
that nothing reaches is worse than a twelve-line test that fires when someone
needs it. §3.3's rule stands; tripwire A stands.

### 3.6 Invariants, listed

1. **One Tauri command produces at most one undo entry** (§3.5). This is the
   headline one; 2 and 3 are how it is held.
2. `edit_reshared` is the only pusher. `try_edit_char` never pushes (§3.3, §3.4).
3. A command that writes more than once holds an `undo::group` guard for its
   whole body, acquired before any slot lock (§3.5).
4. Every entry holds both slots. Never one.
5. A push happens only after the edit succeeded, and **a command that returns
   `Err` from a write leaves both documents and both stacks as it found them**
   (§3.7).
6. A push clears `redo` (§5.2).
7. Slot occupancy is constant for a stack's lifetime: anything that opens or
   closes a slot clears the stack (§5.3). This is what lets `Entry.char == None`
   mean "still empty" rather than "unknown".
8. Locks are taken user → char → history, without exception (§10). The group
   guard holds none of them between acquire and drop.

### 3.7 Atomicity: a failed write leaves nothing behind

Once `edit_reshared` holds a before-state for the undo stack, restoring it on the
`Err` path costs one line and buys atomicity. **Take it.** It was flagged as a
decision rather than assumed because it changes documented behaviour;
the change is strictly better and the decision is made.

Two paths need it, both already shown in §3.2's body:

1. **Ungrouped** — `if let Some(b) = before { b.restore_slot(doc, slot); }` in
   the `Err` arm. This is what makes `apply_mutations` (`ops.rs:209-221`) atomic:
   its closure runs `apply` over the whole batch against one locked doc, so a
   failure at mutation `k` leaves `k-1` applied today.
2. **Grouped** — `h.rollback_group(&mut u, &mut c)` on *every* error path,
   including the `no_document` and read-only guards, which return before the
   closure ever runs. Those two are not hypothetical: they are exactly how
   `overview_copy_columns` fails today, on its second write, with its first
   already applied (§2.3).

**`ops.rs:205-208`'s doc comment must be rewritten in the same commit.** It
currently states the opposite of what the code will do:

> Non-atomic on a mid-batch failure, matching the caller's prior per-mutation
> loop — geometry set_scalars on valid paths don't fail.

It becomes:

```rust
/// Batched sibling of `apply_mutation`: applies every mutation to the same
/// locked doc, then projects the tree once instead of once per mutation.
///
/// Atomic: a mutation that fails part-way puts the document back as it was,
/// from the snapshot `edit_reshared` took for the undo stack (spec 05b §3.7).
/// Callers get all of the batch or none of it.
```

A doc comment left saying "non-atomic" beside code that rolls back is worse than
either behaviour on its own, because the next reader writes a caller against the
comment. Same commit, not a follow-up.

#### Where the two decisions meet

An error partway through a **grouped** command rolls back the **whole group**,
not just the failing write. This is the one genuinely new question the two
decisions create together, and the answer follows from §3.5's rule rather than
from taste: if one user action is one undo step, then a user action that failed
must leave nothing — a half-applied action the user cannot see, sitting under an
error dialog that says the action failed, is precisely the state undo exists to
prevent. Rolling back only the failing write would leave the first half applied
with **no entry on the stack for it**, because the group's entry is popped or was
never pushed. That is worse than today: unreachable by `Ctrl+Z` and invisible.

Concretely, for `overview_copy_columns` onto a read-only character file — the
realistic failure, since `OverviewColumnsTab.svelte:67` already gates the widths
box on a character being *open* but not on it being writable:

| | today | after |
| --- | --- | --- |
| account file | order + visibility copied | untouched |
| character file | untouched (the write errored) | untouched |
| unsaved badge | **not set** — the catch at `OverviewColumnsTab.svelte:79-85` skips `onUserDirty()`, so the copied order is silently dropped at the next save | not set, and correctly so |
| what the user sees | "Copy failed", and a document that half-copied | "Copy failed", and nothing changed |

That is a behaviour change beyond undo, and an intended one: it makes the error
message true. It also closes the dirty-flag hole above without touching the
frontend.

Two limits, stated rather than papered over:

- **Rollback covers writes, not tail projections.** A command that fails *after*
  its write — `set_overview_width` (`ops.rs:445-453`) edits the char slot and
  then calls `overview_columns`, which errors if no account file is open — keeps
  the write, and the entry stays on the stack so `Ctrl+Z` reaches it. That is the
  right outcome (the edit did happen; only its re-projection failed) and it is
  the pre-existing behaviour. Invariant 5 is scoped to writes for this reason.
- **A rolled-back group still costs the redo stack**, because the push that
  opened the group cleared `redo` and nothing kept a copy. Accepted: a failed
  command losing a redo stack is not worth a saved copy of it, and §5.2's
  argument for redo is about not fearing `Ctrl+Z`, not about surviving errors.

One coupling to carry into §4: if the measurement sends `Snap` to encoded bytes,
`restore_slot` and `restore_into` gain a `decode` and therefore a `Result` on the
error path. A rollback that cannot decode must leave the document as it stands
and never panic — the outcome is today's non-atomic behaviour, which is a
degradation and not a corruption. `Fidelity::Editable` (§2.5) makes it
near-impossible, since the bytes were encoded from a tree that round-trips.

---

## 4 Snapshot representation, and the measurement that picks it

### 4.1 The decision

**Ship the `Value` clone. Measure before merge. Swap to encoded bytes if the
measurement crosses the threshold in §4.4.**

The clone is the honest default because it is the smallest correct code:
`Snap(doc.value.clone())` to capture, `doc.value = snap.0` to restore, no
`Result` in either direction. The encoded form is smaller in memory by a large
factor and needs two `Result`s and a new failure mode. Which one wins depends on
a number nobody has measured, so §4.3 measures it.

Both live behind the same two functions so the swap is one commit that touches
`undo.rs` and nothing else:

```rust
impl Snap {
    fn of(doc: &Document) -> Option<Snap>;         // capture
    fn restore(self, doc: &mut Document);          // restore
}
```

### 4.2 Why encoded bytes are a real option and not a hedge

`Fidelity::Editable` means `encode(decode(bytes)) == bytes` for this exact file
(`document.rs:52-63`), so the encoded form is lossless *by the definition of the
only documents this app will edit*. `encode` is
`crates/blue-marshal/src/encode.rs:15`; `decode` is `decode.rs:32`.

Measured on this machine's corpus (`testdata/corpus`, gitignored client output —
6829 `core_char_*.dat`, 3221 `core_user_*.dat`):

| | median | max |
| --- | --- | --- |
| `core_char_*.dat` | 103,337 B | 177,548 B |
| `core_user_*.dat` | 97,472 B | 390,184 B |

So an encoded pair is ~200 KB at the median and ~570 KB at the worst realistic
extreme. Twenty of those is 4 MB, or 11 MB worst case. That is nothing.

The costs are: `encode` returns `Result` (a snapshot that cannot encode has to
degrade to "undo unavailable" rather than fail the user's edit — one more
branch and one more silent-ish failure mode), and undo pays one `decode` of
~100 KB per document, at human keypress speed.

### 4.3 The measurement recipe

The number that decides it is the ratio of resident tree bytes to file bytes.
Add this to `undo.rs`'s test module, `#[ignore]` so it never runs in CI, and run
it once:

```rust
/// cargo test -p eve-settings-editor --lib undo::tests::snapshot_footprint -- --ignored --nocapture
#[test]
#[ignore = "measurement, not a gate; needs testdata/corpus"]
fn snapshot_footprint() {
    fn heap(v: &Value) -> usize {
        use Value::*;
        std::mem::size_of::<Value>() + match v {
            Long(b) | Bytes(b) | Global(b) => b.capacity(),
            Str(s) | StrUcs2(s) => s.capacity(),
            Tuple(xs) | List(xs) => xs.capacity() * std::mem::size_of::<Value>()
                + xs.iter().map(heap).sum::<usize>(),
            Dict(kv) => kv.capacity() * 2 * std::mem::size_of::<Value>()
                + kv.iter().map(|(k, x)| heap(k) + heap(x)).sum::<usize>(),
            Stream(b) | Shared { value: b, .. } => heap(b),
            Instance { class, state } => heap(class) + heap(state),
            Reduce { ctor, items, pairs } => heap(ctor)
                + items.iter().map(heap).sum::<usize>()
                + pairs.iter().map(|(k, x)| heap(k) + heap(x)).sum::<usize>(),
            _ => 0,
        }
    }
    for path in /* walk testdata/corpus for core_char_*.dat and core_user_*.dat */ {
        let bytes = std::fs::read(&path).unwrap();
        let doc = Document::load(&path).unwrap();
        let tree = heap(&doc.value);
        println!("{:>9} file  {:>10} tree  {:>5.1}x  {}",
                 bytes.len(), tree, tree as f64 / bytes.len() as f64,
                 path.file_name().unwrap().to_string_lossy());
    }
}
```

Report the **median ratio** and the **max ratio**. `heap` deliberately counts
`capacity()` rather than `len()` and includes the inline `size_of::<Value>()`
for every node, because that is what the allocator is actually holding; it
under-counts only per-allocation malloc headers, which biases the answer
towards "the clone is fine", i.e. against the change.

The walker is left as a comment because `crates/blue-marshal/tests/corpus.rs:36-47`
already has one (`fn walk`) to copy.

### 4.4 The threshold

Let `R` be the median tree-to-file ratio and `CAP` the depth from §5.1.

- Peak stack memory ≈ `CAP × R × 200 KB`, plus the same again if the redo stack
  is full — so budget `2 × CAP × R × 200 KB`.
- **If that exceeds 40 MB, switch to encoded bytes.** With `CAP = 20` that
  trips at `R > 5`.

My back-of-envelope says `R` lands near 10–12 — `size_of::<Value>()` is 64
bytes (the widest variant is `Reduce`: `Box` + two `Vec`s = 56, plus tag and
padding), a dict entry is two of those, and these files are mostly small
integers under byte-string keys, so nodes-per-file-byte is high. If that holds,
the measurement says **switch**, and §4.1's default loses. I am specifying the
clone as the default anyway because the whole point of §4.3 is to not commit to
an arithmetic guess, and because if `R` comes back at 4 the clone is both
smaller code and fine.

Run the recipe. Write the number into this section. Then choose.

---

## 5 Bounds, redo, lifecycle

### 5.1 Depth cap: 20

`const CAP: usize = 20;`

Twenty, not fifty, for three reasons:

1. **The job is small.** Undo here is "take that back" for the toast (§9) plus a
   short tail. "Revert everything" is already Discard, and it is one click
   (`+page.svelte:520-526`). The depth that matters is the last handful.
2. **Memory is linear in the cap** and a resident tree is a multiple of the
   file, not a fraction of it (§4). Fifty entries makes the §4.4 threshold trip
   at `R > 2`, which it certainly does — so a cap of 50 is a decision to use the
   encoded form, made without measuring.
3. **The failure modes are asymmetric.** A cap that is too small costs one
   constant to raise, and the user still has Discard. A cap that is too large
   costs a settings editor a few hundred megabytes of resident memory on a
   machine that is also running EVE.

`undo` is a `VecDeque`: `push_back`, and `pop_front` when `len() == CAP`. The
dropped entry's trees are owned with no sharing (§2.4), so its memory is freed
at the `pop_front` and **memory is flat at the cap, not growing**. Say so in a
comment, because "bounded" and "bounded and releases" are different claims.

The oldest step is silently unreachable once dropped. No warning, no toast: a
user who has made twenty edits and wants all of them gone wants Discard.

### 5.2 Redo: yes

Include it, cleared by any new edit.

The argument is not symmetry, it is that **undo without redo is frightening**.
Phase 5's thesis is that confirmations should be replaced by reversibility; an
undo the user cannot take back is a new irreversible action, and it undermines
the thing it was added to support. Discard is not a recovery from an over-undo,
because it throws away everything else too.

The cost is small because the entry type already exists:

- `undo()`: capture the current state of both slots into an `Entry`, push it to
  `redo`, pop `undo`'s back, restore from it.
- `redo()`: the mirror — capture into an `Entry`, push to `undo`, pop `redo`,
  restore.
- `History::push` (i.e. any new edit) does `self.redo.clear()`. Standard rule,
  and the one users already expect from every editor.

`redo` is a `Vec`, not a `VecDeque`: it is cleared by any edit and can never
exceed `CAP` entries, because it only ever grows by one per undo and undo is
bounded by the undo stack's depth.

Grouping (§3.5) does not complicate this: a group pushes once, so it clears
`redo` once, exactly like the single-write command it is meant to feel like. The
one seam is that a group rolled back by §3.7 leaves `redo` cleared even though it
changed nothing — noted there and accepted.

### 5.3 Lifecycle: what clears the stack

**Both stacks are cleared, and both counters reset, whenever a slot's document
is replaced or emptied.**

| Site | Action |
| --- | --- |
| `open_file` (`ops.rs:160-188`, slot write at `:173`) | clear `undo` + `redo`; `counters[slot] = saved[slot] = 0` |
| `close_file` (`ops.rs:190-192`) | same |
| `restore_backup` (`ops.rs:340-350`) | inherits it — re-opens through `open_file` at `:349` |
| `save_document` (`ops.rs:223-236`) | on success only: `saved[slot] = counters[slot]`. **Stack untouched.** |

Clearing *both* stacks when only one slot changed is not over-caution; it is
required. An entry holds trees for both slots (§3.1). After `open_file` swaps a
different character into the char slot, an older entry's char tree belongs to a
*different character's settings file*. Restoring it would silently write one
pilot's window geometry into another's document. That is data corruption, not a
stale view, and it is the reason invariant 7 in §3.6 is phrased as absolutely as
it is.

The frontend never opens one slot in isolation — `reconcileUserSlot` and
`reconcileCharSlot` (`+page.svelte:327-367`) always route through `api.open` or
`api.close` — so the invariant holds from that side too.

**Save does not clear the stack.** You can undo past a save; the counter then
differs from the mark and the file is correctly reported unsaved again, because
memory now differs from disk. That falls out of §7.2 rather than needing a rule.

### 5.4 When only one slot is open

`Entry.char` / `Entry.user` are `Option<Snap>`; `None` means the slot was empty
at capture. By invariant 7 a slot cannot become occupied while the stack lives,
so `None` at capture implies `None` at restore, and restore skips it.

```rust
debug_assert_eq!(entry.char.is_some(), guard_char.is_some(),
    "slot occupancy changed under a live undo stack — open_file/close_file must clear it");
```

A character opened with no account paired — the normal state before pairing — is
therefore an entry with `user: None` and half the memory. An account file opened
alone is the mirror.

---

## 6 Backend API

Three commands in `lib.rs`, delegating to `ops.rs` as every other command does
(`lib.rs:47-112` for the pattern), plus three lines in `api.ts` (`api.ts:399-515`).

```rust
#[derive(Serialize)]
pub struct UndoState {
    pub can_undo: bool,
    pub can_redo: bool,
    /// Frontend shows nothing with this; it is for the tests and for a future
    /// history list. Keep it — it is one field, and §3.5's "one command, one
    /// step" tests are all `assert_eq!(depth, 1)`, which nothing else can see.
    pub depth: usize,
}

#[derive(Serialize)]
pub struct UndoOutcome {
    /// Fresh projection per open slot. The Tree view reads `slots[x].tree`
    /// directly (`+page.svelte:625-635`) and no refresh token can reach it — see §7.1.
    pub char_tree: Option<Node>,
    pub user_tree: Option<Node>,
    /// Authoritative unsaved state per slot, after the step. See §7.2.
    pub dirty: SlotFlags,          // { char: bool, user: bool }
    pub state: UndoState,
}
```

```rust
pub fn undo(state: &AppState) -> Option<UndoOutcome>;
pub fn redo(state: &AppState) -> Option<UndoOutcome>;
pub fn undo_state(state: &AppState) -> UndoState;
```

**`Option`, not `Result`.** "Nothing to undo" is not an error and should not
reach `errMessage`; `None` serialises to `null` and the frontend shows a plain
toast. There is no other failure mode: undo takes no arguments, touches no disk,
and restores trees it produced itself.

**No labels.** `edit_reshared` does not know what it is doing, and threading a
`&'static str` through it would touch every call site in `ops.rs` for one
string. It is not needed: the toast-with-Undo from Phase 5 is fired by the view
that performed the action, which knows its own wording ("Tab deleted. **Undo**")
without asking the backend. `Ctrl+Z` invoked cold shows "Undone." §9 covers the
menu. If a history list is ever wanted, add the argument then — it is mechanical
and this design does not block it.

**Both trees, always.** Two `project()` calls per undo, at human speed, against
one per edit today (`ops.rs:200`). Not worth conditioning on which slots changed,
and returning both means the frontend has no branch either.

---

## 7 Frontend fan-out and dirty reconciliation

### 7.1 The refresh mechanism exists — for two views out of six

The proposal says the refresh mechanism already exists and undo just bumps it.
That is half true, and the other half is a change list.

`refreshToken={savedAt}` is passed to exactly two components:

| View | Token? | Reload trigger |
| --- | --- | --- |
| `LayoutView` | **yes** — `+page.svelte:550` | `refreshToken`/`slot`/`userOpen`, `LayoutView.svelte:142-164` |
| `OverviewView` | **yes** — `+page.svelte:566` | `LayoutView`-style `$effect`, `OverviewView.svelte:39` |
| `AutofillView` | **no** — `+page.svelte:576-583` | `userOpen`/`userId` only, `AutofillView.svelte:22` |
| `KeybindsView` | **no** — `+page.svelte:587-593` | `userOpen`/`userId` only, `KeybindsView.svelte:30` |
| `ProbeFormationsView` | **no** — `+page.svelte:597-602` | `userOpen`/`userId` only, `ProbeFormationsView.svelte:148` |
| Tree | n/a | reads `slots[active].tree`, written only by `runMutation`/`runMutations` (`+page.svelte:393`, `:409`) and `api.open` |

Neither `userOpen` nor `userId` changes across an undo, so **Autofill, Keybinds
and Probes would show pre-undo data indefinitely.** They also show stale data
today after a Discard and after a backup restore — `discardChanges` bumps
`savedAt` (`+page.svelte:233`) and `BackupsPanel`'s `onRestored` bumps it
(`+page.svelte:656`), and all three ignore it. That is a pre-existing bug this
phase must fix regardless, and it is three props and three one-line `$effect`
edits.

Note also that `LayoutView`'s `load()` (`LayoutView.svelte:115-135`) is what
fetches `api.hud`, `api.neocomBar`, `api.overviewColumns` and `api.chatPanels`
— so four of the eight projections in the brief's list ride the token that
already works. The token effect also clears `preview`, `fPreview`, `nudging`,
`dropTarget` and `draggingTab` (`LayoutView.svelte:157-161`), which is exactly
what an undo should do to an in-flight canvas gesture.

**The mechanism, then:**

1. `undo`/`redo` return fresh trees; `+page.svelte` assigns
   `slots[s] = { ...slots[s], tree }` for each open slot (the same reassign-don't-
   mutate rule as `runMutation`, `+page.svelte:393`).
2. `savedAt += 1`. Its comment (`+page.svelte:80`) says "bumped after each save";
   update it to say what it now means — *the open documents changed under the
   views* — rather than renaming the identifier, which would collide with the
   Phase 2 and Phase 5 diffs for no functional gain.
3. Add `refreshToken` to `AutofillView`, `KeybindsView` and `ProbeFormationsView`
   and add `void refreshToken;` to each one's existing `$effect`.

`BackupsPanel` also keys on `savedAt` (`BackupsPanel.svelte:22-24`) and will
re-list the backup directory on every undo. It is a directory listing of a
folder with tens of files; leave it.

`layoutAvailable` (`+page.svelte:81`, computed at open, `:266-270`) is not
refreshed. No shipped command adds or removes the first window of a document, so
an undo cannot flip it. If a generic tree mutation ever could, the symptom is a
missing Layout tab until reopen — noted, not handled.

### 7.2 Dirty reconciliation

Today `dirtySlots` (`+page.svelte:41`) is two independent booleans set to `true`
by six sites and cleared by save/open/discard. Undo needs to *clear* them
correctly, and no frontend-only scheme can: after `edit → save → edit → undo`,
the account file is clean, but after one more undo it is dirty again, and only
something that knows where the save point sits can tell those apart.

Hence the counters in §3.1. The rule is one line:

> **dirty[slot] = counters[slot] != saved[slot]**

- `edit_reshared` bumps `counters[slot]` (§3.2). `try_edit_char` bumps
  `counters[char]` (§3.3) — this is the reason it needs the history lock at all,
  and the reason `overview_window_add` marks the character file dirty correctly
  without pushing.
- `save_document` sets `saved[slot] = counters[slot]` on success (§5.3).
- `open_file`/`close_file` zero both for that slot.
- An `Entry` carries the counters as they were **before** its step and restores
  them with the trees, so undo and redo move the dirty state along with the
  document.

Worked example — the case a boolean-valued scheme gets wrong:

| step | `counters[user]` | `saved[user]` | dirty |
| --- | --- | --- | --- |
| open | 0 | 0 | no |
| edit A | 1 | 0 | yes |
| save | 1 | 1 | no |
| edit B | 2 | 1 | yes |
| undo (to post-A) | 1 | 1 | **no** — memory matches disk |
| undo (to pre-A) | 0 | 1 | **yes** — disk has A, memory does not |

`+page.svelte` assigns `dirtySlots.char = r.dirty.char` and
`dirtySlots.user = r.dirty.user` from the response. The existing six
`dirtySlots[x] = true` sites stay exactly as they are — they are the forward
direction and they are already right.

`canUndo`/`canRedo` for the UI need no new plumbing either, because those same
six sites are the frontend's own mirror of `edit_reshared`: every mutating path
in the app reaches one of them (`+page.svelte:394`, `:410`, `:554`, `:569`,
`:570`, `:583`, `:593`, `:602`). Set `canUndo = true; canRedo = false;` beside
each, and correct both from every `undo`/`redo` response and from one
`api.undoState()` call after open/close/discard.

### 7.3 `discardChanges()`

`discardChanges` (`+page.svelte:219-237`) re-opens every open file through
`api.open`, so `open_file` clears the stack and zeroes the counters for it —
**no backend change is needed**. Add `canUndo = false; canRedo = false;` beside
the two existing `dirtySlots` clears at `+page.svelte:231-232`.

Its confirmation copy should also stop being silently wrong once undo exists.
Today it says the backups are untouched; it should also say the undo history
goes. One clause.

`openFile` (`:254-256`), `openPresetPair` (`:294-298`) and `clearSlot` (`:321`)
get the same two lines, for the same reason.

---

## 8 The boundary: what is not undoable

Undo covers **in-memory edits to the two open documents**. Nothing else. The
line is not a judgement call — it is exactly the set of things that route
through `edit_reshared`, which is the set of things §2.1 proved is closed.

### Not undoable, and why

| Action | Where | Why not | Its safety net |
| --- | --- | --- | --- |
| Settings-preset create / rename / delete / import / export | `presets.rs:218-219`, `:348-367`, `:370-375`, `:390-404`, `:409` | writes and deletes directories on disk; never touches an open document | keeps its confirm; delete is `remove_dir_all` and genuinely irreversible |
| Backup restore | `ops.rs:340-350` | overwrites the target file, then re-opens | the backup chain itself; the restored file is still backed up on the next save |
| Batch copy / "Copy settings" | `setup.rs` (`plan_batch`, `setup_apply`) | writes other characters' files directly | every target is backed up first |
| Overview pack **export** | `ops.rs:708-720` | writes a YAML file | it is a new file; delete it |
| Save | `ops.rs:223-236`, `save.rs:42-83` | the write is the point | the backup taken at `save.rs:64` |
| View preferences — clutter overrides, the Detail toggle, target and effect counts | `prefs.svelte.ts:83`, `:109`, `:118`, `:129`, `:143` → `api.setPreferences` | application preferences, not document content | each is a toggle you can toggle back |

### Undoable, including two that are worth advertising

Everything reached through `edit_slot`/`edit_reshared`. Two are worth calling
out because they are among the app's scariest confirmations today:

- **Overview pack import** (`ops.rs:694-704`) is an in-memory edit and becomes
  one `Ctrl+Z`. A pack import currently rewrites whole sections of the account
  file behind a confirm dialog.
- **Clear all autofill** (`ops.rs:742-744`) and **neocom reset**
  (`ops.rs:831-833`) likewise.

### The naming trap, flagged loudly

There are two unrelated things called "preset" and they land on opposite sides
of this boundary:

- `ops.rs:550-560` — `preset_create`, `preset_rename`, `preset_delete` are
  **overview presets stored inside the account document**. Fully undoable.
- `presets.rs` — `settings_preset_*`, exposed as `api.settingsPresetCreate` etc.
  (`api.ts:494-504`), are **on-disk settings-preset folders**. Not undoable.

Whoever writes the UI copy for §9 must not conflate them. An overview preset
delete should get a toast with Undo; a settings-preset delete must keep its
confirm.

### How the UI says so

Three rules, in priority order:

1. **`Ctrl+Z` always means the document stack, never "the last thing you did".**
   It never reinterprets itself based on context. This is the rule that stops
   `Ctrl+Z` after a settings-preset delete from silently reverting an unrelated
   overview edit — it would still revert the overview edit, but that is what the
   user asked for, and rules 2 and 3 make sure they know it.
2. **A non-undoable action's toast carries no Undo button** and, where there is
   a real remedy, names it instead. A backup restore's toast points at the
   History popover. A settings-preset delete keeps its confirm and gets a plain
   toast.
3. **`Ctrl+Z` with an empty stack toasts "Nothing to undo."** — never silence.
   Silence after a delete is exactly how a user concludes the undo did something
   they cannot see.

---

## 9 UI and keyboard

### 9.1 Shortcuts

Handled in the existing `<svelte:window onkeydown>` in `+page.svelte:461-478`,
beside `Ctrl+S` (`:462`) and `Ctrl+F` (`:469`).

| Keys | Action |
| --- | --- |
| `Ctrl+Z` / `Cmd+Z` | undo |
| `Ctrl+Shift+Z` / `Cmd+Shift+Z` | redo |
| `Ctrl+Y` | redo — **also** |

Both redo bindings, because the app ships on all three platforms and this is one
`||`: `Ctrl+Y` is the Windows convention (and Windows is the primary target —
`docs/` and the installed release are Windows), `Cmd+Shift+Z` is the macOS one,
and Linux users use both. Menus and toasts display the platform-native one only,
per the proposal's platform note.

**The text-field guard is mandatory, not polish.** A `Ctrl+Z` inside an input
must reach the webview's own text undo. The repo already has the helper:
`inAField` (`ProbeFormationsView.svelte:426-430`) tests `INPUT`/`SELECT`/
`TEXTAREA`/`isContentEditable`, and `swallowsArrowKeys` (`layout.ts:1004-1012`)
is the stricter arrow-key variant. Lift `inAField` into `layout.ts` beside its
sibling and use it from both places — it is five lines and it currently exists
in one component while being a rule about the whole app.

```js
if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z" && !inAField(e.target)) {
  e.preventDefault();
  void (e.shiftKey ? doRedo() : doUndo());
}
if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "y" && !inAField(e.target)) {
  e.preventDefault();
  void doRedo();
}
```

Keybind capture is already safe and needs nothing: `KeybindsView`'s handler
calls `e.preventDefault(); e.stopPropagation();` on the chip
(`KeybindsView.svelte:73-74`, wired at `:124`), so a captured `Ctrl+Z` never
bubbles to `<svelte:window>`. A test pins it (§12) because that safety is one
`stopPropagation` away from being lost.

`LayoutView` (`LayoutView.svelte:824`) and `ProbeFormationsView`
(`ProbeFormationsView.svelte:462`) also bind `<svelte:window onkeydown>`, but
neither reads `z` or `y` (`LayoutView.svelte:781-801` handles arrows only,
`ProbeFormationsView.svelte:436-448` handles `Ctrl+C` only), so there is no
conflict.

### 9.2 The toast affordance

Phase 5's pattern is *"Do it; toast with **Undo**"*. The wording belongs to the
caller (§6): the view that just deleted a tab shows "Tab deleted." with an Undo
action that calls `api.undo()`. That is what replaces the false *"This can't be
undone."* confirm.

One rule to prevent a real bug: **the toast's Undo button undoes the top of the
stack, not "its own" step.** If the user makes another edit before clicking it,
that button now reverts the newer edit. Either dismiss the toast on any
subsequent edit — which is one line at the same six sites §7.2 already touches,
and is the honest behaviour — or drop the Undo button from the toast and let
`Ctrl+Z` carry it. Dismissing is better and cheap. Do that.

### 9.3 The app menu

There is **no Tauri application menu in the repo today** — grepping `app/` for
`Menu` finds only in-app popovers (`ContextMenu.svelte`, and its users
`WindowPanel.svelte`, `PresetGroup.svelte`, `LayoutView.svelte`). The app menu
is a Phase 5 deliverable.

- **If Phase 5's menu shipped:** Edit → Undo / Redo, each showing its
  accelerator, each disabled with `can_undo`/`can_redo`. Labelled plainly
  "Undo" and "Redo" — no "Undo *Delete tab*", per §6.
- **If 5b lands first:** the toast plus `Ctrl+Z` is the whole affordance, and
  the save cluster from Phase 2 gets a small Undo control beside Discard so
  nothing is shortcut-only. That is the redesign's own rule (proposal, "Nothing
  is palette-only") and it applies here.

---

## 10 Concurrency and lock order

`std::sync::Mutex` is not reentrant, and `ops.rs` already knows it. The file
carries three comments about exactly this:

- `ops.rs:245-250` — `window_layout` locks user before the requested slot, and
  skips the user lock when the caller asked for the user slot, because "locking
  `user` twice would deadlock (std Mutex is not reentrant)".
- `ops.rs:262-263` — `hud_layout`: "Lock user before char, matching
  `overview_columns` — the only other spot that holds both slots at once. A
  consistent order across the file rules out lock-order-inversion deadlock
  between concurrently invoked commands."
- `ops.rs:285-287` — `set_chat_splits` re-projects *after* its guard drops,
  "since `std::sync::Mutex` is not reentrant".

`overview_columns` (`ops.rs:415-417`) and `setup.rs:660-661` follow the same
user-then-char order. So the file's rule already exists and this phase extends
it by one level:

> **Lock order is `user` → `char` → `history`. Always. No function takes a slot
> lock while holding the history lock.**

`history` goes last because it is the only lock that a function might want
*after* discovering something about a document — and because putting it last
means `undo`/`redo`/`edit_reshared`, the three functions that need all three,
take them in one identical sequence with no case analysis.

Concretely:

| Function | Locks taken |
| --- | --- |
| `edit_reshared` | user, char, history — all three held for the body (§3.2) |
| `try_edit_char` | char, history — skips `user`, order preserved |
| `undo` / `redo` | user, char, history |
| `save_document` | slot, history |
| `open_file` / `close_file` | slot, then history (the slot guard is a statement temporary at `ops.rs:173`, `:191`) |
| `undo::group()` and `Group::drop` | history only, taken and released at each end. **Never held in between** (§3.5) |
| everything else | unchanged |

Skipping a level is safe; reordering is not. A function that takes `char` then
`history` can never deadlock against one that takes `user` then `char` then
`history`, because the relative order of every shared pair is identical.

**The one genuinely new risk** is that `edit_reshared` currently takes a single
lock (`ops.rs:112`) and will now take three. Any caller already holding `user`
or `char` when it calls `edit_reshared` would self-deadlock immediately. There
are none: every `edit_slot`/`edit_reshared` call in `ops.rs` is at the top level
of a command, and every command that re-projects does so *after* the edit's
guard has dropped — which is the rule `ops.rs:285-287` states and every
re-projecting command follows (`ops.rs:429-430`, `:451-452`, `:496-497`,
`:734-735`, `:771`, `:779-780`, `:818-819`, `:851-852`). Verified by reading;
re-verify if this spec is implemented against a newer `ops.rs`.

Holding the history lock across the `edit` closure is safe because the closures
are `settings_model` functions over a `&mut Value` — they have no access to
`AppState` and cannot re-enter. Worth a one-line comment at the lock site.

Whether two commands can actually run concurrently is not worth relitigating
here: `lib.rs` has `async` commands (`lib.rs:115`, `:127`) that run on the async
runtime's pool, the existing comments assume concurrency, and the cost of one
consistent order is a comment.

---

## 11 File-by-file change list

### New

- **`app/src-tauri/src/undo.rs`** — `History`, `Entry`, `Snap`, `CAP`, push /
  undo / redo / clear / mark-saved / dirty, plus `Group`, `group()`,
  `capture_unless_grouped` and `rollback_group` (§3.5, §3.7); the `#[cfg(test)]`
  module with both tripwires and `fns_of`; and the `#[ignore]`d measurement
  (§4.3). All of the machinery, so `ops.rs` grows by call sites only.

### Backend

- **`app/src-tauri/src/lib.rs`** — `mod undo;` beside `mod ops;` (`:1-8`); three
  `#[tauri::command]` one-liners for `undo`, `redo`, `undo_state`, matching the
  shape at `:47-112`; three names in `generate_handler!`.
- **`app/src-tauri/src/ops.rs`**
  - `AppState` gains `history: Mutex<undo::History>` (`:35-39`) and
    `AppState::new` initialises it (`:42-44`).
  - `edit_reshared` (`:106-122`) — rewritten per §3.2: three locks in order,
    guards before capture, capture skipped inside a filled group, push after
    success, rollback on every error path.
  - `try_edit_char` (`:637-645`) — bumps `counters[char]`; gains the §3.3 doc
    comment. Behaviour otherwise unchanged.
  - **`let _group = undo::group(state);` as the first statement of the four
    multi-write commands** (§3.5): `overview_copy_columns` (`:463-489`),
    `tab_delete` (`:518-527`), `overview_window_add` (`:608-617`),
    `overview_window_remove` (`:621-630`). One line each, nothing else in those
    functions moves.
  - `apply_mutations`'s doc comment (`:205-208`) — rewritten per §3.7, in the
    same commit as the rollback. It currently promises the opposite.
  - `open_file` (`:173`), `close_file` (`:190-192`) — clear the stacks and reset
    that slot's counters.
  - `save_document` (`:223-236`) — on success, `saved[slot] = counters[slot]`.
  - New: `pub fn undo`, `pub fn redo`, `pub fn undo_state`, plus `UndoOutcome`,
    `UndoState`, `SlotFlags`.
  - Module doc comment (`:1-6`) gains the lock-order rule from §10 and the
    one-command-one-entry rule from §3.5.
  - Test module gains §12's Rust tests, including the §3.4 tripwires.

### Frontend

- **`app/src/lib/api.ts`** — three entries beside the rest (`:399-515`):
  `undo: () => invoke<UndoOutcome | null>("undo")`, `redo`, `undoState`; plus the
  `UndoOutcome` / `UndoState` types.
- **`app/src/routes/+page.svelte`**
  - `canUndo` / `canRedo` `$state`, set beside the six existing
    `dirtySlots[...] = true` sites (`:394`, `:410`, `:554`, `:569-570`, `:583`,
    `:593`, `:602`).
  - `doUndo()` / `doRedo()`: call the command, on `null` toast "Nothing to
    undo/redo", else assign both trees, set `dirtySlots` from `r.dirty`, set
    `canUndo`/`canRedo` from `r.state`, `savedAt += 1`.
  - `<svelte:window onkeydown>` (`:461-478`) — the §9.1 branches.
  - `savedAt`'s comment (`:80`) — say what it now means.
  - `discardChanges` (`:231-232`), `openFile` (`:254`), `openPresetPair`
    (`:294-295`), `clearSlot` (`:321`) — clear `canUndo`/`canRedo`.
  - `discardChanges`'s confirm copy (`:224`) — mention that undo history goes.
  - `refreshToken={savedAt}` added to the three views below (`:576`, `:587`, `:597`).
- **`app/src/lib/AutofillView.svelte`** — `refreshToken: number` prop;
  `void refreshToken;` in the `$effect` at `:22`.
- **`app/src/lib/KeybindsView.svelte`** — same, `$effect` at `:30`.
- **`app/src/lib/ProbeFormationsView.svelte`** — same, `$effect` at `:148`;
  `inAField` (`:426-430`) moves to `layout.ts` and is imported back.
- **`app/src/lib/layout.ts`** — receives `inAField` beside `swallowsArrowKeys`
  (`:1004-1012`).

### Not touched

`crates/settings-model` and `crates/blue-marshal` need **no change**. The stack
is entirely an `app/src-tauri` concern, which is why this phase cannot break the
codec or the save-path invariant chain.

---

## 12 Tests

### Rust — `undo.rs` and `ops.rs` `#[cfg(test)]`

Fixtures follow the existing pattern: `AppState::new()` (`ops.rs:42`),
`testkit::temp_file` (`testkit.rs:21`), and the overview fixture builders at
`ops.rs:1251-1266`.

1. **`undo_reverts_a_single_slot_edit`** — open the user fixture,
   `set_overview_visible`, undo, re-project, assert the original projection.

**One test per multi-write command (§3.5), each asserting `depth == 1` after the
command and full restoration after exactly ONE `undo()`.** The `depth` assert is
the load-bearing half: without it these pass with two entries and one press
reverting half.

2. **`undo_of_overview_window_add_reverts_both_files`** — open both slots, add a
   window, assert the char document gained its `overview_N` geometry, assert
   `depth == 1`, undo once, assert **both** documents are back. The core
   two-slot claim.
3. **`undo_of_tab_delete_reverts_the_char_side_remap`** — the third
   `try_edit_char` caller (§2.2), the one nobody knew was there.
4. **`undo_of_overview_window_remove_reverts_both_files`** — the second.
5. **`undo_of_a_column_copy_with_widths_is_one_step`** — the command Decision A
   was made for. Copy order + visibility + widths onto a second tab, assert both
   documents changed, assert `depth == 1`, undo **once**, assert both are back.
   Fails on the pre-decision design, which is the point.

Then the machinery:

6. **`try_edit_char_callers_are_snapshotted`** and
   **`multi_write_commands_open_a_group`** — the two §3.4 source tripwires.
7. **`a_group_closes_on_an_early_return`** — the RAII property, and the only
   test of it. Call `tab_delete` with a nonexistent tab index so its first
   `edit_slot` returns `Err` mid-group, then make a normal edit and assert
   `depth == 1`. With a leaked flag the normal edit pushes nothing and `depth`
   is 0.
8. **`redo_replays_what_undo_reverted`** and
   **`a_new_edit_clears_the_redo_stack`**.
9. **`the_stack_is_bounded_and_drops_the_oldest`** — `CAP + 5` edits; assert
   `depth == CAP`, undo `CAP` times, assert `can_undo == false` and the document
   is *not* back at its original state.
10. **`opening_a_file_clears_the_stack`** and
    **`closing_a_slot_clears_the_stack`** — the §5.3 corruption guard.
11. **`undo_back_to_the_save_point_reports_clean`** and
    **`undo_past_a_save_reports_dirty`** — §7.2's table, both rows. These are the
    two most likely to rot silently, because nothing else observes the counters.
12. **`a_failed_edit_pushes_nothing`** — `set_overview_visible` on a nonexistent
    tab index; assert `can_undo == false`.
13. **`undo_with_only_one_slot_open`** — open the user slot alone, edit, undo;
    assert no panic and the char slot stays empty (§5.4).

Atomicity (§3.7), which is its own commit and so its own pair:

14. **`a_failed_batch_leaves_the_document_untouched`** — `apply_mutations` with
    a valid `set_scalar` followed by one on a nonexistent path. Assert the
    projection equals the pre-call projection **and** `can_undo == false`: the
    document is untouched and no entry was minted for a batch that did nothing.
    This is the test that pins the rewritten `ops.rs:205-208` comment.
15. **`a_failed_second_write_rolls_back_the_whole_group`** — open a user fixture
    and a **read-only** character document (a file that does not round-trip, so
    `Document::load` decides `Fidelity::ReadOnly`, `document.rs:45-71`), then
    `overview_copy_columns` with `widths: true`. Assert `Err`, assert the
    account file's order is **unchanged**, assert `depth == 0`. This is the
    §3.7 table as a test, and it is the one that fails loudest if the rollback
    is put only in the closure's `Err` arm — the read-only guard returns before
    the closure runs.

### vitest

Following `OverviewView.spec.ts` and `routes/page.spec.ts`, using
`calls.stub` / `calls.of` / `calls.never` from `$lib/test/setup`.

1. **`AutofillView` / `KeybindsView` / `ProbeFormationsView`: a `refreshToken`
   bump re-reads the file** — three tests, each a direct copy of
   `OverviewView.spec.ts:113-124`. These fail today for a different reason (the
   prop does not exist) and are the regression guard for §7.1.
2. **`page.spec.ts`: after an undo, the unsaved badges come from the response** —
   stub `undo` returning `dirty: { char: false, user: false }` with the badges
   showing, assert both badges clear. Pins that the frontend does not keep its
   own opinion.
3. **`page.spec.ts`: `Ctrl+Z` in a text input does not undo** — focus the tree
   search box (`+page.svelte:612-616`), fire `Ctrl+Z`, `calls.never("undo")`.
4. **`KeybindsView.spec.ts`: `Ctrl+Z` while capturing a binding does not undo** —
   the `stopPropagation` at `KeybindsView.svelte:74` is one deletion away from
   being lost.
5. **`page.spec.ts`: `Ctrl+Z` with an empty stack shows "Nothing to undo"** —
   stub `undo` returning `null`, assert the toast, assert no crash.

---

## 13 Risks and rollback

| Risk | Severity | Mitigation |
| --- | --- | --- |
| Resident memory at depth | the real one | §4.3's measurement before merge; §4.4's threshold; `CAP = 20`; the encoded-bytes swap is one commit inside `undo.rs` |
| Deadlock — `edit_reshared` goes from one lock to three | high if wrong, but bounded | one order everywhere (§10), justified by three existing comments in the same file; verified that no caller holds a slot lock across it |
| A future command calls `try_edit_char` without snapshotting | medium — **has already happened once** (§2.2) | §3.4's tripwire A, the doc comment, and one behavioural test per caller |
| A future command writes twice without a group, costing two `Ctrl+Z` | medium — same failure mode, one rule over | §3.4's tripwire B, which needs no hand-maintained list |
| A leaked group flag silently disables undo for every later command | high if wrong | the RAII `Drop` (§3.5), pinned by `a_group_closes_on_an_early_return` (§12) |
| §3.7 changes `overview_copy_columns` behaviour beyond undo: a copy onto a read-only character now applies nothing instead of half | low, and intended | the §3.7 table; it makes "Copy failed" true and closes a dirty-flag hole (`OverviewColumnsTab.svelte:79-85`); called out in the release notes as a fix, not a silent change |
| A failed command clears the redo stack it did not use | low | accepted, §5.2 |
| The three unhooked views stay stale | medium | §7.1's three props; the three vitest tests fail without them; also fixes a pre-existing Discard/restore bug |
| `savedAt` now means two things | low | comment updated; rename deferred to avoid colliding with the Phase 2 and 5 diffs |
| Encoded fallback taken and `encode` fails mid-session | low | `Fidelity::Editable` (§2.5) makes it near-impossible; if it happens, drop the stack and report `can_undo = false` rather than failing the user's edit |
| Toast's Undo button undoes a newer step | low | dismiss the toast on any subsequent edit (§9.2) |

**Rollback** is unusually clean, and worth stating because it is the reason this
phase can be attempted at all. Undo adds a field to `AppState`, three commands,
one rewritten function and three props. Reverting it leaves `discardChanges`
doing exactly what it does today. There is no migration, no on-disk format, and
nothing else in the redesign holds a reference to it.

**Commit order.** The first two are worth keeping even if undo is reverted, which
is why they go first and go alone:

1. `refreshToken` on the three views (§7.1) — standalone; fixes a live
   Discard/restore bug.
2. `inAField` moving to `layout.ts` (§9.1) — standalone; a rule about the app
   living in one component.
3. `undo.rs` plus the `edit_reshared` push points, the group guard on the four
   commands of §3.5, and the two tripwires. The bulk.
4. **Atomicity (§3.7)** — the rollback on every error path, the rewritten
   `ops.rs:205-208` doc comment, and tests 14 and 15. Its own commit and its own
   test, as decided, but it lands *after* 3: the rollback restores the snapshot
   the undo stack takes, so it cannot exist before there is one.
5. The frontend: commands, keybinds, toasts.

Reverting 4 alone restores today's documented non-atomic behaviour and nothing
else; reverting 3 takes 4 with it.

---

## 14 Definition of done

- [ ] `Ctrl+Z` reverts the last document edit; `Ctrl+Shift+Z` and `Ctrl+Y` replay it.
- [ ] **One user action is one `Ctrl+Z`.** "Add overview window", "Remove
      overview window", "Delete tab" and "Copy columns (with widths)" each revert
      **both** files on a single press, and each leaves `depth` at 1.
- [ ] Every command that writes more than once holds an `undo::group` guard as
      its first statement, and the guard closes on the `?` paths too.
- [ ] A command that fails on a write leaves both documents and both stacks as
      it found them — including `apply_mutations` mid-batch and
      `overview_copy_columns` onto a read-only character.
- [ ] `ops.rs:205-208` no longer says "non-atomic", and says so in the same
      commit as the rollback.
- [ ] `Ctrl+Z` inside any text input, and during keybind capture, reaches the
      field's own undo and never the document stack.
- [ ] `Ctrl+Z` with an empty stack says "Nothing to undo." rather than nothing.
- [ ] After an undo, all six views and the Tree show the reverted state with no
      further user action.
- [ ] Undoing back to the last save clears the unsaved badge; undoing past it
      sets it again.
- [ ] Opening, closing, restoring a backup, or discarding clears both stacks;
      saving does not.
- [ ] The stack is capped at `CAP`, drops the oldest, and releases its memory.
- [ ] §4.3's measurement has been **run**, its median and max ratios recorded in
      §4.4, and the representation chosen against the threshold rather than
      against the argument.
- [ ] `try_edit_char` carries the §3.3 comment and both §3.4 tripwires pass.
- [ ] Every lock acquisition in `ops.rs` follows user → char → history, and the
      group guard holds none of them between acquire and drop.
- [ ] No `settings_preset_*`, backup-restore or batch-copy toast offers Undo.
- [ ] All Rust tests in §12 pass; all vitest tests in §12 pass;
      `npm run check` clean.
- [ ] `docs/ui-redesign/00-overview.md`'s phase table still describes this phase
      accurately.

_Added 2026-08-13 (UI/UX redesign, Phase 5b)._
