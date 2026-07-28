# Wrapped Tab Fixtures and the Backed-Out Rewrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the `rewrap` repair on `overview_tabs`'s two container writers, which was written during the live session and backed out because 14 test fixtures encode a shape EVE never writes.

**Architecture:** Move the `overview_tabs.rs` fixtures from a bare `tabsettings_new` dict to the `(timestamp, dict)` shape every real file uses, which unblocks adding the same repair `overview_pack::put` already performs to `tabs_mut` and `groups_mut`. Fixtures first, repair second — in the other order the repair fails the suite exactly as it did before.

**Tech Stack:** Rust (`settings-model`), `cargo test`.

## Global Constraints

- **No new dependencies**, no signature changes, no behaviour change for files that are already correctly shaped.
- **`dict_inner_mut` already tolerates both shapes on read.** This is about what gets *written*, not what can be read. Nothing may start rejecting a bare payload — it must be repaired, never refused.
- Run the Rust suite with `cargo test` from the repo root.
- Commit after each task.

---

## Background: the entry is half done, and the other half is a trap

The ledger entry names two fixture problems. **One is already fixed, and its "bad" fixture is now load-bearing — do not touch it.**

`overview_pack.rs::user_doc()` still seeds a bare `overviewColumnOrder` (line ~1130) and that is now **deliberate**. It is the input for two tests that pin the repair:

- `apply_pack_rewraps_a_bare_payload` — "the fixture's bare `overviewColumnOrder` must come back wrapped"
- `apply_pack_wraps_every_list_section` — whose doc comment records the exact history the ledger entry describes: *"This used to assert the opposite on the strength of a comment, with a fixture that seeded a bare list, so it passed while every real import stripped the wrapper off both column keys."*

`overview_pack::put` already does the repair:

```rust
// A bare payload is not a shape the client writes — 0 of 4,187 container keys
// across five untouched account files. One can only be here because an older
// build of this editor stripped the wrapper, so restore it instead of
// perpetuating it.
other => *other = Value::Tuple(vec![Value::Long(vec![0u8; 8]), value]),
```

**So `overview_pack.rs` is finished. Wrapping its fixture would delete the only coverage of its repair.** Leave that file alone entirely.

What remains is the `overview_tabs.rs` side: its fixtures build a bare `tabsettings_new`, and `tabs_mut` / `groups_mut` have no equivalent repair.

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/settings-model/src/overview_tabs.rs` | Tab/window structural writers + their tests | Modify: fixtures to the wrapped shape, then the repair |
| `docs/small-tasks.md` | The ledger | Modify: close the entry, noting the pack half was already done |

---

### Task 1: Move the tab fixtures to the shape EVE writes

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs` (`mod tests` fixtures only — no production code in this task)

**Interfaces:**
- Consumes / produces: nothing. This task must be **behaviour-neutral**: every test that passes before must pass after, unchanged.

There are roughly seven fixture sites building `tabsettings_new` as a bare `Value::Dict`. Find them with:

```bash
grep -n 'tabsettings_new' crates/settings-model/src/overview_tabs.rs
```

- [ ] **Step 1: Wrap each fixture**

For each fixture site, change the bare dict to the wrapped shape used by every real file and by this module's own `groups_mut`:

```rust
// before
(Value::Bytes(b"tabsettings_new".to_vec()), Value::Dict(vec![(Value::Int(0), tab)])),

// after
(Value::Bytes(b"tabsettings_new".to_vec()), Value::Tuple(vec![
    Value::Long(vec![0u8; 8]),
    Value::Dict(vec![(Value::Int(0), tab)]),
])),
```

The module's test helpers already unwrap both shapes (`dict_inner_ref` delegates to `dict_inner`, which handles `Dict` and `Tuple`), so most assertions need no change. **Where one does**, it is because it destructured `Value::Dict` directly — route it through the existing unwrap helper rather than teaching it about tuples inline.

- [ ] **Step 2: Run the suite — nothing should have changed**

Run from the repo root: `cargo test -p settings-model`
Expected: PASS, with the same test count as before. This task changes no production code, so any failure is a fixture that was asserting the bare shape; fix the assertion to use the unwrap helper, and note it in your report.

