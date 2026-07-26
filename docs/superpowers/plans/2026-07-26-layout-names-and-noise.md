# Layout names and noise Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show EVE's own names for chat windows and window stacks instead of
ours, let the user override what counts as clutter, and persist that in a new
`preferences.json` — plus the layout debt sweep the ledger has accumulated.

**Architecture:** One backend change carries both names: `window_layout` gains
an optional account root and reads two sections it currently ignores (character
`ui → chatchannels`, account `tabgroups`). The shared-aware section resolver
that both need is lifted out of `hud.rs` into `treewalk` — with the
`Step::SharedInner` bug the ledger records fixed on the way — so there is one
copy, not three. Preferences are a new `app/src-tauri/src/prefs.rs` reading and
writing a JSON file in the app config dir, with the clutter override as its
first tenant; the frontend passes overrides into `isClutter` explicitly rather
than smuggling them into `WindowFilter`.

**Tech Stack:** Rust (`settings-model`, dependency-free; `app/src-tauri` with
`serde`/`serde_json`/`tauri`, all already present), SvelteKit 5 runes +
TypeScript, `node --test` for pure frontend tests, `cargo test` for Rust.

**Spec:** `docs/superpowers/specs/2026-07-26-layout-names-and-noise-design.md`

## Global Constraints

- **No new dependencies**, frontend or backend. `serde`, `serde_json` and
  `tauri` are already in `app/src-tauri/Cargo.toml`; `crates/settings-model` and
  `crates/blue-marshal` stay **dependency-free** — nothing new may be added to
  their `Cargo.toml`.
- **`crates/settings-model` never gains a `serde_json` or `tauri` dependency.**
  Preferences live in `app/src-tauri`, not in the model crate.
- **Nothing in this slice writes to an EVE settings file.** The clutter override
  is editor state in the editor's own file. No mutation, no reshare, no backup.
- **Account-file gotcha:** in account files the root section key is a `Ref`, so
  `is_bytes`/`child_dict` miss it. Section keys must resolve through
  `effective`. Use the `treewalk::section` helper from Task 1 — do not hand-roll
  a second resolver.
- **The character root and the account root need SEPARATE `SharedTable`s.**
  Shared slot numbers are per-document; resolving one document's `Ref` against
  the other's table yields the wrong node.
- **Frontend tests are throw-based and framework-free** (a local
  `check(name, ok)` that throws), matching `layout.test.ts` and
  `windowLabels.test.ts`. No `describe`/`it`.
- **`describe(id)` stays pure and unchanged.** It is the fallback path and its
  existing tests must keep passing untouched.
- **Commit style:** sentence-case imperative subject, **no attribution
  trailers**. Match `git log --oneline`.
