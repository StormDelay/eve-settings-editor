# Layout canvas — window stacks (design)

Date: 2026-07-18
Status: approved design, **deferred** — depends on the codec/refactor
foundation (encoder-side auto-dedup) landing first. See §10.
Builds on: M2 layout canvas (`windows.rs` projection, `LayoutView.svelte`,
`WindowPanel.svelte`) and docs/format-notes.md window-geometry mapping.

## 1. Goal

Model EVE window stacks properly and make the layout canvas treat a stack as
one coherent group instead of several independent rectangles: draw one tabbed
rectangle per open stack, move/resize it as a unit, and edit membership
(unstack, add to an existing stack, reorder tabs). Creating a brand-new stack
from two free-floating windows is gated behind a live capture experiment (§7).

This replaces the current `stacksWindows` handling, which is dead on real data
(see §2, finding 2).

## 2. The stack model (established from the corpus, 2026-07-18)

Worked out by decoding real `core_char_*.dat` files (fresh-baseline and
historical snapshots) with `bmdump` and resolving the window-id references.
A stack is defined by **two** sibling dicts under `b"windows"`, both keyed by
window id:

- **`stacksWindows`**: `member_id → container_id`. The value is *another
  window's id* (the stack's frame/anchor window), resolved through Ref/Shared.
  The value may also be `None` (member explicitly not stacked).
- **`preferredIdxInStack3`**: `container_id → { member_id → tab_index }` — the
  tab order within the stack.

A container is itself a real, positioned window: a named frame
(`ChatWindowStack`, `invitestack`) or an arbitrary window id, including
numeric-string ids like `b"76"` produced when two normal windows are tabbed
together. Confirmed: a numeric container (`b"76"`) has its own geometry entry
and open flag, exactly like any other window.

Findings that shaped this design:

1. **The container's geometry is the stack's true position.** In a
   freshly-made stack, every member + the container share the identical rect.
   In long-lived files, members keep stale last-floating geometry (different
   positions, sometimes different saved resolutions), and the container may
   have no geometry entry at all. The currently-active member matches the
   container's rect; stale members don't. So the app anchors a stack on the
   container's geometry (fallback: the frontmost open member), draws one rect,
   and repairs member drift on move.
2. **Stack ids are never integers.** In a 150-file sample: 0 plain-integer
   stack values, 6,624 window-id references, 564 `None`. The current
   `windows.rs::stack_field` only surfaces a stack when the value is
   `Value::Int`, so on real files it surfaces nothing — the "stack id" number
   input in the panel is dead code. Fixing this is part of the milestone.
3. **`preferredIdxInStack3` is a soft ordering hint.** Indices collide (many
   members share one index) and include closed/absent members. Treat it as a
   preferred order, not a strict 0..N; normalize to 0..N on write.
4. **Most members are chat channels, usually closed.** A char file can list
   30–40 members but only a few are open at once, so the canvas grouping
   problem is real but small at any moment (the canvas draws open windows
   only).

All example ids above are the real client's own well-known window names
(`ChatWindowStack`, etc.) or synthetic stand-ins; no character/account ids
appear here, per the repo data rule.

## 3. Backend — projection (`windows.rs`)

Replace the dead `StackField` (Int-only) with real grouping. Pure read
projection — no format knowledge leaks to the UI.

- Resolve `stacksWindows` (member → container, skipping `None`) and
  `preferredIdxInStack3` through Ref/Shared, the same way flag keys already are
  (`effective(k, shared)`).
- Add to `WindowLayout`: `stacks: Vec<Stack>`, where
  `Stack { container_id, container_label, members: Vec<String> }`, members
  ordered by preferred index then id.
- Add to each `WindowRect`: `stack: Option<StackRef>` where
  `StackRef { container_id, role: Container | Member }`, so the frontend can
  group and know which rect is the anchor.
- Anchor rect for a stack = the container's geom if present, else the frontmost
  open member's geom.

Malformed/missing dicts are skipped, never panic (matches the existing
projection contract).

## 4. Backend — authoring

Two write paths, split by what they touch.

