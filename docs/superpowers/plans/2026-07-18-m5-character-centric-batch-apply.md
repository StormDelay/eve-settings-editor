# M5 — Character-centric batch apply — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reframe batch apply around the character — pick a source character and target characters, copy Window layout / Overview / Autofill / Everything — routing each aspect to the char file and/or the account user file, de-duplicating account writes, and naming the collateral characters an account write also changes.

**Architecture:** The M4 engine already splices a subtree at a fixed key path into a file with the full backup+verify chain. M5 adds two subtree categories (`Overview`, `OverviewWidths`) and a thin app-layer orchestration (`ops.rs`): a pure `plan_setup` that resolves characters to files via the M3b `accounts.json` pairing, dedupes account writes, computes collateral, and excludes unpaired targets; plus `setup_preview` / `setup_apply` commands over it. The frontend `BatchView.svelte` is reworked character-centric. M4's file-centric commands are deleted last.

**Tech Stack:** Rust workspace (blue-marshal codec, settings-model, Tauri app crate), SvelteKit + Svelte-5 runes frontend.

## Global Constraints

- **Commits:** sentence-case subject, **NO attribution trailers** (repo convention).
- **Dependency-free spirit:** no new third-party crates; `blue-marshal` is already a normal dep of the app crate.
- **Frontend tests:** `node --test`, zero-dependency (`npm test` in `app/`); the frontend gate is `npm test` + `npm run check` (svelte-check, 0 errors) + `npm run build`.
- **Shell:** `cargo` runs from either shell; **`npm` and `gh` are NOT on the Bash tool's PATH — use the PowerShell tool for `npm`.**
- **Save-path invariant chain is reused, never reimplemented:** every category splice goes through `save()` (encode → verify → backup → atomic write → ReadOnly refusal); every full copy through `full_copy_to` (backup → atomic write).
- **Inline-first idiom:** category extract inlines the whole source; category splice inlines the whole target (`inline_all`) — already handled inside the batch.rs primitives.
- **Dark native controls:** any new `<select>/<option>/<input>` gets explicit dark `background`/`color` (see the dark-native-controls memo); checkboxes use `accent-color`.
- **Aspect → file routing (the whole model, one place):**
  - Window layout → char `windows` (`Category::Layout`).
  - Overview → char `ui → SortHeadersSizes` (`Category::OverviewWidths`) **and** account `overview` (`Category::Overview`).
  - Autofill → account `ui → editHistory` (`Category::Autofill`).
  - Everything → full byte-copy of the char file **and** full byte-copy of the account user file. Exclusive.

---

### Task 1: Two new batch categories — `Overview` and `OverviewWidths`

**Files:**
- Modify: `crates/settings-model/src/batch.rs` (the `Category` enum ~L17-32; add unit tests in the `#[cfg(test)]` module)
- Test: `crates/settings-model/tests/batch_realshape.rs`

**Interfaces:**
- Consumes: existing `extract_categories(&Value, &[Category]) -> Vec<(Category, Value)>`, `apply_to_tree(&mut Value, &[(Category, Value)])`, `inline_all` (unchanged).
- Produces: `Category::Overview` (key path `root → overview`, user file) and `Category::OverviewWidths` (key path `root → ui → SortHeadersSizes`, char file). Both plug into the existing extract/splice machinery with no other change.

- [ ] **Step 1: Write the failing unit tests** (append inside the `mod tests` block in `batch.rs`, after the existing tests, before the closing `}`):

```rust
    /// user root -> overview -> { overviewColumns: ["NAME"], tabsByWindowInstanceID: [[0]] }
    fn user_overview(col: &str) -> Value {
        let overview = Value::Dict(vec![
            (b("overviewColumns"), Value::List(vec![b(col)])),
            (b("tabsByWindowInstanceID"), Value::List(vec![Value::List(vec![Value::Int(0)])])),
        ]);
        Value::Dict(vec![(b("overview"), overview), (b("keep"), Value::Int(7))])
    }

    /// char root -> ui -> SortHeadersSizes -> (ts, { (overviewScroll2, 0): { NAME: w } })
    fn char_widths(w: i64) -> Value {
        let cols = Value::Dict(vec![(b("NAME"), Value::Int(w))]);
        let sizes = Value::Dict(vec![(
            Value::Tuple(vec![b("overviewScroll2"), Value::Int(0)]),
            cols,
        )]);
        let ui = Value::Dict(vec![(b("SortHeadersSizes"), Value::Tuple(vec![ts(), sizes]))]);
        Value::Dict(vec![(b("ui"), ui), (b("other"), Value::Int(9))])
    }

    #[test]
    fn overview_category_replaces_the_overview_subtree_and_keeps_siblings() {
        let extracted = extract_categories(&user_overview("SOURCECOL"), &[Category::Overview]);
        assert_eq!(extracted.len(), 1);
        let mut target = user_overview("TARGETCOL");
        apply_to_tree(&mut target, &extracted);

        // The overview subtree is now the source's: overviewColumns == ["SOURCECOL"].
        let Value::Dict(root) = &target else { panic!() };
        let (_, ov) = root.iter().find(|(k, _)| is_bytes(k, b"overview")).unwrap();
        let Value::Dict(ov) = ov else { panic!() };
        let (_, cols) = ov.iter().find(|(k, _)| is_bytes(k, b"overviewColumns")).unwrap();
        assert_eq!(cols, &Value::List(vec![b("SOURCECOL")]), "overview came from the source");
        assert!(root.iter().any(|(k, v)| is_bytes(k, b"keep") && matches!(v, Value::Int(7))),
            "unrelated sibling survived");
    }

    #[test]
    fn overview_widths_category_replaces_sortheaderssizes_and_keeps_siblings() {
        let extracted = extract_categories(&char_widths(120), &[Category::OverviewWidths]);
        assert_eq!(extracted.len(), 1);
        let mut target = char_widths(999);
        apply_to_tree(&mut target, &extracted);

        // The width came from the source: NAME == 120, not the target's 999.
        let Value::Dict(root) = &target else { panic!() };
        let (_, ui) = root.iter().find(|(k, _)| is_bytes(k, b"ui")).unwrap();
        let Value::Dict(ui) = ui else { panic!() };
        let (_, shs) = ui.iter().find(|(k, _)| is_bytes(k, b"SortHeadersSizes")).unwrap();
        let Value::Tuple(items) = shs else { panic!() };
        let Value::Dict(sizes) = &items[1] else { panic!() };
        let Value::Dict(cols) = &sizes[0].1 else { panic!() };
        assert_eq!(cols.iter().find(|(k, _)| is_bytes(k, b"NAME")).unwrap().1, Value::Int(120));
        assert!(root.iter().any(|(k, v)| is_bytes(k, b"other") && matches!(v, Value::Int(9))),
            "sibling under root survived");
    }
```

- [ ] **Step 2: Run the tests, verify they fail**