- **Commands:** frontend from `app/` in **PowerShell** (`npm` is not on the Bash
  tool's PATH here); Rust from the repo root, also PowerShell.
- After `cargo test`, be aware `target/` regrows to several GB on a
  near-full C: drive. Do not run `cargo clean` mid-plan — the final task says
  when.

---

## File Structure

**Backend**
- `crates/settings-model/src/treewalk.rs` — **modify.** Gains `section`, `text`
  and `hex` as `pub(crate)`, shared by `hud.rs` and `windows.rs`.
- `crates/settings-model/src/hud.rs` — **modify.** Drops its private `section`
  and `hex`, uses `treewalk`'s. Sweep fixes (`locate`, `mint`).
- `crates/settings-model/src/windows.rs` — **modify.** `window_layout` takes the
  optional account root; two new private readers; `WindowRect.name`;
  `Stack.container_label` from the account file.
- `crates/settings-model/src/overview_tabs.rs` — **modify.** One named const.
- `app/src-tauri/src/prefs.rs` — **create.** The preferences file: types,
  `load_from`/`save_to`, and their tests.
- `app/src-tauri/src/ops.rs` — **modify.** `window_layout` passes the account
  document; two preference ops.
- `app/src-tauri/src/lib.rs` — **modify.** `mod prefs;`, two commands, and their
  registration in `generate_handler!`.

**Frontend**
- `app/src/lib/api.ts` — **modify.** `WindowRect.name`, two command bindings.
- `app/src/lib/windowLabels.ts` — **modify.** `nameOf`, `isClutter` overrides.
- `app/src/lib/prefs.svelte.ts` — **create.** Loads preferences once, writes
  through, exposes the override sets.
- `app/src/lib/layout.ts` — **modify.** `windowMatches`/`visibleIds` take the
  overrides.
- `app/src/lib/WindowPanel.svelte` — **modify.** Renders real names; two context
  menu items; the reorder-button edge case.
- `app/src/lib/LayoutView.svelte` — **modify.** Renders real names; threads
  overrides into the filter; the `· N overridden · clear` counter clause.
- `app/src/lib/windowLabels.test.ts`, `app/src/lib/layout.test.ts` — **modify.**

---

## Task 1: One shared-aware section resolver in `treewalk`

Lifts three helpers into one place and fixes a path bug on the way. Pure
refactor plus one behaviour fix — no feature depends on it yet, which is why it
is first and separately reviewable.

**Files:**
- Modify: `crates/settings-model/src/treewalk.rs`
- Modify: `crates/settings-model/src/hud.rs` (its private `section` and `hex`)
- Modify: `crates/settings-model/src/windows.rs` (its private `hex`)
- Test: `crates/settings-model/src/treewalk.rs` (a `#[cfg(test)] mod tests` at
  the end of the file if none exists; otherwise append to it)

**Interfaces:**
- Consumes: existing `treewalk` internals — `effective`, `is_bytes`,
  `unwrap_shared`, `SharedTable`, `Entries`, `NodePath`, `Step`.
- Produces, for Tasks 2 and 6:
  - `pub(crate) fn section<'a>(root: &'a Value, name: &[u8], shared: &SharedTable<'a>) -> Option<(&'a Entries, NodePath)>`
  - `pub(crate) fn text<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<String>`
  - `pub(crate) fn hex(bytes: &[u8]) -> String`

- [ ] **Step 1: Write the failing test**

Append to `crates/settings-model/src/treewalk.rs`. If the file already has a
`#[cfg(test)] mod tests`, add these two functions inside it instead of creating
a second module.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use blue_marshal::Value;

    fn b(s: &str) -> Value { Value::Bytes(s.as_bytes().to_vec()) }

    #[test]
    fn section_finds_a_plain_top_level_dict() {
        let root = Value::Dict(vec![
            (b("windows"), Value::Dict(vec![(b("a"), Value::Int(1))])),
            (b("ui"), Value::Dict(vec![(b("chatchannels"), Value::Int(2))])),
        ]);
        let sh = SharedTable::new();
        let (entries, path) = section(&root, b"ui", &sh).expect("ui section");
        assert_eq!(entries.len(), 1);
        assert_eq!(path, vec![Step::DictValue(1)]);
    }

    #[test]
    fn section_sees_through_a_shared_root_and_records_the_step() {
        // A Shared-wrapped root is what an account file looks like. The old
        // hud.rs copy resolved the VALUE but never pushed Step::SharedInner,
        // so every path it returned was wrong by one hop and resolve_mut
        // failed on it.
        let inner = Value::Dict(vec![(b("tabgroups"), Value::Dict(vec![(b("76_names"), b("Character: Information"))]))]);
        let root = Value::Shared { slot: 1, value: Box::new(inner) };
        let mut sh = SharedTable::new();
        collect_shared(&root, &mut sh);
        let (entries, path) = section(&root, b"tabgroups", &sh).expect("tabgroups section");
        assert_eq!(entries.len(), 1);
        assert_eq!(path.first(), Some(&Step::SharedInner), "the Shared hop must be in the path");
        assert_eq!(path.last(), Some(&Step::DictValue(0)));
    }

    #[test]
    fn section_resolves_a_ref_wrapped_section_key() {
        // Account files store the root section KEY as a Ref — is_bytes alone
        // misses it, which is the gotcha this helper exists to hide.
        let key = Value::Shared { slot: 7, value: Box::new(b("tabgroups")) };
        let root = Value::Dict(vec![
            (key, Value::Dict(vec![(b("76_names"), b("Character: Information"))])),
            (Value::Ref(7), Value::Dict(vec![(b("x"), Value::Int(1))])),
        ]);
        let mut sh = SharedTable::new();
        collect_shared(&root, &mut sh);
        // The SECOND entry's key is a Ref to "tabgroups"; the finder must match
        // the first (a Shared wrapping the same bytes) and stop there.
        let (entries, _) = section(&root, b"tabgroups", &sh).expect("tabgroups section");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn text_reads_every_string_shape_and_refuses_others() {
        let sh = SharedTable::new();
        assert_eq!(text(&b("Local"), &sh).as_deref(), Some("Local"));
        assert_eq!(text(&Value::Str("Local".into()), &sh).as_deref(), Some("Local"));
        assert_eq!(text(&Value::StrUcs2("Local".into()), &sh).as_deref(), Some("Local"));
        assert_eq!(text(&Value::Int(3), &sh), None);
    }

    #[test]
    fn hex_renders_lowercase_two_digit_bytes() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (PowerShell, from the repo root):

```powershell
cargo test -p settings-model treewalk
```

Expected: FAIL to compile — `cannot find function 'section' in this scope` (and
the same for `text`).

- [ ] **Step 3: Add the three helpers to `treewalk.rs`**

Append to `crates/settings-model/src/treewalk.rs`, above the test module:

```rust
/// Resolve a top-level section of a document root by name.
///
/// Hides three things callers keep getting wrong: the root itself may be
/// `Shared` (and the hop MUST appear in the returned path, or `resolve_mut`
/// fails on it), the section key may be a `Ref`/`Shared` rather than plain
/// `Bytes` (account files store it that way), and the section value may be
/// `Shared` too.
pub(crate) fn section<'a>(
    root: &'a Value,
    name: &[u8],
    shared: &SharedTable<'a>,
) -> Option<(&'a Entries, NodePath)> {
    let (root, base) = unwrap_shared(root, Vec::new());
    let Value::Dict(entries) = effective(root, shared) else { return None };
    let (i, (_, v)) = entries
        .iter()
        .enumerate()
        .find(|(_, (k, _))| is_bytes(effective(k, shared), name))?;
    let mut p = base;
    p.push(Step::DictValue(i));
    let (v, p) = unwrap_shared(v, p);
    match v {
        Value::Dict(d) => Some((d, p)),
        _ => None,
    }
}

/// A value's text, whatever string shape the client stored it in.
pub(crate) fn text<'a>(v: &'a Value, sh: &SharedTable<'a>) -> Option<String> {
    match effective(v, sh) {
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Str(s) | Value::StrUcs2(s) => Some(s.clone()),
        _ => None,
    }
}

/// Lowercase hex, for rendering a non-UTF8 key as a stable id.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

- [ ] **Step 4: Point `hud.rs` and `windows.rs` at them**

In `crates/settings-model/src/hud.rs`: delete its private `fn section` (the one
starting `fn section<'a>(root: &'a Value, name: &[u8], shared: &SharedTable<'a>)`)
and its private `fn hex` (around line 269), and add both names to the existing
`use crate::treewalk::{...}` import list.

In `crates/settings-model/src/windows.rs`: delete its private `fn hex` (around
line 338) and add `hex` to its existing `use crate::treewalk::{...}` import
list.

Do not change any call site — the signatures are identical.

- [ ] **Step 5: Run the full model suite**

```powershell
cargo test -p settings-model
```

Expected: PASS, including the five new tests and every existing `hud.rs` test.
The `hud.rs` tests are the regression net for the `section` move: if the
`SharedInner` change broke a real path, they fail here.

- [ ] **Step 6: Commit**

```powershell
git add crates/settings-model/src/treewalk.rs crates/settings-model/src/hud.rs crates/settings-model/src/windows.rs
git commit -m "Fold the section, text and hex helpers into treewalk"
```

---

## Task 2: Real chat names and stack labels in the projection

**Files:**
- Modify: `crates/settings-model/src/windows.rs` — `WindowRect`, `Stack`
  assembly, `window_layout`'s signature, two new private readers
- Modify: `app/src-tauri/src/ops.rs` — `window_layout` (around line 578)
- Modify: `app/src/lib/api.ts` — `WindowRect`
- Test: `crates/settings-model/src/windows.rs` (its existing `#[cfg(test)]`
  module)

**Interfaces:**
- Consumes: `treewalk::{section, text}` from Task 1; existing
  `collect_shared`, `is_bytes`, `effective`, `SharedTable`.
- Produces, for Task 3:
  - `WindowRect.name: Option<String>` (TS: `name: string | null`)
  - `Stack.container_label` now carries EVE's label when the account file has
    one, else the container id as before.
  - `pub fn window_layout(root: &Value, user: Option<&Value>) -> WindowLayout`

- [ ] **Step 1: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` in
`crates/settings-model/src/windows.rs`. Follow the module's existing fixture
style for building a `windows` section; the two tests below only need the parts
they assert on.

```rust
#[test]
fn a_chat_window_takes_its_real_channel_name() {
    // ui → chatchannels is List[Tuple(kind, channelKey, label)]; the
    // channelKey is the window id's suffix after "chatchannel_".
    let root = Value::Dict(vec![
        (bytes("windows"), windows_section(&[("chatchannel_local", 10, 20, 300, 200)])),
        (bytes("ui"), Value::Dict(vec![(
            bytes("chatchannels"),
            Value::List(vec![Value::Tuple(vec![
                Value::Int(1), bytes("local"), bytes("Local"),
            ])]),
        )])),
    ]);
    let layout = window_layout(&root, None);
    let w = layout.windows.iter().find(|w| w.id == "chatchannel_local").expect("the chat window");
    assert_eq!(w.name.as_deref(), Some("Local"));
}

#[test]
fn a_window_with_no_entry_gets_no_name() {
    let root = Value::Dict(vec![
        (bytes("windows"), windows_section(&[("market", 0, 0, 100, 100)])),
        (bytes("ui"), Value::Dict(vec![(
            bytes("chatchannels"),
            Value::List(vec![Value::Tuple(vec![Value::Int(1), bytes("local"), bytes("Local")])]),
        )])),
    ]);
    let layout = window_layout(&root, None);
    let w = layout.windows.iter().find(|w| w.id == "market").expect("the market window");
    assert_eq!(w.name, None, "the frontend derives a name for these; the backend must not guess");
}

#[test]
fn a_stack_takes_its_label_from_the_account_file() {
    let root = stacked_root(); // a char root whose stack container id is "76"
    let user = Value::Dict(vec![(
        bytes("tabgroups"),
        Value::Dict(vec![
            (bytes("76"), Value::Int(0)),
            (bytes("76_names"), bytes("Character: Information")),
        ]),
    )]);
    let layout = window_layout(&root, Some(&user));
    let s = layout.stacks.iter().find(|s| s.container_id == "76").expect("the stack");
    assert_eq!(s.container_label, "Character: Information");
}

#[test]
fn a_stack_with_no_account_entry_keeps_the_container_id() {
    let root = stacked_root();
    let layout = window_layout(&root, None);
    let s = layout.stacks.iter().find(|s| s.container_id == "76").expect("the stack");
    assert_eq!(s.container_label, "76", "an unpaired character must project exactly as before");
}

#[test]
fn the_account_sections_resolve_through_ref_wrapped_keys() {
    // The shape a REAL account file has: the root section key is a Ref. A
    // hand-made flat dict would pass even with the bug this guards.
    let root = stacked_root();
    let key = Value::Shared { slot: 3, value: Box::new(bytes("tabgroups")) };
    let user = Value::Dict(vec![
        (key, Value::Dict(vec![(bytes("76_names"), bytes("Character: Information"))])),
        (Value::Ref(3), Value::Int(0)),
    ]);
    let layout = window_layout(&root, Some(&user));
    let s = layout.stacks.iter().find(|s| s.container_id == "76").expect("the stack");
    assert_eq!(s.container_label, "Character: Information");
}
```

If the test module has no `bytes`, `windows_section` or `stacked_root` helper,
write them beside the tests: `bytes(s)` returns `Value::Bytes(s.as_bytes().to_vec())`;
`windows_section(&[(id, x, y, w, h)])` builds a `windows` dict with a
`windowSizesAndPositions_1` entry per id in the shape the existing tests already
use; `stacked_root()` builds a root whose `windows → stacksWindows` puts one
member under container `"76"`. Reuse whatever the module already has rather than
duplicating it.

- [ ] **Step 2: Run the tests to verify they fail**

```powershell
cargo test -p settings-model windows
```

Expected: FAIL to compile — `this function takes 1 argument but 2 arguments were
supplied`, and `no field 'name' on type 'WindowRect'`.

- [ ] **Step 3: Add the field and the two readers**

In `crates/settings-model/src/windows.rs`, add to `WindowRect` (beside `label`):

```rust
    /// EVE's own display name for this window, when the file carries one.
    /// `None` for the vast majority — only chat windows have one today — in
    /// which case the frontend derives a name from the id.
    pub name: Option<String>,
```

Set `name: None` wherever a `WindowRect` is constructed, then fill it in
Step 4. Add the two readers near the bottom of the file, beside
`reference_resolution`:

```rust
/// `ui → chatchannels` is `List[Tuple(kind, channelKey, label)]` (367 of 384
/// corpus files). Returns channelKey → label; the window id for a channel is
/// `chatchannel_<channelKey>`. An absent section is normal, not an error.
fn chat_channel_names<'a>(root: &'a Value, sh: &SharedTable<'a>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some((ui, _)) = section(root, b"ui", sh) else { return out };
    let Some((_, v)) = ui.iter().find(|(k, _)| is_bytes(effective(k, sh), b"chatchannels")) else {
        return out;
    };
    let Value::List(items) = effective(v, sh) else { return out };
    for it in items {
        let Value::Tuple(parts) = effective(it, sh) else { continue };
        if parts.len() < 3 { continue }
        if let (Some(key), Some(label)) = (text(&parts[1], sh), text(&parts[2], sh)) {
            if !key.is_empty() && !label.is_empty() {
                out.insert(key, label);
            }
        }
    }
    out
}

/// The ACCOUNT file's root `tabgroups` section, in pairs: `<containerId>` →
/// selected tab index, `<containerId>_names` → that tab's label. The container
/// ids are the same numeric ids minted in the character file's
/// `windows → stacksWindows`, so the join needs no translation.
fn stack_tab_labels(user: &Value) -> HashMap<String, String> {
    // The account root needs its OWN shared table: slot numbers are
    // per-document, and resolving one document's Ref against another's table
    // yields the wrong node.
    let mut sh = SharedTable::new();
    collect_shared(user, &mut sh);
    let mut out = HashMap::new();
    let Some((groups, _)) = section(user, b"tabgroups", &sh) else { return out };
    for (k, v) in groups {
        let Some(key) = text(k, &sh) else { continue };
        let Some(id) = key.strip_suffix("_names") else { continue };
        if let Some(label) = text(v, &sh) {
            if !label.is_empty() {
                out.insert(id.to_string(), label);
            }
        }
    }
    out
}
```

Add `use std::collections::HashMap;` if the file does not already have it, and
add `section` and `text` to the existing `use crate::treewalk::{...}` list.

- [ ] **Step 4: Widen `window_layout` and apply both maps**

Change the signature and the empty-return:

```rust
pub fn window_layout(root: &Value, user: Option<&Value>) -> WindowLayout {
```

After the `windows` vector is fully assembled (just before the stack grouping
that builds `order`/`groups`), attach the chat names:

```rust
    // EVE's own name for a chat window, where it has one. Everything else keeps
    // `None` and is named by the frontend from its id.
    let chat = chat_channel_names(root, &shared);
    if !chat.is_empty() {
        for w in &mut windows {
            if let Some(key) = w.id.strip_prefix("chatchannel_") {
                w.name = chat.get(key).cloned();
            }
        }
    }
```

Before the `for container in order` loop, resolve the labels once:

```rust
    let tab_labels = user.map(stack_tab_labels).unwrap_or_default();
```

and in the `stacks.push(Stack { … })`, replace `container_label: container.clone()`
with:

```rust
            container_label: tab_labels.get(&container).cloned().unwrap_or_else(|| container.clone()),
```

- [ ] **Step 5: Run the tests to verify they pass**

```powershell
cargo test -p settings-model
```

Expected: PASS. Existing `windows.rs` tests will fail to compile until their
`window_layout(&root)` calls become `window_layout(&root, None)` — update them;
that is the intended blast radius of the signature change.

- [ ] **Step 6: Pass the account document from `ops.rs`**

Replace `pub fn window_layout` in `app/src-tauri/src/ops.rs` (around line 578):

```rust
pub fn window_layout(state: &AppState, slot: Slot) -> Result<WindowLayout, ErrDto> {
    // Lock user before the requested slot, matching hud_layout and
    // overview_columns — one consistent order across this file rules out
    // lock-order inversion between concurrent commands. When the CALLER asked
    // for the user slot there is no second document to take: locking `user`
    // twice would deadlock (std Mutex is not reentrant), and an account file
    // has no windows to project anyway.
    let uguard = matches!(slot, Slot::Char).then(|| state.user.lock().unwrap());
    let guard = state.doc(slot).lock().unwrap();
    let doc = guard.as_ref().ok_or_else(|| ErrDto::new("no_document", "no file open"))?;
    let user = uguard.as_ref().and_then(|g| g.as_ref()).map(|d| &d.value);
    Ok(project_window_layout(&doc.value, user))
}
```

- [ ] **Step 7: Widen the TypeScript type**

In `app/src/lib/api.ts`, add to `WindowRect` (after `label`):

```ts
  /** EVE's own name for this window when the file has one; null otherwise. */
  name: string | null;
```

- [ ] **Step 8: Verify the whole workspace**

```powershell
cargo test --workspace
```

Expected: PASS. Then, from `app/`:

```powershell
npm run check
```

Expected: 0 errors. Four warnings in `ContextMenu.svelte`, `InsertForm.svelte`
and `TreeNode.svelte` are pre-existing and not yours.

- [ ] **Step 9: Commit**

```powershell
git add crates/settings-model/src/windows.rs app/src-tauri/src/ops.rs app/src/lib/api.ts
git commit -m "Read EVE's own chat and stack names into the layout projection"
```

---

## Task 3: Show the real names in the list, canvas and filter

**Files:**
- Modify: `app/src/lib/windowLabels.ts` — add `nameOf`
- Modify: `app/src/lib/layout.ts` — `windowMatches` searches the real name
- Modify: `app/src/lib/WindowPanel.svelte`, `app/src/lib/LayoutView.svelte`
- Test: `app/src/lib/windowLabels.test.ts`, `app/src/lib/layout.test.ts`

**Interfaces:**
- Consumes: `WindowRect.name` from Task 2.
- Produces, for Task 5: nothing new — Task 5 changes `isClutter`, which `nameOf`
  does not touch.

- [ ] **Step 1: Write the failing tests**

Append to `app/src/lib/windowLabels.test.ts` (add `nameOf` to its import list):

```ts
// --- nameOf: EVE's own name wins, the derived one is the fallback ----------
{
  const real = nameOf({ id: "chatchannel_private_0ee11e4f970011ea", name: "Alliance HQ" });
  check("nameOf prefers the file's own name", real.label === "Alliance HQ");
  check("nameOf keeps the derived detail", real.detail === "private");
  check("nameOf keeps the derived family", real.family === "chatchannel");

  const derived = nameOf({ id: "market", name: null });
  check("nameOf falls back to describe when there is no name", derived.label === "Market");

  const missing = nameOf({ id: "market" });
  check("an absent name field is the same as null", missing.label === "Market");

  const blank = nameOf({ id: "market", name: "" });
  check("an empty name is not a name", blank.label === "Market");
}
```

Append to `app/src/lib/layout.test.ts`, inside the existing filter block or as a
new one (the `win` helper there builds a `WindowRect`; give it a `name`):

```ts
// --- the filter searches the real channel name -----------------------------
{
  const named = { ...win("chatchannel_private_0ee11e4f970011ea", true, true), name: "Alliance HQ" };
  check("text matches EVE's own name", windowMatches(named, { ...NO_FILTER, text: "alliance" }));
  check("text still matches the raw id", windowMatches(named, { ...NO_FILTER, text: "chatchannel" }));
  check("text still matches the derived detail", windowMatches(named, { ...NO_FILTER, text: "private" }));
  const unnamed = win("market", true, true);
  check("an unnamed window still matches its derived label", windowMatches(unnamed, { ...NO_FILTER, text: "market" }));
}
```

The `win` helper in `layout.test.ts` builds a `WindowRect` literal — add
`name: null` to it so it still satisfies the type.

- [ ] **Step 2: Run the tests to verify they fail**

```powershell
node --test src/lib/windowLabels.test.ts src/lib/layout.test.ts
```

Expected: FAIL — `does not provide an export named 'nameOf'`.

- [ ] **Step 3: Add `nameOf`**

In `app/src/lib/windowLabels.ts`, beside `displayName`:

```ts
/**
 * The name to show for a window: EVE's own, when the file carries one, else the
 * one derived from the id. Detail and family always come from the id — they
 * describe the id's shape, which a display name says nothing about.
 */
export function nameOf(w: { id: string; name?: string | null }): WindowName {
  const derived = describe(w.id);
  return w.name ? { ...derived, label: w.name } : derived;
}
```

- [ ] **Step 4: Search the real name in the filter**

In `app/src/lib/layout.ts`, in `windowMatches`, replace `const n = describe(w.id);`
with:

```ts
  const n = nameOf(w);
```

and add `nameOf` to the `./windowLabels.ts` import at the top of the file. The
haystack line below it already reads `${n.label} ${n.detail} ${w.id}`, so the
real name joins the search with no further change.

- [ ] **Step 5: Render the real name**

In `app/src/lib/LayoutView.svelte`, replace both `displayName(...)` calls — the
stack tab (`{displayName(tab.id)}`) and the window label
(`{displayName(unit.anchor.id)}`) — with `nameOf(tab).label` and
`nameOf(unit.anchor).label`, and change the `$lib/windowLabels` import from
`displayName` to `nameOf`. Keep both `title={...id}` attributes exactly as they
are: the raw id on hover is the escape hatch that makes friendly names safe.

In `app/src/lib/WindowPanel.svelte`, do the same for every place it renders a
window's name from `describe`/`displayName`, leaving the `title` attributes and
the *Copy window id* menu item untouched. A stack's frame row shows
`stack.container_label`, which Task 2 already filled with EVE's own label — no
change needed there.

- [ ] **Step 6: Run the tests and the typecheck**

```powershell
node --test src/lib/windowLabels.test.ts src/lib/layout.test.ts
npm run check
```

Expected: PASS, then 0 errors.

- [ ] **Step 7: Commit**

```powershell
git add app/src/lib/windowLabels.ts app/src/lib/windowLabels.test.ts app/src/lib/layout.ts app/src/lib/layout.test.ts app/src/lib/LayoutView.svelte app/src/lib/WindowPanel.svelte
git commit -m "Show EVE's own window names in the layout view"
```

---

## Task 4: The preferences file

**Files:**
- Create: `app/src-tauri/src/prefs.rs`
- Modify: `app/src-tauri/src/lib.rs` — `mod prefs;`, two commands, registration
- Modify: `app/src/lib/api.ts` — types and bindings
- Test: `app/src-tauri/src/prefs.rs` (its own `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `serde`, `serde_json`, `tauri::Manager` (all already dependencies).
- Produces, for Task 5:
  - `pub struct Preferences { pub layout: LayoutPrefs }`
  - `pub struct LayoutPrefs { pub clutter: Vec<String>, pub visible: Vec<String> }`
  - commands `preferences() -> Preferences` and `set_preferences(prefs: Preferences)`
  - TS: `api.preferences()` and `api.setPreferences(prefs)`

- [ ] **Step 1: Write the failing tests**

Create `app/src-tauri/src/prefs.rs` containing ONLY this test module to start:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("eve-prefs-test-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_missing_file_loads_defaults() {
        let p = temp_dir("missing").join("preferences.json");
        let prefs = load_from(&p);
        assert!(prefs.layout.clutter.is_empty());
        assert!(prefs.layout.visible.is_empty());
        assert!(!p.exists(), "loading must not create the file");
    }

    #[test]
    fn it_round_trips() {
        let p = temp_dir("roundtrip").join("preferences.json");
        let mut prefs = Preferences::default();
        prefs.layout.clutter.push("market".into());
        prefs.layout.visible.push("chatchannel_private_x".into());
        save_to(&p, &prefs).unwrap();
        let back = load_from(&p);
        assert_eq!(back.layout.clutter, vec!["market".to_string()]);
        assert_eq!(back.layout.visible, vec!["chatchannel_private_x".to_string()]);
    }

    #[test]
    fn a_corrupt_file_is_moved_aside_not_clobbered() {
        let dir = temp_dir("corrupt");
        let p = dir.join("preferences.json");
        std::fs::write(&p, b"{ this is not json").unwrap();
        let prefs = load_from(&p);
        assert!(prefs.layout.clutter.is_empty(), "a corrupt file must fall back to defaults");
        assert!(dir.join("preferences.json.bad").exists(), "the user's bad file must be recoverable");
    }

    #[test]
    fn an_unknown_key_still_loads() {
        // The forward-compatibility contract: a file written by a LATER build
        // must not break this one, and #[serde(default)] must cover a section
        // this build has never heard of.
        let p = temp_dir("unknown").join("preferences.json");
        std::fs::write(&p, br#"{"layout":{"clutter":["market"]},"future":{"x":1}}"#).unwrap();
        let prefs = load_from(&p);
        assert_eq!(prefs.layout.clutter, vec!["market".to_string()]);
    }

    #[test]
    fn a_missing_section_defaults() {
        let p = temp_dir("partial").join("preferences.json");
        std::fs::write(&p, b"{}").unwrap();
        let prefs = load_from(&p);
        assert!(prefs.layout.clutter.is_empty());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Add `mod prefs;` to `app/src-tauri/src/lib.rs` beside the other `mod` lines
(line 1-4), then:

```powershell
cargo test -p app prefs
```

Expected: FAIL to compile — `cannot find function 'load_from' in this scope`.

- [ ] **Step 3: Write the implementation**

Prepend to `app/src-tauri/src/prefs.rs`, above the test module:

```rust
//! Editor preferences — the app's own settings, not EVE's. Written to a JSON
//! file in the platform config dir; nothing here ever touches a settings file.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `#[serde(default)]` on every struct IS the extensibility contract: a later
/// build can add a field or a sibling section and files written by today's
/// build still load, and vice versa. There is deliberately no version field —
/// a version number with no migration code behind it is decoration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub layout: LayoutPrefs,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutPrefs {
    /// Window ids the user forced INTO the clutter set.
    pub clutter: Vec<String>,
    /// Window ids the user forced OUT of it.
    pub visible: Vec<String>,
}

/// Read the file, or defaults. A file we cannot parse is USER DATA: move it
/// aside so a hand-edit gone wrong is recoverable, rather than silently
/// overwriting it on the next save.
pub fn load_from(path: &Path) -> Preferences {
    let Ok(raw) = std::fs::read_to_string(path) else { return Preferences::default() };
    match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(_) => {
            let _ = std::fs::rename(path, path.with_extension("json.bad"));
            Preferences::default()
        }
    }
}

pub fn save_to(path: &Path, prefs: &Preferences) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(prefs).map_err(std::io::Error::other)?)
}