- [ ] **Step 3: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs
git commit -m "Build the tab fixtures in the shape EVE actually writes"
```

---

### Task 2: Repair a bare payload instead of perpetuating it

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs` (`tabs_mut`, `groups_mut`)

**Interfaces:**
- Consumes: nothing new.
- Produces: no signature change. `tabs_mut` and `groups_mut` keep returning `&mut Entries` / `&mut Vec<Value>`; a bare container they are asked to write through is rewrapped first.

This is the repair that was written during the live session and backed out because Task 1's fixtures failed it.

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block, mirroring `overview_pack.rs`'s `apply_pack_rewraps_a_bare_payload`:

```rust
    /// A bare payload is not a shape the client writes — 0 of 4,187 container
    /// keys across five untouched account files. One can only be there because
    /// an older build of this editor stripped the wrapper, so an edit that
    /// passes through must restore it rather than perpetuate it.
    #[test]
    fn editing_tabs_rewraps_a_bare_tabsettings() {
        let tab = Value::Dict(vec![(Value::Str("name".into()), Value::StrUcs2("A".into()))]);
        let mut v = Value::Dict(vec![(b("overview"), Value::Dict(vec![
            // Deliberately bare, as an older build would have left it.
            (b("tabsettings_new"), Value::Dict(vec![(Value::Int(0), tab)])),
        ]))]);
        rename_tab(&mut v, 0, "B").unwrap();

        let Value::Dict(top) = &v else { panic!() };
        let (_, ov) = top.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(entries) = ov else { panic!() };
        let (_, slot) = entries.iter().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
        let Value::Tuple(items) = slot else {
            panic!("a bare tabsettings_new must come back wrapped, got {slot:?}");
        };
        assert!(matches!(items[0], Value::Long(_)), "the wrapper leads with a timestamp");
    }

    /// The same for the window-groups writer.
    #[test]
    fn editing_window_groups_rewraps_a_bare_mapping() {
        let tab = Value::Dict(vec![(Value::Str("name".into()), Value::StrUcs2("A".into()))]);
        let mut v = Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("tabsettings_new"), Value::Tuple(vec![ts(), Value::Dict(vec![(Value::Int(0), tab)])])),
            // Deliberately bare.
            (b("tabsByWindowInstanceID"), Value::List(vec![Value::List(vec![Value::Int(0)])])),
        ]))]);
        reorder_tabs_in_window(&mut v, 0, &[0]).unwrap();

        let Value::Dict(top) = &v else { panic!() };
        let (_, ov) = top.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(entries) = ov else { panic!() };
        let (_, slot) = entries.iter().find(|(k, _)| is_b(k, b"tabsByWindowInstanceID")).unwrap();
        let Value::Tuple(items) = slot else {
            panic!("a bare tabsByWindowInstanceID must come back wrapped, got {slot:?}");
        };
        assert!(matches!(items[0], Value::Long(_)), "the wrapper leads with a timestamp");
    }

    /// An EXISTING wrapper's own timestamp must survive — the repair is for a
    /// missing wrapper, not an excuse to reset a real one to zero.
    #[test]
    fn rewrapping_never_resets_an_existing_timestamp() {
        let stamp = Value::Long(vec![7u8; 8]);
        let tab = Value::Dict(vec![(Value::Str("name".into()), Value::StrUcs2("A".into()))]);
        let mut v = Value::Dict(vec![(b("overview"), Value::Dict(vec![
            (b("tabsettings_new"), Value::Tuple(vec![stamp.clone(), Value::Dict(vec![(Value::Int(0), tab)])])),
        ]))]);
        rename_tab(&mut v, 0, "B").unwrap();

        let Value::Dict(top) = &v else { panic!() };
        let (_, ov) = top.iter().find(|(k, _)| is_b(k, b"overview")).unwrap();
        let Value::Dict(entries) = ov else { panic!() };
        let (_, slot) = entries.iter().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
        let Value::Tuple(items) = slot else { panic!() };
        assert_eq!(items[0], stamp, "an existing timestamp must not be reset to zero");
    }
```

Use whatever short helpers the module's test block already defines for byte keys and timestamps (`b(..)`, `ts()` or equivalents) rather than introducing new ones — check the block first and adapt.

- [ ] **Step 2: Run the tests to verify they fail**

