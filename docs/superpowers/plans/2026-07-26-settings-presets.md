# Settings Presets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user save a character's settings as a named preset that belongs to no character, edit that preset in the normal editors, apply it to any characters, and share it as one file.

**Architecture:** A preset is a folder holding a char-side and an account-side settings document (`char.dat`, `user.dat`). Because those are ordinary settings documents, `open_file` loads them into the existing char/user slots and every editor works unmodified. Nothing stores a list of what a preset holds — it is derived by running `extract_categories` over the two documents, so a future `Category` becomes a preset aspect for free. Applying reuses the batch pipeline, whose only tie to a real character (`account_of(store, src_id)`) becomes an `Option`.

**Tech Stack:** Rust (`settings-model`, `blue-marshal`, Tauri app crate), Svelte 5 runes, `node --test` for logic tests, vitest + `@testing-library/svelte` for component tests, `cargo test` for Rust.

**Spec:** `docs/superpowers/specs/2026-07-26-settings-presets-design.md`

## Global Constraints

- **Work in the worktree `.claude/worktrees/settings-presets` on branch `settings-presets`.** Another session owns the main checkout at `C:\Users\antoi\claude\eve-settings-editor` and has it on a different branch. Never `git checkout` in the main checkout.
- **No new dependencies.** `blue-marshal` stays dependency-free. The app crate may use its existing `serde` / `serde_json`. The frontend dependency list stays as scaffolded.
- **Command names must not collide.** `preset_create`, `preset_rename` and `preset_delete` are **already taken** by the overview filter presets. Every command in this plan is prefixed `settings_preset_*`, and every `api.ts` method `settingsPreset*`.
- **Commit style:** sentence-case subject, imperative, no attribution trailers, no `Co-Authored-By`.
- **Never commit real character or account IDs.** Test fixtures use synthetic ids only.
- **Native form controls** (`select`, `option`, `input`) get explicit dark `background`/`color`: they render light in the dark WebView2 app.
- **Test commands:** `cargo test` from the repo root; `npm test` (node --test) and `npm run test:ui` (vitest) and `npm run check` from `app/`. `npm` and `gh` are **not on the Bash PATH** — run them through the PowerShell tool.
- **Aspect vocabulary is `ops::Aspect`**: `Layout | Overview | Autofill | Keybinds | Everything`. Never hardcode a subset; always route through `aspect_writes`.
- **Terminology note:** the UI word "Presets" is also used by the Overview view for EVE's own overview filter presets. This plan keeps "Presets" for the new sidebar group per the design; if the user prefers "Templates" later it is a label-only change (Task 13 is where it would land).

## File Structure

**Created:**
- `app/src-tauri/src/presets.rs` — the whole preset library: names, paths, prune, create, list, rename, delete, export, import. One responsibility: turning documents into preset folders and back.
- `crates/settings-model/tests/sparse_document.rs` — the safety net proving every projection survives a document missing its section.
- `app/src/lib/presetLibrary.svelte.ts` — frontend preset state and actions.
- `app/src/lib/PresetGroup.svelte` — the sidebar Presets group (list, create form, context menu).
- `app/src/lib/presetLibrary.test.ts` — node --test coverage of the frontend helpers.
- `app/src/lib/OverviewColumnsTab.spec.ts` — vitest coverage of the `charOpen` gate.

**Modified:**
- `crates/settings-model/src/batch.rs` — `key_path` becomes `pub`.
- `app/src-tauri/src/ops.rs` — `Aspect` gains `Serialize`; `plan_setup` source becomes `Option<u64>`; `scoped_files` splits; `setup_preview`/`setup_apply` take a `BatchSource`.
- `app/src-tauri/src/lib.rs` — `mod presets;` and the six new commands.
- `app/src/lib/api.ts` — `PresetInfo`, `BatchSource`, the new methods, changed batch signatures.
- `app/src/lib/Sidebar.svelte` — renders `PresetGroup`.
- `app/src/routes/+page.svelte` — `openPreset` state, `openPresetPair`, header/title/badges.
- `app/src/lib/OverviewColumnsTab.svelte`, `app/src/lib/OverviewView.svelte` — `charOpen` prop.
- `app/src/lib/BatchView.svelte` — Character/Preset source toggle.
- `CHANGELOG.md`, `docs/small-tasks.md`.

---

### Task 1: Preset names and paths

The only trust boundary in the feature: a typed name becomes a filesystem path.

**Files:**
- Create: `app/src-tauri/src/presets.rs`
- Modify: `app/src-tauri/src/lib.rs:1-5` (module list)

**Interfaces:**
- Consumes: nothing.
- Produces: `presets::presets_dir(&Path) -> PathBuf`, `presets::sanitize_name(&str) -> Result<String, NameError>`, `presets::preset_path(&Path, &str) -> Result<PathBuf, NameError>`, `presets::NameError(pub String)`, consts `CHAR_FILE`, `USER_FILE`, `MARKER_FILE`.

- [ ] **Step 1: Write the failing tests**

Create `app/src-tauri/src/presets.rs` containing ONLY the test module for now:

```rust
//! The preset library: a folder of presets, each holding a char-side and an
//! account-side settings document. Nothing here invents a file format — a
//! preset's two files are ordinary settings documents, which is exactly what
//! lets the editors open one as if it were a character.

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn accepts_ordinary_names() {
        assert_eq!(sanitize_name("PvP layout").unwrap(), "PvP layout");
        assert_eq!(sanitize_name("Mining — Ørca").unwrap(), "Mining — Ørca");
        assert_eq!(sanitize_name("v2.1 setup").unwrap(), "v2.1 setup");
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert!(sanitize_name("").is_err());
        assert!(sanitize_name("   ").is_err());
    }

    #[test]
    fn rejects_path_separators_and_wildcards() {
        for bad in ["a/b", "a\\b", "C:", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b"] {
            assert!(sanitize_name(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn rejects_control_characters() {
        assert!(sanitize_name("a\nb").is_err());
        assert!(sanitize_name("a\0b").is_err());
    }

    #[test]
    fn rejects_leading_or_trailing_dot_or_space() {
        // Windows strips these, so "foo." and "foo" would collide on disk.
        for bad in [".hidden", "trailing.", " lead", "trail ", ".", ".."] {
            assert!(sanitize_name(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn rejects_windows_reserved_device_names() {
        for bad in ["CON", "con", "NUL", "com1", "LPT9", "aux"] {
            assert!(sanitize_name(bad).is_err(), "{bad} must be rejected");
        }
        // Only the stem matters to Windows, so an extension does not save it.
        assert!(sanitize_name("CON.txt").is_err());
        // But a name that merely starts with those letters is fine.
        assert!(sanitize_name("Console setup").is_ok());
    }

    #[test]
    fn rejects_the_backup_directory_name() {
        assert!(sanitize_name("eve-settings-editor-backups").is_err());
    }

    #[test]
    fn rejects_over_long_names() {
        assert!(sanitize_name(&"a".repeat(101)).is_err());
        assert!(sanitize_name(&"a".repeat(100)).is_ok());
    }

    #[test]
    fn preset_path_is_a_direct_child_of_the_presets_dir() {
        let root = Path::new("/data");
        let p = preset_path(root, "PvP layout").unwrap();
        assert_eq!(p, Path::new("/data").join("presets").join("PvP layout"));
    }

    #[test]
    fn preset_path_refuses_traversal_and_absolute_names() {
        // sanitize_name already rejects these; preset_path asserts containment
        // independently, so a future sanitiser gap still cannot escape.
        assert!(preset_path(Path::new("/data"), "../escape").is_err());
        assert!(preset_path(Path::new("/data"), "/etc/passwd").is_err());
        assert!(preset_path(Path::new("/data"), "a/b").is_err());
    }
}
```

Add `mod presets;` to `app/src-tauri/src/lib.rs`, keeping the list alphabetical:

```rust
mod accounts;
mod groups;
mod names;
mod ops;
mod prefs;
mod presets;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p app presets::`
Expected: FAIL — `cannot find function 'sanitize_name' in this scope` (and the same for `preset_path`).

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` module in `app/src-tauri/src/presets.rs`:

```rust
use std::path::{Component, Path, PathBuf};

pub const CHAR_FILE: &str = "char.dat";
pub const USER_FILE: &str = "user.dat";
/// Written only for a full (Everything) preset. Its absence means "pruned",
/// which is the safe reading: fewer aspects offered, never a destructive full
/// copy built on a partial document.
pub const MARKER_FILE: &str = "preset.json";
/// Claimed inside every preset folder by the save chain's backup step.
const BACKUP_DIR: &str = "eve-settings-editor-backups";

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A rejected preset name, carrying the message shown to the user.
#[derive(Debug, PartialEq)]
pub struct NameError(pub String);

/// `<app data dir>/presets` — alongside accounts.json rather than
/// preferences.json: presets are user data, not configuration.
pub fn presets_dir(app_data: &Path) -> PathBuf {
    app_data.join("presets")
}

/// Validate a user-typed preset name. Rejects rather than silently rewriting,
/// so the name the user sees is the name on disk.
pub fn sanitize_name(raw: &str) -> Result<String, NameError> {
    let bad = |m: &str| Err(NameError(m.to_string()));
    if raw.chars().all(char::is_whitespace) {
        return bad("A preset needs a name.");
    }
    if raw.chars().count() > 100 {
        return bad("A preset name can be at most 100 characters.");
    }
    if let Some(c) = raw.chars().find(|c| "/\\:*?\"<>|".contains(*c) || c.is_control()) {
        return Err(NameError(format!("A preset name cannot contain {c:?}.")));
    }
    if raw.starts_with('.') || raw.ends_with('.') || raw.starts_with(' ') || raw.ends_with(' ') {
        return bad("A preset name cannot start or end with a dot or a space.");
    }
    let stem = raw.split('.').next().unwrap_or(raw);
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        return bad("That name is reserved by Windows. Pick another.");
    }
    if raw.eq_ignore_ascii_case(BACKUP_DIR) {
        return bad("That name is used by the editor's own backups.");
    }
    Ok(raw.to_string())
}