/// `<app config dir>/preferences.json` — created lazily, on first save.
pub fn path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .map(|d| d.join("preferences.json"))
        .map_err(|e| format!("no config directory: {e}"))
}
```

Note `path.with_extension("json.bad")` turns `preferences.json` into
`preferences.json.bad` — verify that in the corrupt-file test rather than
assuming it.

- [ ] **Step 4: Run the tests to verify they pass**

```powershell
cargo test -p app prefs
```

Expected: PASS, 5 tests.

- [ ] **Step 5: Add the two commands**

In `app/src-tauri/src/lib.rs`, beside the other `#[tauri::command]` functions:

```rust
#[tauri::command]
fn preferences(app: tauri::AppHandle) -> Result<prefs::Preferences, ErrDto> {
    let path = prefs::path(&app).map_err(|m| ErrDto::new("no_config_dir", &m))?;
    Ok(prefs::load_from(&path))
}

#[tauri::command]
fn set_preferences(app: tauri::AppHandle, prefs: prefs::Preferences) -> Result<(), ErrDto> {
    let path = crate::prefs::path(&app).map_err(|m| ErrDto::new("no_config_dir", &m))?;
    crate::prefs::save_to(&path, &prefs).map_err(|e| ErrDto::new("write_failed", &e.to_string()))
}
```

