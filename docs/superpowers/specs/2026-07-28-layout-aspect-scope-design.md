# The Layout aspect's scope (design)

Status: designed 2026-07-28, not yet planned.

Context: the last open finding from the 2026-07-27/28 live-verification sessions,
and the only one that is a behaviour change rather than a correction — which is
why it was split out of `live-verification-session-a` (PR #29) into its own
branch. Its misleading *label* shipped fixed in v0.22.0; what the aspect actually
carries is what this spec settles.

Builds on: `hud.rs`'s nine-field table, `batch.rs`'s category model,
`ops.rs::aspect_writes` and `plan_setup`, and `presets.rs`'s prune/create path.

## 1. Goal

A batch copy or a preset offers a **Window layout** aspect. A player picking it
expects the target character's screen to end up looking like the source's. It
does not: of the nine fields the HUD editor writes, a Layout copy carries
**one**.

Half-applied is worse than carrying none, because nothing on screen says which
half moved. Confirmed on live files during Session A: after an A1 → A2 Layout
copy, `shipuialignleftoffset` matched at `0.0` while `fightersDetachedPosition`
stayed at A2's own `(326, 54)` against A1's `(0, 0)`.

**This spec makes the aspect carry all nine.**

## 2. What it carries today, and why the ledger's reading was wrong

`Aspect::Layout` (`ops.rs:118-121`) pushes `Category::Layout` (`windows`) and
`Category::NeocomButtons` (`ui → neocomButtonRawData`) into `char_categories`,
and **nothing** into `account_categories`. Cross-referenced against `hud.rs:71-103`,
where every field carries a `HudScope` as well as a section:

| field | scope | path | carried today |
|---|---|---|---|
| `ship_offset` | Char | `windows → shipuialignleftoffset` | **yes** — inside the `windows` subtree Layout already splices |
| `fighter_x` / `fighter_y` | Char | `ui → fightersDetachedPosition` | no |
| `badge_x` / `badge_y` | Char | `notifications → notification_badge_offset` | no |
| `ship_top` | **Account** | `ui → shipuialigntop` | no |
| `fighter_detached` | **Account** | `ui → detachFighterUI` | no |
| `fighter_shown` | **Account** | `ui → displayFighterUI` | no |
| `neocom_width` | **Account** | `windows → neocomWidth` | no |

The small-tasks entry said Layout carried two of the nine, reading
`shipuialignleftoffset` and `neocomWidth` as siblings because both sit under a
key called `windows`. They are in **different files** — one in the character
file, one in the account file — and Layout writes only the character side. It
carries one of nine, not two.

## 3. Decisions

Three questions were put to the developer; all three answers are load-bearing.

**3.1 A Layout copy may write the account file.** The four account-scoped fields
come in, and `writes_account()` becomes true for Layout. Every other character on
the target's account therefore gets the new neocom width and fighter-UI toggles.
That collateral is inherent to EVE — those settings have no per-character form —
and the batch view already names collateral characters for Overview, Autofill and
Keybinds.

**3.2 Unpaired characters are excluded.** `plan_setup` (`ops.rs:187-224`) already
gates on `writes_account()`: an unpaired source refuses the copy up front, and an
unpaired target lands in the excluded panel with "No account paired — pair it in
the Accounts view to include". Layout is currently the only aspect that reaches an
unpaired character; it stops being so. Rejected alternative: writing the char side
anyway and reporting the account side as skipped — that is a reported half-apply,
which is the thing this spec exists to remove.

**3.3 Absent on the source means default, so the target's key is removed.** 851 of
3059 corpus account files store **none** of the four account keys — they sit at
EVE's defaults, and an absent key *is* the default. Copying nothing would leave
the target's own neocom width in place and the two characters looking different,
which is the same half-apply on roughly a quarter of accounts. So a requested
leaf-HUD category the source lacks removes that key from the target, and EVE falls
back to the same default the source is showing.

## 4. Design

### 4.1 Six leaf categories

Two char-side, four account-side:

| variant | side | `key_path` |
|---|---|---|
| `HudFighterPos` | char | `ui → fightersDetachedPosition` |
| `HudBadge` | char | `notifications → notification_badge_offset` |
| `HudShipTop` | account | `ui → shipuialigntop` |
| `HudFighterDetached` | account | `ui → detachFighterUI` |
| `HudFighterShown` | account | `ui → displayFighterUI` |
| `HudNeocomWidth` | account | `windows → neocomWidth` |

`Category::Autofill => &[b"ui", b"editHistory"]` already proves a category can
point at a single key inside a section, so `extract_categories`, `apply_to_tree`
and `presets::parent_entries` all keep working off `key_path` with no change.
`ship_offset` needs no category — it rides inside the char `windows` subtree.

Note `HudNeocomWidth`'s path is the account file's `windows`, which is a different
document from the char `windows` that `Category::Layout` splices whole. They never
meet: the two variants are routed to different sides.

Rejected alternatives: splicing whole sections (char `ui` also holds
`editHistory`, `SortHeadersSizes` and `neocomButtonRawData`, so a Layout copy
would carry the target's autofill away), and extending `Category` to hold several
key paths so all eight become one `Category::Hud` (changes the type and its three
consumers to save five variants).

### 4.2 Routing

`aspect_writes`'s `Aspect::Layout` arm gains the two char categories and the four
account ones. That single arm is what makes `writes_account()` true, and the
collateral naming, the account-write dedup and the unpaired exclusions in
`plan_setup` all follow from it with no further code.

### 4.3 Absent-means-default, scoped

`extract_categories` returns `Vec<(Category, Option<Value>)>`. `None` means
"requested, `absent_means_default`, and the source does not have it", and makes
`apply_to_tree` delete that key from the target.

Removal is a no-op when the target's parent section is missing: there is no key to
delete, and EVE is already at the default.

A whole-section category — `Layout`, `Overview`, `Autofill`, `Keybinds`,
`NeocomButtons` — **never** emits `None`. The gate is a
`Category::absent_means_default()` predicate, true only for the six leaf HUD
variants. A missing source `overview` deleting the target's would be destructive,
so this guard gets its own test rather than resting on the predicate being read
correctly.

Three call sites *inspect* the result and need updating; every other one forwards
it to `apply_to_tree` unchanged:

- `presets.rs:100` `has_category` — `.is_empty()` must become "holds a present
  value", or a requested-but-absent HUD category makes it answer true.
- `presets.rs:142` `prune` — its `present` list must filter to `Some`.
- `ops.rs:359` — same `.is_empty()` reading as `has_category`.

The char-side ship offset gets match semantics for free: a wholesale `windows`
splice replaces the target's subtree, so a key the source lacks disappears with it.

### 4.4 An empty-root source side contributes nothing

Layout presets created before this change carry a `user.dat` of `{}` —
`is_empty_root`'s own doc comment names "a Layout-only preset's `user.dat`" as
exactly that shape. Under §4.3 alone, applying one would **delete** the target's
neocom width and fighter toggles: actively worse than the half-apply being fixed,
and on files the user never re-captured.

So an empty-root source document yields no splices **and no removals**. An old
preset keeps applying char-only, faithful to what it captured.

The check belongs in `extract_categories` itself — an empty root source returns an
empty result — so it holds for every caller rather than for whichever call site
remembered it. A real settings file is never an empty root, so this cannot fire on
a character source; `is_empty_root` moves into `settings-model` beside
`extract_categories`, and `presets.rs` re-exports or calls it there.

### 4.5 What makes §4.4 a reliable discriminator

851 of 3059 account files store none of the four keys, so a *new* Layout preset
from such a character would prune to `{}` as well and be indistinguishable from an
old one — §4.4 would then silently disable the account side for it.

Fix: `prune` builds parent dicts for every **requested** `absent_means_default`
category rather than only the present ones, so a new Layout preset's `user.dat` is
at minimum `{ui: {}, windows: {}}` and never an empty root. Scoped to those
categories, so Autofill and Keybinds presets keep the property `prune`'s comment
records today — "no empty `ui` or `cmd` dict can survive".

This is the one place where the design depends on a shape rather than a value, so
it is pinned directly: a test creates a Layout preset from a character storing no
HUD keys and asserts the user side is not an empty root.

### 4.6 UI

- `BatchView.svelte:70` — the label drops "— not the fighter panel or badge" and
  the entry flips `account: false` → `true`, which is what surfaces the
  collateral-character warning.
- `PresetGroup.svelte:19-25` — drops its `note` caveat and sets
  `needsUser: true`.
- No new guard on preset creation: `presets.rs:161-166` already refuses an aspect
  whose side is not open ("rather than writing an empty document that claims to
  hold it"), and that now covers Layout.

### 4.7 Verified unchanged

- `copies_char_geometry()` already returns true for Layout, which is **right**:
  `notification_badge_offset` is absolute screen px (e.g. `(2519, 131)`), so the
  resolution-mismatch warning must fire for the new fields too.
- `presets::parent_entries` reads `key_path` generically — "a category added later
  needs no change here" — and all six new paths are two levels, inside its
  `debug_assert!(keys.len() <= 2)`.
- All **6502** corpus char files carry the `notifications` section (5024 carry the
  badge key itself), so `apply_to_tree`'s missing-intermediate-parent skip is not
  reachable for the badge offset.
- `derive_aspects` keys Layout off `has_category(char_doc, Category::Layout)` and
  stays as it is — the same precedent as Overview, which spans both sides but is
  derived from the account side alone.

## 5. Non-goals

- **Splitting a HUD aspect out.** Considered and rejected: `ship_offset` cannot
  leave `Category::Layout` without carving up the `windows` subtree, so a separate
  HUD aspect would leave the HUD split across two aspects anyway.
- **Per-character account settings.** EVE stores the four account fields once per
  account; making them per-character is not ours to do.
- **A preset format version or marker.** §4.4 and §4.5 distinguish old presets
  from new by shape, which costs one line in `prune` instead of a migration.
- **Changing what `Everything` does.** It is a full file copy and already carries
  all nine.

## 6. Tests

Rust:

1. Each new `key_path` extracts from a source and applies to a target.
2. Real-shape: a Layout copy between two corpus-shaped documents leaves all nine
   HUD fields equal, asserted through `project_hud` rather than by reading raw
   keys — the projection is what the user sees.
3. Absent on source removes the target's key (one per side).
4. **An absent whole-section category is still skipped, not removed.** The
   destructive case; assert the target's `overview` survives a source that has none.
5. An empty-root source document contributes neither splices nor removals (§4.4).
6. `prune` never yields an empty root for Layout, from a source storing no HUD
   keys (§4.5).
7. `has_category` answers false for a requested-but-absent HUD category (§4.3).
8. `plan_setup` excludes an unpaired target for a Layout-only selection, and
   refuses an unpaired source (§3.2).

Frontend:

9. The collateral-character warning renders for a layout-only selection in
   `BatchView`.
10. `PresetGroup` requires an open account file before offering Layout.

Live smoke — the behaviour change this branch introduces, so it does not merge
without it:

11. Copy Layout A1 → A2 in-game, log in as A2, confirm all nine fields land.
12. Confirm a third character on A2's account sees the account-side four.
13. Copy from a source storing none of the four onto a target that stores them,
    and confirm the target comes up at EVE's defaults (§3.3 — the removal path,
    which no offline test can prove the client honours).
