# Layout Aspect Scope Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the **Window layout** batch/preset aspect carry all nine of the HUD editor's fields instead of one, so a Layout copy moves a character's whole screen furniture rather than half of it.

**Architecture:** Six new leaf `Category` variants point at the individual HUD keys (`Category::Autofill => &[b"ui", b"editHistory"]` already proves a category can address one key inside a section). `Aspect::Layout` routes two of them to the character file and four to the account file, which makes it the first Layout copy that writes an account file. Because an absent key *is* EVE's default, `extract_categories` returns `Option<Value>` so an absent leaf HUD key becomes a **removal** on the target rather than a silent no-op — gated by a predicate so a missing source `overview` can never delete the target's.

**Tech Stack:** Rust (`crates/settings-model`, `app/src-tauri`), Svelte 5 runes + Vitest (`app/src`).

**Spec:** `docs/superpowers/specs/2026-07-28-layout-aspect-scope-design.md` — read §3 (the three decisions) and §4.3–§4.5 before starting; the two "deliberately unchanged" call sites in §4.3 are the easiest thing in this plan to get wrong.

## Global Constraints

- Branch is `layout-aspect-scope`, already created off `master`. Do not merge without the live smoke in Task 7.
- Rust tests: `cargo test --workspace` from the repo root. Frontend: `npm test` and `npm run check`, both from `app/`.
- `Category` variant names are exactly `HudFighterPos`, `HudBadge`, `HudShipTop`, `HudFighterDetached`, `HudFighterShown`, `HudNeocomWidth`. Serde renames them `snake_case` automatically; no `#[serde(rename)]` attributes.
- Never make a whole-section category (`Layout`, `Autofill`, `Overview`, `OverviewWidths`, `Keybinds`, `NeocomButtons`) `absent_means_default`. Deleting a target's `overview` because the source lacks one is data loss.
- Every commit message ends with `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`.

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/settings-model/src/batch.rs` | the category model, extract, apply | six variants, `absent_means_default()`, `Option<Value>`, removal branch, empty-root rule |
| `crates/settings-model/tests/batch_realshape.rs` | category behaviour on real-shaped documents | new cases for the HUD keys |
| `app/src-tauri/src/ops.rs` | aspect → category routing, planning, applying | the `Aspect::Layout` arm, one closure type annotation, tests |
| `app/src-tauri/src/presets.rs` | preset create/prune/derive | `has_category` correctness, tests pinning `prune`'s parent-building |
| `app/src/lib/BatchView.svelte` | batch UI | layout label + `account: true` |
| `app/src/lib/PresetGroup.svelte` | preset create UI | layout `needsUser: true`, drop the now-false caveat |
| `CHANGELOG.md`, `docs/small-tasks.md` | release notes, ledger | close the entry |

---

### Task 1: The six leaf HUD categories

**Files:**
- Modify: `crates/settings-model/src/batch.rs:19-46`
- Test: `crates/settings-model/src/batch.rs` (the `#[cfg(test)] mod tests` at the bottom)

**Interfaces:**
- Consumes: nothing.
- Produces: `Category::HudFighterPos`, `Category::HudBadge`, `Category::HudShipTop`, `Category::HudFighterDetached`, `Category::HudFighterShown`, `Category::HudNeocomWidth`, and `Category::absent_means_default(self) -> bool`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/settings-model/src/batch.rs`:

```rust
#[test]
fn the_hud_categories_address_the_keys_hud_rs_writes() {
    // Exactly the paths in hud.rs's FIELDS table, which is the only other
    // place these keys are named. A drift here half-applies a Layout copy
    // silently, which is the bug this whole branch exists to fix.
    let expected: [(Category, &[&[u8]]); 6] = [
        (Category::HudFighterPos, &[b"ui", b"fightersDetachedPosition"]),
        (Category::HudBadge, &[b"notifications", b"notification_badge_offset"]),
        (Category::HudShipTop, &[b"ui", b"shipuialigntop"]),
        (Category::HudFighterDetached, &[b"ui", b"detachFighterUI"]),
        (Category::HudFighterShown, &[b"ui", b"displayFighterUI"]),
        (Category::HudNeocomWidth, &[b"windows", b"neocomWidth"]),
    ];
    for (cat, path) in expected {
        assert_eq!(cat.key_path(), path, "{cat:?} addresses the wrong key");
        assert!(cat.absent_means_default(), "{cat:?} is a leaf HUD key");
    }
}

