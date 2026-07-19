# Layout canvas — window stacks (design)

Date: 2026-07-18
Status: approved design. The codec/refactor foundation this depended on has
**shipped** (v0.7.0), and the §7 new-stack-creation experiment is **done** —
creation is in scope. Ready for user review, then writing-plans.
Builds on: M2 layout canvas (`windows.rs` projection, `LayoutView.svelte`,
`WindowPanel.svelte`) and docs/format-notes.md window-geometry mapping.

## 1. Goal

Model EVE window stacks properly and make the layout canvas treat a stack as
one coherent group instead of several independent rectangles: draw one tabbed
rectangle per open stack, move/resize it as a unit, and edit membership
(unstack, add to an existing stack, reorder tabs). Creating a brand-new stack
from two free-floating windows is **included**, on the strength of the §7
capture: the container is a free, file-determined id, not unpredictable
runtime state.

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
- Anchor rect for a stack = the container's geom if present, else the geom of
  the **frontmost open member**, defined concretely as the open member with the
  lowest normalized `preferredIdxInStack3` index (ties broken by id) — i.e. the
  first tab. Deterministic, so the anchor never jumps between reads.

Malformed/missing dicts are skipped, never panic (matches the existing
projection contract).

## 4. Backend — authoring

Two write paths, split by what they touch.

**Geometry (coherent move/resize)** — pure `SetScalar`, no structural change.
Moving/resizing a stack emits `set_scalar` on the geometry elements of the
container *and every member that has geometry (open **or** closed — a closed
member left behind would drift out of the stack)*, all pointing at the
reference resolution.
Reuses the existing M2 geometry-mutation path (the projection hands over each
window's `x/y/w/h` paths). Repairing stale member drift is a free side effect.
No dependency on the codec work — this path could ship independently.

**Membership (unstack / add / reorder / create)** — structural edits to `stacksWindows`
+ `preferredIdxInStack3`, whose keys/values are `Shared` window-id stores.
`RemoveEntry` refuses shared subtrees, so these are not raw mutations. A
dedicated `ops.rs` command (mirroring overview/autofill editing) that operates
on a fully-inlined `windows` subtree, edits plain values, and re-encodes:

- **Unstack M**: remove M's `stacksWindows` entry and its
  `preferredIdxInStack3[container]` entry.
- **Add M to container C**: set/insert `stacksWindows[M] = C` and append
  `preferredIdxInStack3[C][M] = nextIdx`.
- **Reorder within C**: rewrite `preferredIdxInStack3[C]` to clean 0..N indices.
- **Create stack from M1, M2** (two free windows; M1 is the window the user
  started the action from, M2 the one picked): mint a container id C free in the
  file (choose a *high* id to avoid any counter reuse; see §7). The new stack
  lands at **M1's current rect** — write C's geometry entry and M2 to that same
  rect (M1 already has it), so the stack appears where M1 was. Set
  `stacksWindows[M1] = stacksWindows[M2] = C`, `preferredIdxInStack3[C] = {M1:
  0, M2: 1}` (M1 is tab 0), mark C and both members open, and add `C = False` to
  the container's boolean state dicts (`isLightBackgroundWindows`,
  `isOverlayedWindows`, `minimizedWindows`). Full byte-level recipe in
  docs/format-notes.md ("Window stacks").

**Encoding.** These commands inline the subtree, edit plain values, and let the
encoder re-derive canonical `Shared`/`Ref` sharing (the v0.7.0 reshare
foundation) — so the saved file is compact and matches what the client writes,
with no manual `inline_all` bloat.

All writes go through the unchanged verify → backup → atomic-write save chain.

## 5. Frontend — canvas (`LayoutView.svelte`)

- Group open windows by stack. Each stack draws **one** rectangle at its anchor
  geom, with member names as tabs along the top; non-stacked windows draw as
  today. A stack with **no open members** is not drawn at all (its container/
  frame is hidden too — an empty stack has nothing to show).
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
- Per non-stacked window: **add to stack →** a picker of existing stacks, or
  **stack with another free window** to create a new stack (§7).
- A selected member still shows its geom/flags detail as today. Shared stack
  geometry is edited from the canvas or the container's detail.
- Removes the dead "stack id" number input in favour of this grouping UI.

Membership editing lives in the panel (buttons/pickers) in V1; canvas
drag-to-stack is deferred polish (§9).

## 7. New-stack creation — experiment result (creation IS in scope)

The live capture ran (2026-07-19; recorded in docs/format-notes.md, "Window
stacks", experiment 6). Tabbing two free-floating windows together mints a
**new numeric-string container id** — a *free* integer id (EVE reused a gap
below other live ids, i.e. a free-list pick, not `max+1`) — materialized as a
real window. The complete byte-level recipe is in format-notes.md and summarized
in §4's create command: give the container a geometry entry, set both members to
that same rect, link both in `stacksWindows` + `preferredIdxInStack3`, open the
container and both members, and add the container to three boolean state dicts.

**Decision: include new-stack creation.** The earlier "container id is
unpredictable runtime state" worry does not hold — the container id need only be
*free in the file*, which is enumerable from these dicts, and the client tracks
live window ids on load, so it will not reclaim an id it sees in use. Creation is
the same inlined-subtree structural edit as add/reorder (§4), plus materializing
the container window.

**One residual to validate in the live smoke:** whether a next-window-id counter
is persisted anywhere *outside* `b"windows"` (the single capture did not rule it
out). Mitigate by choosing a *high* free id, and confirm in-game that creating a
stack then tabbing another window produces no id collision.

## 8. Testing

- **Rust (`settings-model`):** projection unit tests over synthetic trees —
  stack grouping (member→container resolution through Ref/Shared), tab ordering
  from `preferredIdxInStack3`, anchor-geom selection (container present/absent),
  `None` values skipped, colliding-index normalization. Authoring-command
  tests: unstack removes both entries; add inserts both; reorder rewrites
  indices; create mints a free container id, materializes it (geometry + open +
  state-dict entries), and links both members; and a round-trip re-encode is
  canonical (re-opens Editable, no dangling refs, matches the encoder's dedup).
- **Frontend (`node --test`, zero-dep):** pure logic — grouping open windows by
  stack, tab-order sort, coherent-move geometry fan-out. DOM drag not unit
  tested (consistent with M2).
- **Manual smoke (live, project norm):** unstack / add / reorder / create on a
  real char file through the app, save, reopen Editable, confirm in-game. For
  create, run the §7 counter check: after creating a stack, tab another window
  in-game and confirm EVE assigns no id that collides with the minted one.

## 9. Dependencies, scope, deferred

- **Codec/refactor foundation dependency — satisfied (shipped v0.7.0).** The
  membership commands (§4) rely on that encoder-side auto-dedup to inline → edit
  → reshare → encode into a compact, client-matching file; it is now in place.
  The geometry-coherent-move path (§4) never needed it.
- **Sibling milestone item** "resize a layout window from any corner" is
  independent (no codec dependency) — it can ship separately, and its resize
  handles are what the coherent stack resize reuses.
- **New-stack creation** is in scope (§7 experiment done); the one residual — a
  possibly-persisted next-window-id counter — is validated in the live smoke.
- **Deferred:** canvas drag-to-stack (panel-button membership editing in V1);
  authoring the exact stale-member geometry semantics beyond "sync open members
  to the anchor on move."
