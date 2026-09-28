# Leave a chat channel from the layout editor (design)

Status: designed 2026-09-28. Branch `feat/chat-leave`.

## 1. Why, and what the spike proved

The layout editor could not remove a chat window. Removing a
`chatchannel_*` window's entries alone never stuck: EVE rebuilt it on the next
login (`format-notes.md`, "chat/convo windows follow the character's runtime
state"; `live-verification-plan.md`, `chatchannel_corp` back in
`ChatWindowStack`).

Spike, 2026-09-28, on Pilot Echo (`core_char_96000002`, `settings_Default`),
Alpha-Intel (`chatchannel_player_-70000002`, a pinned member of
`ChatWindowStack`). Removed the window id from every dict under `windows` **and**
its row from the character file's `ui → chatchannels`, then logged in and out.
With timestamps normalised, the client's rewrite differed from the backup by
exactly those removals: nothing recreated, nothing added, the other chat tabs'
`preferredIdxInStack3` indices untouched. The `chatchannels` row is what drives
recreation. In game the channel was gone and appeared only as a join
suggestion.

The account file kept `ui → chatPlayerChannelsJoined["player_-70000002"] =
"Alpha-Intel"` through that logout, and is the likely source of the suggestion.

## 2. Scope

**Leavable:** `chatchannel_player_<key>` (player channels) and
`chatchannel_private_<key>` (private conversations).

**Refused:** everything else — `local`, `corp`, `alliance`, `fleet`,
`incursion`, `invasion`, `faction`, and any id that is not one of the two
leavable shapes. Standing channels are the server's to assign and would be
rebuilt; an unknown shape is refused because refusing is the safe default.

**Out of scope:** leaving the channel server-side (the editor cannot), bulk
leave, and cleaning account-side keys of channels already left by other means.

## 3. What one "leave" removes

Given window id `W = chatchannel_<key>`.

### 3.1 Character file — always

- `W` as a key from every dict under `windows`, unwrapping `(timestamp, dict)`
  tuples, and from every dict nested one level inside them (the per-container
  dicts of `preferredIdxInStack3`). This covers geometry, the eight flag dicts,
  `stacksWindows` and stack ordering without naming them, so a dict EVE adds
  later is covered too.
- `ChannelSettingsDlg_<key>` the same way: the channel's settings dialog window.
- The `ui → chatchannels` row whose FIRST element is `<key>`
  (`windows.rs::chat_channel_names` documents why the first).

Window-id keys are matched through `windows::decode_id`, so Bytes, Str and
StrUcs2 keys are all caught.

`NotFound` when neither the window nor the row exists.

### 3.2 Account file — only when no linked character still has the channel

Under the account file's root `ui` section, each removed only if present
(nothing is ever created):

| key | note |
|---|---|
| `chatPlayerChannelsJoined` → entry `<key>` | player channels only; convos have none |
| `W_userlistwidth` | member-list width (`chat.rs`) |
| `chatinputsize_W` | input-box height (`chat.rs`) |
| `chatfontsize_W` | |
| `chatWindowBlink_W` | |
| `chatCondensedUserList_W` and `chatCondensedUserList_<key>` | both spellings occur (`format-notes.md`, "Chat window splits") |

**Linked characters** = the characters the Accounts store pairs to this account
(`accounts::load_roster`, the account id taken from the open
`core_user_<id>.dat` file name), excluding the open character, whose
`core_char_<id>.dat` exists **in the same settings folder** as the open account
file. Each folder has its own copy of the account file, so only that folder's
characters share it. Unpaired characters are ignored (decided 2026-09-28).

The account side is cleaned only if every linked character file decodes and
none lists `<key>` in its `ui → chatchannels`. Otherwise it is kept, and the
result says why (§4.2).

Known ceiling: a linked character that is logged in may have joined the channel
since its file was last written. Its file on disk is what gets checked.

## 4. Architecture

Approach chosen: one command does both halves; the caller resolves the linked
characters' paths and passes them in, so `ops` stays ignorant of pairing — the
same split `overview_window_remove` uses between a required and a conditional
edit.

### 4.1 Model — `crates/settings-model/src/chat.rs`

Beside the existing chat split code. `stacks.rs` is not touched.

- `leave_chat_char(v: &mut Value, window_id: &str) -> Result<String, ChatLeaveError>`:
  validates the shape (§2), applies §3.1, returns `<key>`.
  `ChatLeaveError`: `NotLeavable { window }`, `NotFound { window }`.
  Validation completes before the first mutation, so a refusal leaves the tree
  untouched.