Run: `cargo test -p settings-model overview_category_replaces overview_widths_category_replaces`
Expected: FAIL — `no variant named Overview` / `OverviewWidths` on `Category` (compile error).

- [ ] **Step 3: Add the two variants and their key paths**

In `crates/settings-model/src/batch.rs`, extend the enum and `key_path`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Layout,
    Autofill,
    Overview,
    OverviewWidths,
}

impl Category {
    /// Key path from the document root to this category's subtree VALUE.
    fn key_path(self) -> &'static [&'static [u8]] {
        match self {
            Category::Layout => &[b"windows"],
            Category::Autofill => &[b"ui", b"editHistory"],
            Category::Overview => &[b"overview"],
            Category::OverviewWidths => &[b"ui", b"SortHeadersSizes"],
        }
    }
}
```

- [ ] **Step 4: Run the unit tests, verify they pass**

Run: `cargo test -p settings-model overview_category_replaces overview_widths_category_replaces`
Expected: PASS (2 tests).

- [ ] **Step 5: Add the realshape guards** (append to `crates/settings-model/tests/batch_realshape.rs`):

```rust
/// user root -> { columnDefs: [Shared "NAME"], overview: { overviewColumns: [Ref],
/// tabsByWindowInstanceID: [[0]] } }
///
/// A column token shared between a sibling of `overview` (its Shared def) and a
/// list INSIDE `overview` (a bare Ref). Extracting `overview` without inlining
/// the whole source first clones a subtree with a dangling Ref that fails to encode.
fn user_with_overview() -> Value {
    let name = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"NAME".to_vec())) };
    let overview = Value::Dict(vec![
        (b("overviewColumns"), Value::List(vec![Value::Ref(1)])),
        (b("tabsByWindowInstanceID"), Value::List(vec![Value::List(vec![Value::Int(0)])])),
    ]);
    Value::Dict(vec![
        (b("columnDefs"), Value::List(vec![name])), // Shared def precedes `overview`
        (b("overview"), overview),
    ])
}

/// char root -> { widthDefs: [Shared "NAME"], ui: { SortHeadersSizes: (ts, {
/// (overviewScroll2, 0): { Ref: 120 } }) } }
///
/// A column token shared between `widthDefs` (Shared def) and a width-dict key
/// inside SortHeadersSizes (bare Ref). Same inline-first requirement.
fn char_with_widths() -> Value {
    let name = Value::Shared { slot: 1, value: Box::new(Value::Bytes(b"NAME".to_vec())) };
    let cols = Value::Dict(vec![(Value::Ref(1), Value::Int(120))]);
    let sizes = Value::Dict(vec![(
        Value::Tuple(vec![b("overviewScroll2"), Value::Int(0)]),
        cols,
    )]);
    Value::Dict(vec![
        (b("widthDefs"), Value::List(vec![name])), // Shared def precedes `ui`
        (b("ui"), Value::Dict(vec![(b("SortHeadersSizes"), Value::Tuple(vec![ts(), sizes]))])),
    ])
}

#[test]
fn overview_copy_between_users_encodes_across_the_shared_boundary() {
    let source = user_with_overview();
    encode(&source).expect("source fixture encodes (def precedes ref)");
    let mut target = user_with_overview();
    let extracted = extract_categories(&source, &[Category::Overview]);
    apply_to_tree(&mut target, &extracted);
    encode(&target).expect("post-copy overview encodes (cross-boundary Ref inlined)");
}

#[test]
fn overview_widths_copy_between_chars_encodes_across_the_shared_boundary() {
    let source = char_with_widths();
    encode(&source).expect("source fixture encodes (def precedes ref)");
    let mut target = char_with_widths();
    let extracted = extract_categories(&source, &[Category::OverviewWidths]);
    apply_to_tree(&mut target, &extracted);
    encode(&target).expect("post-copy widths encode (cross-boundary Ref inlined)");
}
```

- [ ] **Step 6: Run the realshape tests, verify they pass**

Run: `cargo test -p settings-model --test batch_realshape`
Expected: PASS (4 tests total).

- [ ] **Step 7: Commit**

```bash
git add crates/settings-model/src/batch.rs crates/settings-model/tests/batch_realshape.rs
git commit -m "Add Overview and OverviewWidths batch categories"
```

---

### Task 2: Aspect model + pure aspect-routing (`aspect_writes`)

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (add after the existing `BatchOp` block ~L114; add tests in the `#[cfg(test)]` module at the bottom)

**Interfaces:**
- Consumes: `settings_model::Category`.
- Produces:
  - `pub enum Aspect { Layout, Overview, Autofill, Everything }` (`#[serde(rename_all = "snake_case")]`, `Deserialize`, `Clone, Copy, PartialEq, Eq, Debug`).
  - `pub struct AspectWrites { char_categories: Vec<Category>, account_categories: Vec<Category>, char_full_copy: bool, account_full_copy: bool }` with methods `writes_account() -> bool`, `writes_char() -> bool`, `copies_char_geometry() -> bool`.
  - `pub fn aspect_writes(aspects: &[Aspect]) -> AspectWrites`.

- [ ] **Step 1: Write the failing tests** (append inside `ops.rs`'s `mod tests`):

```rust
    #[test]
    fn everything_is_full_copy_of_both_files() {
        let w = aspect_writes(&[Aspect::Everything]);
        assert!(w.char_full_copy && w.account_full_copy);
        assert!(w.char_categories.is_empty() && w.account_categories.is_empty());
        assert!(w.writes_account() && w.writes_char() && w.copies_char_geometry());
    }

    #[test]
    fn everything_wins_even_when_mixed_with_others() {
        let w = aspect_writes(&[Aspect::Layout, Aspect::Everything]);
        assert!(w.char_full_copy && w.account_full_copy);
    }

    #[test]
    fn overview_writes_widths_to_char_and_overview_to_account() {
        let w = aspect_writes(&[Aspect::Overview]);
        assert_eq!(w.char_categories, vec![Category::OverviewWidths]);
        assert_eq!(w.account_categories, vec![Category::Overview]);
        assert!(w.writes_account() && w.writes_char());
        assert!(!w.copies_char_geometry(), "overview does not copy window geometry");
    }

    #[test]
    fn layout_is_char_only_no_account_write() {
        let w = aspect_writes(&[Aspect::Layout]);
        assert_eq!(w.char_categories, vec![Category::Layout]);
        assert!(w.account_categories.is_empty());
        assert!(!w.writes_account());
        assert!(w.copies_char_geometry());
    }

    #[test]
    fn autofill_is_account_only() {
        let w = aspect_writes(&[Aspect::Autofill]);
        assert!(w.char_categories.is_empty());
        assert_eq!(w.account_categories, vec![Category::Autofill]);
        assert!(w.writes_account() && !w.writes_char());
    }
```

- [ ] **Step 2: Run, verify fail**

Run: `cd app/src-tauri && cargo test aspect`
Expected: FAIL — `cannot find type Aspect` / `function aspect_writes`.

- [ ] **Step 3: Implement the aspect model** (in `ops.rs`, after the `BatchOp` enum):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    Layout,
    Overview,
    Autofill,
    Everything,
}