**Geometry (coherent move/resize)** — pure `SetScalar`, no structural change.
Moving/resizing a stack emits `set_scalar` on the geometry elements of the
container *and every open member*, all pointing at the reference resolution.
Reuses the existing M2 geometry-mutation path (the projection hands over each
window's `x/y/w/h` paths). Repairing stale member drift is a free side effect.
No dependency on the codec work — this path could ship independently.

**Membership (unstack / add / reorder)** — structural edits to `stacksWindows`
+ `preferredIdxInStack3`, whose keys/values are `Shared` window-id stores.
`RemoveEntry` refuses shared subtrees, so these are not raw mutations. A
dedicated `ops.rs` command (mirroring overview/autofill editing) that operates
on a fully-inlined `windows` subtree, edits plain values, and re-encodes:

- **Unstack M**: remove M's `stacksWindows` entry and its
  `preferredIdxInStack3[container]` entry.
- **Add M to container C**: set/insert `stacksWindows[M] = C` and append
  `preferredIdxInStack3[C][M] = nextIdx`.
- **Reorder within C**: rewrite `preferredIdxInStack3[C]` to clean 0..N indices.

**Codec dependency.** Post-foundation, this command inlines, edits, and lets
the encoder re-derive correct canonical `Shared`/`Ref` sharing — no manual
`inline_all` bloat, and the output matches what the client writes. Without the
foundation this would fall back to the inline-first hack (valid but ~1.5× and
reliant on the client re-deduplicating); the milestone ordering exists to avoid
that. See §9.

All writes go through the unchanged verify → backup → atomic-write save chain.

## 5. Frontend — canvas (`LayoutView.svelte`)

- Group open windows by stack. Each stack draws **one** rectangle at its anchor
  geom, with member names as tabs along the top; non-stacked windows draw as
  today.
- Selecting the stack rect selects the stack; clicking a tab selects that
  member (drives the detail panel). Move/resize acts on the whole stack
  (coherent write, §4).
- Stale individual member positions are not drawn — that is the
  "coherent, not scattered" fix.

## 6. Frontend — panel (`WindowPanel.svelte`)

- Group rows by stack: a stack group header (container label + member count),
  members listed as ordered sub-rows in tab order; non-stacked windows list
  flat as today.
- Per-member controls: **unstack**; **reorder** (up/down) within the stack.
- Per non-stacked window: **add to stack →** a picker of existing stacks (and,
  if new-stack creation is enabled after §7, stack-with-another-window).
- A selected member still shows its geom/flags detail as today. Shared stack
  geometry is edited from the canvas or the container's detail.
- Removes the dead "stack id" number input in favour of this grouping UI.

Membership editing lives in the panel (buttons/pickers) in V1; canvas
drag-to-stack is deferred polish (§9).

## 7. Live experiment — new-stack creation

Before implementing "create a new stack from two free windows," run an
exp-style capture (the docs/format-notes.md exp1–5 method): in-game, take two
free-floating windows, tab them together, log out, diff the before/after
`core_char` dumps. Determine:

- what container id is used (one of the two windows becomes the frame, vs a
  minted numeric id), and
- the exact `stacksWindows` + `preferredIdxInStack3` deltas.

If the container is deterministic/predictable, include new-stack creation.
Otherwise ship unstack / add-to-existing / reorder only and defer creation.
Record findings in docs/format-notes.md (synthetic ids only).

## 8. Testing

- **Rust (`settings-model`):** projection unit tests over synthetic trees —
  stack grouping (member→container resolution through Ref/Shared), tab ordering
  from `preferredIdxInStack3`, anchor-geom selection (container present/absent),
  `None` values skipped, colliding-index normalization. Authoring-command
  tests: unstack removes both entries; add inserts both; reorder rewrites
  indices; and a round-trip re-encode is canonical (re-opens Editable, no
  dangling refs, matches the encoder's dedup once the foundation lands).
- **Frontend (`node --test`, zero-dep):** pure logic — grouping open windows by
  stack, tab-order sort, coherent-move geometry fan-out. DOM drag not unit
  tested (consistent with M2).
- **Manual smoke (live, project norm):** stack/unstack/reorder on a real char
  file through the app, save, reopen Editable, confirm in-game.

## 9. Dependencies, scope, deferred

- **Depends on** the codec/refactor foundation (general encoder-side auto-dedup
  so any editor can inline → edit → reshare). Implement stacks-B after that
  lands. The geometry-coherent-move path (§4) has no such dependency and could
  ship earlier if desired.
- **Sibling milestone item** "resize a layout window from any corner" is
  independent (no codec dependency) — it can ship separately, and its resize
  handles are what the coherent stack resize reuses.
- **New-stack creation** gated on the §8 experiment.
- **Deferred:** canvas drag-to-stack (panel-button membership editing in V1);
  authoring the exact stale-member geometry semantics beyond "sync open members
  to the anchor on move."
