# M4 Batch Apply Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Copy settings from one source file to many same-type target files — either the whole file (full copy) or selected categories (window layout, autofill) — each target backed up and written through the existing save chain.

**Architecture:** A new `crates/settings-model/src/batch.rs` owns the pure tree extract/splice plus two filesystem entry points that reuse the save chain (`save::backup_current`, `save::atomic_write`, `Document::load`, `save::save`). The app crate's `ops.rs` adds a target-discovery filter and a per-target orchestrator that reads the source once and never halts on a single target's failure; `lib.rs` exposes both as Tauri commands. The frontend adds a standalone `BatchView.svelte` main-pane view.

**Tech Stack:** Rust (settings-model + Tauri app crate), TypeScript + Svelte 5 (runes), `blue-marshal` codec (local crate, already a normal dep of both crates).

## Global Constraints

- **No new third-party dependencies.** `blue-marshal` and `settings-model` are local path crates; both are already dependencies of the app crate.
- **Synthetic IDs only in tests/fixtures** — never a real character/account ID.
- **Marshal shares only Bytes/Long/List/Dict/Tuple, never Str.** Any `Shared`/`Ref` string fixture must use `Value::Bytes`, or `encode` fails `NotStorable("str")`.
- **Category copy inlines all sharing** (`treewalk::inline_all`) before splicing — accepted ~1.5× file growth that self-heals on EVE's next re-dedup. This is the already-logged re-share debt, not new.
- **Commits: sentence-case, NO attribution trailers** (repo convention).
- **Rust tests:** `cargo test --manifest-path <crate>/Cargo.toml`. **Frontend tests:** run from `app/` via **PowerShell** (`npm` is not on the Bash tool's PATH): `npm test` (node --test, zero-dep), `npm run check` (svelte-check), `npm run build`.
- **`pub(crate)` reuse:** `save::backup_current`, `save::atomic_write`, `treewalk::inline_all`, `treewalk::is_bytes`, `treewalk::Entries` are all reachable from `batch.rs` (same crate). `save::{save, SaveError, SaveReport}`, `Document`, `Fidelity`, `LoadError` are public.

---

### Task 1: Category model + pure tree extract/splice

**Files:**
- Create: `crates/settings-model/src/batch.rs`
- Modify: `crates/settings-model/src/lib.rs` (add `pub mod batch;` after line 16; add re-export after line 27)

**Interfaces:**
- Consumes: `blue_marshal::Value`; `crate::treewalk::{inline_all, is_bytes, Entries}`.
- Produces:
  - `enum Category { Layout, Autofill }` (`Copy`, serde `snake_case`).
  - `fn extract_categories(source: &Value, cats: &[Category]) -> Vec<(Category, Value)>`
  - `fn apply_to_tree(target: &mut Value, extracted: &[(Category, Value)])`

- [ ] **Step 1: Write the failing test**

Create `crates/settings-model/src/batch.rs` with only a test module first:

```rust
//! Batch apply: extract a projection category's subtree from one document and
//! splice it into another. The category subtree is the VALUE at a fixed key
//! path — `windows` (char file) or `ui -> editHistory` (user file). Extract
//! inlines the source's sharing first so a Ref inside the category that points
//! at a Shared defined elsewhere resolves; splice inlines the target's sharing
//! first so replacing the subtree can never dangle a Ref the rest of the file
//! still holds (the proven autofill.rs / overview.rs inline-first idiom).

use std::path::{Path, PathBuf};

use blue_marshal::Value;
use serde::{Deserialize, Serialize};

use crate::document::{Document, LoadError};
use crate::save::{save, SaveError, SaveReport};
use crate::treewalk::{inline_all, is_bytes, Entries};

#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::{decode, encode};

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }

    /// user root -> ui -> editHistory -> (ts, { "/a": ["Jita"] })
    fn user_a() -> Value {
        let hist = Value::Dict(vec![(b("/a"), Value::List(vec![Value::Str("Jita".into())]))]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        Value::Dict(vec![(b("ui"), ui)])
    }

    /// user root -> ui -> editHistory -> (ts, { "/b": ["Amarr"] }) plus a sibling key.
    fn user_b() -> Value {
        let hist = Value::Dict(vec![(b("/b"), Value::List(vec![Value::Str("Amarr".into())]))]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        Value::Dict(vec![(b("ui"), ui), (b("keep"), Value::Int(7))])
    }

    #[test]
    fn extract_then_apply_replaces_the_category_and_keeps_siblings() {
        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        assert_eq!(extracted.len(), 1);
        let mut target = user_b();
        apply_to_tree(&mut target, &extracted);

        // The autofill category is now A's; the unrelated sibling survived.
        let lists = crate::autofill::project_edit_history(&target);
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].widget, "/a");
        assert_eq!(lists[0].entries, vec!["Jita"]);
        let Value::Dict(root) = &target else { panic!() };
        assert!(root.iter().any(|(k, v)| is_bytes(k, b"keep") && matches!(v, Value::Int(7))));
    }

    #[test]
    fn apply_inserts_the_category_when_the_target_lacks_it() {
        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        // Target has a `ui` dict but no editHistory entry.
        let mut target = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        apply_to_tree(&mut target, &extracted);
        let lists = crate::autofill::project_edit_history(&target);
        assert_eq!(lists[0].entries, vec!["Jita"]);
    }

    #[test]
    fn extract_resolves_a_ref_into_a_shared_defined_outside_the_category() {
        // The category's list holds a Ref; the Shared it points at is defined
        // OUTSIDE editHistory. Without inlining the whole source first, the
        // extracted subtree would carry a dangling Ref that fails to encode.
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
        let hist = Value::Dict(vec![(b("/a"), Value::List(vec![Value::Ref(1)]))]);
        let ui = Value::Dict(vec![
            (b("shareDef"), Value::List(vec![jita])), // Shared def, sibling of editHistory
            (b("editHistory"), Value::Tuple(vec![ts(), hist])),
        ]);
        let source = Value::Dict(vec![(b("ui"), ui)]);
        encode(&source).expect("fixture encodes (def precedes ref)");

        let extracted = extract_categories(&source, &[Category::Autofill]);
        // Put the extracted subtree in a bare target and prove it encodes alone.
        let mut target = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
        apply_to_tree(&mut target, &extracted);
        let bytes = encode(&target).expect("extracted subtree has no dangling Ref");
        let lists = crate::autofill::project_edit_history(&decode(&bytes).unwrap());
        assert_eq!(lists[0].entries, vec!["Jita"]);
    }

    #[test]
    fn apply_inlines_the_target_so_an_outside_ref_into_the_old_category_survives() {
        // Target: the OLD editHistory holds a Shared def; a sibling Ref points at
        // it. Replacing editHistory drops the def — so apply_to_tree must inline
        // the target first or the sibling Ref dangles on encode.
        let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
        let old_hist = Value::Dict(vec![(b("/old"), Value::List(vec![jita]))]);
        let ui = Value::Dict(vec![
            (b("editHistory"), Value::Tuple(vec![ts(), old_hist])), // def, encoded first
            (b("sibling"), Value::List(vec![Value::Ref(1)])),       // ref outside the category
        ]);
        let mut target = Value::Dict(vec![(b("ui"), ui)]);
        encode(&target).expect("target fixture encodes before the splice");

        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        apply_to_tree(&mut target, &extracted);
        encode(&target).expect("post-splice target encodes (outside Ref inlined, not dangled)");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml batch::`
Expected: FAIL to compile — `extract_categories` / `apply_to_tree` / `Category` not found.

- [ ] **Step 3: Write minimal implementation**

Add, above the `#[cfg(test)] mod tests`, in `crates/settings-model/src/batch.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Layout,
    Autofill,
}

impl Category {
    /// Key path from the document root to this category's subtree VALUE.
    fn key_path(self) -> &'static [&'static [u8]] {
        match self {
            Category::Layout => &[b"windows"],
            Category::Autofill => &[b"ui", b"editHistory"],
        }
    }
}

/// Inline the source's sharing, then clone each requested category's subtree.
/// Categories the source lacks are skipped (absent from the result).
pub fn extract_categories(source: &Value, cats: &[Category]) -> Vec<(Category, Value)> {
    let mut s = source.clone();
    inline_all(&mut s);
    let Value::Dict(root) = &s else { return Vec::new() };
    cats.iter()
        .filter_map(|&cat| {
            let keys = cat.key_path();
            let (parent_keys, last) = keys.split_at(keys.len() - 1);
            let parent = descend_ref(root, parent_keys)?;
            let (_, v) = parent.iter().find(|(k, _)| is_bytes(k, last[0]))?;
            Some((cat, v.clone()))
        })
        .collect()
}

/// Inline the target's sharing, then replace (or insert) each category's subtree.
/// A missing intermediate parent dict (e.g. no `ui`) skips that category.
pub fn apply_to_tree(target: &mut Value, extracted: &[(Category, Value)]) {
    inline_all(target);
    let Value::Dict(root) = target else { return };
    for (cat, subtree) in extracted {
        let keys = cat.key_path();
        let (parent_keys, last) = keys.split_at(keys.len() - 1);
        let Some(parent) = descend_mut(root, parent_keys) else { continue };
        match parent.iter_mut().find(|(k, _)| is_bytes(k, last[0])) {
            Some((_, v)) => *v = subtree.clone(),
            None => parent.push((Value::Bytes(last[0].to_vec()), subtree.clone())),
        }
    }
}

/// Inner dict of a plain (post-inline) value, unwrapping a `(ts, dict)` tuple.
fn dict_inner(v: &Value) -> Option<&Entries> {
    match v {
        Value::Dict(d) => Some(d),
        Value::Tuple(items) => items.iter().find_map(|e| match e {
            Value::Dict(d) => Some(d),
            _ => None,
        }),
        _ => None,
    }
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

fn descend_ref<'a>(root: &'a Entries, keys: &[&[u8]]) -> Option<&'a Entries> {
    let mut cur = root;
    for &key in keys {
        let (_, v) = cur.iter().find(|(k, _)| is_bytes(k, key))?;
        cur = dict_inner(v)?;
    }
    Some(cur)
}

fn descend_mut<'a>(root: &'a mut Entries, keys: &[&[u8]]) -> Option<&'a mut Entries> {
    let mut cur = root;
    for &key in keys {
        let (_, v) = cur.iter_mut().find(|(k, _)| is_bytes(k, key))?;
        cur = dict_inner_mut(v)?;
    }
    Some(cur)
}
```

Then wire the module in `crates/settings-model/src/lib.rs`. After line 16 (`pub mod autofill;`) add:

```rust
pub mod batch;
```

After line 27 (the `pub use autofill::...` line) add:

```rust
pub use batch::{apply_categories_to, apply_to_tree, extract_categories, full_copy_to, Category};
```

Note: `apply_categories_to` and `full_copy_to` do not exist yet (Task 2). To keep Task 1 compiling on its own, add ONLY the symbols that exist now:

```rust
pub use batch::{apply_to_tree, extract_categories, Category};
```

(Task 2 extends this line.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml batch::`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/batch.rs crates/settings-model/src/lib.rs
git commit -m "Batch apply: category model and pure tree extract/splice"
```

---

### Task 2: Filesystem entry points — full copy and category apply

**Files:**
- Modify: `crates/settings-model/src/batch.rs` (add two public functions + tests)
- Modify: `crates/settings-model/src/lib.rs` (extend the batch re-export line)

**Interfaces:**
- Consumes: `Document::load`, `save::save`, `save::{backup_current, atomic_write}`, `apply_to_tree` (Task 1).
- Produces:
  - `fn full_copy_to(source_bytes: &[u8], target: &Path) -> Result<PathBuf, String>` — returns the backup path.
  - `fn apply_categories_to(target: &Path, extracted: &[(Category, Value)]) -> Result<SaveReport, String>`

- [ ] **Step 1: Write the failing test**

Add these tests inside the existing `mod tests` in `crates/settings-model/src/batch.rs`:

```rust
    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("batch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn full_copy_overwrites_bytes_and_backs_up() {
        let dir = temp_dir("full");
        let src = dir.join("core_char_1.dat");
        let dst = dir.join("core_char_2.dat");
        let src_bytes = encode(&user_a()).unwrap();
        std::fs::write(&src, &src_bytes).unwrap();
        std::fs::write(&dst, encode(&user_b()).unwrap()).unwrap();

        let backup = full_copy_to(&src_bytes, &dst).unwrap();
        assert!(backup.exists(), "target backed up before overwrite");
        assert_eq!(std::fs::read(&dst).unwrap(), src_bytes, "target now byte-identical to source");
    }

    #[test]
    fn category_apply_replaces_only_the_category_on_disk() {
        let dir = temp_dir("cat");
        let dst = dir.join("core_user_2.dat");
        std::fs::write(&dst, encode(&user_b()).unwrap()).unwrap();

        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        let report = apply_categories_to(&dst, &extracted).unwrap();
        assert!(report.backup_path.exists());

        let reread = decode(&std::fs::read(&dst).unwrap()).unwrap();
        let lists = crate::autofill::project_edit_history(&reread);
        assert_eq!(lists[0].widget, "/a", "category came from the source");
        let Value::Dict(root) = &reread else { panic!() };
        assert!(root.iter().any(|(k, _)| is_bytes(k, b"keep")), "sibling key preserved on disk");
    }

    #[test]
    fn category_apply_refuses_a_read_only_target() {
        // A non-canonical stream (INT8-encoded 1) loads ReadOnly; save refuses it.
        let dir = temp_dir("ro");
        let dst = dir.join("core_user_3.dat");
        std::fs::write(&dst, [0x7E, 0, 0, 0, 0, 0x06, 0x01]).unwrap();
        let extracted = extract_categories(&user_a(), &[Category::Autofill]);
        let err = apply_categories_to(&dst, &extracted).unwrap_err();
        assert!(err.contains("ReadOnly"), "read-only target surfaced as an error: {err}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml batch::`
Expected: FAIL to compile — `full_copy_to` / `apply_categories_to` not found.

- [ ] **Step 3: Write minimal implementation**

Add to `crates/settings-model/src/batch.rs`, after `apply_to_tree`:

```rust
/// Back up `target`, then atomically overwrite it with `source_bytes`. Byte-for-
/// byte; the source is already a valid file. Returns the backup path.
pub fn full_copy_to(source_bytes: &[u8], target: &Path) -> Result<PathBuf, String> {
    let backup = crate::save::backup_current(target)?;
    crate::save::atomic_write(target, source_bytes)?;
    Ok(backup)
}

/// Load `target`, splice each extracted category in, and run the full save chain
/// (encode -> verify -> backup -> atomic write; ReadOnly targets are refused).
/// `force_conflict = true`: the target is loaded fresh in this call, so there is
/// no genuine conflict to guard against.
pub fn apply_categories_to(
    target: &Path,
    extracted: &[(Category, Value)],
) -> Result<SaveReport, String> {
    let mut doc = Document::load(target).map_err(|e| match e {
        LoadError::Io(m) => format!("Io: {m}"),
        LoadError::Decode { message, .. } => format!("Decode: {message}"),
    })?;
    apply_to_tree(&mut doc.value, extracted);
    save(&mut doc, true).map_err(|e| format!("{e:?}"))
}
```

Extend the batch re-export line in `crates/settings-model/src/lib.rs` to:

```rust
pub use batch::{apply_categories_to, apply_to_tree, extract_categories, full_copy_to, Category};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml batch::`
Expected: PASS (7 tests total).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/batch.rs crates/settings-model/src/lib.rs
git commit -m "Batch apply: full-copy and category-apply filesystem paths"
```

---

### Task 3: Real-idiom corpus guard

**Files:**
- Create: `crates/settings-model/tests/batch_realshape.rs`

**Interfaces:**
- Consumes: `settings_model::{extract_categories, apply_to_tree, Category, window_layout, project_edit_history}`.
- Produces: nothing (integration test only).

- [ ] **Step 1: Write the failing test**

Create `crates/settings-model/tests/batch_realshape.rs`:

```rust
//! Real-idiom guard for batch category copy. Synthetic trees reproducing the
//! STRUCTURE real files use — a `windows` container keyed by a Shared window id
//! with a Ref elsewhere (char), a `(ts, dict)` editHistory whose list shares a
//! Bytes string via Shared/Ref (user) — encoded, decoded, and driven through the
//! public batch API only. No bytes were read from a real file.

use blue_marshal::{decode, encode, Value};
use settings_model::{
    apply_to_tree, extract_categories, project_edit_history, window_layout, Category,
};

fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
fn ts() -> Value { Value::Long(vec![0u8; 8]) }

fn geom() -> Value {
    Value::Tuple(vec![
        Value::Int(1), Value::Int(2), Value::Int(3),
        Value::Int(4), Value::Int(2560), Value::Int(1440),
    ])
}

/// char root -> windows -> { windowSizesAndPositions_1: (ts, { "overview": geom }),
///                           openWindows: (ts, { <Shared "overview">: True }) }
/// The window id "overview" is shared between the two sub-dicts (Shared + Ref).
fn char_with_layout(id: &str) -> Value {
    let name = Value::Shared { slot: 1, value: Box::new(Value::Bytes(id.as_bytes().to_vec())) };
    let geoms = Value::Dict(vec![(name, geom())]);
    let opens = Value::Dict(vec![(Value::Ref(1), Value::Bool(true))]);
    Value::Dict(vec![(
        b("windows"),
        Value::Dict(vec![
            (b("windowSizesAndPositions_1"), Value::Tuple(vec![ts(), geoms])),
            (b("openWindows"), Value::Tuple(vec![ts(), opens])),
        ]),
    )])
}

/// user root -> ui -> editHistory -> (ts, { "/a": [Shared "Jita"], "/b": [Ref -> "Jita"] })
fn user_with_history(first: &str) -> Value {
    let jita = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"Jita".to_vec())) };
    let hist = Value::Dict(vec![
        (b(first), Value::List(vec![jita])),
        (b("/b"), Value::List(vec![Value::Ref(1)])),
    ]);
    let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
    Value::Dict(vec![(b("ui"), ui)])
}