/// The folder a preset lives in. Two independent guards: the name rules above,
/// and a containment check that the name is exactly one ordinary path
/// component — so a gap in the first cannot escape the presets directory.
pub fn preset_path(app_data: &Path, name: &str) -> Result<PathBuf, NameError> {
    let name = sanitize_name(name)?;
    let mut comps = Path::new(&name).components();
    let single = matches!(comps.next(), Some(Component::Normal(_))) && comps.next().is_none();
    if !single {
        return Err(NameError("Invalid preset name.".into()));
    }
    Ok(presets_dir(app_data).join(name))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p app presets::`
Expected: PASS — 10 tests.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/presets.rs app/src-tauri/src/lib.rs
git commit -m "Validate preset names before they become paths"
```

---

### Task 2: Prune a document to the ticked aspects

The privacy guarantee: a Layout preset must not carry the author's autofill history.

**Files:**
- Modify: `crates/settings-model/src/batch.rs:28` (make `key_path` public)
- Modify: `app/src-tauri/src/presets.rs`
- Modify: `app/src-tauri/src/ops.rs:67-76` (derive `Serialize` on `Aspect`)

**Interfaces:**
- Consumes: `presets::preset_path`, `presets::{CHAR_FILE, USER_FILE, MARKER_FILE}` from Task 1.
- Produces: `presets::prune(&Value, &[Category]) -> Value`, `presets::create(app_data, name, aspects, CreateInput, overwrite) -> Result<PathBuf, String>`, `presets::CreateInput<'a> { char_doc: Option<&'a Value>, user_doc: Option<&'a Value> }`, `presets::has_category(&Value, Category) -> bool`.

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block in `app/src-tauri/src/presets.rs`:

```rust
    use blue_marshal::{decode, Value};
    use settings_model::Category;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
    fn ts() -> Value { Value::Long(vec![0u8; 8]) }

    fn temp_data(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("eve-presets-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A char document with both char-side categories present.
    fn char_doc() -> Value {
        let windows = Value::Dict(vec![(b("openWindows"), Value::Dict(vec![(b("market"), Value::Bool(true))]))]);
        let sizes = Value::Dict(vec![(b("NAME"), Value::Int(120))]);
        let ui = Value::Dict(vec![(b("SortHeadersSizes"), Value::Tuple(vec![ts(), sizes]))]);
        Value::Dict(vec![(b("windows"), windows), (b("ui"), ui), (b("charStore"), Value::Int(7))])
    }

    /// A user document with all three account-side categories present.
    fn user_doc() -> Value {
        let overview = Value::Dict(vec![(b("overviewColumns"), Value::List(vec![b("NAME")]))]);
        let hist = Value::Dict(vec![(b("/search"), Value::List(vec![Value::Str("Jita".into())]))]);
        let ui = Value::Dict(vec![(b("editHistory"), Value::Tuple(vec![ts(), hist]))]);
        let cmds = Value::Dict(vec![(b("CmdToggleAutopilot"), Value::List(vec![Value::Int(65)]))]);
        let cmd = Value::Dict(vec![(b("customCmds"), Value::Tuple(vec![ts(), cmds]))]);
        Value::Dict(vec![(b("overview"), overview), (b("ui"), ui), (b("cmd"), cmd)])
    }

    fn read_doc(p: &Path) -> Value {
        decode(&std::fs::read(p).expect("preset file exists")).expect("preset file decodes")
    }

    #[test]
    fn a_layout_preset_carries_the_layout_and_nothing_else() {
        let data = temp_data("layout-only");
        let dir = create(
            &data,
            "Layout only",
            &[Aspect::Layout],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();

        let c = read_doc(&dir.join(CHAR_FILE));
        assert!(has_category(&c, Category::Layout), "the layout is there");
        // The privacy guarantee, asserted rather than implied.
        assert!(!has_category(&c, Category::OverviewWidths), "no overview widths leaked");
        let u = read_doc(&dir.join(USER_FILE));
        assert!(!has_category(&u, Category::Autofill), "no autofill history leaked");
        assert!(!has_category(&u, Category::Overview), "no overview leaked");
        assert!(!has_category(&u, Category::Keybinds), "no keybinds leaked");
        assert!(!dir.join(MARKER_FILE).exists(), "a pruned preset is not marked full");
    }

    #[test]
    fn a_pruned_document_keeps_no_unrelated_root_keys() {
        let data = temp_data("no-siblings");
        let dir = create(
            &data,
            "Tidy",
            &[Aspect::Layout],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
        let Value::Dict(root) = read_doc(&dir.join(CHAR_FILE)) else { panic!("root is a dict") };
        // Exactly `windows` — not `charStore`, and no empty `ui` left behind.
        assert_eq!(root.len(), 1, "root holds one key, got {root:?}");
        assert_eq!(root[0].0, b("windows"));
    }

    #[test]
    fn an_autofill_preset_builds_its_parent_dict() {
        let data = temp_data("autofill");
        let dir = create(
            &data,
            "Autofill",
            &[Aspect::Autofill],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
        let u = read_doc(&dir.join(USER_FILE));
        assert!(has_category(&u, Category::Autofill));
        assert!(!has_category(&u, Category::Overview));
        // The char side is a valid, empty document.
        assert_eq!(read_doc(&dir.join(CHAR_FILE)), Value::Dict(vec![]));
    }

    #[test]
    fn a_keybinds_preset_builds_the_cmd_parent() {
        let data = temp_data("keybinds");
        let dir = create(
            &data,
            "Keys",
            &[Aspect::Keybinds],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
        let u = read_doc(&dir.join(USER_FILE));
        assert!(has_category(&u, Category::Keybinds), "keybinds needed a `cmd` parent");
        assert!(!has_category(&u, Category::Autofill));
    }

    #[test]
    fn an_overview_preset_spans_both_sides() {
        let data = temp_data("overview");
        let dir = create(
            &data,
            "Overview",
            &[Aspect::Overview],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
        assert!(has_category(&read_doc(&dir.join(USER_FILE)), Category::Overview));
        assert!(has_category(&read_doc(&dir.join(CHAR_FILE)), Category::OverviewWidths));
    }

    #[test]
    fn everything_copies_both_documents_whole_and_marks_them() {
        let data = temp_data("everything");
        let dir = create(
            &data,
            "Full",
            &[Aspect::Everything],
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
        assert_eq!(read_doc(&dir.join(CHAR_FILE)), char_doc());
        assert_eq!(read_doc(&dir.join(USER_FILE)), user_doc());
        assert!(dir.join(MARKER_FILE).exists(), "a full preset is marked");
    }

    #[test]
    fn a_collision_is_refused_unless_overwrite() {
        let data = temp_data("collision");
        let input = || CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) };
        create(&data, "Same", &[Aspect::Layout], input(), false).unwrap();
        let err = create(&data, "Same", &[Aspect::Layout], input(), false).unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
        create(&data, "Same", &[Aspect::Layout], input(), true).expect("overwrite is allowed");
    }

    #[test]
    fn an_aspect_whose_side_is_not_open_is_refused() {
        let data = temp_data("missing-side");
        let err = create(
            &data,
            "No account",
            &[Aspect::Overview],
            CreateInput { char_doc: Some(&char_doc()), user_doc: None },
            false,
        )
        .unwrap_err();
        assert!(err.contains("account file"), "got: {err}");
        // Nothing was written.
        assert!(!preset_path(&data, "No account").unwrap().exists());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p app presets::`
Expected: FAIL — `cannot find function 'create'`, `cannot find function 'has_category'`, `cannot find type 'CreateInput'`.

- [ ] **Step 3: Make `Category::key_path` public**

In `crates/settings-model/src/batch.rs`, change line 28-29 from:

```rust
    /// Key path from the document root to this category's subtree VALUE.
    fn key_path(self) -> &'static [&'static [u8]] {
```

to:

```rust
    /// Key path from the document root to this category's subtree VALUE.
    /// Public so a caller building a document that holds only some categories
    /// can create exactly the intermediate parent dicts they need — see the
    /// app crate's `presets::prune`.
    pub fn key_path(self) -> &'static [&'static [u8]] {
```

- [ ] **Step 4: Derive Serialize on Aspect**

In `app/src-tauri/src/ops.rs`, change the `Aspect` derive line from:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
```

to:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
```

- [ ] **Step 5: Write the implementation**

Add to `app/src-tauri/src/presets.rs`, above the test module:

```rust
use blue_marshal::{encode, Value};
use settings_model::{apply_to_tree, extract_categories, Category};

use crate::ops::{aspect_writes, Aspect};

/// The documents a preset is cut from — whichever slots are open.
pub struct CreateInput<'a> {
    pub char_doc: Option<&'a Value>,
    pub user_doc: Option<&'a Value>,
}

/// Whether a document holds anything under `cat`.
pub fn has_category(doc: &Value, cat: Category) -> bool {
    !extract_categories(doc, &[cat]).is_empty()
}

/// The intermediate parent dicts these categories need, so `apply_to_tree`'s
/// insert branch has somewhere to put `ui -> editHistory` and
/// `cmd -> customCmds`. Read out of `Category::key_path` itself, so a category
/// added later needs no change here.
fn parent_entries(cats: &[Category]) -> Vec<(Value, Value)> {
    let mut out: Vec<(Value, Value)> = Vec::new();
    for cat in cats {
        let keys = cat.key_path();
        debug_assert!(keys.len() <= 2, "prune handles at most one parent level");
        if keys.len() < 2 {
            continue;
        }
        let key = Value::Bytes(keys[0].to_vec());
        if !out.iter().any(|(k, _)| *k == key) {
            out.push((key, Value::Dict(Vec::new())));
        }
    }
    out
}

/// A document holding only `cats` from `source`, and nothing else.
///
/// Parents are built only for the categories the source ACTUALLY has, so no
/// empty `ui` or `cmd` dict can survive — which also means nothing has to be
/// removed after `apply_to_tree` has already resharded the tree.
pub fn prune(source: &Value, cats: &[Category]) -> Value {
    let extracted = extract_categories(source, cats);
    let present: Vec<Category> = extracted.iter().map(|(c, _)| *c).collect();
    let mut out = Value::Dict(parent_entries(&present));
    apply_to_tree(&mut out, &extracted);
    out
}

/// Write a new preset. The documents come from the open slots (in memory, so
/// unsaved edits are captured — the same choice `pack_export` made).
pub fn create(
    app_data: &Path,
    name: &str,
    aspects: &[Aspect],
    docs: CreateInput<'_>,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let dir = preset_path(app_data, name).map_err(|e| e.0)?;
    if dir.exists() && !overwrite {
        return Err(format!("A preset called \u{201c}{name}\u{201d} already exists."));
    }
    let w = aspect_writes(aspects);
    // Refuse an aspect whose side is not open, rather than writing an empty
    // document that claims to hold it.
    if w.writes_char() && docs.char_doc.is_none() {
        return Err("That needs a character file open.".into());
    }
    if w.writes_account() && docs.user_doc.is_none() {
        return Err("That needs the account file open \u{2014} pair the character first.".into());
    }

    let full = w.char_full_copy || w.account_full_copy;
    let side = |doc: Option<&Value>, cats: &[Category]| match (full, doc) {
        (true, Some(d)) => d.clone(),
        (false, Some(d)) => prune(d, cats),
        (_, None) => Value::Dict(Vec::new()),
    };
    // Encode BEFORE creating anything, so a failure leaves no half-written
    // preset behind.
    let char_bytes = encode(&side(docs.char_doc, &w.char_categories))
        .map_err(|e| format!("encoding the character side failed: {e}"))?;
    let user_bytes = encode(&side(docs.user_doc, &w.account_categories))
        .map_err(|e| format!("encoding the account side failed: {e}"))?;

    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("replacing the preset failed: {e}"))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("creating the preset failed: {e}"))?;
    std::fs::write(dir.join(CHAR_FILE), &char_bytes).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(USER_FILE), &user_bytes).map_err(|e| e.to_string())?;
    if full {
        std::fs::write(dir.join(MARKER_FILE), br#"{"full":true}"#).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p app presets::` then `cargo test`
Expected: PASS — 18 preset tests, and the whole workspace still green (the `key_path` visibility change is source-compatible).

- [ ] **Step 7: Commit**

```bash
git add crates/settings-model/src/batch.rs app/src-tauri/src/presets.rs app/src-tauri/src/ops.rs
git commit -m "Cut a preset down to the aspects it claims to hold"
```

---

### Task 3: Prove the editors survive a sparse document

The genuinely new risk. Every file the app has opened was written by EVE; a pruned preset is the first document that legally lacks a whole section. Do this now, before anything is built on top.

**Files:**
- Create: `crates/settings-model/tests/sparse_document.rs`

**Interfaces:**
- Consumes: `settings_model::{project_overview, project_edit_history, project_keybinds, window_layout, project_hud}`.
- Produces: nothing (a safety net).

- [ ] **Step 1: Write the tests**

Create `crates/settings-model/tests/sparse_document.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p settings-model --test sparse_document`
Expected: PASS if the projections are already defensive; FAIL (panic or wrong shape) if any is not.

- [ ] **Step 3: Fix only what actually failed**

If a projection panics or returns a wrong shape, fix it in its own module (`overview.rs`, `autofill.rs`, `keybinds.rs`, `windows.rs`, `hud.rs`) by returning the module's existing empty value on the missing-section path — do NOT invent a new error type. If nothing failed, skip this step and say so in the commit body.

The signatures above were checked against master and are: `project_overview(&Value, Option<&Value>)`, `project_edit_history(&Value)`, `project_keybinds(Option<&Value>)`, `window_layout(&Value, Option<&Value>)`, `project_hud(&Value, Option<&Value>)`.

- [ ] **Step 4: Re-run the whole suite**

Run: `cargo test`
Expected: PASS — everything green.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/tests/sparse_document.rs crates/settings-model/src/
git commit -m "Pin that every projection survives a document missing its section"
```

---

### Task 4: List, rename and delete presets

**Files:**
- Modify: `app/src-tauri/src/presets.rs`

**Interfaces:**
- Consumes: Task 1's paths, Task 2's `has_category`.
- Produces: `presets::PresetInfo { name, dir, char_path, user_path, modified_unix, aspects, full, error }`, `presets::list(&Path) -> Vec<PresetInfo>`, `presets::rename(&Path, &str, &str) -> Result<(), String>`, `presets::delete(&Path, &str) -> Result<(), String>`, `presets::is_full(&Path) -> bool`.

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block in `app/src-tauri/src/presets.rs`:

```rust
    fn make(data: &Path, name: &str, aspects: &[Aspect]) {
        create(
            data,
            name,
            aspects,
            CreateInput { char_doc: Some(&char_doc()), user_doc: Some(&user_doc()) },
            false,
        )
        .unwrap();
    }

    #[test]
    fn list_derives_each_presets_aspects() {
        let data = temp_data("list");
        make(&data, "Just layout", &[Aspect::Layout]);
        make(&data, "Layout and keys", &[Aspect::Layout, Aspect::Keybinds]);
        make(&data, "The lot", &[Aspect::Everything]);

        let all = list(&data);
        let by = |n: &str| all.iter().find(|p| p.name == n).unwrap_or_else(|| panic!("{n} listed"));

        assert_eq!(by("Just layout").aspects, vec![Aspect::Layout]);
        assert!(!by("Just layout").full);
        assert_eq!(by("Layout and keys").aspects, vec![Aspect::Layout, Aspect::Keybinds]);
        // A full preset holds every aspect AND is marked.
        let lot = by("The lot");
        assert!(lot.full);
        assert!(lot.aspects.contains(&Aspect::Everything));
        assert!(lot.aspects.contains(&Aspect::Overview));
        assert!(all.iter().all(|p| p.error.is_none()));
    }

    #[test]
    fn list_is_sorted_by_name_case_insensitively() {
        let data = temp_data("sorted");
        make(&data, "zeta", &[Aspect::Layout]);
        make(&data, "Alpha", &[Aspect::Layout]);
        let names: Vec<String> = list(&data).into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["Alpha".to_string(), "zeta".to_string()]);
    }

    #[test]
    fn a_missing_marker_reads_as_pruned() {
        let data = temp_data("marker-gone");
        make(&data, "Full", &[Aspect::Everything]);
        std::fs::remove_file(preset_path(&data, "Full").unwrap().join(MARKER_FILE)).unwrap();
        let p = list(&data).into_iter().find(|p| p.name == "Full").unwrap();
        // Safe direction: fewer aspects offered, never a destructive full copy.
        assert!(!p.full);
        assert!(!p.aspects.contains(&Aspect::Everything));
    }

    #[test]
    fn an_undecodable_preset_is_listed_with_an_error() {
        let data = temp_data("broken");
        make(&data, "Broken", &[Aspect::Layout]);
        std::fs::write(preset_path(&data, "Broken").unwrap().join(CHAR_FILE), b"not marshal").unwrap();
        let p = list(&data).into_iter().find(|p| p.name == "Broken").unwrap();
        assert!(p.error.is_some(), "a broken preset must say so, not vanish");
        assert!(p.aspects.is_empty());
    }

    #[test]
    fn list_ignores_stray_files_and_the_backup_dir() {
        let data = temp_data("strays");
        make(&data, "Real", &[Aspect::Layout]);
        std::fs::write(presets_dir(&data).join("loose.txt"), b"x").unwrap();
        std::fs::create_dir_all(presets_dir(&data).join("Not a preset")).unwrap();
        let names: Vec<String> = list(&data).into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["Real".to_string()], "only folders with both documents count");
    }

    #[test]
    fn list_of_a_missing_directory_is_empty_not_an_error() {
        let data = temp_data("never-used");
        assert!(list(&data).is_empty());
        assert!(!presets_dir(&data).exists(), "listing must not create the directory");
    }

    #[test]
    fn rename_moves_the_folder_and_refuses_a_collision() {
        let data = temp_data("rename");
        make(&data, "Old", &[Aspect::Layout]);
        make(&data, "Taken", &[Aspect::Layout]);
        rename(&data, "Old", "New").unwrap();
        assert!(preset_path(&data, "New").unwrap().exists());
        assert!(!preset_path(&data, "Old").unwrap().exists());
        assert!(rename(&data, "New", "Taken").is_err(), "collision refused");
        assert!(rename(&data, "New", "../escape").is_err(), "traversal refused");
    }

    #[test]
    fn delete_removes_the_folder_and_refuses_traversal() {
        let data = temp_data("delete");
        make(&data, "Doomed", &[Aspect::Layout]);
        delete(&data, "Doomed").unwrap();
        assert!(!preset_path(&data, "Doomed").unwrap().exists());
        assert!(delete(&data, "../escape").is_err());
        assert!(delete(&data, "Doomed").is_err(), "deleting a missing preset is an error");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p app presets::`
Expected: FAIL — `cannot find function 'list'` / `'rename'` / `'delete'`.

- [ ] **Step 3: Write the implementation**

Add to `app/src-tauri/src/presets.rs`:

```rust
use serde::Serialize;

/// One row of the library. Everything except `full` is derived by looking at
/// the two documents — there is no stored aspect list to keep in sync.
#[derive(Debug, Clone, Serialize)]
pub struct PresetInfo {
    pub name: String,
    pub dir: String,
    pub char_path: String,
    pub user_path: String,
    pub modified_unix: Option<u64>,
    pub aspects: Vec<Aspect>,
    pub full: bool,
    /// Set when a document failed to decode. Such a preset is still listed —
    /// one that silently vanishes is worse than one that says it is broken.
    pub error: Option<String>,
}

/// Whether the marker file says this preset is a complete copy. Any failure to
/// read or parse it reads as `false`: the safe direction.
pub fn is_full(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join(MARKER_FILE))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("full").and_then(|f| f.as_bool()))
        .unwrap_or(false)
}