Check `ErrDto::new`'s exact signature in `ops.rs` before writing these and match
it — the two calls above assume `new(code, message)` taking `&str`.

Add both names to the `tauri::generate_handler![…]` list (around line 345-351),
on the line with the other layout commands.

- [ ] **Step 6: Bind them in `api.ts`**

In `app/src/lib/api.ts`, add the types beside `WindowLayout`:

```ts
export interface LayoutPrefs {
  clutter: string[];
  visible: string[];
}
export interface Preferences {
  layout: LayoutPrefs;
}
```

and the bindings beside `windowLayout`:

```ts
  preferences: () => invoke<Preferences>("preferences"),
  setPreferences: (prefs: Preferences) => invoke<void>("set_preferences", { prefs }),
```

- [ ] **Step 7: Verify**

```powershell
cargo test --workspace
```

then from `app/`:

```powershell
npm test
npm run check
```

Expected: all pass. `ipc.test.ts` walks both sides and pins the two new
commands automatically — if it fails, the names or argument names disagree
between `api.ts` and Rust, which is exactly what it exists to catch.

- [ ] **Step 8: Commit**

```powershell
git add app/src-tauri/src/prefs.rs app/src-tauri/src/lib.rs app/src/lib/api.ts
git commit -m "Add an editor preferences file"
```

