# Chat Window Names Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the standing chat channels — Corp, Alliance, Local and named private groups — show their real names in the layout editor instead of a wall of identical "Chat" rows.

**Architecture:** A one-element correction in `crates/settings-model/src/windows.rs`. The join from `ui → chatchannels` to window ids already exists and already flows through the DTO and the frontend; it reads the wrong tuple element, so it matches only the rows where two elements happen to be identical. Nothing else changes.

**Tech Stack:** Rust (`settings-model`), `cargo test`.

## Global Constraints

- **No new dependencies**, no DTO change, no frontend change. `WindowRect.name` and the frontend's `nameOf` / `displayNameOf` already do the right thing with a resolved name — this only makes the resolution find them.
- **Corpus copies only in tests.** Never read the live EVE directory from a test; `testdata/corpus/` is the source for real-file assertions.
- Run the Rust suite with `cargo test` from the repo root.
- Commit after each task.

---

## Background: the entry described the symptom, not the cause

The ledger entry says `windowLabels.ts:97` "maps the whole `chatchannel` family to one flat label". That mapping is real but it is the **fallback**, and it is correct as one: `PARAM.chatchannel = "Chat"` is what a window gets when the file carries no name for it.

The machinery to do better already shipped:

| Piece | State |
|---|---|
| `windows.rs::chat_channel_names` reads `ui → chatchannels` | exists |
| `window_layout` assigns it to `WindowRect.name` | exists |
| `WindowRect.name` in the DTO | exists |
| `windowLabels.ts::nameOf` prefers `w.name` over the derived label | exists |

So the entry's "join on that and show the third element" is already built. **It joins on the wrong element.**

`chat_channel_names` keys its map on `parts[1]`. The tuples are `(key, fullChannelId, label)`:

```
("corp",             "corp_98835672",     "Corp")
("alliance",         "alliance_99010468", "Alliance")
("local",            "local_30004758",    "Local")
("player_-78564080", "player_-78564080",  "StormDelay Fam")
("player_-88620541", "player_-88620541",  "Bean-Intel")
```

The window id is `chatchannel_` + **`parts[0]`**. Session A confirmed this in-game on 2026-07-28 and recorded it in `settings-field-reference.md`; the code was never corrected.

Measured over the two characters in the 2026-07-28 `c-after` capture:

| Join on | char 93622368 | char 1985569356 |
|---|---|---|
| `parts[0]` (correct) | **5 of 5** | **3 of 3** |
| `parts[1]` (current) | 2 of 5 | 1 of 3 |

`parts[1]` matches only the `player_*` rows, where both elements are the same string by coincidence. It misses `corp`, `alliance` and `local` — the standing channels, which are exactly the ones a player recognises by name.

**What this does not fix, by design.** Those characters carry 78 and 77 chat windows but only 5 and 3 live channel rows. The rest are stale windows for conversations long closed, and the file holds no name for them — they keep the derived `Chat · <detail>` label, which is correct. This change is about the handful that *can* be named, not about the size of the list. Hiding the stale ones is what `Hide clutter` is for and is a separate concern.

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/settings-model/src/windows.rs` | Window projection incl. the chat-name join | Modify: `chat_channel_names` keys on `parts[0]` |
| `docs/small-tasks.md` | The ledger | Modify: close the entry, correcting its diagnosis |

---

### Task 1: Join on the element the window id is actually built from

**Files:**
- Modify: `crates/settings-model/src/windows.rs` (`chat_channel_names`, and its doc comment)
- Test: `crates/settings-model/src/windows.rs` (the `mod tests` block — it already has `chatchannels` fixtures)

**Interfaces:**
- Consumes: nothing new.
- Produces: no signature change. `chat_channel_names` keeps returning `HashMap<String, String>`; only the key changes.

- [ ] **Step 1: Write the failing test**

The test module already has `chatchannels` fixtures (search for `chatchannels` in `windows.rs` — there are cases around the `local` window). **Read them first**: at least one builds a tuple whose first two elements are the same string, which passes under either element and therefore cannot catch this. Add a case where they differ, which is the shape every standing channel has:

```rust
    #[test]
    fn a_standing_channel_is_named_from_the_first_tuple_element() {
        // Real shape: ("corp", "corp_98835672", "Corp") — the window id is
        // `chatchannel_` + the FIRST element (confirmed in-game 2026-07-28),
        // while the second is the fully-qualified channel id. Joining on the
        // second matches only player_* rows, where the two happen to be equal,
        // and silently misses every standing channel.
        let doc = Value::Dict(vec![
            (bytes("windows"), windows_section(&[("chatchannel_corp", 10, 20, 300, 200)])),
            (bytes("ui"), Value::Dict(vec![(
                bytes("chatchannels"),
                Value::List(vec![Value::Tuple(vec![
                    bytes("corp"),
                    Value::Str("corp_98835672".into()),
                    Value::Str("Corp".into()),
                ])]),
            )])),
        ]);
        let layout = window_layout(&doc, None);
        let w = layout.windows.iter().find(|w| w.id == "chatchannel_corp").expect("the chat window");
        assert_eq!(w.name.as_deref(), Some("Corp"));
    }