/// What a chosen set of aspects writes, split by file side. Pure derivation of
/// the single routing table (plan header): the char file, the account file, or
/// both — as subtree splices or a whole-file copy (`Everything`).
#[derive(Debug, Clone, PartialEq)]
pub struct AspectWrites {
    pub char_categories: Vec<Category>,
    pub account_categories: Vec<Category>,
    pub char_full_copy: bool,
    pub account_full_copy: bool,
}

impl AspectWrites {
    pub fn writes_account(&self) -> bool {
        self.account_full_copy || !self.account_categories.is_empty()
    }
    pub fn writes_char(&self) -> bool {
        self.char_full_copy || !self.char_categories.is_empty()
    }
    /// True when the char write copies window geometry (drives the off-screen
    /// resolution warning): a full char copy, or a Layout splice.
    pub fn copies_char_geometry(&self) -> bool {
        self.char_full_copy || self.char_categories.contains(&Category::Layout)
    }
}

pub fn aspect_writes(aspects: &[Aspect]) -> AspectWrites {
    if aspects.contains(&Aspect::Everything) {
        return AspectWrites {
            char_categories: vec![],
            account_categories: vec![],
            char_full_copy: true,
            account_full_copy: true,
        };
    }
    let mut char_categories = vec![];
    let mut account_categories = vec![];
    for a in aspects {
        match a {
            Aspect::Layout => char_categories.push(Category::Layout),
            Aspect::Overview => {
                char_categories.push(Category::OverviewWidths);
                account_categories.push(Category::Overview);
            }
            Aspect::Autofill => account_categories.push(Category::Autofill),
            Aspect::Everything => unreachable!("handled above"),
        }
    }
    AspectWrites { char_categories, account_categories, char_full_copy: false, account_full_copy: false }
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd app/src-tauri && cargo test aspect`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs
git commit -m "Add the M5 aspect model and pure aspect routing"
```

---

### Task 3: Pure `plan_setup` — resolution, dedupe, collateral, exclusion

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (add the plan types + `plan_setup` + `account_of` helper; tests in `mod tests`)

**Interfaces:**
- Consumes: `Aspect`, `aspect_writes`, `AspectWrites` (Task 2); `crate::accounts::AccountsStore`.
- Produces:
  - `pub struct SetupPlan { char_writes: Vec<CharWrite>, account_writes: Vec<AccountWrite>, excluded: Vec<ExcludedTarget>, source_error: Option<String> }` (`Serialize`, `Default`, `Debug`, `PartialEq`).
  - `pub struct CharWrite { char_id: u64, path: String, full_copy: bool, resolution_mismatch: bool }`.
  - `pub struct AccountWrite { user_id: u64, path: String, full_copy: bool, collateral_char_ids: Vec<u64> }`.
  - `pub struct ExcludedTarget { char_id: u64, reason: String }`.
  - `pub fn plan_setup(char_paths: &HashMap<u64, PathBuf>, user_paths: &HashMap<u64, PathBuf>, store: &AccountsStore, resolutions: &HashMap<u64, (i64, i64)>, source_char: u64, target_chars: &[u64], aspects: &[Aspect]) -> SetupPlan`.

- [ ] **Step 1: Write the failing tests** (append inside `ops.rs`'s `mod tests`; add `use std::collections::HashMap;` and `use std::path::PathBuf;` at the top of the test module if not present):

```rust
    fn store_2accounts() -> accounts::AccountsStore {
        // account 10 has chars {1,2}; account 20 has char {3}. char 4 unpaired.
        let mut s = accounts::AccountsStore::default();
        s.accounts.insert(10, accounts::Account { alias: None, characters: vec![1, 2] });
        s.accounts.insert(20, accounts::Account { alias: None, characters: vec![3] });
        s
    }
    fn paths(ids: &[u64], prefix: &str) -> HashMap<u64, PathBuf> {
        ids.iter().map(|&i| (i, PathBuf::from(format!("{prefix}{i}.dat")))).collect()
    }

    #[test]
    fn overview_dedupes_account_write_and_lists_collateral() {
        // Source char 3 (account 20). Targets 1 and 2 both on account 10.
        let cp = paths(&[1, 2, 3], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 3, &[1, 2], &[Aspect::Overview]);
        assert_eq!(plan.char_writes.len(), 2, "both targets get a char (widths) write");
        assert_eq!(plan.account_writes.len(), 1, "one account write for account 10, deduped");
        assert_eq!(plan.account_writes[0].user_id, 10);
        assert!(plan.account_writes[0].collateral_char_ids.is_empty(),
            "both chars on account 10 are selected — no collateral");
        assert!(plan.source_error.is_none());
    }

    #[test]
    fn overview_warns_collateral_for_unselected_sibling() {
        // Source char 3. Target 1 on account 10 (whose other char 2 is NOT selected).
        let cp = paths(&[1, 2, 3], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 3, &[1], &[Aspect::Overview]);
        assert_eq!(plan.account_writes.len(), 1);
        assert_eq!(plan.account_writes[0].collateral_char_ids, vec![2], "char 2 is collateral");
    }

    #[test]
    fn account_aspect_excludes_an_unpaired_target() {
        let cp = paths(&[1, 3, 4], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 3, &[1, 4], &[Aspect::Autofill]);
        assert_eq!(plan.excluded.len(), 1);
        assert_eq!(plan.excluded[0].char_id, 4);
        assert_eq!(plan.account_writes.len(), 1, "only the paired target's account is written");
    }

    #[test]
    fn layout_only_includes_unpaired_targets_no_account_write() {
        let cp = paths(&[1, 3, 4], "char");
        let up = paths(&[10, 20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 3, &[1, 4], &[Aspect::Layout]);
        assert!(plan.excluded.is_empty(), "layout needs no pairing");
        assert_eq!(plan.char_writes.len(), 2);
        assert!(plan.account_writes.is_empty());
    }

    #[test]
    fn target_on_source_account_skips_the_account_write() {
        // Source char 1 (account 10). Target char 2, same account 10.
        let cp = paths(&[1, 2], "char");
        let up = paths(&[10], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 1, &[2], &[Aspect::Overview]);
        assert_eq!(plan.char_writes.len(), 1, "target still gets its widths");
        assert!(plan.account_writes.is_empty(), "same account already has the source's overview");
    }

    #[test]
    fn unpaired_source_with_account_aspect_is_a_source_error() {
        let cp = paths(&[3, 4], "char");
        let up = paths(&[20], "user");
        let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), 4, &[3], &[Aspect::Overview]);
        assert!(plan.source_error.is_some());
        assert!(plan.char_writes.is_empty() && plan.account_writes.is_empty());
    }

    #[test]
    fn resolution_mismatch_flagged_for_layout_when_screens_differ() {
        let cp = paths(&[1, 3], "char");
        let up = paths(&[10, 20], "user");
        let mut res = HashMap::new();
        res.insert(3u64, (2560i64, 1440i64)); // source
        res.insert(1u64, (1920i64, 1080i64)); // target differs
        let plan = plan_setup(&cp, &up, &store_2accounts(), &res, 3, &[1], &[Aspect::Layout]);
        assert!(plan.char_writes[0].resolution_mismatch);
    }
```

- [ ] **Step 2: Run, verify fail**

Run: `cd app/src-tauri && cargo test plan_setup overview_dedupes overview_warns account_aspect_excludes layout_only_includes target_on_source unpaired_source resolution_mismatch`
Expected: FAIL — `cannot find function plan_setup` / plan types.

- [ ] **Step 3: Implement the plan types and `plan_setup`** (in `ops.rs`, after `aspect_writes`; add `use std::collections::{BTreeMap, HashMap, HashSet};` and `use std::path::PathBuf;` near the top of `ops.rs` if not already imported — `std::path::Path` is already there):

```rust
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct SetupPlan {
    pub char_writes: Vec<CharWrite>,
    pub account_writes: Vec<AccountWrite>,
    pub excluded: Vec<ExcludedTarget>,
    pub source_error: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct CharWrite {
    pub char_id: u64,
    pub path: String,
    pub full_copy: bool,
    pub resolution_mismatch: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AccountWrite {
    pub user_id: u64,
    pub path: String,
    pub full_copy: bool,
    /// Characters on this account that are NOT selected targets — the write
    /// changes them too.
    pub collateral_char_ids: Vec<u64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ExcludedTarget {
    pub char_id: u64,
    pub reason: String,
}

/// The account (user id) that owns `char_id`, per the persisted pairing.
fn account_of(store: &accounts::AccountsStore, char_id: u64) -> Option<u64> {
    store.accounts.iter().find(|(_, a)| a.characters.contains(&char_id)).map(|(&uid, _)| uid)
}

/// Pure planner. All disk-dependent inputs (discovered file paths, the store,
/// each char's stored screen resolution) are passed in, so this is unit-tested
/// without a filesystem. Paths are already folder-scoped by the caller.
pub fn plan_setup(
    char_paths: &HashMap<u64, PathBuf>,
    user_paths: &HashMap<u64, PathBuf>,
    store: &accounts::AccountsStore,
    resolutions: &HashMap<u64, (i64, i64)>,
    source_char: u64,
    target_chars: &[u64],
    aspects: &[Aspect],
) -> SetupPlan {
    let w = aspect_writes(aspects);
    let mut plan = SetupPlan::default();

    let source_account = account_of(store, source_char);
    if w.writes_account() {
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
    let src_res = resolutions.get(&source_char).copied();

    let mut included: Vec<u64> = Vec::new();
    for &t in target_chars {
        if t == source_char {
            continue;
        }
        if !char_paths.contains_key(&t) {
            plan.excluded.push(ExcludedTarget { char_id: t, reason: "Character file not found in this folder.".into() });
            continue;
        }
        if w.writes_account() {
            match account_of(store, t) {
                None => {
                    plan.excluded.push(ExcludedTarget { char_id: t, reason: "No account paired — pair it in the Accounts view to include.".into() });
                    continue;
                }
                Some(uid) if !user_paths.contains_key(&uid) => {
                    plan.excluded.push(ExcludedTarget { char_id: t, reason: "Account file not found in this folder.".into() });
                    continue;
                }
                _ => {}
            }
        }
        included.push(t);
    }

    if w.writes_char() {
        for &t in &included {
            let path = char_paths[&t].to_string_lossy().into_owned();
            let resolution_mismatch = w.copies_char_geometry()
                && match (src_res, resolutions.get(&t).copied()) {
                    (Some(s), Some(d)) => s != d && s != (0, 0) && d != (0, 0),
                    _ => false,
                };
            plan.char_writes.push(CharWrite { char_id: t, path, full_copy: w.char_full_copy, resolution_mismatch });
        }
    }

    if w.writes_account() {
        let mut by_account: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for &t in &included {
            let uid = account_of(store, t).expect("included target is paired");
            by_account.entry(uid).or_default().push(t);
        }
        for (uid, selected_on_acct) in by_account {
            if Some(uid) == source_account {
                continue; // already carries the source's settings
            }
            let path = user_paths[&uid].to_string_lossy().into_owned();
            let selected: HashSet<u64> = selected_on_acct.into_iter().collect();
            let collateral: Vec<u64> = store
                .accounts
                .get(&uid)
                .map(|a| a.characters.iter().copied().filter(|c| !selected.contains(c)).collect())
                .unwrap_or_default();
            plan.account_writes.push(AccountWrite { user_id: uid, path, full_copy: w.account_full_copy, collateral_char_ids: collateral });
        }
    }

    plan
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd app/src-tauri && cargo test plan_setup overview_dedupes overview_warns account_aspect_excludes layout_only_includes target_on_source unpaired_source resolution_mismatch`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs
git commit -m "Add the pure plan_setup: resolution, dedupe, collateral, exclusion"
```

---

### Task 4: Orchestrators `setup_preview` + `setup_apply`

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (add the two orchestrators + a private path/resolution gatherer; add a command-level test)

**Interfaces:**
- Consumes: `plan_setup` + plan types (Task 3); `settings_model::{discover, Profile, FileKind, window_layout as project_window_layout, extract_categories, apply_categories_to, full_copy_to}`; `crate::accounts::load_store`; `blue_marshal::decode`.
- Produces:
  - `pub fn setup_preview(roots: &[PathBuf], dir: &Path, source_char_path: &str, target_char_paths: &[String], aspects: &[Aspect], allow_other_folders: bool) -> SetupPlan`.
  - `pub fn setup_apply(roots: &[PathBuf], dir: &Path, source_char_path: &str, target_char_paths: &[String], aspects: &[Aspect], allow_other_folders: bool) -> Result<Vec<TargetResult>, ErrDto>` (reuses M4's `TargetResult`, `ok_result`, `err_result`).

- [ ] **Step 1: Write the failing command-level test** (append inside `ops.rs`'s `mod tests`; reuses `blue_marshal::encode` and the batch fixtures pattern):

```rust
    #[test]
    fn setup_apply_overview_reports_char_and_account_writes_with_a_readonly_failure() {
        use blue_marshal::{encode, Value};
        let base = std::env::temp_dir().join(format!("app-setup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        // Discovery root with the real install/profile structure discover() expects
        // (mirrors the discovery_tree() helper the M4 target tests use).
        let prof = base.join("root").join("c_eve_sharedcache_tq_tranquility").join("settings_Default");
        std::fs::create_dir_all(&prof).unwrap();
        fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
        fn ts() -> Value { Value::Long(vec![0u8; 8]) }
        let overview = |c: &str| Value::Dict(vec![(b("overview"),
            Value::Dict(vec![(b("overviewColumns"), Value::List(vec![b(c)]))]))]);
        let widths = || Value::Dict(vec![(b("ui"), Value::Dict(vec![(b("SortHeadersSizes"),
            Value::Tuple(vec![ts(), Value::Dict(vec![])]))]))]);
        // source char 100 on account 500; target char 200 on account 600.
        std::fs::write(prof.join("core_char_100.dat"), encode(&widths()).unwrap()).unwrap();
        std::fs::write(prof.join("core_user_500.dat"), encode(&overview("SRC")).unwrap()).unwrap();
        std::fs::write(prof.join("core_char_200.dat"), encode(&widths()).unwrap()).unwrap();
        // read-only stream (INT8-encoded) => save() refuses it => account write fails.
        std::fs::write(prof.join("core_user_600.dat"), [0x7E, 0, 0, 0, 0, 0x06, 0x01]).unwrap();

        // accounts.json lives in the app-data dir, separate from the discovery root.
        let app_dir = base.join("appdata");
        std::fs::create_dir_all(&app_dir).unwrap();
        let mut store = accounts::AccountsStore::default();
        store.accounts.insert(500, accounts::Account { alias: None, characters: vec![100] });
        store.accounts.insert(600, accounts::Account { alias: None, characters: vec![200] });
        std::fs::write(app_dir.join("accounts.json"), serde_json::to_vec(&store).unwrap()).unwrap();

        let roots = vec![base.join("root")];
        let src = prof.join("core_char_100.dat").to_string_lossy().into_owned();
        let tgt = vec![prof.join("core_char_200.dat").to_string_lossy().into_owned()];
        let results = setup_apply(&roots, &app_dir, &src, &tgt, &[Aspect::Overview], false).unwrap();

        // One char write (widths -> char 200, ok) and one account write (overview
        // -> read-only user 600, fails) — the failure did not halt the char write.
        let char_ok = results.iter().any(|r| r.path.contains("core_char_200") && r.ok);
        let acct_fail = results.iter().any(|r| r.path.contains("core_user_600") && !r.ok);
        assert!(char_ok, "char widths write succeeded");
        assert!(acct_fail, "read-only account write failed but was reported, not panicked");
    }
```

- [ ] **Step 2: Run, verify fail**

Run: `cd app/src-tauri && cargo test setup_apply_overview`
Expected: FAIL — `cannot find function setup_apply`.

- [ ] **Step 3: Implement the orchestrators** (in `ops.rs`, after `plan_setup`). The gatherer builds folder-scoped `char_paths`/`user_paths` and, only when the aspects copy geometry, each char's stored resolution:

```rust
/// Discover, folder-scope to the source's profile (unless `allow_other_folders`),
/// and split into char/user id->path maps. Returns the source char's id too.
fn scoped_files(
    roots: &[PathBuf],
    source_char_path: &str,
    allow_other_folders: bool,
) -> Option<(u64, HashMap<u64, PathBuf>, HashMap<u64, PathBuf>)> {
    let profiles = discover(roots);
    let src = Path::new(source_char_path);
    let mut src_id = None;
    let mut src_dir = None;
    for p in &profiles {
        for f in &p.files {
            if f.path == src {
                src_id = f.id;
                src_dir = Some(p.dir.clone());
            }
        }
    }
    let src_id = src_id?;
    let mut char_paths = HashMap::new();
    let mut user_paths = HashMap::new();
    for p in &profiles {
        if !allow_other_folders && Some(&p.dir) != src_dir.as_ref() {
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
    Some((src_id, char_paths, user_paths))
}

/// Each char's stored screen resolution (reference_w, reference_h), for the
/// resolution-mismatch warning. Only the source + requested targets are read.
fn gather_resolutions(char_paths: &HashMap<u64, PathBuf>, ids: &[u64]) -> HashMap<u64, (i64, i64)> {
    let mut out = HashMap::new();
    for &id in ids {
        let Some(path) = char_paths.get(&id) else { continue };
        let Ok(bytes) = fs::read(path) else { continue };
        let Ok(value) = blue_marshal::decode(&bytes) else { continue };
        let wl = project_window_layout(&value);
        out.insert(id, (wl.reference_w, wl.reference_h));
    }
    out
}

/// Map target file paths to char ids within the scoped char map.
fn target_ids(char_paths: &HashMap<u64, PathBuf>, target_char_paths: &[String]) -> Vec<u64> {
    target_char_paths
        .iter()
        .filter_map(|t| {
            let tp = Path::new(t);
            char_paths.iter().find(|(_, p)| p.as_path() == tp).map(|(&id, _)| id)
        })
        .collect()
}

pub fn setup_preview(
    roots: &[PathBuf],
    dir: &Path,
    source_char_path: &str,
    target_char_paths: &[String],
    aspects: &[Aspect],
    allow_other_folders: bool,
) -> SetupPlan {
    let Some((src_id, char_paths, user_paths)) = scoped_files(roots, source_char_path, allow_other_folders)
    else {
        return SetupPlan { source_error: Some("Source file not found.".into()), ..Default::default() };
    };
    let targets = target_ids(&char_paths, target_char_paths);
    let store = accounts::load_store(dir);
    let resolutions = if aspect_writes(aspects).copies_char_geometry() {
        let mut ids = targets.clone();
        ids.push(src_id);
        gather_resolutions(&char_paths, &ids)
    } else {
        HashMap::new()
    };
    plan_setup(&char_paths, &user_paths, &store, &resolutions, src_id, &targets, aspects)
}

pub fn setup_apply(
    roots: &[PathBuf],
    dir: &Path,
    source_char_path: &str,
    target_char_paths: &[String],
    aspects: &[Aspect],
    allow_other_folders: bool,
) -> Result<Vec<TargetResult>, ErrDto> {
    let plan = setup_preview(roots, dir, source_char_path, target_char_paths, aspects, allow_other_folders);
    if let Some(e) = plan.source_error {
        return Err(ErrDto::new("source", e));
    }
    let w = aspect_writes(aspects);

    // Read/decode the source's two files once, extracting each side's subtrees.
    let src_char_bytes = fs::read(source_char_path).map_err(|e| ErrDto::new("io", e.to_string()))?;
    let char_extracted = if !w.char_categories.is_empty() {
        let v = blue_marshal::decode(&src_char_bytes).map_err(|e| ErrDto::new("decode", e.to_string()))?;
        extract_categories(&v, &w.char_categories)
    } else {
        vec![]
    };
    // The account (user) file behind the source char, if any account write is needed.
    let (user_bytes, account_extracted) = if w.writes_account() {
        let Some((src_id, _cp, user_paths)) = scoped_files(roots, source_char_path, allow_other_folders) else {
            return Err(ErrDto::new("source", "Source file not found."));
        };
        let store = accounts::load_store(dir);
        let uid = account_of(&store, src_id).ok_or_else(|| ErrDto::new("source", "Source character has no paired account."))?;
        let upath = user_paths.get(&uid).ok_or_else(|| ErrDto::new("source", "Source account file not found."))?;
        let bytes = fs::read(upath).map_err(|e| ErrDto::new("io", e.to_string()))?;
        let extracted = if !w.account_categories.is_empty() {
            let v = blue_marshal::decode(&bytes).map_err(|e| ErrDto::new("decode", e.to_string()))?;
            extract_categories(&v, &w.account_categories)
        } else {
            vec![]
        };
        (bytes, extracted)
    } else {
        (vec![], vec![])
    };

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

- [ ] **Step 4: Run, verify pass** (and the whole ops test module still passes)

Run: `cd app/src-tauri && cargo test setup_apply_overview` then `cd app/src-tauri && cargo test`
Expected: PASS (the new test + all existing ops/app tests).

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/ops.rs
git commit -m "Add setup_preview and setup_apply orchestrators"
```

---

### Task 5: Wire the commands + frontend API surface

**Files:**
- Modify: `app/src-tauri/src/lib.rs` (add two `#[tauri::command]` wrappers + register them ~L182-191)
- Modify: `app/src/lib/api.ts` (add TS types + wrappers; keep M4's for now)

**Interfaces:**
- Consumes: `ops::{setup_preview, setup_apply, Aspect, SetupPlan, TargetResult}`.
- Produces (Tauri commands): `setup_preview(sourceCharPath, targetCharPaths, aspects, allowOtherFolders) -> SetupPlan`; `setup_apply(...) -> BatchTargetResult[]`.
- Produces (TS): `Aspect`, `SetupPlan`, `CharWrite`, `AccountWrite`, `ExcludedTarget`, and `api.setupPreview` / `api.setupApply`.

- [ ] **Step 1: Add the Rust command wrappers** (in `lib.rs`, after `batch_apply` ~L174):

```rust
#[tauri::command]
fn setup_preview(
    app: tauri::AppHandle,
    source_char_path: String,
    target_char_paths: Vec<String>,
    aspects: Vec<ops::Aspect>,
    allow_other_folders: bool,
) -> ops::SetupPlan {
    ops::setup_preview(
        &settings_model::default_roots(),
        &app_dir(&app),
        &source_char_path,
        &target_char_paths,
        &aspects,
        allow_other_folders,
    )
}

#[tauri::command]
fn setup_apply(
    app: tauri::AppHandle,
    source_char_path: String,
    target_char_paths: Vec<String>,
    aspects: Vec<ops::Aspect>,
    allow_other_folders: bool,
) -> Result<Vec<ops::TargetResult>, ErrDto> {
    ops::setup_apply(
        &settings_model::default_roots(),
        &app_dir(&app),
        &source_char_path,
        &target_char_paths,
        &aspects,
        allow_other_folders,
    )
}
```

Register them in `generate_handler!` (extend the `batch_targets, batch_apply` line):

```rust
            batch_targets, batch_apply,
            setup_preview, setup_apply
```

- [ ] **Step 2: Add the TS types + wrappers** (in `api.ts`, near the batch types ~L190-204 and in the `api` object ~L243-246):

```ts
export type Aspect = "layout" | "overview" | "autofill" | "everything";
export interface CharWrite {
  char_id: number;
  path: string;
  full_copy: boolean;
  resolution_mismatch: boolean;
}
export interface AccountWrite {
  user_id: number;
  path: string;
  full_copy: boolean;
  collateral_char_ids: number[];
}
export interface ExcludedTarget {
  char_id: number;
  reason: string;
}
export interface SetupPlan {
  char_writes: CharWrite[];
  account_writes: AccountWrite[];
  excluded: ExcludedTarget[];
  source_error: string | null;
}
```

Add to the `api` object:

```ts
  setupPreview: (
    sourceCharPath: string,
    targetCharPaths: string[],
    aspects: Aspect[],
    allowOtherFolders: boolean,
  ) =>
    invoke<SetupPlan>("setup_preview", { sourceCharPath, targetCharPaths, aspects, allowOtherFolders }),
  setupApply: (
    sourceCharPath: string,
    targetCharPaths: string[],
    aspects: Aspect[],
    allowOtherFolders: boolean,
  ) =>
    invoke<BatchTargetResult[]>("setup_apply", { sourceCharPath, targetCharPaths, aspects, allowOtherFolders }),
```

- [ ] **Step 3: Verify the app crate compiles and the frontend type-checks**

Run: `cd app/src-tauri && cargo build`
Expected: builds clean.
Run (PowerShell): `cd app; npm run check`
Expected: 0 errors (the new types/wrappers are added; nothing consumes them yet).

- [ ] **Step 4: Commit**

```bash
git add app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Wire setup_preview and setup_apply commands and TS API"
```

---

### Task 6: Rework `BatchView.svelte` to be character-centric

**Files:**
- Rewrite: `app/src/lib/BatchView.svelte`

**Interfaces:**
- Consumes: `api.discover`, `api.setupPreview`, `api.setupApply`; `Aspect`, `SetupPlan`, `BatchTargetResult`, `Profile` from `./api`; `resolvedName`, `byResolvedName` from `./filesort.svelte`; `profileLabels`, `primaryProfileDir` from `./profiles`; `accountsStore` (roster) from `./accounts.svelte`; `loadRoster` from `./accounts.svelte`.
- Produces: the character-centric batch UI. Mount point in `+page.svelte` is unchanged (`<BatchView openPath={…} />`).

- [ ] **Step 1: Replace the file with the character-centric view**

Full new `app/src/lib/BatchView.svelte`:

```svelte
<script lang="ts">
  import { untrack } from "svelte";
  import { api, errMessage, type Profile, type Aspect, type SetupPlan, type BatchTargetResult } from "./api";
  import { byResolvedName, resolvedName } from "./filesort.svelte";
  import { primaryProfileDir, profileLabels } from "./profiles";
  import { accountsStore, loadRoster } from "./accounts.svelte";

  let { openPath }: { openPath: string | null } = $props();

  loadRoster();

  // Character (char) files only — the source and every target is a character.
  let profiles = $state<Profile[]>([]);
  api.discover().then((p) => (profiles = p)).catch(() => {});
  const chars = $derived(
    profiles.flatMap((p) =>
      p.files
        .filter((f) => f.kind === "char")
        .map((f) => ({ path: f.path, file_name: f.file_name, id: f.id, dir: p.dir })),
    ),
  );

  const folders = $derived.by(() => {
    const labels = profileLabels(profiles);
    return profiles
      .filter((p) => chars.some((c) => c.dir === p.dir))
      .map((p) => ({ dir: p.dir, label: labels.get(p.dir)! }));
  });

  let folderPick = $state<string | null>(null);
  const autoFolder = $derived(
    chars.find((c) => c.path === sourcePath)?.dir ?? primaryProfileDir(profiles),
  );
  const folder = $derived(folderPick ?? autoFolder);

  let sourcePath = $state<string | null>(
    untrack(() => (openPath && openPath.includes("core_char_") ? openPath : null)),
  );
  const source = $derived(chars.find((c) => c.path === sourcePath) ?? null);

  function pickFolder(dir: string) {
    folderPick = dir;
    sourcePath = null;
  }

  // Aspects. "Everything" is exclusive.
  const ASPECTS: { key: Aspect; label: string; account: boolean }[] = [
    { key: "layout", label: "Window layout", account: false },
    { key: "overview", label: "Overview (columns, tabs, presets)", account: true },
    { key: "autofill", label: "Autofill (remembered text)", account: true },
    { key: "everything", label: "Everything (full clone of both files)", account: true },
  ];
  let selected = $state<Set<Aspect>>(new Set());
  const everything = $derived(selected.has("everything"));
  const anyAccountAspect = $derived([...selected].some((a) => ASPECTS.find((x) => x.key === a)?.account));
  function toggleAspect(a: Aspect) {
    const next = new Set(selected);
    if (a === "everything") {
      next.has(a) ? next.delete(a) : (next.clear(), next.add(a));
    } else {
      next.delete("everything");
      next.has(a) ? next.delete(a) : next.add(a);
    }
    selected = next;
  }

  // Which char ids are paired (member of some account) — unpaired chars can't
  // receive an account aspect.
  const pairedIds = $derived(
    new Set(accountsStore.roster.accounts.flatMap((acc) => acc.characters)),
  );

  let allowOtherFolders = $state(false);
  const candidates = $derived(
    chars
      .filter((c) => c.path !== sourcePath)
      .filter((c) => allowOtherFolders || c.dir === folder)
      .slice()
      .sort((a, b) =>
        byResolvedName(
          { kind: "char", id: a.id, file_name: a.file_name },
          { kind: "char", id: b.id, file_name: b.file_name },
        ),
      ),
  );
  // The source dropdown lists every character in the folder (the current source
  // included), ordered like the sidebar.
  const sourceOptions = $derived(
    chars
      .filter((c) => allowOtherFolders || c.dir === folder)
      .slice()
      .sort((a, b) =>
        byResolvedName(
          { kind: "char", id: a.id, file_name: a.file_name },
          { kind: "char", id: b.id, file_name: b.file_name },
        ),
      ),
  );
  let selectedTargets = $state<Set<string>>(new Set());
  function toggleTarget(path: string) {
    const next = new Set(selectedTargets);
    next.has(path) ? next.delete(path) : next.add(path);
    selectedTargets = next;
  }
  const targetDisabled = (id: number | null) => anyAccountAspect && !(id != null && pairedIds.has(id));

  const nameOfChar = (id: number | null, fileName: string) =>
    id == null ? fileName : (resolvedName("char", id) ?? `char ${id}`);
  const folderLabelOf = (dir: string) => profileLabels(profiles).get(dir) ?? dir;

  // Reset op + targets when the source changes.
  $effect(() => {
    sourcePath;
    selected = new Set();
    selectedTargets = new Set();
  });

  // Preview from the backend whenever source/aspects/targets settle.
  let plan = $state<SetupPlan | null>(null);
  $effect(() => {
    const sp = sourcePath;
    const asp = [...selected];
    const tgts = [...selectedTargets];
    const allow = allowOtherFolders;
    if (!sp || asp.length === 0 || tgts.length === 0) { plan = null; return; }
    api.setupPreview(sp, tgts, asp as Aspect[], allow).then((p) => (plan = p)).catch(() => (plan = null));
  });

  let busy = $state(false);
  let error = $state<string | null>(null);
  let results = $state<BatchTargetResult[] | null>(null);
  const canApply = $derived(
    !!sourcePath && selected.size > 0 && selectedTargets.size > 0 && !busy &&
    !!plan && !plan.source_error && (plan.char_writes.length + plan.account_writes.length > 0),
  );

  async function apply() {
    if (!sourcePath) return;
    busy = true; error = null; results = null;
    try {
      results = await api.setupApply(sourcePath, [...selectedTargets], [...selected] as Aspect[], allowOtherFolders);
    } catch (e) {
      error = errMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="batch">
  <h2>Copy a character's setup</h2>

  <section>
    <label for="folder">Profile</label>
    <select id="folder" value={folder} onchange={(e) => pickFolder(e.currentTarget.value)}>
      {#each folders as f}<option value={f.dir}>{f.label}</option>{/each}
    </select>

    <label for="src">Source character</label>
    <select id="src" bind:value={sourcePath}>
      <option value={null} disabled>Choose a character…</option>
      {#each sourceOptions as c}
        <option value={c.path}>{nameOfChar(c.id, c.file_name)} — {c.file_name}</option>
      {/each}
    </select>
  </section>

  {#if source}
    <section>
      <div class="head">What to copy</div>
      {#each ASPECTS as a}
        <label class:disabled={everything && a.key !== "everything"}>
          <input type="checkbox" checked={selected.has(a.key)}
            disabled={everything && a.key !== "everything"}
            onchange={() => toggleAspect(a.key)} />
          {a.label}
        </label>
      {/each}
    </section>

    <section>
      <div class="head">
        Target characters
        <label class="inline"><input type="checkbox" bind:checked={allowOtherFolders} /> Show other folders</label>
      </div>
      {#if candidates.length === 0}
        <p class="muted">No other character files found.</p>
      {:else}
        {#each candidates as c}
          <label class:disabled={targetDisabled(c.id)}>
            <input type="checkbox" checked={selectedTargets.has(c.path)}
              disabled={targetDisabled(c.id)} onchange={() => toggleTarget(c.path)} />
            {nameOfChar(c.id, c.file_name)}
            <span class="muted">{c.file_name}{c.dir === folder ? "" : ` · ${folderLabelOf(c.dir)}`}</span>
            {#if targetDisabled(c.id)}<span class="muted"> — pair in the Accounts view to include</span>{/if}
          </label>
        {/each}
      {/if}
    </section>

    {#if plan}
      <section class="preview">
        {#if plan.source_error}
          <p class="err">{plan.source_error}</p>
        {:else}
          <p>Will write {plan.char_writes.length + plan.account_writes.length} file(s) — each is backed up first.</p>
          {#each plan.char_writes.filter((w) => w.resolution_mismatch) as w}
            <p class="warn">⚠ {nameOfChar(w.char_id, "")}: screen resolution differs from the source — copied windows may land off-screen.</p>
          {/each}
          {#each plan.account_writes.filter((w) => w.collateral_char_ids.length > 0) as w}
            <p class="warn">⚠ {w.full_copy ? "Entire account settings replaced" : "Overview / autofill changed"} for account {w.user_id} — also changes: {w.collateral_char_ids.map((id) => nameOfChar(id, `char ${id}`)).join(", ")}.</p>
          {/each}
          {#each plan.excluded as ex}
            <p class="muted">Excluded {nameOfChar(ex.char_id, `char ${ex.char_id}`)} — {ex.reason}</p>
          {/each}
        {/if}
      </section>
    {/if}

    <section>
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
  select, option { background: var(--bg-panel); color: var(--fg); border: 1px solid var(--border); border-radius: 3px; padding: 2px 4px; font: inherit; }
  input[type="checkbox"] { accent-color: var(--accent); }
  .muted { color: var(--fg-dim); }
  .preview p { margin: 0.15rem 0; }
  .warn { color: #d0a000; }
  .err, .fail { color: #e06c6c; }
  .ok { color: #6cc06c; }
  button { padding: 0.35rem 0.9rem; }
</style>
```

- [ ] **Step 2: Type-check and build the frontend**

Run (PowerShell): `cd app; npm run check`
Expected: 0 errors, 0 warnings.
Run (PowerShell): `cd app; npm run build`
Expected: builds `app/build` with no errors.

- [ ] **Step 3: Confirm the existing frontend tests still pass**

Run (PowerShell): `cd app; npm test`
Expected: PASS (no regressions; this view has no unit test, consistent with M4).

- [ ] **Step 4: Commit**

```bash
git add app/src/lib/BatchView.svelte
git commit -m "Rework the batch view to be character-centric"
```

---

### Task 7: Remove the dead M4 file-centric batch code

**Files:**
- Modify: `app/src-tauri/src/ops.rs` (delete `batch_targets`, `batch_apply`, `Candidate`, `BatchOp`, `kind_mismatch`, and their tests — but KEEP `TargetResult`, `ok_result`, `err_result`, which the setup path reuses)
- Modify: `app/src-tauri/src/lib.rs` (delete the `batch_targets` / `batch_apply` command fns and their handler entries)
- Modify: `app/src/lib/api.ts` (delete `Category`, `BatchCandidate`, `BatchOp`, `api.batchTargets`, `api.batchApply`; KEEP `BatchTargetResult`)

**Interfaces:**
- Consumes: nothing new.
- Produces: a single batch model (character-centric); the M4 file-centric surface is gone.

- [ ] **Step 1: Confirm nothing still references the M4 surface**

Run: `git grep -n "batchTargets\|batchApply\|BatchOp\|BatchCandidate\|batch_targets\|batch_apply"`
Expected: only the definitions to be deleted (no live callers in `BatchView.svelte` after Task 6).

- [ ] **Step 2: Delete the Rust M4 command logic** from `ops.rs`: the `Candidate` struct, `batch_targets`, the `BatchOp` enum, `batch_apply`, `kind_mismatch`, and the M4 tests (`batch_targets_same_folder_same_type_excludes_source`, `batch_targets_allow_other_folders_adds_matching_type_elsewhere`, `batch_apply_categories_reports_per_target_including_a_read_only_failure`, `batch_apply_full_copy_makes_targets_byte_identical`, `batch_apply_categories_aborts_when_source_lacks_the_category`, `batch_apply_full_copy_refuses_a_mismatched_target_kind`, `batch_apply_categories_refuses_a_mismatched_target_kind`, `batch_apply_undecodable_source_fails_the_whole_op`). Keep `TargetResult`, `ok_result`, `err_result`, and the `file_kind` import if still used elsewhere (drop it from the `use` list if the compiler flags it unused).

- [ ] **Step 3: Delete the Rust command wrappers** from `lib.rs`: the `batch_targets` and `batch_apply` `#[tauri::command]` fns, and remove `batch_targets, batch_apply,` from `generate_handler!` (leaving `setup_preview, setup_apply`).

- [ ] **Step 4: Delete the M4 TS surface** from `api.ts`: `export type Category`, `export interface BatchCandidate`, `export type BatchOp`, and the `batchTargets` / `batchApply` methods. Keep `BatchTargetResult` (setup_apply returns it).

- [ ] **Step 5: Build, type-check, test everything green**

Run: `cargo test` (workspace root) — Expected: PASS, no references to removed items.
Run: `cd app/src-tauri && cargo build` — Expected: clean (no unused-import warnings; fix any the compiler flags).
Run (PowerShell): `cd app; npm run check` — Expected: 0 errors.
Run (PowerShell): `cd app; npm run build` — Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add app/src-tauri/src/ops.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Remove the superseded M4 file-centric batch surface"
```

---

### Task 8: Live smoke (merge gate) + finish

**Files:**
- Modify: `docs/format-notes.md` (append a `## Status` line recording the M5 smoke result)

This task has no unit test — it is the real merge gate (the EVE overview/window format has been mis-modeled from synthetic data before; only a real-client round-trip proves it).

- [ ] **Step 1: Launch the app against the real settings directory**

Run (PowerShell): `cd app; npm run tauri dev`

- [ ] **Step 2: Overview + layout match.** Open the character-centric batch view. Source = a main character; aspects = Window layout + Overview; target = an alt on a *different* account. Confirm the preview names the alt's account collateral characters. Apply. Verify:
  - Every written file reports ✓ with a backup path.
  - Log into the alt in EVE: its overview columns/tabs match the main; windows appear (accounting for the M4 ceiling — overview-window count now follows the copied account config).
  - Re-open each written file in the tool (or decode) — it loads with no duplicate-key error.

- [ ] **Step 3: Everything (full clone).** Source = the main; aspect = Everything; target = a second alt on another account. Confirm the preview says the entire account is replaced and lists that account's other characters as collateral. Apply, then verify in-game that the alt's char + account settings match the main, and that the warned collateral character was also changed.

- [ ] **Step 4: Collateral truth check.** Pick a target whose account has an unselected sibling character. After an Overview apply, confirm in-game that the sibling's overview also changed (proving the collateral warning was accurate), and that its backup exists.

- [ ] **Step 5: Record the result** in `docs/format-notes.md` under `## Status`: the date, that the M5 character-centric batch (layout + overview + autofill + everything) round-tripped through the real client, the exact account-write trigger observed, and that collateral characters changed as warned.

- [ ] **Step 6: Commit**

```bash
git add docs/format-notes.md
git commit -m "Record the M5 live smoke result"
```

- [ ] **Step 7: Finish the branch** with superpowers:finishing-a-development-branch (whole-branch review, then PR/merge). The M5 release (version bump + CHANGELOG + tag) follows the recorded release process; revisit the small-tasks ledger before tagging.

---

## Notes for the implementer

- **Do not reimplement the save chain.** Every category splice must go through `apply_categories_to` (which calls `save()`); every full copy through `full_copy_to`. These already give you backup + encode-verify + atomic write + ReadOnly refusal.
- **`plan_setup` is pure on purpose.** Keep all disk reads in the orchestrators (`setup_preview` / `setup_apply` / the gatherers). If you need more data in the plan, pass it in — do not make `plan_setup` read files.
- **`setup_apply` re-derives its plan** from `setup_preview` and never trusts the frontend's idea of which files to touch. Keep it that way.
- **Overview carries tabs AND widths together** (account `overview` + char `SortHeadersSizes`), which is why the copied per-tab widths stay index-aligned. Never expose widths as a separately selectable aspect.