---

## Task 5: User-editable clutter

**Files:**
- Create: `app/src/lib/prefs.svelte.ts`
- Modify: `app/src/lib/windowLabels.ts` — `isClutter` takes overrides
- Modify: `app/src/lib/layout.ts` — `windowMatches`/`visibleIds` pass them
- Modify: `app/src/lib/WindowPanel.svelte` — two context-menu items
- Modify: `app/src/lib/LayoutView.svelte` — thread overrides, counter clause
- Test: `app/src/lib/windowLabels.test.ts`, `app/src/lib/layout.test.ts`

**Interfaces:**
- Consumes: `api.preferences()` / `api.setPreferences()` and the `Preferences`
  type from Task 4.
- Produces: nothing later tasks consume.

- [ ] **Step 1: Write the failing tests**

Append to `app/src/lib/windowLabels.test.ts`:

```ts
// --- clutter overrides ------------------------------------------------------
{
  const none = { clutter: new Set<string>(), visible: new Set<string>() };
  check("no overrides leaves the built-in verdict alone (clutter)", isClutter("ChatInvitation_x", none));
  check("no overrides leaves the built-in verdict alone (ordinary)", !isClutter("market", none));
  check("an absent overrides argument is the same as empty", isClutter("ChatInvitation_x"));

  const forced = { clutter: new Set(["market"]), visible: new Set<string>() };
  check("an override can make an ordinary window clutter", isClutter("market", forced));

  const freed = { clutter: new Set<string>(), visible: new Set(["ChatInvitation_x"]) };
  check("an override can rescue a window the tables call clutter", !isClutter("ChatInvitation_x", freed));

  // Only reachable by hand-editing preferences.json: the UI keeps the two sets
  // disjoint. Pinned so the precedence is not accidental.
  const both = { clutter: new Set(["market"]), visible: new Set(["market"]) };
  check("visible wins when a hand-edited file lists an id twice", !isClutter("market", both));
}
```