```

Match the fixture helpers the neighbouring chat tests already use (`bytes`, `windows_section`, and whatever they wrap the `ui` section in) rather than inventing new ones — if those tests build `ui` differently, follow them.

- [ ] **Step 2: Run the test to verify it fails**

Run from the repo root: `cargo test -p settings-model --lib a_standing_channel`
Expected: FAIL — `assertion failed: left: None, right: Some("Corp")`. The map is keyed `"corp_98835672"`, the lookup key is `"corp"`.

- [ ] **Step 3: Write the implementation**

In `crates/settings-model/src/windows.rs`, in `chat_channel_names`, change the key from the second element to the first:

```rust
        if let (Some(key), Some(label)) = (text(&parts[0], sh), text(&parts[2], sh)) {
```

And correct the function's doc comment, which currently describes the wrong element as the join key:

```rust
/// `ui → chatchannels` is `List[Tuple(key, fullChannelId, label)]` (367 of 384
/// corpus files). Returns key → label; the window id for a channel is
/// `chatchannel_<key>` — the FIRST element, confirmed in-game 2026-07-28 for
/// both a standing channel and a private conversation.
///
/// Not the second: for `player_*` rows the first two elements are the same
/// string, so keying on the second appears to work and then silently misses
/// every standing channel (`corp`, `alliance`, `local`), whose second element
/// is the fully-qualified `corp_98835672` form. Measured on the 2026-07-28
/// capture: the first element matched 5 of 5 and 3 of 3, the second 2 of 5 and
/// 1 of 3.
///
/// An absent section is normal, not an error.
```

- [ ] **Step 4: Run the test to verify it passes**

Run from the repo root: `cargo test -p settings-model --lib windows::`
Expected: PASS, including the pre-existing chat tests. If a pre-existing test now fails, read it before changing it — one may have been written around the wrong element and would be asserting the bug.

- [ ] **Step 5: Run the whole suite**

Run from the repo root: `cargo test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/settings-model/src/windows.rs
git commit -m "Name chat windows from the tuple element their id is built from"
```

---

### Task 2: Close the ledger entry, correcting its diagnosis

**Files:**
- Modify: `docs/small-tasks.md`

**Interfaces:** none — documentation only.

- [ ] **Step 1: Move the entry to Shipped**

Delete the `- [ ] **Every chat window is labelled just "Chat".**` entry from **Open** and add under `### Unreleased (on master)`:

```markdown
- [x] **Every chat window is labelled just "Chat".** The entry's diagnosis was
  wrong in a way worth recording: the join it asked for was already built —
  `windows.rs::chat_channel_names` reads `ui → chatchannels`, `window_layout`
  assigns it to `WindowRect.name`, and `windowLabels.ts::nameOf` already prefers
  it. `PARAM.chatchannel = "Chat"` is the *fallback*, and correct as one. The
  defect was that the join keyed on the tuple's SECOND element while the window
  id is built from the FIRST. For `player_*` rows those two are the same string,
  so it appeared to work while silently missing every standing channel. Measured
  on the 2026-07-28 capture: the first element matches 5 of 5 and 3 of 3, the
  second 2 of 5 and 1 of 3. Session A had already confirmed the correct element
  in-game on 2026-07-28; the correction simply never reached `windows.rs`.

  Corp, Alliance, Local and named private groups now show their real names. The
  ~70 remaining chat windows per character are stale conversations the file holds
  no name for; they keep the derived `Chat · <detail>` label, which is right —
  thinning that list is what `Hide clutter` is for. _Added 2026-07-27; done
  2026-07-28._
```

- [ ] **Step 2: Commit**

```bash
git add docs/small-tasks.md
git commit -m "Close the chat-label task, and correct what was actually wrong"
```

---

## Self-review notes

- **Scope.** One element index and a doc comment. The entry read like a feature request ("join on that and show the third element"); it is a one-character bug fix, because the feature was already there.
- **Why the existing tests missed it.** The chat fixtures in `windows.rs` use a channel key whose first two tuple elements are identical — the one shape that passes under either reading. That is the same class of hole as the `tabsettings` fixture problem: a fixture that shares the code's assumption cannot falsify it.
- **Not in scope, deliberately.** The number of chat windows (a `Hide clutter` concern), the `chatchannel` fallback label, and anything about which windows a stack shows.
