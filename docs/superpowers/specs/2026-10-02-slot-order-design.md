# Ship slot layouts (design)

Status: designed 2026-10-02, not yet planned.

## 1. Goal

When a player drags modules around the ship HUD, EVE remembers the arrangement
**per hull**: per ship item id, not per ship type. It stores it in the account
file. A player with several accounts arranges the same ship again on every one,
and a fresh account starts with no arrangements at all.

This slice ships:

1. **A Racks view** that lists every ship id in the open account file and edits
   its layout: drag to swap two positions, add a ship, remove one, and copy one
   ship's layout onto others.
2. **A folder merge** in the batch view. It collects every ship layout from the
   ticked account files of a profile and writes the merged set back into each
   of them, so every character on every account finds every ship arranged.
3. **The same in the MCP server**: an editor tool pair, plus a merge
   preview/apply pair beside `copy_files`.

Out of scope: naming ships (the file holds bare item ids, and ESI only maps
them through an authenticated assets call); pruning ships that no longer exist
(the client never prunes either); a slot-order aspect for the character copy
(`everything` already carries it inside the whole file).

## 2. The format (confirmed from the client code and the corpus)

### 2.1 Wire shape

`ui -> slotOrder`, account file (`core_user_<id>.dat`), wrapped like every `ui`
key as `(FILETIME, payload)`. Payload: `{shipItemID: [24 Int]}`.

- **Key** = `session.shipid`, the item id of the ship (`Long`, e.g. ~1e12).
- **Value** = 24 inventory flags, always a permutation of **11–34**:
  low slots 11–18, mid slots 19–26, high slots 27–34.

Measured 2026-10-02 over 157 account files: 21,720 entries, every one exactly 24
flags drawn from 11–34. The most common orders are **not** the default order. A
player drags mid slots into the top row, and that shows up as mid flags in
positions 0–7.

### 2.2 What the client does with it

From `eve/client/script/ui/inflight/shipHud/__init__.py` and `slotsContainer.py`
(decoded from `code.ccp`, 2026-10-02):

- `GetSlotOrder()` builds the **default** `[Hi0..7, Med0..7, Lo0..7]` =
  `27..34, 19..26, 11..18`, then returns
  `settings.user.ui.Get('slotOrder', {}).get(session.shipid, default)`.
  An absent entry means the default order.
- `InitDrawSlots` reads `myOrder[r * 8 + i]` for `r in range(3)`,
  `i in range(8)`. So **position `p` is row `p // 8`, column `p % 8`**. The row
  grid is `[[1.0, 0.0], [1.5, 1.0], [1.0, 2.0]]` (x offset, y): row 0 is the
  **top** rack, row 1 the **middle** (staggered half a column), row 2 the
  **bottom**. This matches the HUD stagger measured in format-notes
  "Ship HUD internals".
- `SwapSlots(flag1, flag2)` copies the current order, swaps the two flags'
  positions (`current.index(flag)`), writes the entry for `session.shipid` back,
  and redraws. **This is the only writer.** No code path deletes an entry.

A malformed entry breaks the HUD. A short list raises `IndexError` in
`InitDrawSlots`, and a missing flag raises `ValueError` in `SwapSlots`. So
**every write here is a full 24-flag permutation of 11–34**, and nothing else
reaches the file.

Deleting an entry is therefore exactly "reset this ship to the default order".

## 3. Backend

### 3.1 `crates/settings-model/src/slot_order.rs`

All format knowledge lives here, in the shape of `fleet.rs`'s watch-list map.

```rust
pub const DEFAULT_ORDER: [u8; 24] = [27..=34, 19..=26, 11..=18]; // flattened
pub struct SlotEntry { pub ship_id: u64, pub order: Option<[u8; 24]> }
pub fn project(user: &Value) -> Vec<SlotEntry>
pub fn add(user: &mut Value, ship_id: u64, from: Option<u64>) -> Result<(), SlotOrderError>
pub fn set(user: &mut Value, ship_id: u64, order: [u8; 24]) -> Result<(), SlotOrderError>
pub fn remove(user: &mut Value, ship_id: u64) -> Result<(), SlotOrderError>
pub fn swap(user: &mut Value, ship_id: u64, a: u8, b: u8) -> Result<(), SlotOrderError> // by flag
pub fn slot_label(flag: u8) -> String          // 27 -> "H1", 19 -> "M1", 11 -> "L1"
pub fn parse_slot(s: &str) -> Option<u8>       // "H1" or "27" -> 27
pub fn copy(user: &mut Value, from: u64, to: &[u64]) -> Result<(), SlotOrderError>
pub fn merge(files: &[(PathBuf, SystemTime, &Value)]) -> Merge
```