#[test]
fn a_whole_section_category_never_means_default() {
    // The destructive case: absent_means_default makes apply_to_tree DELETE
    // the target's value. A source with no overview must never wipe one.
    for cat in [
        Category::Layout,
        Category::Autofill,
        Category::Overview,
        Category::OverviewWidths,
        Category::Keybinds,
        Category::NeocomButtons,
    ] {
        assert!(!cat.absent_means_default(), "{cat:?} must never delete on the target");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model batch::tests::the_hud_categories -- --exact --nocapture`
Expected: FAIL to compile — `no variant named HudFighterPos`, `no method named absent_means_default`.

- [ ] **Step 3: Add the variants and the predicate**

In `crates/settings-model/src/batch.rs`, extend the enum:

```rust
pub enum Category {
    Layout,
    Autofill,
    Overview,
    OverviewWidths,
    Keybinds,
    NeocomButtons,
    // The HUD's individual keys. `hud.rs`'s FIELDS table is the source of
    // truth for these paths; `Aspect::Layout` carries all six so a layout
    // copy moves the whole of a character's screen furniture. They cannot be
    // whole-section splices: char `ui` also holds editHistory and
    // SortHeadersSizes, so copying the section would carry the target's
    // autofill away.
    HudFighterPos,
    HudBadge,
    HudShipTop,
    HudFighterDetached,
    HudFighterShown,
    HudNeocomWidth,
}
```

Add the arms to `key_path` (after `Category::NeocomButtons`):

```rust
            Category::HudFighterPos => &[b"ui", b"fightersDetachedPosition"],
            Category::HudBadge => &[b"notifications", b"notification_badge_offset"],
            Category::HudShipTop => &[b"ui", b"shipuialigntop"],
            Category::HudFighterDetached => &[b"ui", b"detachFighterUI"],
            Category::HudFighterShown => &[b"ui", b"displayFighterUI"],
            // Account-side `windows`, which holds only this key — a different
            // document from the char-side `windows` Category::Layout splices.
            Category::HudNeocomWidth => &[b"windows", b"neocomWidth"],
```

Add the predicate inside the same `impl Category` block:

```rust
    /// Whether an absent key on the SOURCE means "EVE's default" rather than
    /// "nothing to copy". True only for the leaf HUD keys: 851 of 3059 corpus
    /// account files store none of them, so treating absence as "leave the
    /// target alone" would half-apply a Layout copy on a quarter of accounts.
    /// Never true for a whole-section category — a source with no `overview`
    /// deleting the target's would be data loss, not a copy.
    pub fn absent_means_default(self) -> bool {
        matches!(
            self,
            Category::HudFighterPos
                | Category::HudBadge
                | Category::HudShipTop
                | Category::HudFighterDetached
                | Category::HudFighterShown
                | Category::HudNeocomWidth
        )
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p settings-model batch::tests::`
Expected: PASS, including the pre-existing category tests.

- [ ] **Step 5: Commit**

```bash
git add crates/settings-model/src/batch.rs
git commit -m "Add a category per HUD key

The HUD editor writes nine fields; the Layout aspect carries one. These
six categories address the other eight (the ship offset already rides
inside the char windows subtree), each pointing at a single key so a copy
cannot drag a whole ui section along with it.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: Absent on the source removes it on the target

**Files:**
- Modify: `crates/settings-model/src/batch.rs:48-83` (`extract_categories`, `apply_to_tree`)
- Modify: `app/src-tauri/src/ops.rs:533` (one closure return type)
- Test: `crates/settings-model/src/batch.rs` (`mod tests`)

**Interfaces:**
- Consumes: `Category::absent_means_default` from Task 1.
- Produces: `extract_categories(&Value, &[Category]) -> Vec<(Category, Option<Value>)>` where `None` means "requested, `absent_means_default`, source lacks it"; `apply_to_tree(&mut Value, &[(Category, Option<Value>)])` which deletes the key for a `None`. Tasks 3–5 consume both signatures.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/settings-model/src/batch.rs`:

```rust
/// A user doc holding the account-side HUD keys the copy cares about.
fn user_with_hud() -> Value {
    Value::Dict(vec![
        (b("ui"), Value::Dict(vec![(b("shipuialigntop"), Value::Bool(true))])),
        (b("windows"), Value::Dict(vec![(b("neocomWidth"), Value::Int(72))])),
    ])
}

#[test]
fn an_absent_leaf_hud_key_removes_the_targets_own_value() {
    // The source is at EVE's default (no key at all), so the target must end
    // up at the same default rather than keeping its own 72.
    let source = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
    let extracted = extract_categories(&source, &[Category::HudNeocomWidth]);
    assert_eq!(extracted.len(), 1, "the absence is reported, not dropped");
    assert!(extracted[0].1.is_none(), "absence is a removal");

    let mut target = user_with_hud();
    apply_to_tree(&mut target, &extracted);

    let Value::Dict(root) = &target else { panic!("root is a dict") };
    let (_, windows) = root.iter().find(|(k, _)| is_bytes(k, b"windows")).expect("windows survives");
    let Value::Dict(w) = windows else { panic!("windows is a dict") };
    assert!(
        !w.iter().any(|(k, _)| is_bytes(k, b"neocomWidth")),
        "the target's own neocomWidth is gone"
    );
}

#[test]
fn an_absent_whole_section_category_leaves_the_target_alone() {
    // The destructive case. A source with no overview must not wipe one.
    let source = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
    let extracted = extract_categories(&source, &[Category::Overview]);
    assert!(extracted.is_empty(), "a missing section is nothing to copy");

    let mut target = Value::Dict(vec![(b("overview"), Value::Int(7))]);
    apply_to_tree(&mut target, &extracted);
    let Value::Dict(root) = &target else { panic!("root is a dict") };
    assert!(root.iter().any(|(k, _)| is_bytes(k, b"overview")), "the target's overview survives");
}

#[test]
fn a_removal_with_no_parent_section_on_the_target_is_a_no_op() {
    let source = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
    let extracted = extract_categories(&source, &[Category::HudBadge]);
    let mut target = Value::Dict(vec![(b("keep"), Value::Int(1))]);
    apply_to_tree(&mut target, &extracted);
    let Value::Dict(root) = &target else { panic!("root is a dict") };
    assert!(root.iter().any(|(k, _)| is_bytes(k, b"keep")), "nothing else was touched");
}

#[test]
fn an_empty_root_source_contributes_neither_values_nor_removals() {
    // A Layout preset created before the aspect grew an account side has a
    // user.dat of `{}`. Applying it must not delete the target's HUD keys.
    let extracted = extract_categories(
        &Value::Dict(vec![]),
        &[Category::HudNeocomWidth, Category::HudShipTop],
    );
    assert!(extracted.is_empty(), "a pruned-away side carries no absences");

    let mut target = user_with_hud();
    apply_to_tree(&mut target, &extracted);
    let Value::Dict(root) = &target else { panic!("root is a dict") };
    let (_, windows) = root.iter().find(|(k, _)| is_bytes(k, b"windows")).expect("windows survives");
    let Value::Dict(w) = windows else { panic!("windows is a dict") };
    assert!(
        w.iter().any(|(k, _)| is_bytes(k, b"neocomWidth")),
        "an old preset leaves the target's neocom width alone"
    );
}

#[test]
fn a_present_leaf_hud_key_is_copied_over_the_targets_own() {
    let extracted = extract_categories(&user_with_hud(), &[Category::HudNeocomWidth]);
    let mut target = Value::Dict(vec![(
        b("windows"),
        Value::Dict(vec![(b("neocomWidth"), Value::Int(37))]),
    )]);
    apply_to_tree(&mut target, &extracted);
    let Value::Dict(root) = &target else { panic!("root is a dict") };
    let (_, windows) = root.iter().find(|(k, _)| is_bytes(k, b"windows")).expect("windows exists");
    let Value::Dict(w) = windows else { panic!("windows is a dict") };
    let (_, v) = w.iter().find(|(k, _)| is_bytes(k, b"neocomWidth")).expect("the key was copied");
    assert_eq!(*v, Value::Int(72), "the source's width won");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model batch::tests::an_absent_leaf`
Expected: FAIL — `extracted[0].1.is_none()` does not compile against `Vec<(Category, Value)>` (`no method named is_none` on `Value`).

- [ ] **Step 3: Change the two signatures**

Replace `extract_categories` in `crates/settings-model/src/batch.rs`:

```rust
/// Inline the source's sharing, then clone each requested category's subtree.
/// A category the source lacks is skipped — EXCEPT an `absent_means_default`
/// one, which is returned as `(cat, None)` so the splice removes the target's
/// own value. An absent leaf HUD key is EVE's default, not "nothing to copy".
pub fn extract_categories(source: &Value, cats: &[Category]) -> Vec<(Category, Option<Value>)> {
    let mut s = source.clone();
    inline_all(&mut s);
    let Value::Dict(root) = &s else { return Vec::new() };
    // An empty root is a preset side that was pruned away, never a real
    // settings file. It holds no values AND claims no absences: a Layout
    // preset created before the aspect grew an account side must not delete
    // the target's HUD keys. See the spec's §4.4.
    if root.is_empty() {
        return Vec::new();
    }
    cats.iter()
        .filter_map(|&cat| {
            let keys = cat.key_path();
            let (parent_keys, last) = keys.split_at(keys.len() - 1);
            let found = descend_ref(root, parent_keys)
                .and_then(|parent| parent.iter().find(|(k, _)| is_bytes(k, last[0])))
                .map(|(_, v)| v.clone());
            match found {
                Some(v) => Some((cat, Some(v))),
                None if cat.absent_means_default() => Some((cat, None)),
                None => None,
            }
        })
        .collect()
}
```

Replace `apply_to_tree`'s loop body in the same file:

```rust
/// Inline the target's sharing, then replace (or insert) each category's
/// subtree — or REMOVE it, for a `None` (see `extract_categories`).
/// A missing intermediate parent dict (e.g. no `ui`) skips that category.
pub fn apply_to_tree(target: &mut Value, extracted: &[(Category, Option<Value>)]) {
    inline_all(target);
    if let Value::Dict(root) = target {
        for (cat, subtree) in extracted {
            let keys = cat.key_path();
            let (parent_keys, last) = keys.split_at(keys.len() - 1);
            let Some(parent) = descend_mut(root, parent_keys) else { continue };
            match subtree {
                // The source is at EVE's default, so the target's own value
                // has to go — leaving it would half-apply the copy.
                None => parent.retain(|(k, _)| !is_bytes(k, last[0])),
                Some(subtree) => match parent.iter_mut().find(|(k, _)| is_bytes(k, last[0])) {
                    Some((_, v)) => *v = subtree.clone(),
                    None => parent.push((Value::Bytes(last[0].to_vec()), subtree.clone())),
                },
            }
        }
    }
    // Re-derive compact immutable-only sharing so the saved file is not the
    // ~1.5x fully-inlined blob (no reliance on EVE re-deduplicating).
    *target = blue_marshal::reshare(target);
}
```

Update `apply_categories_to`'s parameter type in the same file:

```rust
pub fn apply_categories_to(
    target: &Path,
    extracted: &[(Category, Option<Value>)],
) -> Result<SaveReport, String> {
```

- [ ] **Step 4: Fix the one compile break in the app crate**

`app/src-tauri/src/ops.rs:533` — the closure's annotated return type:

```rust
    let extract_side = |bytes: &[u8], cats: &[Category]| -> Result<Vec<(Category, Option<Value>)>, ErrDto> {
```

Nothing else in the app crate needs touching: `presets::prune` maps `(c, _)` and `has_category`/`source_side_empty` call `.is_empty()`, all of which still compile. Task 3 and Task 4 cover why two of those must *stay* as they are.

- [ ] **Step 5: Run the whole workspace**

Run: `cargo test --workspace`
Expected: PASS. The pre-existing `batch_realshape.rs` and `mutation_smoke.rs` cases forward the result straight to `apply_to_tree`, so they need no edit — if one fails to compile, it is inspecting the tuple and needs `.1` unwrapped with `.as_ref()`, not a signature revert.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/batch.rs app/src-tauri/src/ops.rs
git commit -m "Treat an absent leaf HUD key as EVE's default

An absent key IS the default, so a source that lacks one must clear the
target's rather than leave it: 851 of 3059 corpus account files store none
of the four account-side HUD keys, and leaving them alone would half-apply
a layout copy on all of them. Scoped by absent_means_default so a source
with no overview can never wipe the target's.

An empty root source contributes neither values nor removals, so a preset
captured before the aspect grew an account side still applies char-only.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: `has_category` answers about values, not absences

**Files:**
- Modify: `app/src-tauri/src/presets.rs:98-101`
- Test: `app/src-tauri/src/presets.rs` (`mod tests`)

**Interfaces:**
- Consumes: `extract_categories` returning `Option<Value>` from Task 2.
- Produces: `has_category` unchanged in signature, correct for leaf HUD categories.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `app/src-tauri/src/presets.rs`:

```rust
#[test]
fn has_category_reports_values_not_absences() {
    // extract_categories returns (cat, None) for a requested-but-absent leaf
    // HUD key, so an `.is_empty()` reading here would answer "yes, it has
    // one" for a document that has nothing.
    let doc = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
    assert!(!has_category(&doc, Category::HudShipTop), "an absent key is not a category the doc holds");

    let with_key = Value::Dict(vec![(
        b("ui"),
        Value::Dict(vec![(b("shipuialigntop"), Value::Bool(true))]),
    )]);
    assert!(has_category(&with_key, Category::HudShipTop), "a present key is");
}

#[test]
fn prune_builds_a_parent_for_a_requested_but_absent_hud_key() {
    // This is the mechanism behind a Layout preset never having an empty-root
    // user.dat: `present` maps over the (cat, None) entries too, so
    // parent_entries builds `ui` and `windows` for them. Filtering this to
    // Some would silently reintroduce the old-preset ambiguity.
    let source = Value::Dict(vec![(b("unrelated"), Value::Int(1))]);
    let out = prune(&source, &[Category::HudShipTop, Category::HudNeocomWidth]);
    assert!(!is_empty_root(&out), "a pruned Layout account side is never an empty root");
    let Value::Dict(root) = &out else { panic!("root is a dict") };
    for key in [b"ui".as_slice(), b"windows".as_slice()] {
        assert!(
            root.iter().any(|(k, _)| matches!(k, Value::Bytes(v) if v.as_slice() == key)),
            "the parent for {} was built",
            String::from_utf8_lossy(key)
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify one fails**

Run: `cargo test -p eve-settings-editor presets::tests::has_category_reports`
Expected: FAIL — `assert!(!has_category(...))` fires, because `.is_empty()` sees the `(cat, None)` entry.
`prune_builds_a_parent_for_a_requested_but_absent_hud_key` should already PASS; run it too and confirm, since it pins behaviour that must not change.

- [ ] **Step 3: Fix `has_category`**

```rust
/// Whether a document holds a VALUE under `cat`. An `absent_means_default`
/// category the document lacks comes back as `(cat, None)` — a removal
/// instruction, not something the document holds — so this cannot be an
/// `.is_empty()` check.
pub fn has_category(doc: &Value, cat: Category) -> bool {
    extract_categories(doc, &[cat]).iter().any(|(_, v)| v.is_some())
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p eve-settings-editor presets::`
Expected: PASS, including the existing preset create/prune cases.

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src/presets.rs
git commit -m "Make has_category answer about values, not absences

extract_categories now reports a requested-but-absent HUD key as a removal
instruction, which an is_empty() reading would count as the document
holding one. derive_aspects only passes whole-section categories today, so
this closes a footgun rather than a live bug.

Also pins prune's parent-building for an absent HUD key: that is what keeps
a Layout preset's user.dat off the empty-root shape an old preset has.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: Route the aspect

**Files:**
- Modify: `app/src-tauri/src/ops.rs:116-131` (the `Aspect::Layout` arm)
- Test: `app/src-tauri/src/ops.rs` (`mod tests`, alongside the existing `aspect_writes` cases at ~line 1825)

**Interfaces:**
- Consumes: the six categories (Task 1), `Option<Value>` extract/apply (Task 2).
- Produces: `aspect_writes(&[Aspect::Layout])` whose `char_categories` are `[Layout, NeocomButtons, HudFighterPos, HudBadge]` and whose `account_categories` are `[HudShipTop, HudFighterDetached, HudFighterShown, HudNeocomWidth]`, making `writes_account()` true for Layout.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `app/src-tauri/src/ops.rs`:

```rust
#[test]
fn layout_carries_the_whole_hud_across_both_files() {
    let w = aspect_writes(&[Aspect::Layout]);
    assert_eq!(
        w.char_categories,
        vec![
            Category::Layout,
            Category::NeocomButtons,
            Category::HudFighterPos,
            Category::HudBadge
        ]
    );
    assert_eq!(
        w.account_categories,
        vec![
            Category::HudShipTop,
            Category::HudFighterDetached,
            Category::HudFighterShown,
            Category::HudNeocomWidth
        ]
    );
    assert!(w.writes_account(), "layout writes the account file now");
    assert!(w.copies_char_geometry(), "the badge offset is absolute px, so the resolution warning must fire");
}

#[test]
fn a_layout_copy_leaves_every_hud_field_equal() {
    // Asserted through project_hud rather than raw keys: the projection is
    // what the HUD editor shows, so this is the user-visible claim.
    let w = aspect_writes(&[Aspect::Layout]);
    let (src_char, src_user) = (hud_char_doc(), hud_user_doc());

    let mut tgt_char = Value::Dict(vec![(b("windows"), Value::Dict(vec![]))]);
    let mut tgt_user = Value::Dict(vec![
        (b("ui"), Value::Dict(vec![(b("shipuialigntop"), Value::Bool(false))])),
        (b("windows"), Value::Dict(vec![(b("neocomWidth"), Value::Int(37))])),
    ]);
    settings_model::apply_to_tree(&mut tgt_char, &extract_categories(&src_char, &w.char_categories));
    settings_model::apply_to_tree(&mut tgt_user, &extract_categories(&src_user, &w.account_categories));

    let before = settings_model::project_hud(&src_char, Some(&src_user));
    let after = settings_model::project_hud(&tgt_char, Some(&tgt_user));
    for (b_entry, a_entry) in before.entries.iter().zip(after.entries.iter()) {
        assert_eq!(b_entry.name, a_entry.name, "same field order");
        assert_eq!(b_entry.value, a_entry.value, "{} did not come across", b_entry.name);
    }
}

#[test]
fn an_account_side_of_only_removals_is_not_suppressed_as_a_no_op() {
    // source_side_empty feeds setup_preview's no-op suppression. A source
    // storing none of the four account HUD keys yields four REMOVALS, which
    // is real work — counting only present values here would silently kill
    // the removal path and half-apply the copy again.
    let w = aspect_writes(&[Aspect::Layout]);
    let source_without_hud = Value::Dict(vec![(b("ui"), Value::Dict(vec![]))]);
    let extracted = extract_categories(&source_without_hud, &w.account_categories);
    assert_eq!(extracted.len(), 4, "four removals");
    assert!(!extracted.is_empty(), "so the side is not a no-op");
}
```

Add these fixtures next to the other test helpers in the same `mod tests`. `ops.rs`'s test module defines `ts()` locally inside several individual tests and has **no shared `b()`** — add one at module scope alongside the fixtures, matching the idiom used in `batch.rs` and `presets.rs`:

```rust
fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }
```


```rust
/// A char doc carrying the char-side half of the HUD.
fn hud_char_doc() -> Value {
    Value::Dict(vec![
        (b("windows"), Value::Dict(vec![(b("shipuialignleftoffset"), Value::Float(-1052.0))])),
        (
            b("ui"),
            Value::Dict(vec![(
                b("fightersDetachedPosition"),
                Value::Tuple(vec![Value::Int(326), Value::Int(54)]),
            )]),
        ),
        (
            b("notifications"),
            Value::Dict(vec![(
                b("notification_badge_offset"),
                Value::Tuple(vec![Value::Int(2519), Value::Int(131)]),
            )]),
        ),
    ])
}

/// An account doc carrying the account-side half.
fn hud_user_doc() -> Value {
    Value::Dict(vec![
        (
            b("ui"),
            Value::Dict(vec![
                (b("shipuialigntop"), Value::Bool(true)),
                (b("detachFighterUI"), Value::Bool(true)),
                (b("displayFighterUI"), Value::Bool(true)),
            ]),
        ),
        (b("windows"), Value::Dict(vec![(b("neocomWidth"), Value::Int(72))])),
    ])
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p eve-settings-editor ops::tests::layout_carries`
Expected: FAIL — `char_categories` is `[Layout, NeocomButtons]` and `account_categories` is empty.

- [ ] **Step 3: Route the six categories**

`app/src-tauri/src/ops.rs`, the `Aspect::Layout` arm of `aspect_writes`:

```rust
            Aspect::Layout => {
                char_categories.push(Category::Layout);
                char_categories.push(Category::NeocomButtons);
                // The char-side HUD keys. The ship offset needs no category:
                // it lives inside the `windows` subtree Category::Layout
                // already splices whole.
                char_categories.push(Category::HudFighterPos);
                char_categories.push(Category::HudBadge);
                // The account-side four. These are what make a layout copy
                // write the account file — and therefore change every other
                // character on it. EVE stores them per account; there is no
                // per-character form to carry instead.
                account_categories.push(Category::HudShipTop);
                account_categories.push(Category::HudFighterDetached);
                account_categories.push(Category::HudFighterShown);
                account_categories.push(Category::HudNeocomWidth);
            }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p eve-settings-editor ops::`
Expected: PASS for the new cases. **Existing `plan_setup` tests using `Aspect::Layout` will now fail** — `layout` writes the account file, so an unpaired target is excluded and an unpaired source errors. That is the intended behaviour change (spec §3.2). Update each failing case to pair its characters, and add:

```rust
#[test]
fn a_layout_copy_excludes_an_unpaired_target() {
    let (cp, up) = paths_2chars();
    let plan = plan_setup(&cp, &up, &store_2accounts(), &HashMap::new(), Some(3), &[1, 4], &[Aspect::Layout]);
    assert!(
        plan.excluded.iter().any(|e| e.char_id == 4 && e.reason.contains("No account paired")),
        "an unpaired target cannot receive the account-side HUD fields"
    );
}
```

Reuse whichever `paths_2chars` / `store_2accounts` helpers the neighbouring `plan_setup` tests already use — do not invent new fixtures; check char id 4's pairing in the existing store and pick an unpaired id if 4 is paired.

- [ ] **Step 5: Run the whole workspace**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add app/src-tauri/src/ops.rs
git commit -m "Carry the whole HUD in the Window layout aspect

Layout carried one of the HUD editor's nine fields, so copying it moved a
character's ship HUD offset and left the fighter panel, the notification
badge and the neocom width behind. It now carries all nine, which makes it
write the account file for the first time — so an unpaired target is
excluded exactly as it is for Overview and Autofill.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 5: The two UI surfaces stop describing the old behaviour

**Files:**
- Modify: `app/src/lib/BatchView.svelte:70`
- Modify: `app/src/lib/PresetGroup.svelte:16-30`, `app/src/lib/PresetGroup.svelte:179`
- Test: `app/src/lib/BatchView.spec.ts`; **create** `app/src/lib/PresetGroup.spec.ts` — the component has no spec file today, so copy the render/mock scaffolding from `BatchView.spec.ts` (same view layer, same `api` mocking idiom) rather than inventing one

**Interfaces:**
- Consumes: the routing from Task 4 (the backend now refuses what these surfaces now prevent).
- Produces: no new exports — `ASPECTS` entries change shape only in `PresetGroup` (the optional `note` field goes away with its last user).

- [ ] **Step 1: Write the failing tests**

In `app/src/lib/BatchView.spec.ts`, following the file's existing render/helper conventions:

```ts
test("a layout-only selection warns that the account file is written", async () => {
  // Layout carries the account-side HUD fields now, so it must disable
  // unpaired targets and surface the collateral-character warning the way
  // every other account aspect does.
  const { aspect, targetRow } = await renderBatch();
  await fireEvent.click(aspect("Window layout"));
  await waitFor(() => expect(targetRow(UNPAIRED_CHAR_ID).disabled).toBe(true));
});
```

In the new `app/src/lib/PresetGroup.spec.ts`:

```ts
test("creating a layout preset needs the account file open", async () => {
  const { aspect } = await renderPresets({ userOpen: false });
  await waitFor(() => expect(aspect("Window layout").disabled).toBe(true));
});
```

Match the surrounding tests' actual helper names and render signatures — read each spec file first and reuse what is there rather than importing the names above literally.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd app && npm test`
Expected: FAIL — the layout aspect is `account: false` / `needsUser: false`, so nothing is disabled.

- [ ] **Step 3: Update `BatchView.svelte`**

Line 70 becomes:

```svelte
    { key: "layout", label: "Window layout (positions, neocom, ship HUD, fighter panel, badge)", account: true },
```

The label doubles as the short name in the account-write warning (`changedAspectNames`, line 164), which is why it stays a single phrase — same shape as the Overview and Autofill entries already there.

- [ ] **Step 4: Update `PresetGroup.svelte`**

Replace the comment and the layout entry (lines 16-25):

```svelte
  const ASPECTS: { key: Aspect; label: string; needsUser: boolean }[] = [
    { key: "layout", label: "Window layout", needsUser: true },
```

`note` was carried by the layout entry alone, so drop the field from the type (done above) and its use in the markup at line 179:

```svelte
        <label
          class:disabled={(everything && a.key !== "everything") || (a.needsUser && !userOpen)}>
```

- [ ] **Step 5: Run the tests and the type check**

Run: `cd app && npm test && npm run check`
Expected: PASS both. `npm run check` is what catches a leftover `a.note` reference.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/BatchView.svelte app/src/lib/PresetGroup.svelte app/src/lib/BatchView.spec.ts app/src/lib/PresetGroup.spec.ts
git commit -m "Say that Window layout now carries the whole HUD

Both surfaces described the old half: the batch label named what it left
behind and the preset checkbox carried the same caveat on hover. Neither is
true any more, and both surfaces now treat layout as the account-writing
aspect it has become.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 6: Real-shape coverage on corpus-shaped documents

**Files:**
- Modify: `crates/settings-model/tests/batch_realshape.rs`
- Test: same file

**Interfaces:**
- Consumes: everything from Tasks 1-4.
- Produces: no code — a gate proving the categories work on the `(timestamp, dict)` wrappers real files use rather than only on the bare dicts the unit tests build.

- [ ] **Step 1: Write the failing test**

Read the existing cases in `crates/settings-model/tests/batch_realshape.rs` first — they build documents in the real wrapped shape and are the pattern to follow. Add:

```rust
#[test]
fn the_hud_keys_survive_the_timestamped_wrapper_real_files_use() {
    // Real sections are `(timestamp, dict)`, not bare dicts. descend_ref and
    // descend_mut unwrap that via dict_inner; this pins that they do for the
    // new leaf categories too, on both the read and the write side.
    let source = wrapped_user_doc_with_neocom_width(72);
    let mut target = wrapped_user_doc_with_neocom_width(37);
    let extracted = extract_categories(&source, &[Category::HudNeocomWidth]);
    apply_to_tree(&mut target, &extracted);
    assert_eq!(neocom_width_of(&target), Some(72), "the source's width landed through the wrapper");
}

#[test]
fn a_removal_reaches_through_the_timestamped_wrapper_too() {
    let source = wrapped_user_doc_without_neocom_width();
    let mut target = wrapped_user_doc_with_neocom_width(37);
    let extracted = extract_categories(&source, &[Category::HudNeocomWidth]);
    apply_to_tree(&mut target, &extracted);
    assert_eq!(neocom_width_of(&target), None, "the target fell back to EVE's default");
}
```

Write the three helpers (`wrapped_user_doc_with_neocom_width`, `wrapped_user_doc_without_neocom_width`, `neocom_width_of`) in the same file, wrapping the `windows` section as `Value::Tuple(vec![ts(), Value::Dict(...)])` to match the file's existing fixtures. `neocom_width_of` returns `Option<i64>` and must unwrap the tuple the same way, so a fixture that silently lost its wrapper cannot pass.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p settings-model --test batch_realshape the_hud_keys_survive`
Expected: FAIL to compile until the helpers exist; then PASS once they do — if the *first* test fails on values, `dict_inner` is not being reached and Task 2's `descend_*` calls are wrong.

- [ ] **Step 3: Run the whole workspace**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/settings-model/tests/batch_realshape.rs
git commit -m "Gate the HUD categories on the shape real files carry

The unit tests build bare dicts; every real section is a (timestamp, dict)
tuple. A fixture that shares the code's assumptions cannot falsify them,
which is how the chat-name and column-wrapper bugs both survived their
tests.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 7: Close it out — changelog, ledger, live smoke

**Files:**
- Modify: `CHANGELOG.md` (the `## [Unreleased]` section)
- Modify: `docs/small-tasks.md` (the open Layout-aspect entry, ~line 75)

**Interfaces:**
- Consumes: the finished feature.
- Produces: nothing code-facing.

- [ ] **Step 1: Run the full verification**

Run, from the repo root: `cargo test --workspace`
Then from `app/`: `npm test && npm run check && npm run build`
Expected: all green. Record the actual output; do not write the changelog off an assumption that they passed.

- [ ] **Step 2: Write the changelog entry**

Under `## [Unreleased]`, in the house voice (what changed for a player, not what changed in the code):

```markdown
### Changed
- **"Window layout" now carries your whole screen, not half of it.** It moved
  window positions, the neocom and the ship HUD offset, but left the fighter
  panel, the notification badge and the neocom width behind — so a copy landed
  a character somewhere between their own layout and the one you gave them.
  All nine settings now travel together. Two consequences worth knowing: the
  copy writes the account file, so **every character on the target's account**
  gets the neocom width and fighter-UI toggles (the preview names them, as it
  already did for Overview and Autofill), and a character with no paired
  account can no longer receive a layout copy — pair it in the Accounts view
  first. Where the source is at EVE's default, the target is reset to that
  default rather than keeping its own value, which is what makes the two
  characters actually match.
```

- [ ] **Step 3: Close the ledger entry**

In `docs/small-tasks.md`, move the **"Decide what the Layout aspect should mean, then make it carry that"** entry from Open into `## Shipped` → `### Unreleased (on master)`, flipping `- [ ]` to `- [x]`, and append what was decided and what it cost — including the correction that the aspect carried **one** of nine, not the two the entry claimed, because `neocomWidth` is in the account file rather than the character file's `windows`.

- [ ] **Step 4: Run the live smoke**

This branch changes what gets written to a real player's files, so it does not merge without it. Log the results into `docs/live-verification-session-c.md` (or a new session file) in the same style as the existing entries:

1. Copy Layout A1 → A2 in-game, log in as A2, confirm all nine fields land.
2. Confirm a third character on A2's account sees the account-side four.
3. Copy from a source storing none of the four onto a target that stores them, and confirm the target comes up at EVE's defaults — the removal path, which no offline test can prove the client honours.

Note the write order: the editor saves on demand, EVE writes its settings on **logout**, so log the character out before saving or the client overwrites it on exit.

- [ ] **Step 5: Commit**

```bash
git add CHANGELOG.md docs/small-tasks.md docs/live-verification-session-c.md
git commit -m "Record the Layout aspect change and its live verification

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Self-review

**Spec coverage:** §3.1 → Task 4. §3.2 → Task 4 (the exclusion test) and Task 5 (the UI disabling that matches it). §3.3 → Task 2. §4.1 → Task 1. §4.2 → Task 4. §4.3 → Task 2, with its three call sites split across Tasks 2 (the compile fix), 3 (`has_category`) and 4 (the `source_side_empty` pin). §4.4 → Task 2. §4.5 → Task 3. §4.6 → Task 5. §4.7 → Task 4 asserts `copies_char_geometry`; the rest is verified-unchanged and needs no task. Spec tests 1-15 → Tasks 1, 2, 3, 4, 6 (Rust), 5 (frontend), 7 (live).

**Types:** `extract_categories -> Vec<(Category, Option<Value>)>` and `apply_to_tree(&mut Value, &[(Category, Option<Value>)])` are used consistently in Tasks 2, 3, 4 and 6. `absent_means_default(self) -> bool` is defined in Task 1 and consumed in Task 2. Variant names match the spec's table throughout.

**Known soft spots, called out rather than papered over:** Task 4 Step 4 and Task 5 Step 1 depend on fixture and helper names in files the implementer must read first — both say so explicitly instead of inventing names that may not exist. Task 6's helpers are described rather than written for the same reason: the existing file's wrapper idiom is the thing to copy.
