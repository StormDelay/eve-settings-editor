# Codec re-share foundation (design)

Date: 2026-07-18
Status: approved, pre-plan.
Builds on: `blue-marshal` encoder (`encode.rs`), `settings-model`
`treewalk::inline_all`, and the inline-first structural editors
(`overview.rs`, `autofill.rs`, `batch.rs`).
Unblocks: the layout-canvas window-stacks milestone (membership editing) —
see `docs/superpowers/specs/2026-07-18-layout-canvas-window-stacks-design.md`.

## 1. Problem

The encoder replays the decoded `Shared`/`Ref` slot structure exactly, which is
what makes `decode → encode` byte-identical across all 5,022 corpus files (the
project's core safety gate). But structural editors can't edit a shared tree
directly — `mutate.rs` refuses to remove a `Shared` subtree, and dropping a
`Shared` definition that a `Ref` elsewhere still needs fails to encode
(`RefBeforeStore`). So every structural editor first calls `treewalk::inline_all`
to drop *all* sharing, then encodes the fully-inlined tree: valid, but ~1.5×
larger, and it works in-game only because EVE re-deduplicates the file on its
next save. Removing that reliance is the goal, and it is the clean foundation
the window-stacks membership editor needs.

## 2. Goal & scope

Add a **`reshare` canonicalization pass** that re-introduces valid, compact,
immutable-only `Shared`/`Ref` before encode on the inline-first edit paths, so
edited files are compact and self-contained (no dependence on EVE's self-heal).
Also fold in the long-deferred `inline_user` dedup cleanup.

**Non-goals** (§6): matching CCP's exact bytes, removing the fidelity tags, and
sharing mutable containers are all explicitly out.

## 3. Architecture — `reshare` in `blue-marshal`

A pure `pub fn reshare(&Value) -> Value` beside `encode`:

- Count occurrences of each **shareable-immutable** subtree by structural
  equality. Shareable-immutable = the `storable_with_flag` kinds that are also
  immutable: `Bytes` (len ≥ 2), `Long`, `Global`, and non-empty `Tuple`s whose
  elements are all (recursively) immutable. Never `List` / `Dict` / `Instance`
  / `Reduce` / `Stream`, and never a `Tuple` containing any of those.
- Walk the tree in **encode traversal order** (dict value-before-key, tuple/list
  in order, etc.). On the first occurrence of a repeated shareable-immutable,
  wrap it `Shared { slot }` (slot = next counter); on later occurrences emit
  `Ref(slot)`. Non-repeated and non-shareable nodes pass through, recursing into
  children.
- First-occurrence-stores-before-any-ref plus emit-order slot assignment makes
  the output satisfy the encoder's store-before-ref invariant with contiguous
  `1..=count` slots by construction. Slots are numbered in encounter order, so
  the tail map is identity-ordered (this is where the output legitimately
  differs from CCP's non-encounter-order numbering — see §6).

Why a separate `Value → Value` pass rather than teaching `encode` to dedup on
the fly: it isolates the new logic and leaves the corpus-critical byte-identical
replay encoder completely untouched. The coupling to encode traversal order is
self-checking — any drift makes `reshare` emit a `Ref` before its `Shared`, which
the encoder already rejects with `RefBeforeStore`, so tests catch it immediately.

The recursive "is this subtree immutable" predicate lives here too, next to
`storable_with_flag`.

## 4. Integration (`settings-model`)

- The inline-first structural editors change from *inline → edit → encode* to
  *inline → edit → **reshare** → encode*. Concretely, wherever a command hands a
  fully-inlined tree to the save chain today, it hands `reshare(&tree)` instead.
  Touches `overview.rs`, `autofill.rs`, `batch.rs`, and the future stacks
  membership command. Nothing else moves.
- The in-place `SetScalar` path (M2 layout geometry, raw-tree edits) is
  **untouched**: it preserves the original sharing, and the replay encoder is
  already compact and byte-faithful there. `reshare` is invoked only by the
  paths that currently inline — not baked into the generic save chain — so the
  byte-faithful paths keep their behavior and the diff stays minimal.
- Resharded files re-open **Editable**: they are valid `Shared`/`Ref` trees, so
  `encode(decode(bytes)) == bytes` (replay) holds, same as inlined files today.
- Folds in the deferred cleanup: delete `overview.rs`'s private `inline_user`;
  all structural editors use `treewalk::inline_all` for the edit and
  `blue_marshal::reshare` for the compaction.

## 5. Testing

- **`blue-marshal` unit tests:** `reshare` never wraps a mutable container or a
  tuple containing one; a tree with repeated immutable values returns with the
  repeats shared and non-repeats untouched; `inline_all(decode(encode(reshare(t))))`
  equals the inlined `t` (semantic preservation); `reshare` is deterministic and
  idempotent; `encode(reshare(t)).len() <= encode(inline_all_of(t)).len()` (it
  compacts, never bloats).
- **Corpus gate (strong):** for every corpus file,
  `inline_all(decode(bytes))` → `reshare` → `encode` → `decode` → `inline_all`
  equals `inline_all(decode(bytes))` (reshare preserves semantics over the whole
  corpus), and each reshared encode is no larger than the inlined encode. Lives
  alongside the existing byte-identical round-trip gate, which stays green and
  unchanged.
- **`settings-model`:** existing overview/autofill/batch encode tests assert the
  saved output is compact (resharded) and re-opens Editable; add a no-bloat size
  assertion so a regression that silently inlines is caught.

## 6. Non-goals (explicit)

- **Byte-identity to the client.** CCP's serializer numbers shared slots in a
  non-encounter order (963,660 such tail-map entries in the corpus) whose scheme
  is opaque; reproducing it is almost certainly infeasible. `reshare` produces
  valid identity-ordered slots, so an unedited file round-tripped through
  `inline_all → reshare` is compact and valid but not byte-identical to CCP's
  original. Edited files were never byte-identical anyway. The byte-identical
  gate continues to protect the pristine replay path, not the reshare path.
- **Removing the `Shared`/`Ref` fidelity tags / unifying decode+encode.** Depends
  on the infeasible byte-identity above; the tags stay.
- **Sharing mutable containers** (`List`/`Dict`/`Instance`/`Reduce`/`Stream`).
  CCP shares by object identity; `reshare` can only match by structural
  equality, which for mutables could alias values the client kept distinct (an
  in-session mutation-aliasing edge). The dominant bloat is repeated immutable
  byte-strings (window ids, overview column names, autofill widget paths), so
  the immutable-only set recovers essentially all the size at zero aliasing
  risk. Revisitable behind a file-size measurement if ever justified.