- `project` returns the entries in file order. An entry whose value is not a
  24-flag permutation projects `order: None` (shown as **unreadable**) instead
  of vanishing, so its id cannot be added again as a duplicate key. An
  unreadable entry can be removed or overwritten by `set`.
- `add` appends `(key, list)`, where the list is `from`'s order, or
  `DEFAULT_ORDER` when `from` is `None`.
  - It fails with `AlreadyPresent` when the ship exists, and with `Missing` or
    `Unreadable` when `from` is unusable.
  - When `slotOrder` is absent it mints `(zero FILETIME, {})`, the same way
    `fleet.rs` mints its map (format-notes "A minted zero timestamp is safe").
  - Keys are written as the minimal-width `Long`. Ids are matched by value, not
    by wire kind.
- `set` validates that `order` is a permutation of 11–34 (`InvalidOrder`
  otherwise), then overwrites the existing entry's list in place. It fails with
  `Missing` when the ship is absent.
- `remove` deletes the entry. It fails with `Missing` when the ship is absent.
- `copy` writes `from`'s order onto each target, adding the ones that are
  absent: this is the one path that may either add or overwrite, and copying
  says so. A `from` that is missing or unreadable is an error and writes
  nothing.
- `merge` builds the union of all entries:
  - When the files disagree on a ship, the entry from the file with the newest
    mtime wins. Files with equal mtimes are ordered by path, so the result is
    deterministic.
  - Unreadable entries never take part. They neither win nor get overwritten in
    their own file.
  - It returns the merged map, a `conflicts` list (`ship_id`, the winning file,
    the losing files), and per file `{ gained, overwritten }`: the counts of
    ships it lacks and of ships whose order differs from the merged one.

`slotOrder` is a whole-`ui`-key edit, so inlining follows the existing
inline-first idiom (`autofill.rs`, `fleet.rs`).

### 3.2 `ops.rs` and `lib.rs` (the editor)

- `slot_orders(state) -> Vec<SlotEntry>` needs the account file open.
- `add_slot_order(state, ship_id, from)`, `set_slot_order(state, ship_id, order)`,
  `remove_slot_order(state, ship_id)` and `copy_slot_order(state, from, to)`
  each call `edit_slot` on `Slot::User`.
  Each is one Tauri command and one undo entry, and goes through the normal
  `save`.

### 3.3 `setup.rs` (the merge)

Beside `copy_files`, with the same path checks against discovery:

- `slot_order_merge_preview(roots, files: &[String]) -> MergePlan`
  - Every path must be a discovered **user** file, otherwise that row is
    excluded with its reason.
  - It decodes each file, runs `merge`, and returns per file
    `{ path, account_id, gained, overwritten, unchanged: bool }` plus
    `conflicts`.
- `slot_order_merge_apply(roots, files) -> Vec<TargetResult>`
  - It recomputes the plan from disk, so the files can't change between preview
    and apply.
  - For each file that changes, it writes the merged map into `ui -> slotOrder`
    through `Document` + `save` (backup first). The file's own unreadable
    entries are carried over untouched.
  - Unchanged files are skipped and reported `ok` with no backup.
  - Results use `TargetResult`, as `copy_files` does.
- An account file that is open in the editor with unsaved edits is handled
  exactly as `setup_apply` handles it today. This slice does not invent a new
  rule.

The source and the target are the same ticked list: a ticked file is read and
written, and an unticked file is neither.

### 3.4 `api.ts`

`slotOrders()`, `addSlotOrder(shipId, from | null)`, `setSlotOrder(shipId, order)`,
`removeSlotOrder(shipId)`,
`copySlotOrder(from, to)`, `slotOrderMergePreview(files)`,
`slotOrderMergeApply(files)`. Ship ids cross IPC as plain `number`s, as
`setWatchlistColour(charId: number)` does. Item ids are ~1e12, far below 2^53.

## 4. UI

### 4.1 The Racks view