Append to `app/src/lib/layout.test.ts`:

```ts
// --- overrides reach the filter --------------------------------------------
{
  const market = win("market", true, true);
  const invite = win("ChatInvitation_x", true, true);
  const forced = { clutter: new Set(["market"]), visible: new Set<string>() };
  check("hideClutter drops an overridden-clutter window",
    !windowMatches(market, { ...NO_FILTER, hideClutter: true }, forced));
  check("without hideClutter the override changes nothing",
    windowMatches(market, { ...NO_FILTER }, forced));

  const freed = { clutter: new Set<string>(), visible: new Set(["ChatInvitation_x"]) };
  check("hideClutter keeps a rescued window",
    windowMatches(invite, { ...NO_FILTER, hideClutter: true }, freed));

  const ids = visibleIds([market, invite], { ...NO_FILTER, hideClutter: true }, freed);
  check("visibleIds honours the overrides", ids.has("ChatInvitation_x") && ids.has("market"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```powershell
node --test src/lib/windowLabels.test.ts src/lib/layout.test.ts
```

Expected: FAIL — `isClutter` currently takes one argument, so the override
cases assert the un-overridden verdict and the `forced`/`freed` checks throw.

- [ ] **Step 3: Teach `isClutter` about overrides**

In `app/src/lib/windowLabels.ts`, replace `isClutter`:

```ts
/** Per-window user overrides of the built-in clutter tables. The two sets are
 * kept disjoint by the UI; `visible` wins if a hand-edited file lists an id in
 * both. */
export interface ClutterOverrides {
  clutter: ReadonlySet<string>;
  visible: ReadonlySet<string>;
}

/** True for a window EVE spawns per conversation/item/dialog rather than one
 * the player placed. Hidden in both the list and the canvas, whether open or
 * closed — open/closed is not the axis; kind of window is.
 *
 * The built-in tables can never be complete (see the note above CLUTTER_IDS),
 * so a user override outranks them in both directions. */
