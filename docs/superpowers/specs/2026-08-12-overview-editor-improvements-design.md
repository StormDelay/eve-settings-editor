# Overview editor improvements (design)

Status: designed 2026-08-12.

Three independent additions to the overview editor. They share no code and can
land in any order; they are one spec because they are one request and one
release note. Nothing here changes the settings *format* — every write goes
through an existing model entry point or one small new one.

## 1. Select/deselect all in a filter category

### The problem

`OverviewFiltersTab.svelte` renders the group catalog as `<details>` per
category. **Entity** alone holds ~400 groups. Ticking a category's worth of
groups is one click per group, and each click is a backend round trip
(`presetSetGroups` / `presetFork`) that re-projects the whole overview.

### The change

Each category summary gains `All` and `None` buttons. Both resolve to **one**
backend call, because `presetSetGroups` already takes the complete membership
list — only the component's per-checkbox call shape stands in the way.

The edit path is factored so both the single and bulk case share it:

```ts
async function applyGroups(next: number[]) { …fork-or-set, unchanged… }
const setPresetGroup = (id, on)  => applyGroups(toggleGroup(presetGroups, id, on));
const setCategory    = (cat, on) => applyGroups(toggleGroups(presetGroups, ids(cat), on));
```

`toggleGroups(groups, ids, on)` joins `toggleGroup` in `groups.ts` — pure, no
Svelte or Tauri deps, so it unit-tests under vitest like its sibling.

### The rule that needs stating

**`All` acts on the groups currently shown, not on the category's full
membership.** With a search query active, `filterCatalog` has already narrowed
each category to its matches; ticking "everything I can see" is the only
behaviour that matches the button's position on screen, and it turns the search
box into a bulk-select tool ("hauler" → All). With an empty query the two
readings coincide, which is the common case.

The buttons live inside `<summary>`, so their handlers must
`stopPropagation` — a click on a `<summary>` descendant toggles the `<details>`
otherwise, and selecting a category would collapse it.

## 2. Copy column settings to other tabs

### Why this one needs a backend command

Order and visibility are per-tab lists in the **account** file
(`tabColumnOrder`, `tabColumns`); widths are per-tab in the **character** file
(`SortHeadersSizes` keyed `(overviewScroll2, tabIndex)`). The existing frontend
API is `setOverviewOrder` (whole list, per tab) and `setOverviewVisible`
(**one column**, per tab). Copying to 10 tabs would be ~10 order calls plus
~140 visibility calls, each re-projecting the overview.

So: one new command.

```
overview_copy_columns(from_tab: i64, to_tabs: Vec<i64>, order: bool, visible: bool, widths: bool)
```

- `order` / `visible` → account file → caller marks `onUserDirty()`
- `widths` → character file → caller marks `onCharDirty()`

Both slots in one command, mirroring `overviewWindowAdd`, which already writes
grouping *and* geometry and is dirtied on both sides for exactly this reason.

### Model layer

`overview.rs` gains `copy_tab_columns(user, from, to, order, visible)` and
`copy_tab_widths(char_tree, from, to)`, alongside the existing
`set_column_*` functions and reusing their tab-lookup helpers.

### Inheriting source tabs

A tab that owns neither list inherits the account defaults, and the editor
already shows those defaults as the tab's columns. Copying from such a tab
writes **what the UI shows** into the targets, materialising their own lists.
That is the same thing editing an inheriting tab does today, so it needs no
special case — only a line of documentation.

### UI

A `Copy columns…` button in the Columns tab opens an inline panel (not a
modal — the app has no modal pattern, and `OverviewView`'s name-entry rows set
the precedent for inline forms):

- Target tabs, grouped by overview window the way the tab `<select>` groups
  them, each a checkbox, with `Select all` / `None` above.
- Three property checkboxes: column order, visible columns, widths.
- `Copy` and `Cancel`.

**Defaults: all three properties ticked, no target tabs ticked.** Properties
are ticked because copying a partial column setup is the unusual ask. Targets
are not, because the panel's whole purpose is a destructive overwrite and a
stray `Copy` on a pre-filled target list would rewrite every tab in the
account.

The widths checkbox is disabled and unticked when no character is open, with
the reason shown — the same condition that already disables the width inputs.

## 3. Tab name markup

### What EVE actually stores

Overview tab names are markup-bearing strings. From the corpus (`testdata/dumps`,
134 real account files):