- `View` gains `"racks"` (label **Racks**, EVE's own word for a slot row), placed
  before Raw. Not `"slots"`: the shell already says `subject.slots` for the
  char/user document pair. It is account-scoped
  (`ACCOUNT_SCOPED` → `ScopeBanner`) and available when an account file is open.
- **Left:** a filterable list of ship ids (`SearchField` filtering by
  substring). Unreadable entries show a **unreadable** chip.
- **Right:** the selected ship as the HUD's three racks. There are 3 rows × 8
  round buttons, the middle row offset half a column (the shape `DetailParts`
  already draws). Each button shows its flag as **H1–H8 / M1–M8 / L1–L8**,
  coloured by rack. Dragging one button onto another swaps them, which is
  exactly `SwapSlots`. Keyboard: select a button, then the arrow keys and Enter
  swap.
- Actions on the selected ship:
  - **Remove ship** = remove the entry, which resets the ship to the default
    order in-game.
  - **Copy to…** takes a multi-select of the other ids.
- **Add ship:** an id field, starting from the selected ship's order or the
  default (`add`; an id already listed is refused inline).
- Edits are live, with undo and save as in every other editor.

### 4.2 The batch view

`BatchView` gains a fourth source kind, **"Ship slot layouts, merged across the
profile"**:

- There is no source picker and no aspects. The target list shows the profile's
  **account** files, all ticked by default, each labelled the way the batch
  view already labels accounts.
- The preview lists, per file, "gains N ships, N changed" or "no change", and
  then the conflicts ("ship 90000001: kept the order from account B, newest").
- Apply goes through `slot_order_merge_apply`, and results render like
  `copy_files` results.

## 5. MCP

- `slot_order_get` returns `{ships: [{ship_id, rows: {top, middle, bottom}}]}`,
  each row 8 labels `H1`…`L8`, so the model never needs the flag table. An
  unreadable entry is `{ship_id, unreadable: true}`.
- `slot_order_edit` is a batch, like `fleet_edit` (one undo step, first failure
  rolls back). Ops:
  - `add {ship_id, from?}` adds a ship. It starts from another ship's order,
    or from the default order when `from` is omitted. It is refused when the
    ship is already present, so it can never silently overwrite.
  - `remove {ship_id}` removes the ship, and with it any custom layout: the
    client falls back to the default order. It is refused when the ship is
    absent.
  - `set {ship_id, order}` replaces an existing ship's order: 24 strings, each
    a label (`H1`) or a flag (`"27"`). The MCP schema rules forbid a union
    type, so both forms are strings. It is refused when the ship is absent; use
    `add`.
  - `swap {ship_id, a, b}` swaps two slots, each a label or a flag string. It
    swaps by slot, exactly as the client's `SwapSlots(flag1, flag2)` does.
  - `copy {from, to: []}` adds or overwrites each target.

  Nothing reaches disk until `save`.
- `slot_order_merge_preview {files?}` / `slot_order_merge_apply {files?}`.
  `files` defaults to every account file in the first profile folder. Both are
  listed with the write-immediately tools (`copy_apply`, `copy_files`). The
  apply description says to preview first and show the user the plan.
- The primer gains one line: flags 11–18 low, 19–26 mid, 27–34 high; positions
  0–7 top row, 8–15 middle, 16–23 bottom.

## 6. Tests

All fixtures are synthetic: ships `90000001…`, accounts `90000101…`.

- **Model:**
  - project / add / set / remove / copy round-trips; `add` refuses a present ship and `set`/`remove` an absent one;
  - `set` refuses a short list, a repeated flag, and a flag outside 11–34;
  - minting on an absent key;
  - an unreadable entry projects as `None` and survives a merge untouched;
  - `merge` with three files: a union, one conflict won by the newest mtime, and
    the per-file gained/overwritten counts.
- **Setup:**
  - merge preview and apply over temp copies, with a backup per changed file;
  - an unchanged file is not written;
  - a char-file path is excluded.
- **MCP:**
  - `slot_order_edit` swap → save → reopen round trip;
  - merge preview/apply beside the existing copy tests.
- **UI specs:**
  - a drag swap calls `setSlotOrder` with the swapped order;
  - the batch view's fourth source lists only account files, all ticked.

## 7. Docs

- `docs/format-notes.md` gains a section "Ship slot order", holding §2 above.
- `docs/settings-field-reference.md`'s `slotOrder` mention points to it.
- `CHANGELOG.md` is written at release time, as for every other feature.

## 8. Definition of done

- The Racks view and the batch merge work in the app.
- The MCP tools round-trip.
- `cargo test` and `npm test` pass (verified by exit code).
- In-game check: after a merge, a ship arranged on account A shows the same HUD
  order when boarded on account B.