- `chat_channel_keys(v: &Value) -> Vec<String>`: the first element of each
  `ui → chatchannels` row, through the reader `chat_channel_names` already uses
  (refactor it to expose the keys rather than duplicate the walk).
- `leave_chat_account(v: &mut Value, window_id: &str) -> bool`: applies §3.2,
  returns whether anything was removed.

### 4.2 App — `app/src-tauri/src/ops.rs`

```rust
pub fn chat_leave(state: &AppState, window_id: &str, linked: &[PathBuf])
    -> Result<ChatLeaveResult, ErrDto>
```

Under one `undo::group`:

1. `edit_slot(Slot::Char, leave_chat_char)` — required; its error fails the
   command with nothing changed.
2. Decide the account side:
   - account file not open, or read-only (including the account lock) →
     `KeptNoAccountFile`;
   - any linked file fails to read or decode → `KeptUnreadable { chars }`;
   - any linked file lists the key → `KeptShared { chars }`;
   - else `edit_slot(Slot::User, leave_chat_account)` → `Cleaned`.

`ChatLeaveResult { layout: WindowLayout, account: AccountOutcome }`, where
`AccountOutcome` serialises as
`{"outcome": "cleaned" | "kept_shared" | "kept_unreadable" | "kept_no_account_file", "chars": [ids]}`.

Both slot edits are `edit_slot` calls inside one group, so the first captures
both trees and one `Ctrl+Z` reverts both files. `try_edit_char` is not used, so
its caller-list test is untouched.

A shared helper resolves `linked` from `(roots, app dir, open user path, open
char path)`. It is called by the Tauri command in `lib.rs` (roots from
`default_roots()`, dir from `app_dir`) and by the MCP (`self.roots`,
`self.dir`).

Nothing reaches disk until save; each file is backed up by the normal save.

### 4.3 MCP — `app/src-tauri/src/mcp.rs`

A new `layout_edit` op: `{"op": "chat_leave", "window": "chatchannel_…"}`,
added to the `op_item` enum and the tool description. `layout_edit` results
gain a `chat_leave` array with one `{window, account}` entry per op of this kind
in the batch, so an assistant can report what happened to the account file.
One line in `mcp_primer.md`: only player channels and convos can be left, and
the character must be logged out.

### 4.4 UI — `app/src/lib/WindowPanel.svelte`, `LayoutView.svelte`, `api.ts`

- In the chat section at `WindowPanel.svelte:304`, for a leavable window: a
  **Leave channel** button (**Leave conversation** for `chatchannel_private_*`),
  disabled on a read-only file with the usual reason.
- Confirm, house style: "Leave Alpha-Intel? Its window is removed from this
  character's layout." / "Leave channel".
- Toast by outcome:
  - `cleaned` — "Left Alpha-Intel."
  - `kept_shared` — "Left Alpha-Intel. Its account settings stay: {names} still has it."
  - `kept_unreadable` — "Left Alpha-Intel. Its account settings stay: {names}'s file couldn't be read."
  - `kept_no_account_file` — "Left Alpha-Intel. Open the account file to clear its account settings too."
- A command-palette entry (`layout.leaveChat`), enabled when a leavable chat
  window is selected.

## 5. Testing

- **Model** (`chat.rs` unit tests):
  - char side purges every `windows` dict, nested pref dicts, the dialog window
    and the `chatchannels` row, with Bytes and Str keys;
  - leaves every other window and row alone;
  - the edited tree still encodes;
  - standing channels and unknown shapes refused with the tree unchanged;
  - `NotFound`;
  - account side removes only what exists and creates nothing.
- **Corpus** (`tests/chat_names_corpus.rs` or a sibling): every
  `chatPlayerChannelsJoined` key across `testdata/corpus` has the `player_`
  shape, and every `chatchannels` first element is `player_*`, `private_*` or a
  standing name, so §2's classification covers the real world.
- **ops**: one undo reverts both files; each of the four outcomes.
- **MCP stdio**: `chat_leave` op round trip, result carries the outcome.
- **UI spec**: button present only on leavable windows; toast per outcome.
- **Live, before release**, on Pilot Echo:
  1. Leave a private conversation; log in and out; it stays gone.
  2. With the account side cleaned, the Alpha-Intel join suggestion is gone.
     If it survives, the account cleanup is tidiness only — keep it, and make
     no promise about suggestions in the release note.

## 6. Documentation

`format-notes.md` gains a "Leaving a chat channel" section: the `chatchannels`
row drives recreation (§1), and the account-side keys of §3.2 including
`chatPlayerChannelsJoined`.