Run from the repo root: `cargo test -p settings-model --lib rewrap`
Expected: FAIL on the two rewrap tests — the bare payload stays bare. `rewrapping_never_resets_an_existing_timestamp` should PASS already; it is the guard on the fix, not a demonstration of the bug. Confirm it can fail by checking it after Step 3 too.

- [ ] **Step 3: Write the implementation**

In `crates/settings-model/src/overview_tabs.rs`, add a helper beside `dict_inner_mut`:

```rust
/// Restore the `(timestamp, payload)` wrapper on a container key that lost it.
///
/// A bare payload is not a shape the client writes — 0 of 4,187 container keys
/// across five untouched account files. One can only be there because an older
/// build of this editor stripped the wrapper, so a write passing through here
/// repairs it rather than perpetuating it. Mirrors `overview_pack::put`.
///
/// An existing wrapper is left alone, timestamp and all: the repair is for a
/// MISSING wrapper, and resetting a real timestamp to zero would be a different
/// kind of damage.
fn rewrap(slot: &mut Value) {
    if matches!(slot, Value::Dict(_) | Value::List(_)) {
        let inner = std::mem::replace(slot, Value::None);
        *slot = Value::Tuple(vec![Value::Long(vec![0u8; 8]), inner]);
    }
}
```

Then call it in both writers, immediately after locating the slot and before unwrapping it. In `tabs_mut`, after the `find` that resolves `tabsettings_new`:

```rust
    let (_, v) = ov.iter_mut().find(|(k, _)| is_b(k, b"tabsettings_new")).unwrap();
    rewrap(v);
    dict_inner_mut(v).expect("tabsettings_new is a dict or (ts,dict)")
```

And the same in `groups_mut`, after its `find` for `tabsByWindowInstanceID` and before `list_inner_mut`.

- [ ] **Step 4: Run the tests to verify they pass**

Run from the repo root: `cargo test -p settings-model`
Expected: PASS, all of it. Task 1 is what makes this possible — if fixtures still fail here, one was missed.

- [ ] **Step 5: Run the whole suite**

Run from the repo root: `cargo test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/overview_tabs.rs
git commit -m "Repair a bare tab container instead of perpetuating it"
```

---

### Task 3: Close the ledger entry

**Files:**
- Modify: `docs/small-tasks.md`

**Interfaces:** none — documentation only.

- [ ] **Step 1: Move the entry to Shipped**

Delete the `- [ ] **Test fixtures encode bare container payloads, a shape EVE never writes.**` entry from **Open** and add under `### Unreleased (on master)`:

```markdown
- [x] **Test fixtures encode bare container payloads, a shape EVE never writes.**
  Half of this was already done and the entry did not know it: `overview_pack`'s
  repair shipped, and its `user_doc()` fixture's bare `overviewColumnOrder` is now
  *deliberate* — it is the input for `apply_pack_rewraps_a_bare_payload`, and
  `apply_pack_wraps_every_list_section` records the same history the entry
  describes. Wrapping that fixture would have deleted the only coverage of the
  repair, so it was left alone.

  The `overview_tabs` side is what remained. Its fixtures now build
  `tabsettings_new` in the `(timestamp, dict)` shape every real file uses, which
  unblocked the `rewrap` repair that was written during the live session and
  backed out because those fixtures failed it. `tabs_mut` and `groups_mut` now
  restore a missing wrapper the way `overview_pack::put` does — and leave an
  existing timestamp alone, which has its own test, because resetting a real one
  to zero would be a different kind of damage. _Added 2026-07-27; done
  2026-07-28._
```

- [ ] **Step 2: Commit**

```bash
git add docs/small-tasks.md
git commit -m "Close the bare-payload fixture task"
```

---

## Self-review notes

- **Ordering is the whole point.** Task 1 before Task 2 is not cosmetic: reversing them reproduces exactly the failure that caused the repair to be backed out in the first place.
- **The trap.** `overview_pack.rs` is explicitly out of scope. Its bare fixture looks like the defect this entry describes and is in fact the fix's test input.
- **Read, don't refuse.** `dict_inner_mut` keeps tolerating both shapes. Nothing here should make a bare payload unreadable — a file written by an older build must still open.
- **Not in scope.** Any other fixture in any other module, and the `(timestamp, payload)` convention elsewhere in the codebase.