fn modified_of(paths: [&Path; 2]) -> Option<u64> {
    paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .filter_map(|m| m.modified().ok())
        .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .max()
}

/// Every preset in the library, sorted by name. A missing presets directory is
/// an empty library, not an error — and listing never creates it.
pub fn list(app_data: &Path) -> Vec<PresetInfo> {
    let root = presets_dir(app_data);
    let Ok(entries) = std::fs::read_dir(&root) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let (char_path, user_path) = (dir.join(CHAR_FILE), dir.join(USER_FILE));
        // A folder without both documents is not a preset — this is also what
        // skips the backups directory, which holds neither.
        if !char_path.is_file() || !user_path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let full = is_full(&dir);
        let (aspects, error) = match (load(&char_path), load(&user_path)) {
            (Ok(c), Ok(u)) => (derive_aspects(&c, &u, full), None),
            (Err(e), _) | (_, Err(e)) => (Vec::new(), Some(e)),
        };
        out.push(PresetInfo {
            name,
            dir: dir.to_string_lossy().into_owned(),
            char_path: char_path.to_string_lossy().into_owned(),
            user_path: user_path.to_string_lossy().into_owned(),
            modified_unix: modified_of([&char_path, &user_path]),
            aspects,
            full,
            error,
        });
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// Decode one of a preset's documents.
pub fn load(path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    blue_marshal::decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// What a preset holds, read off the documents themselves. Adding a `Category`
/// and its `Aspect` is the only change a new aspect ever needs.
fn derive_aspects(char_doc: &Value, user_doc: &Value, full: bool) -> Vec<Aspect> {
    let mut out = Vec::new();
    if has_category(char_doc, Category::Layout) {
        out.push(Aspect::Layout);
    }
    if has_category(user_doc, Category::Overview) {
        out.push(Aspect::Overview);
    }
    if has_category(user_doc, Category::Autofill) {
        out.push(Aspect::Autofill);
    }
    if has_category(user_doc, Category::Keybinds) {
        out.push(Aspect::Keybinds);
    }
    if full {
        out.push(Aspect::Everything);
    }
    out
}

pub fn rename(app_data: &Path, old: &str, new: &str) -> Result<(), String> {
    let from = preset_path(app_data, old).map_err(|e| e.0)?;
    let to = preset_path(app_data, new).map_err(|e| e.0)?;
    if !from.is_dir() {
        return Err(format!("No preset called \u{201c}{old}\u{201d}."));
    }
    if to.exists() {
        return Err(format!("A preset called \u{201c}{new}\u{201d} already exists."));
    }
    std::fs::rename(&from, &to).map_err(|e| format!("renaming the preset failed: {e}"))
}

pub fn delete(app_data: &Path, name: &str) -> Result<(), String> {
    let dir = preset_path(app_data, name).map_err(|e| e.0)?;
    if !dir.is_dir() {
        return Err(format!("No preset called \u{201c}{name}\u{201d}."));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("deleting the preset failed: {e}"))
}
```

Note the `ponytail:` reality: `list` decodes every preset's two documents on every call. Add this comment above `list`:

```rust
// ponytail: list decodes every preset on every call. Presets are small (pruned)
// or settings-file sized (full), there will be a handful, and the list is only
// rebuilt on user action. If a large library ever drags, cache by (path, mtime).
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p app presets::`
Expected: PASS — 26 tests.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/presets.rs
git commit -m "Read the preset library off the folders themselves"
```

---

### Task 5: Export and import one file

**Files:**
- Modify: `app/src-tauri/src/presets.rs`

**Interfaces:**
- Consumes: Tasks 1, 2, 4.
- Produces: `presets::export_to(&Path, &str, &Path) -> Result<(), String>`, `presets::import_from(&Path, &Path) -> Result<String, String>` (returns the name it landed under).

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block:

```rust
    #[test]
    fn export_then_import_round_trips_byte_for_byte() {
        let data = temp_data("roundtrip");
        make(&data, "Original", &[Aspect::Layout, Aspect::Keybinds]);
        let src = preset_path(&data, "Original").unwrap();
        let bundle = data.join("shared.evepreset");
        export_to(&data, "Original", &bundle).unwrap();

        // Import into a DIFFERENT library so the name is free.
        let other = temp_data("roundtrip-target");
        let landed = import_from(&other, &bundle).unwrap();
        assert_eq!(landed, "Original");
        let dst = preset_path(&other, "Original").unwrap();
        assert_eq!(std::fs::read(src.join(CHAR_FILE)).unwrap(), std::fs::read(dst.join(CHAR_FILE)).unwrap());
        assert_eq!(std::fs::read(src.join(USER_FILE)).unwrap(), std::fs::read(dst.join(USER_FILE)).unwrap());
        let p = list(&other).into_iter().find(|p| p.name == "Original").unwrap();
        assert_eq!(p.aspects, vec![Aspect::Layout, Aspect::Keybinds]);
    }

    #[test]
    fn export_carries_the_full_marker() {
        let data = temp_data("export-full");
        make(&data, "Full", &[Aspect::Everything]);
        let bundle = data.join("full.evepreset");
        export_to(&data, "Full", &bundle).unwrap();
        let other = temp_data("export-full-target");
        import_from(&other, &bundle).unwrap();
        assert!(is_full(&preset_path(&other, "Full").unwrap()), "full survives the round trip");
    }

    #[test]
    fn import_suffixes_a_name_already_taken() {
        let data = temp_data("dupe");
        make(&data, "Same", &[Aspect::Layout]);
        let bundle = data.join("same.evepreset");
        export_to(&data, "Same", &bundle).unwrap();
        let landed = import_from(&data, &bundle).unwrap();
        assert_eq!(landed, "Same (2)");
        assert!(preset_path(&data, "Same (2)").unwrap().is_dir());
    }

    #[test]
    fn import_rejects_a_file_that_is_not_a_preset() {
        let data = temp_data("not-a-preset");
        // Valid marshal, but none of the preset keys.
        let other = blue_marshal::encode(&Value::Dict(vec![(b("hello"), Value::Int(1))])).unwrap();
        let p = data.join("other.evepreset");
        std::fs::write(&p, other).unwrap();
        let err = import_from(&data, &p).unwrap_err();
        assert!(err.contains("not a preset"), "got: {err}");

        // Not even marshal.
        let junk = data.join("junk.evepreset");
        std::fs::write(&junk, b"hello").unwrap();
        assert!(import_from(&data, &junk).is_err());
    }

    #[test]
    fn import_writes_nothing_when_an_embedded_document_is_corrupt() {
        let data = temp_data("corrupt-embed");
        let bundle_value = Value::Dict(vec![
            (b("preset"), b("Bad")),
            (b("char"), Value::Bytes(b"not marshal".to_vec())),
            (b("user"), Value::Bytes(blue_marshal::encode(&Value::Dict(vec![])).unwrap())),
            (b("full"), Value::Bool(false)),
        ]);
        let p = data.join("bad.evepreset");
        std::fs::write(&p, blue_marshal::encode(&bundle_value).unwrap()).unwrap();
        assert!(import_from(&data, &p).is_err());
        assert!(!preset_path(&data, "Bad").unwrap().exists(), "nothing written on failure");
    }

    #[test]
    fn import_rejects_an_embedded_name_that_is_not_a_legal_preset_name() {
        let data = temp_data("evil-name");
        let doc = blue_marshal::encode(&Value::Dict(vec![])).unwrap();
        let bundle_value = Value::Dict(vec![
            (b("preset"), b("../escape")),
            (b("char"), Value::Bytes(doc.clone())),
            (b("user"), Value::Bytes(doc)),
            (b("full"), Value::Bool(false)),
        ]);
        let p = data.join("evil.evepreset");
        std::fs::write(&p, blue_marshal::encode(&bundle_value).unwrap()).unwrap();
        assert!(import_from(&data, &p).is_err(), "an untrusted name goes through sanitize_name");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p app presets::`
Expected: FAIL — `cannot find function 'export_to'` / `'import_from'`.

- [ ] **Step 3: Write the implementation**

Add to `app/src-tauri/src/presets.rs`:

```rust
/// The shared form: one marshal blob wrapping both documents. It exists only at
/// the export/import boundary — the working form is always the folder, so there
/// is no pack/unpack lifecycle and nothing to lose if the app dies mid-edit.
/// The codec is the one already round-trip tested against the whole corpus, so
/// this costs no new dependency and no new serializer.
fn bytes_field<'a>(root: &'a [(Value, Value)], key: &[u8]) -> Option<&'a Vec<u8>> {
    root.iter().find_map(|(k, v)| match (k, v) {
        (Value::Bytes(kb), Value::Bytes(vb)) if kb.as_slice() == key => Some(vb),
        _ => None,
    })
}

pub fn export_to(app_data: &Path, name: &str, out: &Path) -> Result<(), String> {
    let dir = preset_path(app_data, name).map_err(|e| e.0)?;
    if !dir.is_dir() {
        return Err(format!("No preset called \u{201c}{name}\u{201d}."));
    }
    let char_bytes = std::fs::read(dir.join(CHAR_FILE)).map_err(|e| e.to_string())?;
    let user_bytes = std::fs::read(dir.join(USER_FILE)).map_err(|e| e.to_string())?;
    let bundle = Value::Dict(vec![
        (Value::Bytes(b"preset".to_vec()), Value::Bytes(name.as_bytes().to_vec())),
        (Value::Bytes(b"char".to_vec()), Value::Bytes(char_bytes)),
        (Value::Bytes(b"user".to_vec()), Value::Bytes(user_bytes)),
        (Value::Bytes(b"full".to_vec()), Value::Bool(is_full(&dir))),
    ]);
    let encoded = encode(&bundle).map_err(|e| format!("building the preset file failed: {e}"))?;
    std::fs::write(out, encoded).map_err(|e| format!("writing the preset file failed: {e}"))
}

/// Read a shared preset file into the library. Returns the name it landed
/// under, which may be suffixed if the original was taken.
pub fn import_from(app_data: &Path, file: &Path) -> Result<String, String> {
    let raw = std::fs::read(file).map_err(|e| e.to_string())?;
    let decoded = blue_marshal::decode(&raw)
        .map_err(|_| "That file is not a preset file.".to_string())?;
    let Value::Dict(root) = &decoded else {
        return Err("That file is not a preset file.".into());
    };
    let name_bytes = bytes_field(root, b"preset")
        .ok_or_else(|| "That file is not a preset file.".to_string())?;
    let char_bytes = bytes_field(root, b"char")
        .ok_or_else(|| "That preset file is missing its character side.".to_string())?;
    let user_bytes = bytes_field(root, b"user")
        .ok_or_else(|| "That preset file is missing its account side.".to_string())?;
    let full = root.iter().any(|(k, v)| {
        matches!((k, v), (Value::Bytes(kb), Value::Bool(true)) if kb.as_slice() == b"full")
    });

    // The name comes from an untrusted file, so it goes through the same gate a
    // typed one does.
    let wanted = String::from_utf8(name_bytes.clone())
        .map_err(|_| "That preset file has an unreadable name.".to_string())?;
    let base = sanitize_name(&wanted).map_err(|e| e.0)?;

    // Both documents must decode BEFORE anything is written, so a bad file
    // cannot leave an unopenable preset behind.
    blue_marshal::decode(char_bytes)
        .map_err(|e| format!("the character side of that preset is corrupt: {e}"))?;
    blue_marshal::decode(user_bytes)
        .map_err(|e| format!("the account side of that preset is corrupt: {e}"))?;

    let mut name = base.clone();
    let mut n = 2;
    while preset_path(app_data, &name).map_err(|e| e.0)?.exists() {
        name = format!("{base} ({n})");
        n += 1;
        if n > 100 {
            return Err("Too many presets with that name.".into());
        }
    }
    let dir = preset_path(app_data, &name).map_err(|e| e.0)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("creating the preset failed: {e}"))?;
    std::fs::write(dir.join(CHAR_FILE), char_bytes).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(USER_FILE), user_bytes).map_err(|e| e.to_string())?;
    if full {
        std::fs::write(dir.join(MARKER_FILE), br#"{"full":true}"#).map_err(|e| e.to_string())?;
    }
    Ok(name)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p app presets::`
Expected: PASS — 32 tests.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/presets.rs
git commit -m "Wrap a preset into one file for sharing, and unwrap it back"
```

---

### Task 6: Let the batch planner work without a source character

**Files:**
- Modify: `app/src-tauri/src/ops.rs:166-256` (`plan_setup`), `app/src-tauri/src/ops.rs:334-376` (`setup_preview` call site), and the test module at `app/src-tauri/src/ops.rs:1102+`

**Interfaces:**
- Consumes: nothing new.
- Produces: `plan_setup(char_paths, user_paths, store, resolutions, source_char: Option<u64>, target_chars, aspects) -> SetupPlan`.

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `app/src-tauri/src/ops.rs`. Find an existing `plan_setup` test first (`grep -n "fn plan_setup\|plan_setup(" app/src-tauri/src/ops.rs`) and reuse its fixture helpers rather than writing new ones; the tests below assume helpers named `paths(ids)` (char id → path map), `users(ids)`, and `store_with(pairs)` exist. If they are named differently, use the real names.

```rust
    #[test]
    fn a_preset_source_needs_no_pairing_and_excludes_nobody() {
        // Two targets on two different accounts, and NO source character.
        let char_paths = paths(&[1, 2]);
        let user_paths = users(&[10, 20]);
        let store = store_with(&[(10, &[1]), (20, &[2])]);
        let plan = plan_setup(
            &char_paths,
            &user_paths,
            &store,
            &HashMap::new(),
            None,
            &[1, 2],
            &[Aspect::Overview],
        );
        assert!(plan.source_error.is_none(), "a preset source needs no paired account");
        assert_eq!(plan.char_writes.len(), 2, "both targets get their overview widths");
        assert_eq!(plan.account_writes.len(), 2, "neither account is skipped as 'the source's'");
        assert!(plan.excluded.is_empty());
    }

    #[test]
    fn a_preset_source_still_excludes_an_unpaired_target() {
        let char_paths = paths(&[1, 2]);
        let user_paths = users(&[10]);
        let store = store_with(&[(10, &[1])]); // char 2 is unpaired
        let plan = plan_setup(
            &char_paths,
            &user_paths,
            &store,
            &HashMap::new(),
            None,
            &[1, 2],
            &[Aspect::Autofill],
        );
        assert_eq!(plan.excluded.len(), 1);
        assert_eq!(plan.excluded[0].char_id, 2);
    }

    #[test]
    fn a_preset_source_warns_on_no_resolution_mismatch() {
        // With no source character there is no source resolution, so the
        // off-screen warning is correctly silent.
        let char_paths = paths(&[1]);
        let mut res = HashMap::new();
        res.insert(1u64, (1920i64, 1080i64));
        let plan = plan_setup(
            &char_paths,
            &HashMap::new(),
            &store_with(&[]),
            &res,
            None,
            &[1],
            &[Aspect::Layout],
        );
        assert_eq!(plan.char_writes.len(), 1);
        assert!(!plan.char_writes[0].resolution_mismatch);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p app ops::`
Expected: FAIL — `expected u64, found Option<u64>` (a compile error, which is the point: every call site must be visited).

- [ ] **Step 3: Change the planner**

In `app/src-tauri/src/ops.rs`, change `plan_setup`'s signature parameter from `source_char: u64` to `source_char: Option<u64>`, then make exactly these four edits inside the body:

```rust
    let source_account = source_char.and_then(|c| account_of(store, c));
    if w.writes_account() && source_char.is_some() {
        match source_account {
            None => {
                plan.source_error = Some(
                    "The source character has no paired account — pair it in the Accounts view first."
                        .into(),
                );
                return plan;
            }
            Some(uid) if !user_paths.contains_key(&uid) => {
                plan.source_error = Some("The source character's account file was not found.".into());
                return plan;
            }
            _ => {}
        }
    }
    let src_res = source_char.and_then(|c| resolutions.get(&c).copied());
```

and in the target loop:

```rust
        if Some(t) == source_char {
            continue;
        }
```

The account-write loop's `if Some(uid) == source_account { continue; }` needs **no change**: with no source character `source_account` is `None`, so nothing is skipped, which is exactly right.

Update the one existing call site in `setup_preview` to pass `Some(src_id)`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p app`
Expected: PASS — the three new tests plus every pre-existing `plan_setup` test unchanged.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs
git commit -m "Let the batch planner run without a source character"
```

---

### Task 7: Apply a preset through the batch pipeline

**Files:**
- Modify: `app/src-tauri/src/ops.rs:260-294` (`scoped_files`), `:334-442` (`setup_preview` / `setup_apply`)

**Interfaces:**
- Consumes: Task 4's `presets::list`/`load`, Task 6's `plan_setup`.
- Produces: `ops::BatchSource` enum; `setup_preview(roots, dir, &BatchSource, targets, aspects, allow_other_folders) -> SetupPlan`; `setup_apply(...) -> Result<Vec<TargetResult>, ErrDto>`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `app/src-tauri/src/ops.rs`:

```rust
    #[test]
    fn everything_from_a_pruned_preset_is_refused() {
        // A full copy built on a three-key document would wipe the target's
        // whole file. The UI hides the option; the backend refuses it too.
        let data = std::env::temp_dir().join(format!("eve-preset-apply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        let doc = blue_marshal::Value::Dict(vec![]);
        crate::presets::create(
            &data,
            "Partial",
            &[Aspect::Layout],
            crate::presets::CreateInput { char_doc: Some(&doc), user_doc: Some(&doc) },
            false,
        )
        .unwrap();
        let dir = crate::presets::preset_path(&data, "Partial").unwrap();
        let source = BatchSource::Preset {
            dir: dir.to_string_lossy().into_owned(),
            anchor_dir: String::new(),
        };
        let plan = setup_preview(&[], &data, &source, &[], &[Aspect::Everything], false);
        assert!(
            plan.source_error.as_deref().unwrap_or("").contains("only part"),
            "got: {:?}",
            plan.source_error
        );
    }

    #[test]
    fn a_missing_preset_directory_is_a_source_error() {
        let source = BatchSource::Preset {
            dir: "/no/such/preset".into(),
            anchor_dir: String::new(),
        };
        let plan = setup_preview(&[], Path::new("/tmp"), &source, &[], &[Aspect::Layout], false);
        assert!(plan.source_error.is_some());
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p app ops::`
Expected: FAIL — `cannot find type 'BatchSource'`.

- [ ] **Step 3: Split `scoped_files` and add the source enum**

In `app/src-tauri/src/ops.rs`, replace `scoped_files` with these two functions:

```rust
/// The source character's id and the profile directory it lives in.
fn locate_source(roots: &[PathBuf], source_char_path: &str) -> Option<(u64, PathBuf)> {
    let src = Path::new(source_char_path);
    for p in discover(roots) {
        for f in &p.files {
            if f.path == src {
                return f.id.map(|id| (id, p.dir.clone()));
            }
        }
    }
    None
}

/// Discover, folder-scope to `anchor_dir` (unless `allow_other_folders`), and
/// split into char/user id->path maps. The anchor is passed in rather than
/// derived from a source path, because a preset source has no profile of its
/// own — the batch view supplies the profile the targets are chosen from.
fn scoped_files(
    roots: &[PathBuf],
    anchor_dir: Option<&Path>,
    allow_other_folders: bool,
) -> (HashMap<u64, PathBuf>, HashMap<u64, PathBuf>) {
    let mut char_paths = HashMap::new();
    let mut user_paths = HashMap::new();
    for p in discover(roots) {
        if !allow_other_folders && Some(p.dir.as_path()) != anchor_dir {
            continue;
        }
        for f in &p.files {
            let Some(id) = f.id else { continue };
            match f.kind {
                FileKind::Char => { char_paths.insert(id, f.path.clone()); }
                FileKind::User => { user_paths.insert(id, f.path.clone()); }
                FileKind::Other => {}
            }
        }
    }
    (char_paths, user_paths)
}

/// Where a batch copy's settings come from.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BatchSource {
    Character { path: String },
    /// A preset folder, plus the profile directory whose characters the target
    /// list is drawn from (a preset belongs to no profile).
    Preset { dir: String, anchor_dir: String },
}

/// The two documents a source contributes, as (char side, account side).
/// For a preset these are its own files; for a character, its file and its
/// paired account file.
struct SourceSides {
    char_path: Option<PathBuf>,
    user_path: Option<PathBuf>,
    char_id: Option<u64>,
    anchor: Option<PathBuf>,
}
```

- [ ] **Step 4: Rewrite `setup_preview` and `setup_apply` around the source**

Replace `setup_preview` and `setup_apply` in `app/src-tauri/src/ops.rs` with:

```rust
/// Resolve a source into the pieces the planner and the applier both need.
fn resolve_source(
    roots: &[PathBuf],
    dir: &Path,
    source: &BatchSource,
    aspects: &[Aspect],
) -> Result<SourceSides, String> {
    match source {
        BatchSource::Character { path } => {
            let Some((id, profile_dir)) = locate_source(roots, path) else {
                return Err("Source file not found.".into());
            };
            let store = accounts::load_store(dir);
            let user_path = account_of(&store, id).and_then(|uid| {
                let (_, users) = scoped_files(roots, Some(&profile_dir), true);
                users.get(&uid).cloned()
            });
            Ok(SourceSides {
                char_path: Some(PathBuf::from(path)),
                user_path,
                char_id: Some(id),
                anchor: Some(profile_dir),
            })
        }
        BatchSource::Preset { dir: pdir, anchor_dir } => {
            let pdir = PathBuf::from(pdir);
            let (c, u) = (pdir.join(crate::presets::CHAR_FILE), pdir.join(crate::presets::USER_FILE));
            if !c.is_file() || !u.is_file() {
                return Err("That preset could not be read.".into());
            }
            if aspects.contains(&Aspect::Everything) && !crate::presets::is_full(&pdir) {
                return Err(
                    "This preset holds only part of a character's settings, so it cannot replace a whole file. Pick the aspects it holds instead."
                        .into(),
                );
            }
            Ok(SourceSides {
                char_path: Some(c),
                user_path: Some(u),
                char_id: None,
                anchor: if anchor_dir.is_empty() { None } else { Some(PathBuf::from(anchor_dir)) },
            })
        }
    }
}

pub fn setup_preview(
    roots: &[PathBuf],
    dir: &Path,
    source: &BatchSource,
    target_char_paths: &[String],
    aspects: &[Aspect],
    allow_other_folders: bool,
) -> SetupPlan {
    let sides = match resolve_source(roots, dir, source, aspects) {
        Ok(s) => s,
        Err(e) => return SetupPlan { source_error: Some(e), ..Default::default() },
    };
    let (char_paths, user_paths) =
        scoped_files(roots, sides.anchor.as_deref(), allow_other_folders);
    let targets = target_ids(&char_paths, target_char_paths);
    let store = accounts::load_store(dir);
    let w = aspect_writes(aspects);
    let resolutions = if w.copies_char_geometry() {
        let mut ids = targets.clone();
        if let Some(id) = sides.char_id {
            ids.push(id);
        }
        gather_resolutions(&char_paths, &ids)
    } else {
        HashMap::new()
    };
    // A preset carries its own screen resolution in its char document, so the
    // off-screen warning works without any stored metadata.
    let mut resolutions = resolutions;
    if sides.char_id.is_none() && w.copies_char_geometry() {
        if let Some(p) = sides.char_path.as_deref() {
            if let Some(res) = resolution_of(p) {
                // A sentinel id no character can have, matched by plan_setup's
                // `source_char` being None -> it is only used for the warning.
                let _ = res; // see below
            }
        }
    }
    let mut plan = plan_setup(
        &char_paths,
        &user_paths,
        &store,
        &resolutions,
        sides.char_id,
        &targets,
        aspects,
    );

    // Drop no-op splice writes: a splice aspect whose categories are all absent
    // from the source would back up and rewrite every target for nothing.
    if !w.char_full_copy && !plan.char_writes.is_empty() {
        if let Some(p) = sides.char_path.as_deref() {
            if source_side_empty(p, &w.char_categories) {
                plan.char_writes.clear();
            }
        }
    }
    if !w.account_full_copy && !plan.account_writes.is_empty() {
        if let Some(p) = sides.user_path.as_deref() {
            if source_side_empty(p, &w.account_categories) {
                plan.account_writes.clear();
            }
        }
    }
    plan
}

pub fn setup_apply(
    roots: &[PathBuf],
    dir: &Path,
    source: &BatchSource,
    target_char_paths: &[String],
    aspects: &[Aspect],
    allow_other_folders: bool,
) -> Result<Vec<TargetResult>, ErrDto> {
    let plan = setup_preview(roots, dir, source, target_char_paths, aspects, allow_other_folders);
    if let Some(e) = plan.source_error {
        return Err(ErrDto::new("source", e));
    }
    let sides = resolve_source(roots, dir, source, aspects).map_err(|e| ErrDto::new("source", e))?;
    let w = aspect_writes(aspects);

    let read_side = |p: Option<&Path>| -> Result<Vec<u8>, ErrDto> {
        match p {
            Some(p) => fs::read(p).map_err(|e| ErrDto::new("io", e.to_string())),
            None => Ok(Vec::new()),
        }
    };
    let extract_side = |bytes: &[u8], cats: &[Category]| -> Result<Vec<(Category, Value)>, ErrDto> {
        if cats.is_empty() || bytes.is_empty() {
            return Ok(Vec::new());
        }
        let v = blue_marshal::decode(bytes).map_err(|e| ErrDto::new("decode", e.to_string()))?;
        Ok(extract_categories(&v, cats))
    };

    let src_char_bytes = read_side(sides.char_path.as_deref())?;
    let char_extracted = extract_side(&src_char_bytes, &w.char_categories)?;
    let user_bytes = if w.writes_account() { read_side(sides.user_path.as_deref())? } else { Vec::new() };
    let account_extracted = extract_side(&user_bytes, &w.account_categories)?;

    let mut results = Vec::new();
    for cw in &plan.char_writes {
        let r = if cw.full_copy {
            full_copy_to(&src_char_bytes, Path::new(&cw.path))
                .map(|bk| ok_result(&cw.path, bk.to_string_lossy().into_owned()))
        } else {
            apply_categories_to(Path::new(&cw.path), &char_extracted)
                .map(|rep| ok_result(&cw.path, rep.backup_path.to_string_lossy().into_owned()))
        };
        results.push(r.unwrap_or_else(|e| err_result(&cw.path, e)));
    }
    for aw in &plan.account_writes {
        let r = if aw.full_copy {
            full_copy_to(&user_bytes, Path::new(&aw.path))
                .map(|bk| ok_result(&aw.path, bk.to_string_lossy().into_owned()))
        } else {
            apply_categories_to(Path::new(&aw.path), &account_extracted)
                .map(|rep| ok_result(&aw.path, rep.backup_path.to_string_lossy().into_owned()))
        };
        results.push(r.unwrap_or_else(|e| err_result(&aw.path, e)));
    }
    Ok(results)
}
```

**Simplify the resolution block before moving on.** The block marked `// see below` in `setup_preview` is dead as written — `plan_setup` only warns when it has BOTH a source resolution and a target one, and with `source_char: None` it has neither. Delete that whole `if sides.char_id.is_none()` block and the `let mut resolutions` shadow, leaving the original `let resolutions = ...`. The spec's claim that a preset carries its own resolution is about a **later** enhancement; for this slice a preset source simply issues no resolution warning, which is honest and matches Task 6's third test. Add this comment where the block was:

```rust
    // A preset source issues no resolution warning: plan_setup needs a source
    // resolution to compare against, and there is no source character. The
    // preset's char.dat does carry `reference_w/h`, so wiring the warning up is
    // a later, additive change — see the spec's §6.
```

Then add the missing import for `Value` if it is not already in scope: `use blue_marshal::Value;`.

- [ ] **Step 5: Update the two Tauri command wrappers**

In `app/src-tauri/src/lib.rs`, change `setup_preview` and `setup_apply` to take `source: ops::BatchSource` instead of `source_char_path: String`, passing `&source` through.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p app`
Expected: PASS. Pre-existing `setup_preview`/`setup_apply` tests will need their call sites updated to `&BatchSource::Character { path: ... }` — do that, changing no assertions.

- [ ] **Step 7: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs
git commit -m "Let a batch copy take its settings from a preset"
```

---

### Task 8: Expose the preset commands

**Files:**
- Modify: `app/src-tauri/src/lib.rs`, `app/src/lib/api.ts`

**Interfaces:**
- Consumes: everything from Tasks 1-7.
- Produces: commands `settings_preset_list`, `settings_preset_create`, `settings_preset_rename`, `settings_preset_delete`, `settings_preset_export`, `settings_preset_import`; `api.settingsPresetList()` etc.; TS types `PresetInfo`, `BatchSource`.

- [ ] **Step 1: Add the ops glue**

Add to `app/src-tauri/src/ops.rs` (it owns slot access; `presets.rs` stays filesystem-only and unit-testable):

```rust
/// Create a preset from the OPEN documents, so unsaved edits are captured.
pub fn preset_save(
    state: &AppState,
    app_data: &Path,
    name: &str,
    aspects: &[Aspect],
    overwrite: bool,
) -> Result<(), ErrDto> {
    let char_guard = state.char.lock().unwrap();
    let user_guard = state.user.lock().unwrap();
    crate::presets::create(
        app_data,
        name,
        aspects,
        crate::presets::CreateInput {
            char_doc: char_guard.as_ref().map(|d| &d.value),
            user_doc: user_guard.as_ref().map(|d| &d.value),
        },
        overwrite,
    )
    .map(|_| ())
    .map_err(|m| ErrDto::new("preset", m))
}
```

- [ ] **Step 2: Add the six commands**

Add to `app/src-tauri/src/lib.rs`:

```rust
#[tauri::command]
fn settings_preset_list(app: tauri::AppHandle) -> Vec<presets::PresetInfo> {
    presets::list(&app_dir(&app))
}

#[tauri::command]
fn settings_preset_create(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    name: String,
    aspects: Vec<ops::Aspect>,
    overwrite: bool,
) -> Result<Vec<presets::PresetInfo>, ErrDto> {
    ops::preset_save(&state, &app_dir(&app), &name, &aspects, overwrite)?;
    Ok(presets::list(&app_dir(&app)))
}

#[tauri::command]
fn settings_preset_rename(
    app: tauri::AppHandle,
    old_name: String,
    new_name: String,
) -> Result<Vec<presets::PresetInfo>, ErrDto> {
    presets::rename(&app_dir(&app), &old_name, &new_name)
        .map_err(|m| ErrDto { code: "preset".into(), message: m })?;
    Ok(presets::list(&app_dir(&app)))
}

#[tauri::command]
fn settings_preset_delete(
    app: tauri::AppHandle,
    name: String,
) -> Result<Vec<presets::PresetInfo>, ErrDto> {
    presets::delete(&app_dir(&app), &name)
        .map_err(|m| ErrDto { code: "preset".into(), message: m })?;
    Ok(presets::list(&app_dir(&app)))
}

#[tauri::command]
fn settings_preset_export(app: tauri::AppHandle, name: String, path: String) -> Result<(), ErrDto> {
    presets::export_to(&app_dir(&app), &name, std::path::Path::new(&path))
        .map_err(|m| ErrDto { code: "preset".into(), message: m })
}

#[tauri::command]
fn settings_preset_import(
    app: tauri::AppHandle,
    path: String,
) -> Result<Vec<presets::PresetInfo>, ErrDto> {
    presets::import_from(&app_dir(&app), std::path::Path::new(&path))
        .map_err(|m| ErrDto { code: "preset".into(), message: m })?;
    Ok(presets::list(&app_dir(&app)))
}
```

Register all six in `generate_handler!`, on their own line after `setup_preview, setup_apply,`:

```rust
            settings_preset_list, settings_preset_create, settings_preset_rename,
            settings_preset_delete, settings_preset_export, settings_preset_import,
```

- [ ] **Step 3: Mirror it in api.ts**

In `app/src/lib/api.ts`, add after the `SetupPlan` interface:

```ts
export interface PresetInfo {
  name: string;
  dir: string;
  char_path: string;
  user_path: string;
  modified_unix: number | null;
  aspects: Aspect[];
  full: boolean;
  /** Set when a document failed to decode; the row is shown but not openable. */
  error: string | null;
}

export type BatchSource =
  | { kind: "character"; path: string }
  | { kind: "preset"; dir: string; anchor_dir: string };
```

Change the two batch methods and add the six preset methods to the `api` object:

```ts
  setupPreview: (source: BatchSource, targetCharPaths: string[], aspects: Aspect[], allowOtherFolders: boolean) =>
    invoke<SetupPlan>("setup_preview", { source, targetCharPaths, aspects, allowOtherFolders }),
  setupApply: (source: BatchSource, targetCharPaths: string[], aspects: Aspect[], allowOtherFolders: boolean) =>
    invoke<BatchTargetResult[]>("setup_apply", { source, targetCharPaths, aspects, allowOtherFolders }),
  // The overview view already owns `presetCreate`/`presetRename`/`presetDelete`
  // for EVE's own overview filter presets — these are the settings-preset
  // library, hence the longer names.
  settingsPresetList: () => invoke<PresetInfo[]>("settings_preset_list"),
  settingsPresetCreate: (name: string, aspects: Aspect[], overwrite: boolean) =>
    invoke<PresetInfo[]>("settings_preset_create", { name, aspects, overwrite }),
  settingsPresetRename: (oldName: string, newName: string) =>
    invoke<PresetInfo[]>("settings_preset_rename", { oldName, newName }),
  settingsPresetDelete: (name: string) =>
    invoke<PresetInfo[]>("settings_preset_delete", { name }),
  settingsPresetExport: (name: string, path: string) =>
    invoke<void>("settings_preset_export", { name, path }),
  settingsPresetImport: (path: string) =>
    invoke<PresetInfo[]>("settings_preset_import", { path }),
```

- [ ] **Step 4: Verify it compiles both sides**

Run: `cargo test -p app` — expected PASS.
Run (PowerShell, from `app/`): `npm run check` — expected: errors ONLY in `BatchView.svelte`, which still calls `setupPreview` with a string. That is Task 11's job; leave it failing and note it in the commit body.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/lib.rs app/src-tauri/src/ops.rs app/src/lib/api.ts
git commit -m "Expose the preset library over the command surface"
```

---

### Task 9: Stop conflating "a character is open" with "it has an id"

**Files:**
- Create: `app/src/lib/OverviewColumnsTab.spec.ts`
- Modify: `app/src/lib/OverviewColumnsTab.svelte:5-6,17,52`, `app/src/lib/OverviewView.svelte:10-11,351`, `app/src/routes/+page.svelte:477`

**Interfaces:**
- Consumes: nothing.
- Produces: `OverviewColumnsTab` takes `charOpen: boolean` in place of using `charId` as a gate. It keeps `charId` — the width write still needs it? **No**: check `setOverviewWidth`'s signature in `api.ts` — it takes `(tabIndex, column, width)` and no char id. So `charId` can be dropped from `OverviewColumnsTab` entirely.

- [ ] **Step 1: Write the failing test**

Create `app/src/lib/OverviewColumnsTab.spec.ts`:

```ts
// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// The width field was gated on the open character's *id*, using it as a proxy
// for "a character document is open". A preset holds column widths but has no
// character id, so the proxy is wrong — this pins the real condition.
import { describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/svelte";
import OverviewColumnsTab from "$lib/OverviewColumnsTab.svelte";
import type { OverviewColumns } from "$lib/api";

const data: OverviewColumns = {
  tabs: [
    {
      index: 0,
      name: "Default",
      preset: "All",
      inherits: false,
      columns: [{ name: "NAME", label: "Name", visible: true, width: 120 }],
    },
  ],
  windows: [{ index: 0, tab_indices: [0] }],
  presets: [],
  appearance: {
    background: { enabled: [], order: [] },
    flag: { enabled: [], order: [] },
    colors: [],
    bools: [],
    defaulted: false,
  },
};

const noop = () => {};

describe("the column width field", () => {
  test("is editable whenever a character document is open", () => {
    render(OverviewColumnsTab, {
      data,
      tabIndex: 0,
      charOpen: true,
      onChanged: noop,
      onUserDirty: noop,
      onCharDirty: noop,
    });
    expect(screen.getByRole("spinbutton")).not.toBeDisabled();
  });

  test("is disabled when no character document is open", () => {
    render(OverviewColumnsTab, {
      data,
      tabIndex: 0,
      charOpen: false,
      onChanged: noop,
      onUserDirty: noop,
      onCharDirty: noop,
    });
    expect(screen.getByRole("spinbutton")).toBeDisabled();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run (PowerShell, from `app/`): `npm run test:ui -- OverviewColumnsTab`
Expected: FAIL — the first case's input is disabled, because `charOpen` is not a prop the component reads.

- [ ] **Step 3: Change the three components**

`app/src/lib/OverviewColumnsTab.svelte` — replace `charId` with `charOpen` in the props block and both gates:

```svelte
  let { data, tabIndex, charOpen, onChanged, onUserDirty, onCharDirty }:
    { data: OverviewColumns | null; tabIndex: number | null; charOpen: boolean;
```

line 17: `if (!charOpen || raw.trim() === "" || Number.isNaN(width)) return;`
line 52: `disabled={!charOpen}`

`app/src/lib/OverviewView.svelte` — add `charOpen` to the props block beside `charId` (which the pair-prompt at line 226 still legitimately needs), and pass it down at line 351:

```svelte
<OverviewColumnsTab {data} {tabIndex} {charOpen} onChanged={(next) => (data = next)} {onUserDirty} {onCharDirty} />
```

`app/src/routes/+page.svelte` line 477 area — add the new prop next to `charId={openCharId}`:

```svelte
            charOpen={slots.char?.status === "opened"}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run (PowerShell, from `app/`): `npm run test:ui -- OverviewColumnsTab`
Expected: PASS — 2 tests.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/OverviewColumnsTab.svelte app/src/lib/OverviewColumnsTab.spec.ts app/src/lib/OverviewView.svelte app/src/routes/+page.svelte
git commit -m "Gate the column width on an open character, not on its id"
```

---

### Task 10: The Presets group in the sidebar

**Files:**
- Create: `app/src/lib/presetLibrary.svelte.ts`, `app/src/lib/presetLibrary.test.ts`, `app/src/lib/PresetGroup.svelte`
- Modify: `app/src/lib/Sidebar.svelte`, `app/src/routes/+page.svelte`

**Interfaces:**
- Consumes: `api.settingsPreset*` from Task 8.
- Produces: `presetLibrary.presets` (state), `loadPresets()`, `aspectLabel(a: Aspect): string`, `nextFreeName(base, taken)`; `PresetGroup.svelte` with props `{ onOpenPreset: (p: PresetInfo) => void; charOpen: boolean; userOpen: boolean }`; `+page.svelte`'s `openPresetPair(p: PresetInfo)`.

- [ ] **Step 1: Write the failing test for the pure helpers**

Create `app/src/lib/presetLibrary.test.ts`:

```ts
// Run: npm test (node --test). No framework, no @types/node — a throw is a
// failing exit code, which is all a runner needs.
import { aspectLabel, nextFreeName, summarise } from "./presetLibrary.svelte.ts";
import type { PresetInfo } from "./api.ts";

const check = (name: string, ok: boolean) => {
  if (!ok) throw new Error(`FAIL: ${name}`);
  console.log(`  ok - ${name}`);
};

const info = (name: string, aspects: PresetInfo["aspects"], full = false): PresetInfo => ({
  name,
  dir: `/data/presets/${name}`,
  char_path: `/data/presets/${name}/char.dat`,
  user_path: `/data/presets/${name}/user.dat`,
  modified_unix: 0,
  aspects,
  full,
  error: null,
});

check("layout label", aspectLabel("layout") === "Layout");
check("overview label", aspectLabel("overview") === "Overview");
check("autofill label", aspectLabel("autofill") === "Autofill");
check("keybinds label", aspectLabel("keybinds") === "Keybinds");
check("everything label", aspectLabel("everything") === "Everything");

// A full preset says so once, rather than listing every aspect it implies.
check(
  "a full preset summarises as Everything",
  summarise(info("F", ["layout", "overview", "everything"], true)) === "Everything",
);
check(
  "a pruned preset lists what it holds",
  summarise(info("P", ["layout", "keybinds"])) === "Layout · Keybinds",
);
check("a broken preset summarises as unreadable", summarise({ ...info("B", []), error: "boom" }) === "unreadable");
check("an empty preset says so", summarise(info("E", [])) === "empty");

check("a free name is unchanged", nextFreeName("PvP", new Set()) === "PvP");
check("a taken name is suffixed", nextFreeName("PvP", new Set(["PvP"])) === "PvP (2)");
check("suffixing keeps going", nextFreeName("PvP", new Set(["PvP", "PvP (2)"])) === "PvP (3)");
```

- [ ] **Step 2: Run it to verify it fails**

Run (PowerShell, from `app/`): `npm test`
Expected: FAIL — `Cannot find module './presetLibrary.svelte.ts'`.

- [ ] **Step 3: Write the library module**

Create `app/src/lib/presetLibrary.svelte.ts`:

```ts
// The preset library: the editor's own folder of saved settings, loaded on
// demand and refreshed from whatever the backend returns after each mutation.
// Nothing here touches an EVE settings file — see app/src-tauri/src/presets.rs.
import { api } from "$lib/api";
import type { Aspect, PresetInfo } from "$lib/api";

let presets = $state<PresetInfo[]>([]);

export const allPresets = (): PresetInfo[] => presets;

/** Replace the library with what the backend just returned. Every mutating
 * command answers with the fresh list, so there is one refresh path. */
export function setPresets(next: PresetInfo[]): void {
  presets = next;
}

/** A failure leaves the list alone: the library is a convenience, and the
 * editor must open without it. */
export async function loadPresets(): Promise<void> {
  presets = await api.settingsPresetList().catch(() => presets);
}

const LABELS: Record<Aspect, string> = {
  layout: "Layout",
  overview: "Overview",
  autofill: "Autofill",
  keybinds: "Keybinds",
  everything: "Everything",
};

export const aspectLabel = (a: Aspect): string => LABELS[a];

/** One line describing what a preset holds. A full preset says "Everything"
 * rather than listing the aspects it implies. */
export function summarise(p: PresetInfo): string {
  if (p.error) return "unreadable";
  if (p.full) return LABELS.everything;
  if (p.aspects.length === 0) return "empty";
  return p.aspects.map(aspectLabel).join(" · ");
}

/** `base`, or `base (2)`, `base (3)`, … — the first name not already taken. */
export function nextFreeName(base: string, taken: Set<string>): string {
  if (!taken.has(base)) return base;
  for (let n = 2; n < 1000; n += 1) {
    const candidate = `${base} (${n})`;
    if (!taken.has(candidate)) return candidate;
  }
  return `${base} (${Date.now()})`;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run (PowerShell, from `app/`): `npm test`
Expected: PASS — 12 checks.

- [ ] **Step 5: Add the open path to +page.svelte**

In `app/src/routes/+page.svelte`, add beside the other state declarations:

```svelte
  // Set while a preset (rather than a character) is open in the two slots.
  let openPreset = $state<string | null>(null);
```

Add this function next to `openFile`:

```svelte
  // Open a preset: BOTH slots at once, so the pairing machinery never runs.
  // Deliberately not routed through openFile, whose char branch would call
  // reconcileUserSlot and replace the preset's account side with a character's.
  async function openPresetPair(p: PresetInfo) {
    if (!(await confirmDiscardIfDirty())) return;
    try {
      const [charOutcome, userOutcome] = await Promise.all([
        api.open("char", p.char_path),
        api.open("user", p.user_path),
      ]);
      slots.char = charOutcome;
      slots.user = userOutcome;
      dirtySlots.char = false;
      dirtySlots.user = false;
      openPreset = p.name;
      treeFile = "char";
      savedAt += 1;
      const priorView = view;
      mainView = "file";
      selectedWindowId = null;
      reveal = null;
      try {
        layoutAvailable = (await api.windowLayout("char")).windows.length > 0;
      } catch {
        layoutAvailable = false;
      }
      if (!viewAvailable(priorView)) view = "tree";
    } catch (e) {
      await message(errMessage(e), { title: "Open failed", kind: "error" });
    }
  }
```

Add `openPreset = null;` as the first line inside `openFile`'s `try` block, so opening a character leaves preset mode.

Make the header and title name the preset. Find `openDisplay` (line 113) and make the preset name win:

```svelte
  const openDisplay = $derived.by(() => {
    if (openPreset !== null) return `${openPreset} (preset)`;
    /* …existing body unchanged… */
  });
```

Change the two unsaved badges (lines 441-442) so a preset reads as one thing:

```svelte
        {#if openPreset !== null}
          {#if dirtySlots.char || dirtySlots.user}<span class="badge dirty">preset: unsaved</span>{/if}
        {:else}
          {#if dirtySlots.char}<span class="badge dirty">character: unsaved</span>{/if}
          {#if dirtySlots.user}<span class="badge dirty">account: unsaved</span>{/if}
        {/if}
```

Import `PresetInfo` from `$lib/api` at the top of the script block.

- [ ] **Step 6: Build the sidebar group**

Create `app/src/lib/PresetGroup.svelte`:

```svelte
<script lang="ts">
  import { open as openDialog, save as saveDialog, confirm, message } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, type Aspect, type PresetInfo } from "./api";
  import { allPresets, loadPresets, setPresets, summarise } from "./presetLibrary.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  let { onOpenPreset, charOpen, userOpen }: {
    onOpenPreset: (p: PresetInfo) => void;
    charOpen: boolean;
    userOpen: boolean;
  } = $props();

  loadPresets();

  const ASPECTS: { key: Aspect; label: string; needsUser: boolean }[] = [
    { key: "layout", label: "Window layout", needsUser: false },
    { key: "overview", label: "Overview", needsUser: true },
    { key: "autofill", label: "Autofill", needsUser: true },
    { key: "keybinds", label: "Keybindings", needsUser: true },
    { key: "everything", label: "Everything", needsUser: true },
  ];

  let creating = $state(false);
  let newName = $state("");
  let picked = $state<Set<Aspect>>(new Set(["layout"]));
  let busy = $state(false);
  const everything = $derived(picked.has("everything"));

  function toggle(a: Aspect) {
    const next = new Set(picked);
    next.has(a) ? next.delete(a) : next.add(a);
    picked = next;
  }

  async function run(fn: () => Promise<PresetInfo[] | void>, title: string) {
    busy = true;
    try {
      const next = await fn();
      if (next) setPresets(next);
    } catch (e) {
      await message(errMessage(e), { title, kind: "error" });
    } finally {
      busy = false;
    }
  }

  async function create() {
    const name = newName.trim();
    if (!name || picked.size === 0) return;
    const exists = allPresets().some((p) => p.name === name);
    if (exists && !(await confirm(`Replace the existing preset “${name}”?`, { title: "Preset exists" })))
      return;
    await run(() => api.settingsPresetCreate(name, [...picked], exists), "Preset not created");
    creating = false;
    newName = "";
  }

  async function importPreset() {
    const path = await openDialog({ filters: [{ name: "Preset", extensions: ["evepreset"] }] });
    if (typeof path !== "string") return;
    await run(() => api.settingsPresetImport(path), "Import failed");
  }

  async function exportPreset(p: PresetInfo) {
    const path = await saveDialog({
      defaultPath: `${p.name}.evepreset`,
      filters: [{ name: "Preset", extensions: ["evepreset"] }],
    });
    if (typeof path !== "string") return;
    if (p.full) {
      const ok = await confirm(
        "This preset is a complete copy of both settings files. It carries everything the editor does not model, including your autofill history — station names, searches and typed text. Share it anyway?",
        { title: "Share a full preset?" },
      );
      if (!ok) return;
    }
    await run(async () => { await api.settingsPresetExport(p.name, path); }, "Export failed");
  }

  // Rename uses an inline input rather than window.prompt, matching the pattern
  // the overview-window slice introduced.
  let renaming = $state<string | null>(null);
  let renameTo = $state("");
  async function commitRename() {
    const from = renaming;
    const to = renameTo.trim();
    renaming = null;
    if (!from || !to || to === from) return;
    await run(() => api.settingsPresetRename(from, to), "Rename failed");
  }

  async function remove(p: PresetInfo) {
    const ok = await confirm(`Delete the preset “${p.name}”? This cannot be undone.`, {
      title: "Delete preset",
    });
    if (!ok) return;
    await run(() => api.settingsPresetDelete(p.name), "Delete failed");
  }

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  function openMenu(e: MouseEvent, p: PresetInfo) {
    e.preventDefault();
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "Rename…", run: () => { renaming = p.name; renameTo = p.name; } },
        { label: "Export…", run: () => void exportPreset(p) },
        { label: "Delete…", run: () => void remove(p) },
      ],
    };
  }
</script>

<details open>
  <summary>Presets</summary>
  <div class="actions">
    <button onclick={() => (creating = !creating)} disabled={!charOpen && !userOpen}
      title={charOpen || userOpen ? "Save the open character's settings as a preset" : "Open a character first"}
      >New from open character…</button>
    <button onclick={importPreset} disabled={busy}>Import…</button>
  </div>

  {#if creating}
    <form class="new" onsubmit={(e) => { e.preventDefault(); void create(); }}>
      <input placeholder="Preset name" bind:value={newName} />
      {#each ASPECTS as a}
        <label class:disabled={(everything && a.key !== "everything") || (a.needsUser && !userOpen)}>
          <input type="checkbox" checked={picked.has(a.key)}
            disabled={(everything && a.key !== "everything") || (a.needsUser && !userOpen)}
            onchange={() => toggle(a.key)} />
          {a.label}
        </label>
      {/each}
      <div class="actions">
        <button type="submit" disabled={busy || !newName.trim() || picked.size === 0}>Save</button>
        <button type="button" onclick={() => (creating = false)}>Cancel</button>
      </div>
    </form>
  {/if}

  {#if allPresets().length === 0}
    <p class="hint">No presets yet. Open a character and save one.</p>
  {:else}
    <ul>
      {#each allPresets() as p (p.dir)}
        <li>
          {#if renaming === p.name}
            <input bind:value={renameTo} onblur={commitRename}
              onkeydown={(e) => { if (e.key === "Enter") void commitRename(); if (e.key === "Escape") renaming = null; }} />
          {:else}
            <button class="file" onclick={() => onOpenPreset(p)} oncontextmenu={(e) => openMenu(e, p)}
              disabled={p.error !== null} title={p.error ?? p.dir}>
              {p.name}
              <span class="meta">{summarise(p)}</span>
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</details>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  /* Native controls render light in the dark WebView2 app unless told otherwise. */
  .new input[type="text"], .new input:not([type]) {
    background: var(--bg-input, #1b1e24);
    color: var(--fg, #e6e6e6);
    border: 1px solid var(--border, #3a3f47);
  }
  .new { display: flex; flex-direction: column; gap: 0.25rem; padding: 0.35rem 0.1rem; }
  .new label { display: flex; align-items: center; gap: 0.4em; font-size: 0.9em; }
  .new label.disabled { opacity: 0.5; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; padding: 0.25rem 0; }
  .hint { opacity: 0.7; font-size: 0.85em; padding: 0.25rem 0.1rem; }
  .meta { color: var(--fg-dim); font-size: 0.85em; margin-left: 0.4em; }
</style>
```

- [ ] **Step 7: Mount it in the sidebar**

In `app/src/lib/Sidebar.svelte`: import `PresetGroup`, add `onOpenPreset`, `charOpen`, `userOpen` to the props block, and render it directly after the `{#if profiles.length === 0}` hint and before the `{#each rows …}` loop:

```svelte
  <PresetGroup {onOpenPreset} {charOpen} {userOpen} />
```

In `app/src/routes/+page.svelte`, pass them to `<Sidebar>` (around line 415):

```svelte
      onOpenPreset={openPresetPair}
      charOpen={slots.char?.status === "opened"}
      userOpen={slots.user?.status === "opened"}
```

- [ ] **Step 8: Verify**

Run (PowerShell, from `app/`): `npm test` then `npm run check`
Expected: `npm test` PASS. `npm run check` — errors ONLY in `BatchView.svelte` (Task 11).

- [ ] **Step 9: Commit**

```bash
git add app/src/lib/presetLibrary.svelte.ts app/src/lib/presetLibrary.test.ts app/src/lib/PresetGroup.svelte app/src/lib/Sidebar.svelte app/src/routes/+page.svelte
git commit -m "List, create and open presets from the sidebar"
```

---

### Task 11: Choose a preset as the batch source

**Files:**
- Modify: `app/src/lib/BatchView.svelte`

**Interfaces:**
- Consumes: `api.setupPreview`/`setupApply` taking `BatchSource` (Task 8), `presetLibrary` (Task 10).
- Produces: nothing downstream.

- [ ] **Step 1: Add the source kind**

In `app/src/lib/BatchView.svelte`, add to the script block:

```ts
  import { allPresets, loadPresets, summarise } from "./presetLibrary.svelte";
  import type { BatchSource, PresetInfo } from "./api";

  loadPresets();

  let sourceKind = $state<"character" | "preset">("character");
  let presetDir = $state<string | null>(null);
  const preset = $derived(allPresets().find((p) => p.dir === presetDir) ?? null);

  // What the chosen source can offer. A preset offers only what it holds, so
  // Autofill cannot be ticked on a preset that has none.
  const offered = $derived<Aspect[]>(
    sourceKind === "preset"
      ? (preset?.aspects ?? [])
      : ["layout", "overview", "autofill", "keybinds", "everything"],
  );

  const batchSource = $derived<BatchSource | null>(
    sourceKind === "character"
      ? (sourcePath ? { kind: "character", path: sourcePath } : null)
      : (presetDir ? { kind: "preset", dir: presetDir, anchor_dir: folder ?? "" } : null),
  );
```

Change the reset `$effect` so switching source kind clears the selection too:

```ts
  $effect(() => {
    sourcePath;
    presetDir;
    sourceKind;
    selected = new Set();
    selectedTargets = new Set();
  });
```

Change the preview `$effect` and `apply()` to use `batchSource`:

```ts
  $effect(() => {
    const src = batchSource;
    const asp = [...selected];
    const tgts = effectiveTargets;
    const allow = allowOtherFolders;
    if (!src || asp.length === 0 || tgts.length === 0) { plan = null; return; }
    const seq = ++previewSeq;
    api.setupPreview(src, tgts, asp as Aspect[], allow)
      .then((p) => { if (seq === previewSeq) plan = p; })
      .catch(() => { if (seq === previewSeq) plan = null; });
  });
```

```ts
  async function apply() {
    const src = batchSource;
    if (!src) return;
    busy = true; error = null; results = null;
    try {
      results = await api.setupApply(src, effectiveTargets, [...selected] as Aspect[], allowOtherFolders);
    } catch (e) {
      error = errMessage(e);
    } finally {
      busy = false;
    }
  }
```

Change `canApply`'s first clause from `!!sourcePath` to `!!batchSource`, and the `{#if source}` guard in the markup to `{#if batchSource}`.

- [ ] **Step 2: Add the picker markup**

Replace the "Source character" `<label>`/`<select>` pair (around lines 196-202) with:

```svelte
    <div class="head">Source</div>
    <label class="inline">
      <input type="radio" bind:group={sourceKind} value="character" /> A character
    </label>
    <label class="inline">
      <input type="radio" bind:group={sourceKind} value="preset" /> A preset
    </label>

    {#if sourceKind === "character"}
      <label for="src">Source character</label>
      <select id="src" bind:value={sourcePath}>
        <option value={null} disabled>Choose a character…</option>
        {#each sourceOptions as c}
          <option value={c.path}>{nameOfChar(c.id, c.file_name)} — {c.file_name}</option>
        {/each}
      </select>
    {:else}
      <label for="srcpreset">Source preset</label>
      <select id="srcpreset" bind:value={presetDir}>
        <option value={null} disabled>Choose a preset…</option>
        {#each allPresets().filter((p) => p.error === null) as p}
          <option value={p.dir}>{p.name} — {summarise(p)}</option>
        {/each}
      </select>
      {#if allPresets().length === 0}
        <p class="muted">No presets yet — save one from the sidebar first.</p>
      {/if}
    {/if}
```

Restrict the aspect checkboxes to what the source offers, by changing the `{#each ASPECTS as a}` loop's opening line to:

```svelte
      {#each ASPECTS.filter((a) => offered.includes(a.key)) as a}
```

Give the two selects and the radios explicit dark styling in the component's `<style>` block if they do not already inherit it:

```css
  select, option { background: var(--bg-input, #1b1e24); color: var(--fg, #e6e6e6); }
```

- [ ] **Step 3: Verify**

Run (PowerShell, from `app/`): `npm run check` — expected: **no errors**.
Run: `npm run test:ui` — expected PASS; if `BatchView.spec.ts` calls `setupPreview` with a string, update its stub to the `BatchSource` shape, changing no assertions.

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/BatchView.svelte app/src/lib/BatchView.spec.ts
git commit -m "Let a batch copy start from a preset instead of a character"
```

---

### Task 12: Full-suite verification

**Files:** none changed unless something fails.

- [ ] **Step 1: Rust**

Run: `cargo test`
Expected: PASS, every crate.

- [ ] **Step 2: Rust lints**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: PASS. Fix anything it flags in the new code only.

- [ ] **Step 3: Frontend**

Run (PowerShell, from `app/`): `npm test`
Expected: PASS.

Run (PowerShell, from `app/`): `npm run check`
Expected: 0 errors, 0 warnings.

Run (PowerShell, from `app/`): `npm run build`
Expected: PASS.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "Fix what the full suite turned up"
```

If nothing failed, skip the commit and say so.

---

### Task 13: Changelog and ledger

**Files:**
- Modify: `CHANGELOG.md`, `docs/small-tasks.md`

- [ ] **Step 1: Write the changelog entry**

Under `## [Unreleased]` in `CHANGELOG.md`:

```markdown
## [Unreleased]

### Added
- **Settings presets.** Save a character's settings as a named preset that
  belongs to no character, then apply it to any characters later — even after
  the character you saved it from has moved on. Pick what a preset holds when
  you save it (window layout, overview, autofill, keybindings, or a complete
  copy), and it holds only that: a layout preset you share carries no trace of
  your autofill history.
- **Presets are editable.** A preset opens in the sidebar like a character and
  every editor works on it — Layout, Overview, Autofill, Keybinds and the raw
  tree — with the same save chain and the same backups. So a preset is
  something you build and refine, not just a snapshot you replay.
- **Share a preset as one file.** Export writes a single `.evepreset` file;
  Import reads one back. Exporting a complete-copy preset warns first, because
  it carries everything in both settings files.
- **Batch apply can start from a preset.** The source picker now offers
  Character or Preset; everything downstream is unchanged, including the
  collateral-character warning and the per-target backups.

### Fixed
- **The overview column width field is editable whenever a character file is
  open**, rather than only when that file has a character id in its name.
```

- [ ] **Step 2: Add the follow-ups to the ledger**

Add to the **Open** list in `docs/small-tasks.md`:

```markdown
- [ ] **Run the settings-presets live in-game smoke.** From the spec's §12:
  (1) create a Layout-only preset from a real character, apply it to a
  *different* character, launch EVE, confirm the windows land where the preset
  had them and the target's overview and autofill are untouched; (2) open that
  preset, move a window, save, re-apply, and confirm the edit landed; (3) create
  an `Everything` preset from a configured client and apply it to a character
  whose files EVE has only just created (the fresh-install case); (4) open a
  preset holding only Autofill and confirm Overview and Layout show honest empty
  states, then add an overview preset to it from scratch and confirm EVE accepts
  the result (the slice-2b minting path from nothing); (5) export, re-import
  under a new name, confirm the two behave identically; (6) confirm the
  per-column width field is editable with a preset open. _Added 2026-07-26._

- [ ] **A preset source issues no resolution-mismatch warning.** `plan_setup`
  needs a source resolution to compare a target against, and a preset source has
  no source character. A preset's `char.dat` does carry `reference_w/h` (it is a
  char document, and `gather_resolutions` already reads it), so wiring the
  warning up is additive: give `plan_setup` an explicit source resolution rather
  than looking one up by id. Worth doing the first time someone applies a layout
  preset across two different monitors. _Added 2026-07-26._

- [ ] **"Presets" now means two things in the UI.** The sidebar's Presets group
  (saved settings bundles) and the Overview view's presets (EVE's own overview
  filter presets, which is CCP's term and cannot be renamed) share a word.
  Context disambiguates today; if it confuses anyone, rename the sidebar group
  to "Templates" — a label-only change, since every command and type is already
  prefixed `settings_preset_*`. _Added 2026-07-26._
```

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md docs/small-tasks.md
git commit -m "Record the settings presets slice and its follow-ups"
```

---

## Self-Review

**Spec coverage**

| Spec section | Task |
|---|---|
| §2 model — preset is a document pair | 2, 10 |
| §2 — no stored aspect list, derived | 4 (`derive_aspects`) |
| §3 storage layout, app_data_dir | 1, 2 |
| §3.1 the `full` bit and its safe failure | 2, 4 |
| §3.2 names as a trust boundary | 1 |
| §4 creating, in-memory docs, refuse missing side | 2, 8 |
| §5 listing, opening, `openPreset` | 4, 10 |
| §5.1 the `charId`/`charOpen` conflation | 9 |
| §6 applying: `Option<u64>`, anchor dir, source enum | 6, 7 |
| §6 `Everything` only from a full preset | 7 |
| §7 sharing: export/import, full-preset warning | 5, 10 |
| §8 frontend surfaces | 10, 11 |
| §9 testing incl. sparse documents | 2, 3, 4, 5, 6, 9, 10 |
| §12 live smoke | 13 (ledgered) |

**Known deviation from the spec, deliberate and recorded:** §6 claims the resolution-mismatch warning works for a preset "for free". It does not — `plan_setup` looks the source resolution up by character id, and a preset has none. Task 7 Step 4 removes the dead code that pretended otherwise, and Task 13 ledgers the additive fix. Everything else in §6 holds.

**Placeholder scan:** no TBD/TODO; every code step carries the code. Task 3 Step 3 and Task 12 Step 4 are explicitly conditional ("fix only what failed") rather than vague — each names the files and the shape of the fix.

**Type consistency:** `PresetInfo` fields match between `presets.rs` (Task 4), `api.ts` (Task 8) and `presetLibrary.svelte.ts` (Task 10) — `name`, `dir`, `char_path`, `user_path`, `modified_unix`, `aspects`, `full`, `error`. `BatchSource` matches between `ops.rs` (`#[serde(tag = "kind", rename_all = "snake_case")]`, Task 7) and the TS union (Task 8). `CreateInput` is used identically in Tasks 2, 7 and 8. `aspect_writes` is the single routing table throughout.

**One assumption the implementer must check:** Task 6's tests assume `plan_setup` fixture helpers named `paths`, `users` and `store_with` exist in `ops.rs`'s test module. Grep first and use the real names.
