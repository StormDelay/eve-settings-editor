# Editing the chat window splits — member list and input box (design)

Status: designed 2026-07-30.

Milestone context: the **layout depth** milestone. This is the editing half of the
ledger entry the canvas detail layer opened, "Draggable chat splits and overview
column edges on the canvas" (`docs/small-tasks.md`) — narrowed to the chat half,
and to fields rather than dragging (§3).

**Branch dependency:** built on `worktree-canvas-detail-layer` (PR #47), which
adds the `chat.rs` projection this extends. It cannot merge to master before that
does.

## 1. Goal

The detail layer draws each chat window's member-list and input-box splits from
the real stored values, but they are decoration — nothing can change them. This
slice makes both editable, per channel and across a whole chat stack.

## 2. Where the values live (verified 2026-07-30)

Confirmed by dumping three real account files and inventorying every chat key
family. There are exactly **two** geometry keys per channel, both in the account
file's root `ui` section, both ordinary `(timestamp, value)` leaves holding an
**absolute pixel count**:

| what | key | observed values |
|---|---|---|
| Member list width | `chatchannel_<ch>_userlistwidth` | 50, 102, 104, 107, 126, 135 |
| Input box height | `chatinputsize_chatchannel_<ch>` | 63, 64 |

**There is no key for the chat history area.** It is whatever is left after the
member list and the input box are subtracted from the window. So "resize the
history area" is not a thing the file can express — you move the two edges
inward, and the history takes the remainder. The panel says so by showing the
remainder rather than offering a third field.

The full chat key inventory, for the record: `chatfontsize_*`,
`chatCondensedUserList_*`, `chatWindowBlink_*`, `chatchannel_<ch>_mode`, and the
account-wide `timestampchat`, `guestCondensedUserList`, `logmessageamount`. None
of them is geometry.

### 2.1 The split and the window are in different files

The split is **account**-scoped; the window's own `x/y/w/h` is **character**-
scoped (`windowSizesAndPositions_1`). Cross-referencing one real pair:
`chatchannel_player_-78564080` is a 256×424 window carrying a 104px member list
and a 63px input — 41% and 15% of it.

Two consequences the design has to respect:

- Two characters on one account **share the split but not the window size**. The
  same 135px member list is a third of one character's window and half of
  another's.
- Nothing keeps a stored width inside the window it is drawn in. Shrink the
  window on one character and the account-wide split can exceed it.

This is why §6 refuses to clamp against the open character's geometry.

## 3. Fields, not dragging

The ledger entry imagined dragging the splitter on the canvas. This slice uses
numeric fields on the selected window instead, and drops the drag:

- The detail layer is `pointer-events: none` **by construction** — that one
  declaration is what guarantees decoration can never swallow a canvas gesture,
  and it is pinned by a test. Dragging a splitter means punching a hole in it,
  adding `Drag` variants, and adding hit-test exclusions so a split drag does not
  start a window move underneath.
- At a typical canvas scale of ~0.3 a chat window is about 77×127 screen px and
  its input band about 19px tall. That is not a comfortable drag target.
- Fields can express the stack apply (§5); a drag cannot.

Dragging stays in the ledger for the canvas, unblocked by this.

## 4. Backend

### 4.1 `chat.rs` gains a setter

The module is read-only today. It gains one function, structured exactly like
`hud.rs::set_hud_value` — same section resolution, same mint, same reshare rule:

```rust
pub enum ChatField { UserlistWidth, InputHeight }

pub fn set_chat_splits(
    user_root: &mut Value,
    ids: &[String],
    userlist: Option<i64>,
    input: Option<i64>,
) -> Result<(), ChatError>;
```

- **Both fields in one call, both optional.** Editing one field passes one id and
  one `Some`; the stack apply passes N ids and both. One op rather than two means
  the stack apply cannot half-succeed and leave a stack with mismatched splits.
- **Key present** → build a `Mutation::SetScalar` and run it through
  `mutate::apply`. No reshare: a scalar overwrite is not a structural edit.
- **Key absent** → `inline_all`, insert `Value::Tuple([Long(0), Int(v)])` under
  the `ui` section, then `reshare`. The zero-timestamp mint is already proven on
  real files by the overview-presets container and by `set_hud_value`.
- **Ids are validated against `chatchannel_*`** and anything else is refused. The
  key names are built by string concatenation, so an unchecked id would happily
  mint `market_userlistwidth` — a key EVE will never read and nothing will ever
  clean up.
- **A negative value is refused** with a `ChatError`, not silently clamped to 0.
  Silently rewriting a number the user typed is how a field stops being
  trustworthy; a refusal surfaces through the same error dialog every other edit
  path uses.

### 4.2 `ops.rs` and the command

`set_chat_splits(ids, userlist, input) -> Vec<ChatPanel>` — locks the **user**
slot only (the character document is not touched), marks it dirty, and returns
the fresh projection so the frontend never re-derives anything. Same shape as
`set_hud_field`.

A missing account file is an error here, unlike the read path: there is nowhere
to write. The UI disables the fields in that state (§6), so it is a guard rather
than a path users hit.

## 5. Frontend

### 5.1 Where the fields go

`WindowPanel.svelte`'s `{#snippet detail(w)}` is rendered from **three** call
sites — free window, stack container, stack member. Extending that one snippet
covers every case, and stack members are the case that matters most: real chat
windows almost always live in `ChatWindowStack`.

The block itself is a new **`ChatSplit.svelte`**, not inline markup.
`WindowPanel` is already 720 lines with one job, and this repo's precedent is to
split rather than grow it — `HudPanel` came out of exactly this situation.

It renders only for ids matching `chatchannel_*`, the only ids these keys exist
for.

Contents:

- two number inputs, Member list and Input box;
- a read-only line showing the resulting history area, computed from **this
  character's** `w.geom`;
- the account-wide legend naming sibling characters, the same treatment
  `HudPanel` gives its account-scoped rows;
- the stack button (§5.2).

### 5.2 Apply to the stack

When the selected chat window belongs to a stack, a button reading **"Apply to
all N channels in this stack"**. It writes the window's two current values to
every chat channel in that stack, in one call.

The targets come from a pure helper in `app/src/lib/detail.ts`, beside
`windowDetail` and `overviewIndex` — the chat-window id knowledge already lives
there, and putting it anywhere else would be a second place that knows what a
chat id looks like:

```ts
export function chatStackTargets(stack: Stack, windows: WindowRect[]): string[]
```

— the stack's members filtered to `chatchannel_*`, so a non-chat window sharing
the stack is skipped rather than having a meaningless key minted for it. The
button is hidden entirely when the window is not stacked, rather than shown
disabled: an unstacked chat window has no stack to apply to, and a permanently
dead control invites the question every time.

**No modal confirm.** It writes N account-file keys, which the account-wide
legend already announces, and the change lands in the open document — Discard and
the backup chain both cover it. The repo reserves confirms for deletion
(`onDeleteOrphans`), and this destroys nothing. The count in the button label is
what makes the blast radius visible before the click.

### 5.3 Wiring

`LayoutView` already loads `chats: ChatPanel[]`. It passes `chats` and one
callback down through `WindowPanel` into `ChatSplit`; the callback calls
`api.setChatSplits(...)`, takes the returned projection, and marks the `user`
slot dirty — the same three lines `setHud` uses.

## 6. Bounds, and what is deliberately not validated

A negative value is refused (§4.1). Nothing else is bounded — in particular
values are **not** clamped to the window's width or height.

Clamping is the obvious thing to want and it is wrong here. The split is
account-scoped; the geometry is character-scoped (§2.1). Clamping against
whichever character happens to be open would write a value chosen for that
character's window into a setting every sibling shares — silently making the
number wrong for the others, in a way nothing on screen would explain.

Instead the panel shows the consequence: the history-area line is computed from
the open character's geometry and, when a split leaves nothing for it, goes
negative and turns warning-coloured. That reports what is true for the character
in front of you without pretending it is authoritative for the account.

Disabled states, all reusing props `LayoutView` already threads: no account file
open, and `accountReadOnly`. Both show the panel's existing "Not present in this
file" tooltip.

## 7. Testing

**Rust (`chat.rs`)**, synthetic trees with invented channel names:

- set an existing key, re-project, value changed;
- mint an absent key, re-project, value present with a zero timestamp;
- several ids in one call, all written;
- both fields in one call;
- a non-chat id (`market`) is refused and **nothing is written** — including no
  partial write of the valid ids alongside it;
- a negative value is refused;
- a `Shared` key whose value is a `Ref` still resolves on the write path, not
  only the read path;
- a malformed existing value is refused rather than clobbered, matching the read
  path's skip.

**Frontend (`node --test`):** `chatStackTargets` filters non-chat members, returns
`[]` for a stack with none, and preserves member order. The history-area maths,
including the negative case.

**Component (`ChatSplit.spec.ts`, vitest + jsdom, alongside `HudPanel.spec.ts`):**
the block renders for a `chatchannel_*` id and not for others; the stack button
is absent when unstacked and names the right count when stacked; inputs disable
when the account document is unavailable or read-only.

## 8. Non-goals

- **Dragging the splitter on the canvas** — §3. Stays in the ledger.
- **Overview column-width editing**, the other half of that ledger entry. The
  Overview view already edits column widths; wiring the same thing into the
  Layout view is a separate question about where that control belongs.
- **A third field for the history area.** There is no key; §2.
- **`chatfontsize_*`, `chatCondensedUserList_*` and the other chat keys.** None is
  geometry, and none is what this slice is for.
- **Applying across every channel in the file** rather than a stack. The stack is
  the grouping the player actually maintains; "all 30+ channels" includes
  long-dead private conversations.
- **Clamping to the window** — §6.