```
"<color=0xFFFFFFFF>  *  </color>"
"<color=0xFFFF6F75>   <b>main</b>   </color>"
"<b> Exit! </b>"
"  main  "
```

Tags occurring on tab names anywhere in the corpus: `<color=0xAARRGGBB>` and
`<b>`. `<fontsize=N>` appears too, but only on bracket labels' `pre`/`post`
strings — never on a tab name — so it is out of scope.

This is the mechanism overview packs use: `overview_pack.rs`'s `apply_tabs`
writes a tab's `name` and nothing else, so a pack colours its tabs by embedding
markup in the name.

### What this is *not*

The tab dict also carries `b"color"` (None, or a 3-float RGB tuple) — that is
what the in-game **Overview Settings → Tabs** colour picker writes, and it is a
separate mechanism. This slice leaves it untouched and unprojected, exactly as
today. Consequence worth knowing: a tab coloured through the in-game picker
renders coloured in EVE and shows no colour in the editor's swatch.

### The palette

The in-game picker offers 24 colours in a 3×8 grid — sampled from the
screenshot, a hue wheel at 15° steps with every channel drawn from
`{0x40, 0x6f, 0x9f, 0xcf, 0xff}`:

```
ff4040 ff6f40 ff9f40 ffcf40 ffff40 cfff40 9fff40 6fff40
40ff40 40ff6f 40ff9f 40ffcf 40ffff 40cfff 409fff 406fff
4040ff 6f40ff 9f40ff cf40ff ff40ff ff40cf ff409f ff406f
```

The editor offers these plus **None** (no colour span at all). Arbitrary
colours already in a file are preserved and shown; the palette is what you can
*pick*, not what you can *hold*.

### Module

New pure module `app/src/lib/tabName.ts` — no Svelte, no Tauri, node-testable:

```ts
interface TabName { color: string | null; bold: boolean; text: string }
parseTabName(raw: string): TabName
formatTabName(n: TabName): string
plainTabName(raw: string): string
EVE_PALETTE: string[]
```

**No backend change.** `rename_tab` writes an arbitrary string, so setting a
colour is `api.tabRename(idx, formatTabName({...current, color}))`.

### Round-tripping, and the accepted ceiling

`parseTabName` strips tags and keeps the text between them **verbatim,
including spaces** — real names are padded (`"  main  "`, `"  3  "`) to widen
the tab, and losing that padding would silently resize a user's overview.

Parsing normalises tag nesting. `<color=…>   <b>main</b>   </color>` re-emits
as `<color=…><b>   main   </b></color>`: same rendering, different bytes. This
is acceptable because a name is only ever rewritten when the user changes
something, and `formatTabName(parseTabName(x))` is pinned by test to be stable
under repeated application.

A name that does not fit `[colour][bold]text` — nested spans, several colours,
an unknown tag — parses to `{color: null, bold: false, text: raw}`: the plain
Rename box still edits it as raw text, and the swatch shows no colour. It is
never silently rewritten; only an explicit colour or bold click replaces it.

### UI

The tab-actions row (beside New / Rename / Delete) gains, for the selected tab:

- a colour swatch button opening a 3×8 palette grid plus `None`
- a `B` toggle

The tab chips (`.tab-chip`, real buttons) render their colour and weight. The
tab `<select>` shows `plainTabName` — `<option>` styling is not reliable in
WebView2, and raw `<color=0x…>` in the dropdown would be worse than plain text.

## 4. Testing

- `groups.test.ts` — `toggleGroups`: add-all, remove-all, mixed starting state,
  ids already present, empty id list.
- `tabName.test.ts` — parse of each corpus name above; padding preserved;
  unparseable input falls through to raw; `format(parse(x))` stable under
  repetition; palette is 24 entries.
- `OverviewFiltersTab.spec.ts` — `All` on a category issues **one** backend
  call carrying every shown group; `All` under an active query carries only the
  matches; the click does not collapse the `<details>`.
- `OverviewColumnsTab.spec.ts` — the copy panel's select-all ticks every target;
  the widths checkbox is disabled with no character open; `Copy` issues one
  command with the ticked targets and properties.
- Rust: `copy_tab_columns` from an owning tab, from an inheriting tab
  (materialises the account default), to a tab that owned nothing, and an
  unknown target index erroring rather than half-applying.
