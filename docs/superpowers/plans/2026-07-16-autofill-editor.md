# Autofill Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A purpose-built editor for the client's remembered text-input history (`editHistory` in the account file) — per-list add/remove/edit/reorder/clear, plus a bulk "clear all" privacy nuke.

**Architecture:** Mirror the overview editor. A read-only projection in `settings-model` (`crates/settings-model/src/autofill.rs`) enumerates the widget-keyed string lists, resolving `Ref`/`Shared`. Edits are dedicated settings-model functions that `inline_all` the tree first (drop all sharing so replacing a list can't dangle a `Ref`), then rewrite list contents — the same proven pattern `overview.rs` uses. Thin tauri commands read/edit the **user** slot and re-project; a frontend `AutofillView.svelte` (a 4th button beside Tree/Layout/Overview) drives them, with a frontend-only label map for the cryptic widget paths.

**Tech Stack:** Rust (settings-model, `blue-marshal` codec, Tauri app crate), TypeScript + Svelte 5 (runes) frontend, `node --test` for zero-dep frontend unit tests.

> **Spec-correction note (read first):** The spec §5 said writes would reuse the raw `apply_mutation`. They do **not**. `mutate.rs::apply` refuses to remove any subtree containing a `Shared` store (`MutateError::SharedSubtree`), and real files dedup repeated/empty editHistory entries into `Shared`s — so a clear/remove would fail exactly when a list has deduped entries. This plan uses dedicated inline-first edit functions instead (the `overview.rs` pattern). Behavior is unchanged; only the mechanism differs. The spec doc has been updated to match.

## Global Constraints

- **No new dependencies.** `settings-model` and `blue-marshal` stay dependency-free; the frontend dependency list stays as-scaffolded (no test framework, no `@types/node`). Copied verbatim from repo conventions.
- **Commit messages:** sentence-case, **no** attribution trailers (no `Co-Authored-By`). Repo convention.
- **Live-directory rule:** automated tests never read or write the live EVE settings dir. Only the final manual smoke (Task 8) touches a live file, through the app's full backup→verify→atomic-write save chain.
- **Frontend tests:** `node --test`; import siblings with the `.ts` extension (e.g. `./autofill.ts`); no `node:test`/`node:assert` imports — a thrown error is the failure signal.
- **Dark native controls:** every new `<select>`/`<option>`/`<input>` gets explicit dark `background`/`color` (the app runs in a dark WebView2). Follow `OverviewView.svelte`'s `<style>` block.

---

### Task 1: Read projection `project_edit_history`

**Files:**
- Create: `crates/settings-model/src/autofill.rs`
- Modify: `crates/settings-model/src/lib.rs` (register module + re-export)

**Interfaces:**
- Produces:
  - `pub struct RememberedList { pub widget: String, pub entries: Vec<String> }` (derives `Debug, Serialize, PartialEq`)
  - `pub fn project_edit_history(user: &blue_marshal::Value) -> Vec<RememberedList>`

- [ ] **Step 1: Write the failing test**

Append to `crates/settings-model/src/autofill.rs` (module + tests together; the impl comes in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }

    /// user root -> b"ui" -> b"editHistory" -> (ts, { widget: [entries] })
    fn user_with_history() -> Value {
        let hist = Value::Dict(vec![
            (b("/addressbook/.../SingleLineEditText"),
             Value::List(vec![Value::Str("Jita".into()), Value::Str("Amarr".into())])),
            (b("/inventory/.../quickFilter"), Value::List(vec![Value::Str("veldspar".into())])),
        ]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        Value::Dict(vec![(b("ui"), ui)])
    }

    #[test]
    fn projects_widget_lists_in_order() {
        let lists = project_edit_history(&user_with_history());
        assert_eq!(lists.len(), 2);
        assert_eq!(lists[0].widget, "/addressbook/.../SingleLineEditText");
        assert_eq!(lists[0].entries, vec!["Jita", "Amarr"]);
        assert_eq!(lists[1].entries, vec!["veldspar"]);
    }

    #[test]
    fn a_file_without_edit_history_projects_empty() {
        assert!(project_edit_history(&Value::Dict(vec![])).is_empty());
    }

    #[test]
    fn resolves_ref_shared_keys_values_and_coerces_empty_bytes() {
        // Real idiom: the `editHistory` VALUE list holds a Shared-deduped string
        // and a bare Ref to it; a widget list also carries an empty Bytes entry.
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Str("Jita".into())) };
        let hist = Value::Dict(vec![
            (b("/a/box"), Value::List(vec![jita, Value::Bytes(vec![])])), // "Jita", ""
            (b("/b/box"), Value::List(vec![Value::Ref(1)])),              // -> "Jita"
        ]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        let user = Value::Dict(vec![(b("ui"), ui)]);
        let lists = project_edit_history(&user);
        assert_eq!(lists[0].entries, vec!["Jita", ""], "Shared resolved, empty Bytes -> \"\"");
        assert_eq!(lists[1].entries, vec!["Jita"], "Ref resolved to the Shared value");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p settings-model autofill`
Expected: FAIL to compile — `project_edit_history` / `RememberedList` not found.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/settings-model/src/autofill.rs` (above the `#[cfg(test)]` block):

```rust
//! Read + edit projection of the autofill / remembered-text category. All of it
//! lives in `core_user` under `ui -> editHistory -> (timestamp, dict)`, where the
//! dict maps a UI widget path (Bytes) to a list of remembered strings (Str, with
//! the occasional empty Bytes). Reads resolve Ref/Shared and unwrap the
//! (ts, dict)/(ts, list) wrappers; edits inline all sharing first (see the write
//! functions in Task 2) so replacing a list can never dangle a Ref.

use blue_marshal::Value;
use serde::Serialize;

use crate::treewalk::{collect_shared, effective, is_bytes, Entries, SharedTable};

#[derive(Debug, Serialize, PartialEq)]
pub struct RememberedList {
    pub widget: String,
    pub entries: Vec<String>,
}

pub fn project_edit_history(user: &Value) -> Vec<RememberedList> {
    let mut sh = SharedTable::new();
    collect_shared(user, &mut sh);
    let Value::Dict(root) = effective(user, &sh) else { return vec![] };
    let Some(ui) = find_child(root, b"ui", &sh).and_then(|v| as_dict(v, &sh)) else { return vec![] };
    let Some(eh) = find_child(ui, b"editHistory", &sh).and_then(|v| as_dict(v, &sh)) else { return vec![] };
    eh.iter()
        .filter_map(|(k, v)| {
            let widget = bytes_str(effective(k, &sh))?;
            let entries = as_list(v, &sh)?.iter().map(|e| entry_str(effective(e, &sh))).collect();
            Some(RememberedList { widget, entries })
        })
        .collect()
}

// ponytail: these four resolvers duplicate overview.rs's private copies rather
// than lifting them into treewalk — overview.rs is the repo's most-delicate code
// (mis-modeled three times) and not worth re-touching for ~20 shared lines.

/// Value of the entry whose RESOLVED key is `Bytes(name)`, itself resolved.
fn find_child<'a>(dict: &'a Entries, name: &[u8], sh: &SharedTable<'a>) -> Option<&'a Value> {
    dict.iter()
        .find(|(k, _)| matches!(effective(k, sh), Value::Bytes(b) if b.as_slice() == name))
        .map(|(_, v)| effective(v, sh))
}

/// Resolve to a dict, unwrapping a `(timestamp, dict)` wrapper.
fn as_dict<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Entries> {
    match effective(v, sh) {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter().find_map(|e| match effective(e, sh) {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

/// Resolve to a list, unwrapping a `(timestamp, list)` wrapper.
fn as_list<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<&'a Vec<Value>> {
    match effective(v, sh) {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter().find_map(|e| match effective(e, sh) {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}

fn bytes_str(v: &Value) -> Option<String> {
    match v {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }
}

/// Coerce a remembered entry to a display string. Entries are Str; the
/// occasional empty Bytes becomes "" (see the module doc); anything else "".
fn entry_str(v: &Value) -> String {
    match v {
        Value::Str(s) | Value::StrUcs2(s) => s.clone(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    }
}
```

Then register the module in `crates/settings-model/src/lib.rs`. Add after `pub mod overview;` (line 15):

```rust
pub mod autofill;
```

And add after the `pub use overview::{...};` line (line 25):

```rust
pub use autofill::{project_edit_history, RememberedList};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p settings-model autofill`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/autofill.rs crates/settings-model/src/lib.rs
git commit -m "Autofill: project editHistory remembered-string lists"
```

---

### Task 2: `inline_all` helper + `set_list_entries` write

**Files:**
- Modify: `crates/settings-model/src/treewalk.rs` (add `inline_all`)
- Modify: `crates/settings-model/src/autofill.rs` (add error type + write fn + mut helpers)
- Modify: `crates/settings-model/src/lib.rs` (re-export)

**Interfaces:**
- Consumes: `crate::treewalk::{collect_shared, inline_shares}` (already `pub(crate)`).
- Produces:
  - `pub(crate) fn treewalk::inline_all(v: &mut blue_marshal::Value)`
  - `pub enum AutofillError { NoHistory, NoList }` (`Debug, PartialEq, Serialize`, `#[serde(tag="code", rename_all="snake_case")]`)
  - `pub fn autofill::set_list_entries(user: &mut Value, widget: &str, entries: &[String]) -> Result<(), AutofillError>`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `crates/settings-model/src/autofill.rs`:

```rust
    #[test]
    fn set_list_entries_replaces_a_widget_list() {
        let mut user = user_with_history();
        set_list_entries(&mut user, "/inventory/.../quickFilter", &["scordite".into(), "pyroxeres".into()]).unwrap();
        let lists = project_edit_history(&user);
        let l = lists.iter().find(|l| l.widget == "/inventory/.../quickFilter").unwrap();
        assert_eq!(l.entries, vec!["scordite", "pyroxeres"]);
    }

    #[test]
    fn set_list_entries_can_clear_and_reports_missing() {
        let mut user = user_with_history();
        set_list_entries(&mut user, "/inventory/.../quickFilter", &[]).unwrap();
        let l = project_edit_history(&user).into_iter().find(|l| l.widget == "/inventory/.../quickFilter").unwrap();
        assert!(l.entries.is_empty());
        assert_eq!(set_list_entries(&mut user, "/nope", &["x".into()]), Err(AutofillError::NoList));
        assert_eq!(set_list_entries(&mut Value::Dict(vec![]), "/a", &[]), Err(AutofillError::NoHistory));
    }

    #[test]
    fn clearing_a_list_with_a_shared_entry_still_encodes() {
        // Real idiom: an identical remembered string in two widget lists is
        // deduped into one Shared, Ref'd from the other list. Replacing the list
        // that holds the Shared DEFINITION would dangle the Ref (RefBeforeStore on
        // encode) — inline_all before the edit prevents it. This is exactly the
        // case the raw apply_mutation refuses (SharedSubtree), which is why this
        // milestone uses a dedicated inline-first write.
        use blue_marshal::{decode, encode};
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Str("Jita".into())) };
        let hist = Value::Dict(vec![
            (b("/a/box"), Value::List(vec![jita, Value::Str("Amarr".into())])),
            (b("/b/box"), Value::List(vec![Value::Ref(1)])),
        ]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        let mut user = Value::Dict(vec![(b("ui"), ui)]);
        encode(&user).expect("fixture must encode before the edit");

        set_list_entries(&mut user, "/a/box", &[]).unwrap(); // clears the Shared def holder

        let bytes = encode(&user).expect("edited tree must still encode (no dangling Ref)");
        let lists = project_edit_history(&decode(&bytes).unwrap());
        assert!(lists.iter().find(|l| l.widget == "/a/box").unwrap().entries.is_empty());
        assert_eq!(lists.iter().find(|l| l.widget == "/b/box").unwrap().entries, vec!["Jita"],
            "widget B keeps its formerly-Ref'd value, now inlined");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p settings-model autofill`
Expected: FAIL to compile — `set_list_entries` / `AutofillError` not found.

- [ ] **Step 3: Write minimal implementation**

Add to `crates/settings-model/src/treewalk.rs` (near the existing `inline_shares`):

```rust
/// Drop ALL Shared/Ref sharing from a tree in place (inline every Shared to its
/// value, resolve every Ref). Used before a structural list edit so replacing a
/// list can never destroy a Shared definition that a Ref elsewhere still needs.
/// The re-saved file is larger (dedup gone) but valid; EVE re-dedups on logout.
pub(crate) fn inline_all(v: &mut Value) {
    let mut sh = SharedTable::new();
    collect_shared(v, &mut sh);
    *v = inline_shares(v, &sh);
}
```

Add to `crates/settings-model/src/autofill.rs`. First extend the imports:

```rust
use crate::treewalk::{collect_shared, effective, inline_all, is_bytes, Entries, SharedTable};
```

Then add the error type and write function (above the `#[cfg(test)]` block):

```rust
#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum AutofillError {
    /// The file has no `ui -> editHistory` structure at all.
    NoHistory,
    /// No remembered-string list for that widget path.
    NoList,
}

/// Replace one widget's remembered-string list with `entries` (written as Str).
/// An empty slice clears the list. Inlines all sharing first so the wholesale
/// replacement cannot dangle a Ref (see `inline_all`).
pub fn set_list_entries(user: &mut Value, widget: &str, entries: &[String]) -> Result<(), AutofillError> {
    inline_all(user);
    let eh = edit_history_mut(user).ok_or(AutofillError::NoHistory)?;
    let (_, v) = eh.iter_mut().find(|(k, _)| is_bytes(k, widget.as_bytes())).ok_or(AutofillError::NoList)?;
    let list = list_inner_mut(v).ok_or(AutofillError::NoList)?;
    *list = entries.iter().map(|s| Value::Str(s.clone())).collect();
    Ok(())
}

/// Mutable inner dict of root -> ui -> editHistory -> (ts, dict). Assumes a plain
/// tree (post-`inline_all`), so keys are plain Bytes and values plain wrappers.
fn edit_history_mut(user: &mut Value) -> Option<&mut Entries> {
    let Value::Dict(root) = user else { return None };
    let ui = child_dict_mut(root, b"ui")?;
    child_dict_mut(ui, b"editHistory")
}

fn child_dict_mut<'a>(dict: &'a mut Entries, name: &[u8]) -> Option<&'a mut Entries> {
    let (_, v) = dict.iter_mut().find(|(k, _)| is_bytes(k, name))?;
    dict_inner_mut(v)
}

fn dict_inner_mut(v: &mut Value) -> Option<&mut Entries> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
}

fn list_inner_mut(v: &mut Value) -> Option<&mut Vec<Value>> {
    match v {
        Value::List(l) => Some(l),
        Value::Tuple(items) => items.iter_mut().find_map(|e| match e {
            Value::List(l) => Some(l),
            _ => None,
        }),
        _ => None,
    }
}
```

Re-export in `crates/settings-model/src/lib.rs` — extend the autofill line from Task 1:

```rust
pub use autofill::{project_edit_history, set_list_entries, AutofillError, RememberedList};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p settings-model autofill`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/treewalk.rs crates/settings-model/src/autofill.rs crates/settings-model/src/lib.rs
git commit -m "Autofill: edit a widget's remembered list, inlining shares first"
```

---

### Task 3: `clear_all_history` write (the privacy nuke)

**Files:**
- Modify: `crates/settings-model/src/autofill.rs`
- Modify: `crates/settings-model/src/lib.rs` (re-export)

**Interfaces:**
- Produces: `pub fn autofill::clear_all_history(user: &mut Value) -> Result<(), AutofillError>` (empties every list; `Ok(())` no-op if the file has no editHistory).

- [ ] **Step 1: Write the failing test**

Add to the tests module in `crates/settings-model/src/autofill.rs`:

```rust
    #[test]
    fn clear_all_history_empties_every_list() {
        let mut user = user_with_history();
        clear_all_history(&mut user).unwrap();
        let lists = project_edit_history(&user);
        assert_eq!(lists.len(), 2, "widget keys are kept");
        assert!(lists.iter().all(|l| l.entries.is_empty()), "every list emptied");
    }

    #[test]
    fn clear_all_history_is_a_noop_without_edit_history() {
        assert_eq!(clear_all_history(&mut Value::Dict(vec![])), Ok(()));
    }

    #[test]
    fn clear_all_history_survives_shared_entries() {
        use blue_marshal::encode;
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Str("Jita".into())) };
        let hist = Value::Dict(vec![
            (b("/a/box"), Value::List(vec![jita, Value::Str("Amarr".into())])),
            (b("/b/box"), Value::List(vec![Value::Ref(1)])),
        ]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        let mut user = Value::Dict(vec![(b("ui"), ui)]);
        clear_all_history(&mut user).unwrap();
        encode(&user).expect("cleared tree must still encode");
        assert!(project_edit_history(&user).iter().all(|l| l.entries.is_empty()));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p settings-model autofill`
Expected: FAIL to compile — `clear_all_history` not found.

- [ ] **Step 3: Write minimal implementation**

Add to `crates/settings-model/src/autofill.rs` (above the `#[cfg(test)]` block):

```rust
/// Empty every remembered-string list in the file (widget keys kept). A no-op
/// success when the file has no editHistory. Inlines sharing first, like
/// `set_list_entries`, so a Shared entry never leaves a dangling Ref.
pub fn clear_all_history(user: &mut Value) -> Result<(), AutofillError> {
    inline_all(user);
    let Some(eh) = edit_history_mut(user) else { return Ok(()) };
    for (_, v) in eh.iter_mut() {
        if let Some(list) = list_inner_mut(v) {
            list.clear();
        }
    }
    Ok(())
}
```

Extend the re-export in `crates/settings-model/src/lib.rs`:

```rust
pub use autofill::{clear_all_history, project_edit_history, set_list_entries, AutofillError, RememberedList};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p settings-model autofill`
Expected: PASS (9 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/autofill.rs crates/settings-model/src/lib.rs
git commit -m "Autofill: clear all remembered text in one pass"
```

---

### Task 4: Realshape corpus guard

**Files:**
- Create: `crates/settings-model/tests/autofill_realshape.rs`

**Interfaces:**
- Consumes: `settings_model::{project_edit_history, set_list_entries, clear_all_history}`.

Rationale: the M3c lesson — the overview model was wrong three times because nothing exercised real-file idioms (Ref/Shared keys, deduped values, `(ts, …)` wrappers) through the codec. This integration test reproduces those idioms in a synthetic tree, round-trips it through `encode`/`decode`, and asserts the projection + edits behave. (True real-file validation is the Task 8 manual smoke; the local-only corpus stays gitignored.)

- [ ] **Step 1: Write the failing test**

Create `crates/settings-model/tests/autofill_realshape.rs`:

```rust
//! Real-idiom corpus guard for the autofill (editHistory) projection and edits.
//! A synthetic tree reproducing the STRUCTURE real `core_user` files use —
//! (timestamp, dict) editHistory wrapper, widget lists that share a repeated
//! string via Shared/Ref, an empty-Bytes junk entry, a (timestamp, list)-wrapped
//! widget value — encoded, decoded, and driven through the public API only. No
//! bytes were read from a real file.

use blue_marshal::{decode, encode, Value};
use settings_model::{clear_all_history, project_edit_history, set_list_entries};

fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
fn ts() -> Value { Value::Long(vec![0u8; 8]) }

/// root -> b"ui" -> b"editHistory" -> (ts, {
///   "/a/box": ["Jita"(Shared 1), ""(empty Bytes)],
///   "/b/box": (ts, [Ref 1 -> "Jita"]),   // (timestamp, list)-wrapped value
/// })
fn realshape_user() -> Value {
    // EVE's marshal only shares Bytes/Long/List/Dict — never Str — so a shared
    // remembered string is stored as Bytes; `entry_str` lossy-decodes it to "Jita".
    let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
    let hist = Value::Dict(vec![
        (b("/a/box"), Value::List(vec![jita, Value::Bytes(vec![])])),
        (b("/b/box"), Value::Tuple(vec![ts(), Value::List(vec![Value::Ref(1)])])),
    ]);
    let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
    Value::Dict(vec![(b("ui"), ui)])
}

#[test]
fn realshape_round_trips_and_projects() {
    let bytes = encode(&realshape_user()).expect("fixture must encode");
    let decoded = decode(&bytes).expect("must decode back");
    let lists = project_edit_history(&decoded);
    assert_eq!(lists.len(), 2);
    let a = lists.iter().find(|l| l.widget == "/a/box").unwrap();
    assert_eq!(a.entries, vec!["Jita", ""], "Shared resolved; empty Bytes -> \"\"");
    let bl = lists.iter().find(|l| l.widget == "/b/box").unwrap();
    assert_eq!(bl.entries, vec!["Jita"], "Ref resolved through the (ts,list) wrapper");
}

#[test]
fn realshape_edit_then_clear_all_still_encode() {
    let mut user = decode(&encode(&realshape_user()).unwrap()).unwrap();
    // Editing the list that owns the Shared def must not dangle /b/box's Ref.
    set_list_entries(&mut user, "/a/box", &["Dodixie".into()]).unwrap();
    let bytes = encode(&user).expect("post-edit tree must encode");
    let lists = project_edit_history(&decode(&bytes).unwrap());
    assert_eq!(lists.iter().find(|l| l.widget == "/a/box").unwrap().entries, vec!["Dodixie"]);
    assert_eq!(lists.iter().find(|l| l.widget == "/b/box").unwrap().entries, vec!["Jita"]);

    clear_all_history(&mut user).unwrap();
    encode(&user).expect("post-clear tree must encode");
    assert!(project_edit_history(&user).iter().all(|l| l.entries.is_empty()));
}
```

- [ ] **Step 2: Run test to verify it fails, then passes**

Run: `cargo test -p settings-model --test autofill_realshape`
Expected: PASS (the API already exists after Tasks 1–3; this test guards it). If it fails, the projection/edit has a real-idiom gap — fix in `autofill.rs`, not the test.

- [ ] **Step 3: Commit**

```bash
git add crates/settings-model/tests/autofill_realshape.rs
git commit -m "Autofill: guard editHistory against real Ref/Shared idioms"
```

---

### Task 5: Backend commands (ops + tauri wrappers)

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (imports, three functions + a helper, tests)
- Modify: `app/src-tauri/src/lib.rs` (three `#[tauri::command]` wrappers + registration)

**Interfaces:**
- Consumes: `settings_model::{project_edit_history, set_list_entries, clear_all_history, RememberedList, AutofillError}`.
- Produces (ops):
  - `pub fn autofill_lists(state: &AppState) -> Result<Vec<RememberedList>, ErrDto>`
  - `pub fn set_autofill_list(state: &AppState, widget: &str, entries: Vec<String>) -> Result<Vec<RememberedList>, ErrDto>`
  - `pub fn clear_all_autofill(state: &AppState) -> Result<Vec<RememberedList>, ErrDto>`
- Produces (commands): `autofill_lists`, `set_autofill_list`, `clear_all_autofill`.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `app/src-tauri/src/ops.rs` (after `overview_without_a_user_slot_errors`):

```rust
    fn autofill_user_bytes() -> Vec<u8> {
        // root -> b"ui" -> b"editHistory" -> (ts, { "/a/box": ["Jita", "Amarr"] })
        let hist = Value::Dict(vec![(
            Value::Bytes(b"/a/box".to_vec()),
            Value::List(vec![Value::Str("Jita".into()), Value::Str("Amarr".into())]),
        )]);
        let ui = Value::Dict(vec![(
            Value::Bytes(b"editHistory".to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), hist]),
        )]);
        encode(&Value::Dict(vec![(Value::Bytes(b"ui".to_vec()), ui)])).unwrap()
    }

    #[test]
    fn autofill_reads_edits_and_clears_the_user_slot() {
        let path = temp_file("af-user", &autofill_user_bytes());
        let state = AppState::new();
        open_file(&state, Slot::User, path.to_str().unwrap()).unwrap();

        let lists = autofill_lists(&state).unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].entries, vec!["Jita", "Amarr"]);

        let lists = set_autofill_list(&state, "/a/box", vec!["Dodixie".into()]).unwrap();
        assert_eq!(lists[0].entries, vec!["Dodixie"]);

        let lists = clear_all_autofill(&state).unwrap();
        assert!(lists[0].entries.is_empty(), "list emptied, widget kept");
    }

    #[test]
    fn autofill_without_a_user_slot_errors() {
        let state = AppState::new();
        assert_eq!(autofill_lists(&state).unwrap_err().code, "no_document");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p app autofill`
Expected: FAIL to compile — `autofill_lists` etc. not found.

- [ ] **Step 3: Write minimal implementation**

In `app/src-tauri/src/ops.rs`, extend the `use settings_model::{…}` block (lines 10-16) to add:

```rust
    clear_all_history, project_edit_history, set_list_entries, AutofillError, RememberedList,
```

Add after `set_overview_width` (around line 281), mirroring `edit_user_overview`:

```rust
pub fn autofill_lists(state: &AppState) -> Result<Vec<RememberedList>, ErrDto> {
    let user = state.user.lock().unwrap();
    let udoc = user.as_ref().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
    Ok(project_edit_history(&udoc.value))
}

/// Edit the user slot's editHistory, then re-project.
fn edit_user_autofill<F>(state: &AppState, edit: F) -> Result<Vec<RememberedList>, ErrDto>
where
    F: FnOnce(&mut blue_marshal::Value) -> Result<(), AutofillError>,
{
    {
        let mut guard = state.user.lock().unwrap();
        let doc = guard.as_mut().ok_or_else(|| ErrDto::new("no_document", "no account file open"))?;
        if let Fidelity::ReadOnly { reason } = &doc.fidelity {
            return Err(ErrDto::new("read_only", reason.clone()));
        }
        edit(&mut doc.value).map_err(|e| ErrDto::new("autofill", format!("{e:?}")))?;
    }
    autofill_lists(state)
}

pub fn set_autofill_list(state: &AppState, widget: &str, entries: Vec<String>) -> Result<Vec<RememberedList>, ErrDto> {
    edit_user_autofill(state, |v| set_list_entries(v, widget, &entries))
}

pub fn clear_all_autofill(state: &AppState) -> Result<Vec<RememberedList>, ErrDto> {
    edit_user_autofill(state, |v| clear_all_history(v))
}
```

In `app/src-tauri/src/lib.rs`, add three command wrappers after `set_overview_width` (line 148):

```rust
#[tauri::command]
fn autofill_lists(state: tauri::State<'_, AppState>) -> Result<Vec<settings_model::RememberedList>, ErrDto> {
    ops::autofill_lists(&state)
}
#[tauri::command]
fn set_autofill_list(state: tauri::State<'_, AppState>, widget: String, entries: Vec<String>) -> Result<Vec<settings_model::RememberedList>, ErrDto> {
    ops::set_autofill_list(&state, &widget, entries)
}
#[tauri::command]
fn clear_all_autofill(state: tauri::State<'_, AppState>) -> Result<Vec<settings_model::RememberedList>, ErrDto> {
    ops::clear_all_autofill(&state)
}
```

Add them to the `generate_handler!` list (line 162), after `set_overview_width`:

```rust
            overview_columns, set_overview_visible, set_overview_order, set_overview_width,
            autofill_lists, set_autofill_list, clear_all_autofill
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p app autofill && cargo build -p app`
Expected: PASS (2 tests) and a clean build (the `generate_handler!` registration compiles).

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs
git commit -m "Autofill: expose read/edit/clear commands on the user slot"
```

---

### Task 6: Frontend label module

**Files:**
- Create: `app/src/lib/autofill.ts`
- Create: `app/src/lib/autofill.test.ts`

**Interfaces:**
- Produces: `export function labelFor(widget: string): string` — a curated friendly name for known widgets, else a derived label; the raw path is shown separately by the view, never replaced.

- [ ] **Step 1: Write the failing test**

Create `app/src/lib/autofill.test.ts`:

```typescript
// Run: npm test  (node --test; Node strips the types). No test framework / no
// @types/node on purpose. A throw is the failing signal.
import { labelFor } from "./autofill.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

// Curated hit: a known People & Places search widget.
check(
  "curated widget gets its friendly name",
  labelFor("/addressbook/content/main/SearchPanel/Container/SingleLineEditText") ===
    "People & Places search",
);

// Curated hit via substring match (the needle appears mid-path).
check(
  "curated needle matches as a substring",
  labelFor("/inventory/content/main/quickFilter/SingleLineEditText") === "Quick Filter",
);

// Derived fallback: an UNCURATED widget must exercise derive() itself —
// strip boilerplate segments, split camelCase, title-case. (Must not match any
// curated needle, or it would never reach derive.)
check(
  "uncurated widget derives a readable label from camelCase",
  labelFor("/someWindow/content/main/mediumTimer/SingleLineEditText") === "Medium Timer",
);

// Never empty, even for a degenerate path.
check("empty-ish path never yields an empty label", labelFor("/") !== "");
check("raw-ish path with no useful segment falls back to the raw string",
  labelFor("///") === "///");

console.log("labelFor: all checks passed");
```

- [ ] **Step 2: Run test to verify it fails**

Run (from `app/`): `npm test`
Expected: FAIL — cannot resolve `./autofill.ts`.

- [ ] **Step 3: Write minimal implementation**

Create `app/src/lib/autofill.ts`:

```typescript
// Friendly labels for editHistory widget paths. The keys here are matched as
// substrings of the widget path (paths are long and version-specific, so an
// exact match would be brittle). Anything unmatched derives a label from the
// path; the raw path is always shown by the view, so a miss is never confusing.
const CURATED: [needle: string, label: string][] = [
  ["/addressbook/", "People & Places search"],
  ["quickFilter", "Quick Filter"],
  ["/wallet/", "Wallet transfer reason"],
  ["overviewExport", "Overview export filename"],
  ["/fitting", "Fitting name"],
  ["/fleet", "Fleet name"],
  ["structureBrowser", "Structure browser search"],
  ["skillCatalog", "Skill catalogue search"],
  ["channelName", "Chat channel name"],
  ["bugReport", "Bug report title"],
];

// Path segments that carry no meaning for a human — dropped before deriving.
const BOILERPLATE = new Set(["content", "main", "container", "singlelineedittext", "editText"]);

export function labelFor(widget: string): string {
  const lower = widget.toLowerCase();
  for (const [needle, label] of CURATED) {
    if (lower.includes(needle.toLowerCase())) return label;
  }
  return derive(widget);
}

function derive(widget: string): string {
  const segments = widget
    .split("/")
    .filter((s) => s.length > 0 && !BOILERPLATE.has(s.toLowerCase()));
  const pick = segments[segments.length - 1];
  if (!pick) return widget; // nothing useful — show the raw path rather than "".
  // Split camelCase / snake into words and title-case them.
  const words = pick
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/[_-]+/g, " ")
    .trim()
    .split(/\s+/);
  return words.map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");
}
```

- [ ] **Step 4: Run test to verify it passes**

Run (from `app/`): `npm test`
Expected: PASS — `labelFor: all checks passed` (and the existing suites still pass).

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/autofill.ts app/src/lib/autofill.test.ts
git commit -m "Autofill: label remembered-string lists for humans"
```

---

### Task 7: Frontend view + wiring

**Files:**
- Modify: `app/src/lib/api.ts` (types + three bindings)
- Create: `app/src/lib/AutofillView.svelte`
- Modify: `app/src/routes/+page.svelte` (import, switcher button, view block)

**Interfaces:**
- Consumes: `api.autofillLists`, `api.setAutofillList`, `api.clearAllAutofill`, `labelFor`.

- [ ] **Step 1: Add the API bindings**

In `app/src/lib/api.ts`, add the type after `OverviewColumns` (line 183):

```typescript
export interface RememberedList {
  widget: string;
  entries: string[];
}
```

Add to the `api` object (after the `setOverviewWidth` binding, line 217):

```typescript
  autofillLists: () => invoke<RememberedList[]>("autofill_lists"),
  setAutofillList: (widget: string, entries: string[]) =>
    invoke<RememberedList[]>("set_autofill_list", { widget, entries }),
  clearAllAutofill: () => invoke<RememberedList[]>("clear_all_autofill"),
```

- [ ] **Step 2: Create the view**

Create `app/src/lib/AutofillView.svelte` (mirrors `OverviewView.svelte`'s props/reload/dark-controls conventions):

```svelte
<script lang="ts">
  import { api, errMessage, type RememberedList } from "./api";
  import { labelFor } from "./autofill";
  import { message, confirm } from "@tauri-apps/plugin-dialog";

  let { userOpen, onUserDirty }: { userOpen: boolean; onUserDirty: () => void } = $props();

  let lists = $state<RememberedList[] | null>(null);
  let error = $state<string | null>(null);

  async function reload() {
    if (!userOpen) { lists = null; return; }
    error = null;
    try { lists = await api.autofillLists(); }
    catch (e) { error = errMessage(e); }
  }
  $effect(() => { void userOpen; reload(); });

  // Sort by friendly label for findability; the raw path is shown per row.
  const sorted = $derived(
    lists ? [...lists].sort((a, b) => labelFor(a.widget).localeCompare(labelFor(b.widget))) : [],
  );

  async function commit(widget: string, entries: string[]) {
    try { lists = await api.setAutofillList(widget, entries); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  const removeAt = (l: RememberedList, i: number) =>
    commit(l.widget, l.entries.filter((_, j) => j !== i));
  const editAt = (l: RememberedList, i: number, text: string) =>
    commit(l.widget, l.entries.map((e, j) => (j === i ? text : e)));
  const addTo = (l: RememberedList, text: string) => {
    if (text.trim() === "") return;
    commit(l.widget, [...l.entries, text]);
  };
  const clearList = (l: RememberedList) => commit(l.widget, []);

  // Drag-reorder within one list.
  let drag = $state<{ widget: string; from: number } | null>(null);
  function drop(l: RememberedList, to: number) {
    if (!drag || drag.widget !== l.widget) { drag = null; return; }
    const next = [...l.entries];
    const [moved] = next.splice(drag.from, 1);
    next.splice(to, 0, moved);
    drag = null;
    commit(l.widget, next);
  }

  async function clearAll() {
    const ok = await confirm(
      "Clear ALL remembered text in this account file? Every autofill list will be emptied. A backup is taken on save.",
      { title: "Clear all remembered text", kind: "warning" },
    );
    if (!ok) return;
    try { lists = await api.clearAllAutofill(); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Clear all failed", kind: "error" }); }
  }
</script>

{#if !userOpen}
  <p class="hint">Open an account file to edit its remembered text.</p>
{:else if error}
  <p class="error">{error}</p>
{:else if lists && lists.length === 0}
  <p class="hint">No remembered text in this account file yet.</p>
{:else if lists}
  <div class="af-top">
    <button class="danger" onclick={clearAll}>Clear all remembered text</button>
  </div>
  {#each sorted as l (l.widget)}
    <section class="af-list">
      <header>
        <span class="title" title={l.widget}>{labelFor(l.widget)}</span>
        <span class="path">{l.widget}</span>
        <button class="mini" onclick={() => clearList(l)} disabled={l.entries.length === 0}>Clear</button>
      </header>
      <ul>
        {#each l.entries as entry, i (i)}
          <li draggable="true"
              ondragstart={(e) => { drag = { widget: l.widget, from: i };
                e.dataTransfer?.setData("text/plain", String(i));
                if (e.dataTransfer) e.dataTransfer.effectAllowed = "move"; }}
              ondragover={(e) => { e.preventDefault();
                if (e.dataTransfer) e.dataTransfer.dropEffect = "move"; }}
              ondrop={(e) => { e.preventDefault(); drop(l, i); }}
              ondragend={() => (drag = null)}>
            <span class="grip" title="Drag to reorder">⠿</span>
            <input value={entry}
                   onchange={(e) => editAt(l, i, (e.target as HTMLInputElement).value)} />
            <button class="mini" title="Remove" onclick={() => removeAt(l, i)}>×</button>
          </li>
        {/each}
        <li class="add">
          <input placeholder="+ add remembered text…"
                 onkeydown={(e) => { if (e.key === "Enter") {
                   const t = e.target as HTMLInputElement; addTo(l, t.value); t.value = ""; } }} />
        </li>
      </ul>
    </section>
  {/each}
{/if}

<style>
  .af-top { margin-bottom: 0.75rem; }
  .af-list { margin-bottom: 1rem; }
  .af-list header { display: flex; align-items: baseline; gap: 0.6rem; }
  .af-list .title { font-weight: 600; }
  .af-list .path { color: var(--fg-dim); font-size: 0.8em; overflow: hidden; text-overflow: ellipsis; }
  .af-list ul { list-style: none; padding: 0; margin: 0.25rem 0 0; }
  .af-list li { display: flex; align-items: center; gap: 0.4rem; padding: 0.1rem 0; }
  .grip { cursor: grab; opacity: 0.6; }
  /* Dark native controls: the app runs in a dark WebView2 (see the memo). */
  input, button.mini, button.danger {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 6px; font: inherit;
  }
  .af-list li input { flex: 1; }
  button.danger { border-color: #a33; }
  .hint, .error { color: var(--fg-dim); }
  .error { color: #e66; }
</style>
```

- [ ] **Step 3: Wire it into the page**

In `app/src/routes/+page.svelte`:

Add the import beside the other view imports (near line 8):

```svelte
  import AutofillView from "$lib/AutofillView.svelte";
```

Add the switcher button right after the Overview button (line 359), so it appears only when a user file is open:

```svelte
            {#if slots.user?.status === "opened"}<button class:active={view === "autofill"} onclick={() => (view = "autofill")}>Autofill</button>{/if}
```

Add the view block after the `{:else if view === "overview"}` block (after line 388, before the `{:else}` tree block):

```svelte
      {:else if view === "autofill"}
        <div class="tree-area">
          <AutofillView
            userOpen={slots.user?.status === "opened"}
            onUserDirty={() => (dirtySlots.user = true)} />
        </div>
```

- [ ] **Step 4: Typecheck and build**

Run (from `app/`): `npm run check && npm run build`
Expected: 0 errors from `svelte-check`; a clean build.

Note: `view` is a string-union `$state`; adding `"autofill"` as a value it can hold requires no type change if it's typed by inference from its initial `"tree"`. If `svelte-check` flags the new literal, widen the declaration to include `"autofill"` (search for `let view` near the top of `+page.svelte`).

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/api.ts app/src/lib/AutofillView.svelte app/src/routes/+page.svelte
git commit -m "Autofill: add the editor view and Tree/Layout/Overview switch"
```

---

### Task 8: Manual live smoke (spec §8 exit gate)

**Files:**
- Modify: `docs/format-notes.md` (record the smoke result under `## Status`)

No automated test can validate that EVE accepts the edited file — this is the sole live-directory step, run through the app's full save chain (backup taken).

- [ ] **Step 1: Build and run the app**

Run (from `app/`): `npm run tauri dev`

- [ ] **Step 2: Exercise the editor against a real account file**

- Open a `core_user_*.dat` (an account you can log into).
- Open the **Autofill** view. Confirm the remembered-string lists appear with friendly labels and raw paths, and that a list you recognize (e.g. a People & Places search you've run) shows your past entries.
- **Add** a distinctive marker string (e.g. `ZZ-smoke-marker`) to one list; **Save**. Confirm a backup file appears in the profile's `eve-settings-editor-backups/`.
- **Remove** one existing entry and **reorder** another; Save.
- Use **Clear all remembered text**; confirm the dialog, Save.

- [ ] **Step 3: Verify in-game**

- Launch EVE, log into that account.
- Confirm the client loads normally (no settings reset / no corruption warning).
- Type in the widget you added the marker to and confirm the autocomplete reflects your edits (marker present before the clear-all; gone after a clear-all + save).

- [ ] **Step 4: Record the result**

Add a dated line under `## Status` in `docs/format-notes.md` noting the autofill editor passed the live smoke (what was added/removed/cleared, that EVE accepted the file). If anything failed, STOP and open a `systematic-debugging` pass instead of recording success.

- [ ] **Step 5: Commit**

```bash
git add docs/format-notes.md
git commit -m "Autofill: record the live smoke result"
```

---

## Self-Review

**Spec coverage:**
- §1 purpose (curation + cleanup) → Tasks 2 (edit), 3 (clear-all), 7 (view with add/edit/remove/reorder/clear + clear-all button). ✓
- §2 format (`ui → editHistory → (ts, dict)` → widget → list) → Task 1 projection + Tasks 2/3 writes. ✓
- §3 placement (Autofill button, user-file-gated) → Task 7 Step 3. ✓
- §4 read-only projection, raw widget paths, Ref/Shared resolved → Task 1. ✓
- §5 writes — **corrected**: inline-first dedicated functions, not `apply_mutation` (see the spec-correction note; spec doc updated). Clear/clear-all in one apply → Tasks 2/3. ✓
- §6 frontend (list-of-lists, per-list clear, add/edit/remove/reorder, clear-all with confirm, curated+derived labels, raw path shown, dark controls) → Tasks 6, 7. ✓
- §7 edge cases (timestamp preserved — `list_inner_mut` replaces list contents, leaving the `(ts, …)` wrapper; empty Bytes → ""; no editHistory → empty state / no-op clear) → Tasks 1, 3, 7. ✓
- §8 testing (projection units + realshape guard + round-trip encode + label unit + manual smoke) → Tasks 1–4, 6, 8. ✓
- §9 out-of-scope (filter presets, new widget creation) → not implemented, correctly. ✓

**Placeholder scan:** none — every step has full code or an exact command.

**Type consistency:** `RememberedList { widget, entries }` identical in `autofill.rs`, `RememberedList` re-export, ops signatures, `api.ts` interface, and the view. `set_list_entries(user, widget, entries: &[String])` and `set_autofill_list(state, widget, entries: Vec<String>)` consistent. `AutofillError { NoHistory, NoList }` used identically in autofill.rs and mapped in ops. Command names `autofill_lists` / `set_autofill_list` / `clear_all_autofill` match across ops, lib.rs, `generate_handler!`, and `api.ts` invoke strings. ✓