export function isClutter(id: string, o?: ClutterOverrides): boolean {
  if (o?.visible.has(id)) return false;
  if (o?.clutter.has(id)) return true;
  if (CLUTTER_IDS.has(id)) return true;
  const n = describe(id);
  if (n.family === "chatchannel") return CLUTTER_CHAT_DETAILS.has(n.detail);
  // detail === "" means a bare parent window (e.g. plain "ShipCargo") — keep it.
  return CLUTTER_FAMILIES.has(n.family) && n.detail !== "";
}
```

- [ ] **Step 4: Thread the overrides through the filter**

In `app/src/lib/layout.ts`, add `type ClutterOverrides` to the
`./windowLabels.ts` import, then change the two signatures:

```ts
export function windowMatches(w: WindowRect, f: WindowFilter, o?: ClutterOverrides): boolean {
```

with its `isClutter(w.id)` call becoming `isClutter(w.id, o)`, and:

```ts
export function visibleIds(windows: WindowRect[], f: WindowFilter, o?: ClutterOverrides): Set<string> {
  return new Set(windows.filter((w) => windowMatches(w, f, o)).map((w) => w.id));
}
```

The overrides stay a parameter rather than a field on `WindowFilter`: they are
a preference, not a filter setting, and they persist while the filter
deliberately does not.

- [ ] **Step 5: Create the preferences store**

Create `app/src/lib/prefs.svelte.ts`:

```ts
// Editor preferences, loaded once at startup and written through on change.
// Nothing here touches an EVE settings file — see app/src-tauri/src/prefs.rs.
import { api } from "$lib/api";
import type { Preferences } from "$lib/api";
import type { ClutterOverrides } from "$lib/windowLabels";

let prefs = $state<Preferences>({ layout: { clutter: [], visible: [] } });

/** Load once. A failure leaves the defaults in place: preferences are a
 * convenience, and the editor must open without them. */
export async function loadPrefs(): Promise<void> {
  prefs = await api.preferences().catch(() => prefs);
}

export const clutterOverrides = (): ClutterOverrides => ({
  clutter: new Set(prefs.layout.clutter),
  visible: new Set(prefs.layout.visible),
});

export const overrideCount = () => prefs.layout.clutter.length + prefs.layout.visible.length;

/** Force a window into or out of the clutter set, or drop the override. The
 * two lists are kept disjoint here, which is what lets `isClutter` treat them
 * as independent. */
export function setClutterOverride(id: string, mode: "clutter" | "visible" | "default"): void {
  const l = prefs.layout;
  prefs = {
    ...prefs,
    layout: {
      clutter: l.clutter.filter((x) => x !== id).concat(mode === "clutter" ? [id] : []),
      visible: l.visible.filter((x) => x !== id).concat(mode === "visible" ? [id] : []),
    },
  };
  void api.setPreferences($state.snapshot(prefs));
}

export function clearClutterOverrides(): void {
  prefs = { ...prefs, layout: { clutter: [], visible: [] } };
  void api.setPreferences($state.snapshot(prefs));
}
```

- [ ] **Step 6: Load the preferences at startup**

In `app/src/routes/+page.svelte`, import `loadPrefs` from `$lib/prefs.svelte`
(the `$lib` alias, since this one is outside `src/lib`; the components inside
`src/lib` use the relative `./prefs.svelte` form instead — see
`AccountsView.svelte`'s `./accounts.svelte`)
and call it once from the same place the app does its other startup work (the
existing `onMount`/startup `$effect` that discovers profiles). One call, not
awaited by anything else — a preferences failure must not delay or block the
file discovery.

- [ ] **Step 7: Wire the overrides into the view**

In `app/src/lib/LayoutView.svelte`, import
`{ clutterOverrides, overrideCount, clearClutterOverrides, setClutterOverride }`
from `./prefs.svelte` (the house import style for these stores — see
`AccountsView.svelte`'s `./accounts.svelte`), and pass the overrides at both filter call sites:

```ts
  const visible = $derived(
    layout && filterIsActive(filter) ? visibleIds(layout.windows, filter, clutterOverrides()) : null,
  );
```

Add the counter clause inside the existing `.ref` paragraph, after the
`filterIsActive` block, so an override is never invisibly in effect:

```svelte
        {#if overrideCount() > 0}
          <span class="showing">
            · {overrideCount()} overridden
            <button class="linkish" onclick={clearClutterOverrides}>clear</button>
          </span>
        {/if}
```

Pass `setClutterOverride` and `clutterOverrides` down to `WindowPanel` as props
(`onClutterOverride`, `overrides`) rather than importing the store there too, so
the panel stays a presentational component like every other prop it takes.

- [ ] **Step 8: Add the two context-menu items**

In `app/src/lib/WindowPanel.svelte`, accept the two new props alongside the
existing ones, and extend `rowMenu(w)` (which already pushes *Copy window id*
and *Select on canvas*):

```ts
    // One item, never both, labelled for what the click will do. The built-in
    // tables can never be complete, so this is the per-window escape hatch.
    const overridden = overrides.clutter.has(w.id) || overrides.visible.has(w.id);
    if (overridden) {
      items.push({ label: "Use the default clutter rule", run: () => onClutterOverride(w.id, "default") });
    } else if (isClutter(w.id, overrides)) {
      items.push({ label: "Stop treating as clutter", run: () => onClutterOverride(w.id, "visible") });
    } else {
      items.push({ label: "Treat as clutter", run: () => onClutterOverride(w.id, "clutter") });
    }
```

Import `isClutter` from `$lib/windowLabels` if the file does not already.

- [ ] **Step 9: Verify**

```powershell
node --test src/lib/windowLabels.test.ts src/lib/layout.test.ts
npm test
npm run check
npm run build
```

Expected: all pass, 0 errors, build succeeds.

- [ ] **Step 10: Commit**

```powershell
git add app/src/lib/prefs.svelte.ts app/src/lib/windowLabels.ts app/src/lib/windowLabels.test.ts app/src/lib/layout.ts app/src/lib/layout.test.ts app/src/lib/WindowPanel.svelte app/src/lib/LayoutView.svelte app/src/routes/+page.svelte
git commit -m "Let the user decide what counts as layout clutter"
```

---

## Task 6: The debt sweep

Independent one-liners from the ledger. Grouped into one task because no
reviewer would meaningfully approve one and reject another, and each is a line
or two.

**Files:**
- Modify: `crates/settings-model/src/overview_tabs.rs`
- Modify: `crates/settings-model/src/hud.rs`
- Modify: `app/src/lib/layout.test.ts`
- Modify: `app/src-tauri/src/ops.rs` (test only) or
  `crates/settings-model/src/overview_tabs.rs` (test only) — wherever
  `remove_overview_window` is unit-tested
- Modify: `app/src/lib/WindowPanel.svelte`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: nothing later tasks consume.

- [ ] **Step 1: Name the cascade offset**

In `crates/settings-model/src/overview_tabs.rs`, in
`add_overview_window_geometry`, replace the bare `40` with a named constant
declared above the function:

```rust
/// Each new overview window is offset from the primary so it does not land
/// exactly on top of it.
const OVERVIEW_WINDOW_OFFSET: i64 = 40;
```

- [ ] **Step 2: Fix the tautological `drawnWindowCount` test**

In `app/src/lib/layout.test.ts`, find the third `drawnWindowCount` check — it
reads `stackUnits(x, null)` compared against `stackUnits(x)`, which is the same
call because `null` is the default, so it never exercises the regression its
name claims. Replace that check with a real filtered case:

```ts
  // A stack container that matches the filter while NONE of its members do
  // draws nothing — counting the raw window list would over-report it.
  const containerOnly = new Set(["C"]);
  const filtered = stackUnits(layout as any, containerOnly);
  check("a container-only filter match draws nothing", drawnWindowCount(filtered) === 0);
```

using the same `layout` fixture the surrounding `drawnWindowCount` block
already builds (the one with container `C` and members `m1`/`m2`).

- [ ] **Step 3: Cover `remove_overview_window`'s `UnknownWindow` guard**

Find the existing tests for `remove_overview_window` (grep for
`remove_overview_window` under `crates/` and `app/src-tauri/`) and add a case
that passes a `window_idx` at or beyond the window count while at least two
windows exist, asserting the `UnknownWindow` error. Follow the shape of the
neighbouring `LastWindow` test in the same module — reuse its fixture builder
rather than writing a new one.

- [ ] **Step 4: Fix the `hud.rs` leftovers**

Two ledger items, both in `crates/settings-model/src/hud.rs`:

- `locate()` computes an `Option<String>` half that the writer path discards.
  Drop the computation on that path (keep the reader's use of it).
- `mint` has three separate `Err(HudError::NoSection)` guard returns. Collapse
  them into one guard at the top of the function.

Read the function bodies before editing; if either turns out to be load-bearing
in a way the ledger did not anticipate, leave it and say so in your report
rather than forcing the change.

- [ ] **Step 5: Disable the stack reorder button on a hidden predecessor**

In `app/src/lib/WindowPanel.svelte`, the stack's ↑ button stays enabled on the
first *visible* member when a hidden member precedes it at true index 0, so the
click swaps with a row the filter is hiding. Keep the true-index contract — it
is what stops reordering from scrambling under a filter — and instead disable
the control when the neighbour it would swap with is not currently visible.

- [ ] **Step 6: Verify**

```powershell
cargo test --workspace
```

then from `app/`:

```powershell
npm test
npm run check
```

Expected: all pass.

- [ ] **Step 7: Commit**

```powershell
git add -A
git commit -m "Clear the layout debt the ledger accumulated"
```

---

## Task 7: Whole-slice verification, smoke and PR

**Files:**
- Modify: `docs/small-tasks.md` — move the closed items out of **Open**
- Modify: `app/src/lib/layout.ts` (the `HUD_NOMINAL` table and the
  `shipOffsetFromX`/`hudPointFromRect` comments) — only if Step 3 finds the HUD
  conventions wrong

**Interfaces:**
- Consumes: everything above.
- Produces: a merge-ready branch.

- [ ] **Step 1: Full gate**

```powershell
cargo test --workspace
```

then from `app/`:

```powershell
npm run check
npm test
npm run build
```

Expected: all green.

- [ ] **Step 2: Live smoke — names and overrides**

Run `npm run tauri dev` and, on a real character with chat windows open and a
paired account:

- a chat window shows its real channel name in the list, on the canvas and on a
  stack tab, with the raw id still on hover;
- typing the channel's real name into the filter finds it — the reported
  symptom that opened this item;
- a window with no entry still shows its derived name (nothing regressed);
- a stack shows EVE's own label (`Character: Information`) rather than
  `Window stack · 76`;
- opening an UNPAIRED character still works and simply shows the old stack
  labels;
- *Treat as clutter* on an ordinary window hides it under `Hide clutter`, and
  the counter shows `· 1 overridden`; *clear* restores it;
- *Stop treating as clutter* rescues a window the tables hide;
- the override survives a full restart of the app, and
  `%APPDATA%\io.github.stormdelay.eve-settings-editor\preferences.json` exists
  and contains it.

- [ ] **Step 3: Live smoke — the HUD convention check**

v0.15.0 shipped the ship HUD, fighter UI and badge with **assumed** geometry:
`HUD_NOMINAL`'s sizes, the centre-relative ship offset and the top-left point
convention in `app/src/lib/layout.ts` are guesses, flagged as such in the code
and the changelog. With a real client running, this is the cheapest chance to
settle them:

- move the ship HUD, the fighter UI and the badge in-game, quit the client so it
  writes, and reload the file in the editor;
- compare each element's drawn rectangle against where it actually sits on the
  EVE screen.

If a convention is wrong, correct it **together with its inverse** —
`shipOffsetFromX` for the ship offset, `hudPointFromRect` for the
fighter/badge point — since they are matched pairs, and update the
`layout.test.ts` round-trip cases that pin them. If they are right, delete the
hedging from the comments and say so. Either way, record the outcome in your
report: leaving this unknown across another release is the thing to avoid.

- [ ] **Step 4: Update the ledger**

In `docs/small-tasks.md`, move every item this slice closed out of **Open** and
into **Shipped**, marked with this slice. Those are: the chat-channel-name gap,
the `tabgroups` stack labels, the window-stacks `container_label` follow-up, the
user-editable clutter list, the two slice-1a follow-ups, and the individual
HUD-furniture and Phase-B minors listed in the spec's §6. Leave the rest in
**Open** — in particular per-environment canvas views, orphan-frame deletion,
the discard-changes button, `HudPanel`'s hardcoded colours, the number-input
desync, the `set_hud_field` reshare measurement, and the account-row read-only
asymmetry, all of which the spec names as deliberately out of scope.

- [ ] **Step 5: Commit and open the PR**

```powershell
git add -A
git commit -m "Update the small-tasks ledger for the names-and-noise slice"
gh pr create --title "Layout: real names, and noise you control" --body-file <path-to-body>
```

`--body-file`, not `--body`: multi-line bodies do not survive the shell here.
The body should state what shipped (real chat and stack names, the preferences
file, user-editable clutter, the debt sweep), the outcome of the HUD convention
check, that the live smoke was run and what it covered, and that nothing in the
slice writes to an EVE settings file.

- [ ] **Step 6: Reclaim the disk**

`cargo test --workspace` regrows `target/` to several GB on a near-full C:
drive. Once the PR is open and the dev app is closed:

```powershell
cargo clean
```

---

## Self-Review

**Spec coverage**

| Spec section | Task |
|---|---|
| §3 `window_layout(root, user)` | Task 2 Step 4 |
| §3 chat names from `ui → chatchannels` | Task 2 Steps 3-4 |
| §3 stack labels from account `tabgroups` | Task 2 Steps 3-4 |
| §3 account-file `Ref` gotcha, resolved through `effective` | Task 1 (the shared `section` helper) + Task 2 Step 1's `Ref`-shaped test |
| §3 separate `SharedTable` per document | Task 2 Step 3 (`stack_tab_labels` builds its own) |
| §3 `WindowRect.name`, not folded into `label` | Task 2 Steps 3, 7 |
| §3 `nameOf`, `describe` untouched, filter searches the name | Task 3 |
| §4 `preferences.json` shape, location, `#[serde(default)]` | Task 4 Step 3 |
| §4 failure policy incl. the `.bad` rename | Task 4 Steps 1, 3 |
| §4 whole-document `preferences`/`set_preferences` | Task 4 Steps 5-6 |
| §4 `load_from`/`save_to` take a path, tested in a temp dir | Task 4 Steps 1, 3 |
| §4 `isClutter(id, overrides)`, `visible` wins | Task 5 Step 3 |
| §4 overrides as a parameter, not a `WindowFilter` field | Task 5 Step 4 |
| §4 context-menu items, one never both | Task 5 Step 8 |
| §4 `· N overridden · clear` counter | Task 5 Step 7 |
| §5 nothing writes to an EVE file | Global Constraints; no mutation appears in any task |
| §6 the debt sweep | Task 1 Steps 3-4 (`hex`, `section`'s `SharedInner`), Task 6 |
| §7 testing | Tasks 1-5 test steps; Task 7 Step 2 |
| §8 HUD convention check | Task 7 Step 3 |

**Placeholder scan:** the only bracketed placeholder is `<path-to-body>` in
Task 7 Step 5, a file the implementer writes at PR time. Task 6 Steps 3-5 and
Task 3 Step 5 describe edits by their location and contract rather than quoting
code, because the exact lines depend on code this plan does not reproduce — each
names the file, the function and the acceptance condition.

**Type consistency:** `Preferences`/`LayoutPrefs` are spelled identically in
Rust (Task 4 Step 3), in `api.ts` (Task 4 Step 6) and in `prefs.svelte.ts`
(Task 5 Step 5). `ClutterOverrides` is defined once in `windowLabels.ts` (Task 5
Step 3) and imported by `layout.ts` (Step 4) and `prefs.svelte.ts` (Step 5).
`nameOf(w: { id; name? })` (Task 3 Step 3) accepts the `WindowRect` that Task 2
Step 7 defines. `window_layout(root, user)` is spelled the same in Task 2
Steps 4 and 6 and in every test in Step 1.