#[test]
fn layout_copy_between_chars_encodes_and_matches_source() {
    let source = char_with_layout("overview");
    encode(&source).expect("source fixture encodes");
    let mut target = char_with_layout("market"); // different window id
    encode(&target).expect("target fixture encodes");

    let extracted = extract_categories(&source, &[Category::Layout]);
    apply_to_tree(&mut target, &extracted);
    let bytes = encode(&target).expect("post-copy target encodes (no dangling Ref)");

    let wl = window_layout(&decode(&bytes).unwrap());
    let ids: Vec<&str> = wl.windows.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, vec!["overview"], "target now carries the source's window");
}

#[test]
fn autofill_copy_between_users_encodes_and_matches_source() {
    let source = user_with_history("/a");
    encode(&source).expect("source fixture encodes");
    let mut target = user_with_history("/other");
    encode(&target).expect("target fixture encodes");

    let extracted = extract_categories(&source, &[Category::Autofill]);
    apply_to_tree(&mut target, &extracted);
    let bytes = encode(&target).expect("post-copy target encodes");

    let lists = project_edit_history(&decode(&bytes).unwrap());
    let widgets: Vec<&str> = lists.iter().map(|l| l.widget.as_str()).collect();
    assert!(widgets.contains(&"/a"), "target now has the source's widget list");
    assert!(!widgets.contains(&"/other"), "target's old category was replaced wholesale");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml --test batch_realshape`
Expected: FAIL to compile only if a symbol is missing; otherwise it should PASS (the API exists from Tasks 1–2). If it passes first try, that is acceptable — this task is a guard, not new production code. Confirm both tests run.

- [ ] **Step 3: Write minimal implementation**

No production code — this task only adds the guard. If a test fails, the bug is in Task 1/2 code; fix it there.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path crates/settings-model/Cargo.toml --test batch_realshape`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/tests/batch_realshape.rs
git commit -m "Batch apply: real-idiom corpus guard for category copy"
```

---

### Task 4: Target discovery filter (`batch_targets`)

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (add `Candidate` + `batch_targets` + tests; extend the `use settings_model::{...}` block with `FileKind`)

**Interfaces:**
- Consumes: `settings_model::{discover, FileKind, Profile}`.
- Produces:
  - `struct Candidate { path: String, file_name: String, id: Option<u64>, folder: String, same_folder: bool }` (Serialize)
  - `fn batch_targets(roots: &[PathBuf], source_path: &str, allow_other_folders: bool) -> Vec<Candidate>`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `app/src-tauri/src/ops.rs`:

```rust
    fn discovery_tree() -> PathBuf {
        // Two profile folders, each with char + user files.
        let root = std::env::temp_dir().join(format!("app-batchtargets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let tq = root.join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        let sisi = root.join("c_eve_sharedcache_sisi_singularity").join("settings_Default");
        fs::create_dir_all(&tq).unwrap();
        fs::create_dir_all(&sisi).unwrap();
        for p in [&tq, &sisi] {
            fs::write(p.join("core_char_90000001.dat"), b"x").unwrap();
            fs::write(p.join("core_char_90000002.dat"), b"x").unwrap();
            fs::write(p.join("core_user_987654.dat"), b"x").unwrap();
        }
        root
    }

    #[test]
    fn batch_targets_same_folder_same_type_excludes_source() {
        let root = discovery_tree();
        let src = root
            .join("c_eve_sharedcache_tq_tranquility/settings_Default/core_char_90000001.dat");
        let out = batch_targets(&[root], src.to_str().unwrap(), false);
        // Only the OTHER char in the same folder — not the user file, not sisi, not itself.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].file_name, "core_char_90000002.dat");
        assert!(out[0].same_folder);
    }

    #[test]
    fn batch_targets_allow_other_folders_adds_matching_type_elsewhere() {
        let root = discovery_tree();
        let src = root
            .join("c_eve_sharedcache_tq_tranquility/settings_Default/core_char_90000001.dat");
        let out = batch_targets(&[root], src.to_str().unwrap(), true);
        // Same-folder other char + both chars in sisi = 3.
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|c| c.file_name.starts_with("core_char_")));
        assert!(out.iter().any(|c| !c.same_folder), "cross-folder candidate present");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml batch_targets`
Expected: FAIL to compile — `Candidate` / `batch_targets` not found.

- [ ] **Step 3: Write minimal implementation**

In `app/src-tauri/src/ops.rs`, add `FileKind` to the `use settings_model::{...}` import block (line ~10–17). Then add near the other DTOs:

```rust
#[derive(Debug, Serialize)]
pub struct Candidate {
    pub path: String,
    pub file_name: String,
    pub id: Option<u64>,
    pub folder: String,
    pub same_folder: bool,
}

/// Discovered files eligible as batch targets for `source_path`: same file type,
/// and (unless `allow_other_folders`) the source's own profile folder. The source
/// itself is never a candidate.
pub fn batch_targets(roots: &[PathBuf], source_path: &str, allow_other_folders: bool) -> Vec<Candidate> {
    let profiles = discover(roots);
    let src = Path::new(source_path);
    let mut src_kind: Option<&FileKind> = None;
    let mut src_dir: Option<PathBuf> = None;
    for p in &profiles {
        for f in &p.files {
            if f.path == src {
                src_kind = Some(&f.kind);
                src_dir = Some(p.dir.clone());
            }
        }
    }
    let Some(kind) = src_kind else { return Vec::new() };
    let mut out = Vec::new();
    for p in &profiles {
        let same = Some(&p.dir) == src_dir.as_ref();
        if !same && !allow_other_folders {
            continue;
        }
        for f in &p.files {
            if &f.kind != kind || f.path == src {
                continue;
            }
            out.push(Candidate {
                path: f.path.to_string_lossy().into_owned(),
                file_name: f.file_name.clone(),
                id: f.id,
                folder: format!("{}/{}", p.server, p.profile),
                same_folder: same,
            });
        }
    }
    out
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml batch_targets`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs
git commit -m "Batch apply: target discovery filter"
```

---

### Task 5: Per-target orchestrator + Tauri commands (`batch_apply`)

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (add `BatchOp`, `TargetResult`, `batch_apply` + test; extend the `use settings_model::{...}` block with `extract_categories, apply_categories_to, full_copy_to, Category`)
- Modify: `app/src-tauri/src/lib.rs` (add two `#[tauri::command]` wrappers + register both in `generate_handler!`)

**Interfaces:**
- Consumes: `settings_model::{extract_categories, apply_categories_to, full_copy_to, Category}`; `batch_targets` (Task 4); `blue_marshal::decode`.
- Produces:
  - `enum BatchOp { FullCopy, Categories { categories: Vec<Category> } }` (Deserialize, tag `kind`, `snake_case`)
  - `struct TargetResult { path: String, ok: bool, backup_path: Option<String>, error: Option<String> }` (Serialize)
  - `fn batch_apply(source_path: &str, op: BatchOp, targets: &[String]) -> Result<Vec<TargetResult>, ErrDto>`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `app/src-tauri/src/ops.rs`:

```rust
    fn af_bytes(widget: &str, entry: &str) -> Vec<u8> {
        let hist = Value::Dict(vec![(
            Value::Bytes(widget.as_bytes().to_vec()),
            Value::List(vec![Value::Str(entry.into())]),
        )]);
        let ui = Value::Dict(vec![(
            Value::Bytes(b"editHistory".to_vec()),
            Value::Tuple(vec![Value::Long(vec![0u8; 8]), hist]),
        )]);
        encode(&Value::Dict(vec![(Value::Bytes(b"ui".to_vec()), ui)])).unwrap()
    }

    #[test]
    fn batch_apply_categories_reports_per_target_including_a_read_only_failure() {
        let dir = std::env::temp_dir().join(format!("app-batchapply-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("core_user_1.dat");
        let good = dir.join("core_user_2.dat");
        let bad = dir.join("core_user_3.dat");
        fs::write(&src, af_bytes("/from_source", "Jita")).unwrap();
        fs::write(&good, af_bytes("/old", "Amarr")).unwrap();
        fs::write(&bad, [0x7E, 0, 0, 0, 0, 0x06, 0x01]).unwrap(); // non-canonical -> ReadOnly

        let op = BatchOp::Categories { categories: vec![Category::Autofill] };
        let targets = vec![good.to_string_lossy().into_owned(), bad.to_string_lossy().into_owned()];
        let results = batch_apply(src.to_str().unwrap(), op, &targets).unwrap();

        assert_eq!(results.len(), 2);
        let g = results.iter().find(|r| r.path == targets[0]).unwrap();
        assert!(g.ok && g.backup_path.is_some());
        let bd = results.iter().find(|r| r.path == targets[1]).unwrap();
        assert!(!bd.ok && bd.error.is_some(), "read-only target failed but did not halt the batch");

        // The good target actually received the source category.
        let reread = decode(&fs::read(&good).unwrap()).unwrap();
        assert_eq!(project_edit_history(&reread)[0].widget, "/from_source");
    }

    #[test]
    fn batch_apply_full_copy_makes_targets_byte_identical() {
        let dir = std::env::temp_dir().join(format!("app-batchfull-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("core_char_1.dat");
        let dst = dir.join("core_char_2.dat");
        let src_bytes = af_bytes("/x", "y");
        fs::write(&src, &src_bytes).unwrap();
        fs::write(&dst, af_bytes("/other", "z")).unwrap();

        let results = batch_apply(src.to_str().unwrap(), BatchOp::FullCopy, &[dst.to_string_lossy().into_owned()]).unwrap();
        assert!(results[0].ok);
        assert_eq!(fs::read(&dst).unwrap(), src_bytes);
    }

    #[test]
    fn batch_apply_undecodable_source_fails_the_whole_op() {
        let dir = std::env::temp_dir().join(format!("app-batchbadsrc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("core_user_1.dat");
        fs::write(&src, [0xFF, 0xFF]).unwrap(); // undecodable
        let err = batch_apply(
            src.to_str().unwrap(),
            BatchOp::Categories { categories: vec![Category::Autofill] },
            &[],
        )
        .unwrap_err();
        assert_eq!(err.code, "decode");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml batch_apply`
Expected: FAIL to compile — `BatchOp` / `TargetResult` / `batch_apply` not found.

- [ ] **Step 3: Write minimal implementation**

In `app/src-tauri/src/ops.rs`, extend the `use settings_model::{...}` block with `extract_categories, apply_categories_to, full_copy_to, Category`. Add:

```rust
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BatchOp {
    FullCopy,
    Categories { categories: Vec<Category> },
}

#[derive(Debug, Serialize)]
pub struct TargetResult {
    pub path: String,
    pub ok: bool,
    pub backup_path: Option<String>,
    pub error: Option<String>,
}

/// Apply `op` from `source_path` to each target. The source is read (and, for a
/// category copy, decoded and extracted) exactly once. One target's failure is
/// recorded and never halts the rest. A source that cannot be read/decoded fails
/// the whole op up front, before any target is touched.
pub fn batch_apply(source_path: &str, op: BatchOp, targets: &[String]) -> Result<Vec<TargetResult>, ErrDto> {
    let source = Path::new(source_path);
    let bytes = fs::read(source).map_err(|e| ErrDto::new("io", e.to_string()))?;
    match op {
        BatchOp::FullCopy => Ok(targets
            .iter()
            .map(|t| match settings_model::full_copy_to(&bytes, Path::new(t)) {
                Ok(bk) => ok_result(t, bk.to_string_lossy().into_owned()),
                Err(e) => err_result(t, e),
            })
            .collect()),
        BatchOp::Categories { categories } => {
            let value = blue_marshal::decode(&bytes).map_err(|e| ErrDto::new("decode", e.to_string()))?;
            let extracted = extract_categories(&value, &categories);
            Ok(targets
                .iter()
                .map(|t| match apply_categories_to(Path::new(t), &extracted) {
                    Ok(r) => ok_result(t, r.backup_path.to_string_lossy().into_owned()),
                    Err(e) => err_result(t, e),
                })
                .collect())
        }
    }
}

fn ok_result(path: &str, backup: String) -> TargetResult {
    TargetResult { path: path.to_string(), ok: true, backup_path: Some(backup), error: None }
}
fn err_result(path: &str, error: String) -> TargetResult {
    TargetResult { path: path.to_string(), ok: false, backup_path: None, error: Some(error) }
}
```

Then in `app/src-tauri/src/lib.rs`, add two command wrappers (after the autofill commands, before `pub fn run`):

```rust
#[tauri::command]
fn batch_targets(source_path: String, allow_other_folders: bool) -> Vec<ops::Candidate> {
    ops::batch_targets(&settings_model::default_roots(), &source_path, allow_other_folders)
}
#[tauri::command]
fn batch_apply(
    source_path: String,
    op: ops::BatchOp,
    targets: Vec<String>,
) -> Result<Vec<ops::TargetResult>, ErrDto> {
    ops::batch_apply(&source_path, op, &targets)
}
```

Register them in `generate_handler!` — add to the macro list (after `clear_all_autofill`):

```rust
            autofill_lists, set_autofill_list, clear_all_autofill,
            batch_targets, batch_apply
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path app/src-tauri/Cargo.toml batch_apply`
Expected: PASS (3 tests).
Then confirm the whole app crate builds with the commands wired: `cargo build --manifest-path app/src-tauri/Cargo.toml`
Expected: builds clean.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs
git commit -m "Batch apply: per-target orchestrator and Tauri commands"
```

---

### Task 6: Frontend API surface + Batch view component

**Files:**
- Modify: `app/src/lib/api.ts` (add types + two invoke wrappers)
- Create: `app/src/lib/BatchView.svelte`

**Interfaces:**
- Consumes: `api.batchTargets`, `api.batchApply`, `api.discover`; `names` store; `aliasFor` from `accounts.svelte`.
- Produces: `BatchView` component with a `openPath: string | null` prop.

- [ ] **Step 1: Write the API types and wrappers**

In `app/src/lib/api.ts`, add near the other interfaces (e.g. after `RememberedList`):

```ts
export type Category = "layout" | "autofill";
export interface BatchCandidate {
  path: string;
  file_name: string;
  id: number | null;
  folder: string;
  same_folder: boolean;
}
export type BatchOp = { kind: "full_copy" } | { kind: "categories"; categories: Category[] };
export interface BatchTargetResult {
  path: string;
  ok: boolean;
  backup_path: string | null;
  error: string | null;
}
```

Add to the `api` object (after `clearAllAutofill`):

```ts
  batchTargets: (sourcePath: string, allowOtherFolders: boolean) =>
    invoke<BatchCandidate[]>("batch_targets", { sourcePath, allowOtherFolders }),
  batchApply: (sourcePath: string, op: BatchOp, targets: string[]) =>
    invoke<BatchTargetResult[]>("batch_apply", { sourcePath, op, targets }),
```

- [ ] **Step 2: Write the Batch view component**

Create `app/src/lib/BatchView.svelte`:

```svelte
<script lang="ts">
  import { api, errMessage, type Profile, type Category, type BatchCandidate, type BatchTargetResult, type BatchOp } from "./api";
  import { names } from "./names.svelte";
  import { aliasFor } from "./accounts.svelte";

  let { openPath }: { openPath: string | null } = $props();

  // All char/user files across discovery, as source options.
  let profiles = $state<Profile[]>([]);
  api.discover().then((p) => (profiles = p)).catch(() => {});
  const sources = $derived(
    profiles.flatMap((p) =>
      p.files
        .filter((f) => f.kind === "char" || f.kind === "user")
        .map((f) => ({ path: f.path, file_name: f.file_name, id: f.id, kind: f.kind })),
    ),
  );

  let sourcePath = $state<string | null>(openPath);
  const source = $derived(sources.find((s) => s.path === sourcePath) ?? null);
  const sourceKind = $derived(source?.kind ?? null);
  // Reset op + targets whenever the source changes.
  $effect(() => {
    sourcePath; // track
    fullCopy = false;
    selectedCats = new Set();
    selectedTargets = new Set();
  });

  // Categories available for the source's type.
  const availableCats = $derived<Category[]>(
    sourceKind === "char" ? ["layout"] : sourceKind === "user" ? ["autofill"] : [],
  );
  const catLabel: Record<Category, string> = { layout: "Window layout", autofill: "Autofill (remembered text)" };

  let fullCopy = $state(false);
  let selectedCats = $state<Set<Category>>(new Set());
  function toggleCat(c: Category) {
    const next = new Set(selectedCats);
    next.has(c) ? next.delete(c) : next.add(c);
    selectedCats = next;
  }

  let allowOtherFolders = $state(false);
  let candidates = $state<BatchCandidate[]>([]);
  let selectedTargets = $state<Set<string>>(new Set());
  let loadingTargets = $state(false);
  $effect(() => {
    const sp = sourcePath;
    const allow = allowOtherFolders;
    selectedTargets = new Set();
    if (!sp) { candidates = []; return; }
    loadingTargets = true;
    api.batchTargets(sp, allow)
      .then((c) => (candidates = c))
      .catch(() => (candidates = []))
      .finally(() => (loadingTargets = false));
  });
  function toggleTarget(path: string) {
    const next = new Set(selectedTargets);
    next.has(path) ? next.delete(path) : next.add(path);
    selectedTargets = next;
  }

  const nameOf = (id: number | null, kind: string, fileName: string) => {
    if (id == null) return fileName;
    return aliasFor(id) ?? names[id]?.name ?? (kind === "user" ? `account ${id}` : `char ${id}`);
  };
  const opChosen = $derived(fullCopy || selectedCats.size > 0);
  const canApply = $derived(!!sourcePath && opChosen && selectedTargets.size > 0 && !busy);

  let busy = $state(false);
  let error = $state<string | null>(null);
  let results = $state<BatchTargetResult[] | null>(null);

  async function apply() {
    if (!sourcePath) return;
    busy = true; error = null; results = null;
    const op: BatchOp = fullCopy
      ? { kind: "full_copy" }
      : { kind: "categories", categories: [...selectedCats] };
    try {
      results = await api.batchApply(sourcePath, op, [...selectedTargets]);
    } catch (e) {
      error = errMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="batch">
  <h2>Batch apply</h2>

  <section>
    <label for="src">Source file</label>
    <select id="src" bind:value={sourcePath}>
      <option value={null} disabled>Choose a file…</option>
      {#each sources as s}
        <option value={s.path}>{nameOf(s.id, s.kind, s.file_name)} — {s.kind} — {s.file_name}</option>
      {/each}
    </select>
  </section>

  {#if source}
    <section>
      <div class="head">What to copy</div>
      <label><input type="checkbox" bind:checked={fullCopy} /> Full copy (entire file — overrides categories)</label>
      {#each availableCats as c}
        <label class:disabled={fullCopy}>
          <input type="checkbox" disabled={fullCopy} checked={selectedCats.has(c)} onchange={() => toggleCat(c)} />
          {catLabel[c]}
        </label>
      {/each}
    </section>

    <section>
      <div class="head">
        Targets
        <label class="inline"><input type="checkbox" bind:checked={allowOtherFolders} /> Show other folders</label>
      </div>
      {#if loadingTargets}
        <p class="muted">Loading…</p>
      {:else if candidates.length === 0}
        <p class="muted">No other {sourceKind} files found.</p>
      {:else}
        {#each candidates as c}
          <label>
            <input type="checkbox" checked={selectedTargets.has(c.path)} onchange={() => toggleTarget(c.path)} />
            {nameOf(c.id, sourceKind ?? "", c.file_name)}
            <span class="muted">{c.file_name}{c.same_folder ? "" : ` · ${c.folder}`}</span>
          </label>
        {/each}
      {/if}
    </section>

    <section>
      {#if selectedTargets.size > 0 && opChosen}
        <p class="preview">Will overwrite {selectedTargets.size} file(s) — each is backed up first.</p>
      {/if}
      <button disabled={!canApply} onclick={apply}>{busy ? "Applying…" : "Apply"}</button>
      {#if error}<p class="err">{error}</p>{/if}
    </section>

    {#if results}
      <section class="results">
        <div class="head">Result</div>
        {#each results as r}
          <div class:ok={r.ok} class:fail={!r.ok}>
            {r.ok ? "✓" : "✗"} {r.path.split(/[\\/]/).pop()}
            {#if r.error}<span class="muted"> — {r.error}</span>{/if}
          </div>
        {/each}
      </section>
    {/if}
  {/if}
</div>

<style>
  .batch { padding: 1rem; max-width: 46rem; }
  section { margin: 0.75rem 0; }
  .head { font-weight: 600; margin-bottom: 0.25rem; display: flex; gap: 1rem; align-items: baseline; }
  label { display: block; padding: 0.15rem 0; }
  label.disabled { opacity: 0.5; }
  label.inline { display: inline; font-weight: 400; }
  /* Native controls render light in the dark WebView2 shell unless told otherwise. */
  select, input { background: #2a2a2e; color: #eee; border: 1px solid #555; }
  .muted { color: #999; }
  .preview { color: #d0a000; }
  .err, .fail { color: #e06c6c; }
  .ok { color: #6cc06c; }
  button { padding: 0.35rem 0.9rem; }
</style>
```

- [ ] **Step 3: Verify it type-checks and the existing tests still pass**

Run (PowerShell, from `app/`): `npm run check` then `npm test`
Expected: svelte-check 0 errors; existing node --test suite still green.

- [ ] **Step 4: Verify the build**

Run (PowerShell, from `app/`): `npm run build`
Expected: build succeeds.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/api.ts app/src/lib/BatchView.svelte
git commit -m "Batch apply: frontend API surface and view component"
```

---

### Task 7: Mount the Batch view (main-pane switch + sidebar button)

**Files:**
- Modify: `app/src/routes/+page.svelte` (add `"batch"` to `mainView`, import + mount `BatchView`, add `onShowBatch` handlers)
- Modify: `app/src/lib/Sidebar.svelte` (add a "Batch apply" button with an `onShowBatch` prop)

**Interfaces:**
- Consumes: `BatchView` (Task 6); the existing `mainView` state and `Sidebar` props (`onShowAccounts` is the pattern to mirror).
- Produces: user-reachable Batch view.

- [ ] **Step 1: Wire `+page.svelte`**

In `app/src/routes/+page.svelte`:

1. Add the import next to the other view imports (near line 7):

```js
  import BatchView from "$lib/BatchView.svelte";
```

2. Widen the `mainView` type (line 24):

```js
  let mainView: "file" | "accounts" | "batch" = $state("file");
```

3. Mount it next to the accounts view (near line 344, alongside the `{#if mainView === "accounts"}` block) — add a sibling block:

```svelte
  {#if mainView === "batch"}
    <BatchView openPath={current?.status === "opened" ? current.path : null} />
  {/if}
```

4. Pass an `onShowBatch` handler everywhere `onShowAccounts` is passed to `Sidebar` (lines ~338 and ~401):

```svelte
        onShowBatch={() => (mainView = "batch")}
```

- [ ] **Step 2: Wire the Sidebar button**

In `app/src/lib/Sidebar.svelte`, add `onShowBatch` to the component's `$props()` destructuring alongside `onShowAccounts`, then add a button next to the existing Accounts button:

```svelte
  <button class="nav-btn" onclick={onShowBatch}>Batch apply</button>
```

(Match the exact markup/classes of the existing Accounts button in that file. If `onShowAccounts` is typed in a props interface, add `onShowBatch: () => void` there too.)

- [ ] **Step 3: Verify it type-checks**

Run (PowerShell, from `app/`): `npm run check`
Expected: svelte-check 0 errors.

- [ ] **Step 4: Verify the build**

Run (PowerShell, from `app/`): `npm run build`
Expected: build succeeds.

- [ ] **Step 5: Commit**

```bash
git add app/src/routes/+page.svelte app/src/lib/Sidebar.svelte
git commit -m "Batch apply: mount the view and add the sidebar entry"
```

---

### Task 8: Live smoke (merge gate)

**Files:**
- Modify: `docs/format-notes.md` (append the smoke result under `## Status`)

This is a manual verification against the real EVE client — the true gate, since synthetic tests cannot prove EVE accepts the written files. Do NOT skip.

- [ ] **Step 1: Build and run the app**

Run (PowerShell, from `app/`): `npm run tauri dev`
(Or the project's usual dev command.)

- [ ] **Step 2: Category copy — window layout (char → char)**

Open the Batch view. Pick a char source with a distinctive window layout. Select "Window layout", pick one or more OTHER char targets in the same folder, Apply. Confirm each result row is ✓ and names a backup. Launch EVE, log in as a target character, and verify its windows match the source's. Confirm the target file still decodes (open it in the app — Editable, no error).

- [ ] **Step 3: Category copy — autofill (user → user)**

Pick a user source with known remembered-text entries. Select "Autofill", pick another user target, Apply. In EVE, on a character of the target account, verify the remembered-text autocomplete reflects the source's lists.

- [ ] **Step 4: Full copy (char → char)**

Pick a char source, select "Full copy", pick a char target, Apply. Verify EVE loads the target character with the source's full settings and the file is valid.

- [ ] **Step 5: Confirm no duplicate keys / valid files**

For at least one category-copied target, open it in the app after the copy: it must load Editable (proves it decodes and re-encodes identically — no duplicate keys, no corruption).

- [ ] **Step 6: Record the result and commit**

Append a dated entry to `docs/format-notes.md` under `## Status` summarizing what was copied, that EVE accepted each file, and any surprises.

```bash
git add docs/format-notes.md
git commit -m "Batch apply: record the live smoke result"
```

---

## Self-Review

**Spec coverage:**
- §1 scope (char→layout, user→autofill, overview excluded) → Task 1 `Category` (only Layout/Autofill), Tasks 4/6 gate by file type. ✓
- §2 full copy + multi-select category copy → Tasks 2, 5 (`BatchOp::Categories { Vec<Category> }`), Task 6 (checkboxes, Full-copy exclusivity). ✓
- §3 mechanism (full copy = backup+atomic_write; category = load→inline→splice→save; extract inlines source, splice inlines target) → Tasks 1, 2. ✓
- §4 commands (`batch_targets` scoping + source-excluded; `batch_apply` source-once, non-halting per-target report) → Tasks 4, 5. ✓
- §5 frontend (dedicated view, source default = open file, op checkboxes, target multi-select + Show-other-folders, preview, report) → Tasks 6, 7. ✓
- §6 safety (backup before overwrite, ReadOnly refused, preview/confirm, same-folder default + escape hatch, source excluded) → Tasks 2 (backup, ReadOnly), 4 (scoping), 5 (source-excluded via targets), 6 (preview). ✓
- §7 ceilings (inline growth; filename identity) → encoded in mechanism (Task 1/2) and the Global Constraints. ✓
- §8 testing (batch.rs units, realshape guard, command scoping + partial failure, live smoke) → Tasks 1, 2, 3, 4, 5, 8. ✓

**Placeholder scan:** No TBD/TODO; every code step shows complete code. Task 7's Sidebar button says "match the existing Accounts button's markup" because that markup is not quoted here — the implementer must open the file; this is a deliberate follow-existing-pattern instruction, not a placeholder.

**Type consistency:** `Category` (Rust `snake_case` serde) ↔ `Category = "layout" | "autofill"` (TS). `BatchOp` tag `kind` with `full_copy`/`categories` ↔ TS `{ kind: "full_copy" }` / `{ kind: "categories"; categories }`. `TargetResult { path, ok, backup_path, error }` ↔ `BatchTargetResult`. `Candidate { path, file_name, id, folder, same_folder }` ↔ `BatchCandidate`. Command arg names camelCase→snake_case (`sourcePath`→`source_path`, `allowOtherFolders`→`allow_other_folders`) per the existing `setOverviewVisible` precedent. Consistent. ✓
