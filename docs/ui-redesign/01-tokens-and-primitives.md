# Phase 1 — Design tokens and shared primitives

Status: **planned, nothing implemented.** Prerequisite for phases 2–5b.
Behaviour change: **none.** Layout moves: **none.**

---

## 1 Goal

Replace the nine-property `:root` block in `app/src/app.css` with a solved token
system, build the twelve components every view needed and none of them had, and
then delete the local copies. When this lands, `app/src` contains **zero colour
literals outside the token block**, one type scale, one spacing scale, three
corner radii, and one disabled treatment.

Nothing moves and nothing changes what it does. This is worth stating twice
because the temptation to fix the structural faults while you are already in the
file will be strong: the tab strip still hides tabs rather than disabling them,
Accounts is still a dead end, Save still fires two modals. Those are phases 2, 3
and 5. Phase 1's value comes precisely from being reviewable as a mechanical
diff — if it also moves a control, the reviewer has to think about two things at
once and the "revert the whole phase" escape hatch stops working.

Three things do get *fixed* here, because they are bugs in the styling layer
itself and fixing them is what building the primitive means:

- **Four buttons that are invisible but clickable.** `app.css:96` sets
  `.mini { opacity: 0 }` and `app.css:97` reveals it only via `.row:hover .mini`.
  Four `.mini` buttons live outside any `.row` and are therefore permanently
  invisible: `AutofillView.svelte:110` (Clear a list),
  `AutofillView.svelte:129` (remove an entry), `KeybindsView.svelte:136` (reset
  a binding to EVE's default) and `routes/+page.svelte:621` (clear the tree
  search). Already logged in `docs/small-tasks.md`. `Button variant="ghost"`
  retires the pattern rather than patching four call sites.
- **The contrast floor.** Measured with APCA, the app's most-used text colour
  (`--fg-dim` `#8a919e`, 51 uses) scores **Lc 42** and the three status badges
  score **Lc 51 / 55 / 43** — all of them at 12px, where APCA wants Lc 75+.
  §3 and §7 make the floor a test rather than a hope.
- **Two undefined variables.** `--line` (3 uses) and `--panel` (1 use) are
  referenced in `AccountsView.svelte:185,189,197,198` and declared nowhere, so
  every card, chip and panel border in that view falls back to `#3333` — a
  colour no other view uses. The `no-undefined-tokens` test in §7 is the thing
  that would have caught this.

Everything else is find-and-replace.

---

## 2 Current state, with evidence

Every number below was counted against `app/src` on branch
`worktree-ui-redesign-specs`, excluding `*.spec.ts` and `*.test.ts`. Where a
count differs from the proposal artifact, the measured number is used and the
difference is noted.

### 2.1 Colour

| | Count | Evidence |
| --- | --- | --- |
| Distinct hex literals in source | **45** | 9 of them are the `:root` declarations at `app.css:5-13`; the other **36** are hardcoded at their point of use |
| `rgba()` literals | **25**, in 9 files | `app.css` ×2, `ContextMenu` ×1, `DetailParts` ×5, `HudPanel` ×1, `LayoutView` ×6, `OverviewView` ×1, `ProbeFormationsView` ×1, `ProbeViewer` ×4, `WindowPanel` ×4 |
| Custom properties declared | **9** | `app.css:5-13` — `--bg --bg-panel --fg --fg-dim --accent --danger --ok --warn --border` |
| Custom properties referenced but never declared | **2** | `--line` at `AccountsView.svelte:185,189,197`; `--panel` at `AccountsView.svelte:198` |

The 36 hardcoded values are not 36 different intentions. They are six
intentions expressed six different ways each:

| Meaning | Declared token | Also written as |
| --- | --- | --- |
| Danger / error | `--danger` `#e06c60` | `#e06c6c` (`BatchView:426`, `ProbeViewer:786`), `#c0392b` (`AccountsView:196`), `#e66` (`AutofillView:169`), `#a33` (×4 — `AutofillView:158`, `OverviewFiltersTab:320`, `OverviewView:504`, `ProbeFormationsView:658`) |
| Warning | `--warn` `#d9a441` | `#d0a000` (`BatchView:425`), `#d08770` (`Sidebar:198`, as a dead fallback), `#f59e0b` (`HudPanel:287,291`, `LayoutView:1063,1076,1103,1124,1169`), `#fde68a` (`HudPanel:287,310`, `LayoutView:1065`) |
| Accent | `--accent` `#4f9cf0` | `#60a5fa` (`LayoutView:1095`), `#6c9ce0` (`ProbeViewer:788`), `#dbeafe` (`ChatSplit:108,124`, `LayoutView:1096,1162`) |
| Success | `--ok` `#62b268` | `#6cc06c` (`BatchView:427`), `#7bc47b` (`ProbeViewer:787`), `#34d399` (`LayoutView:1115`) |
| Dim text | `--fg-dim` `#8a919e` | `#888` (`ChatSplit:114`, `LayoutView:1191,1204,1222`), `#666` (`LayoutView:1199`), `#aaa` (`ChatSplit:97`), `#94a3b8` (`DetailParts:43`, `LayoutView:1038`), `#64748b` (`LayoutView:1037`) |
| Border | `--border` `#2c3038` | `#444` (`ChatSplit:107,123`, `LayoutView:1031`, `NeocomButtons:128`), `#333` (`ChatSplit:83`), `#3333` (×3, `AccountsView`), `#0006` (`OverviewView:536`), `#0001` (`AccountsView:198`) |

Four files — `LayoutView`, `HudPanel`, `ChatSplit` and `DetailParts` — run an
entirely second palette lifted from Tailwind (`#60a5fa`, `#f59e0b`, `#94a3b8`,
`#64748b`, `#34d399`, `#dbeafe`, `#fde68a`). Those four are the layout canvas
and its inspector, so the half of the app that draws the game screen is a
different colour scheme from the shell around it.

`HudPanel.svelte:286-289` is the one place the duplication is deliberate and
documented:

> The two ambers below are deliberately NOT app.css variables: they match the
> canvas's selected-furniture colour (LayoutView's own `#f59e0b`/`#fde68a`), and
> the pair has to move together or the panel stops agreeing with the rectangle
> it describes.

That comment is correct about the requirement and wrong about the mechanism —
a hardcoded pair in two files is the *weakest* way to keep two things coupled.
§4.4 resolves it: both sides reference `--warn`, and the coupling becomes
structural instead of a comment asking two humans to remember.

### 2.2 Scale drift

| Dimension | Distinct values | Detail |
| --- | --- | --- |
| `border-radius` | **8** | `3px` ×29, `4px` ×7, `8px` ×3, `50%` ×3, `999px` ×1, `6px` ×1, `2px` ×1, `0` ×1 |
| `font-size` | **10** | `0.85em` ×39, `10px` ×9, `11px` ×8, `0.9em` ×6, `12px` ×4, `1em` ×2, `9px`, `13px`, `14px`, `0.8em` |
| `padding` | **55** | px and rem mixed freely; `0.25rem 0.1rem 0.5rem`, `0.2rem 0 0.4rem 1rem`, `0.15em 0.5em` all appear once each |
| `gap` | **22** | `0.4rem` ×14, `0.6rem` ×9, `0.5rem` ×9, `0.3rem` ×9, `0.35rem` ×7, `6px` ×6, `1rem` ×6, then a tail of 15 more |
| `opacity` | **33 declarations**, 9 distinct values | `0`, `0.3`, `0.4`, `0.45`, `0.5`, `0.6`, `0.7`, `0.75`, `1` across 17 files |

`0.85em` used 39 times is the mechanical source of the "text sizes don't match"
complaint. `em` compounds, so `0.85em` resolves to 11.9px inside the 14px root,
11.05px inside `WindowPanel`'s 13px block (`WindowPanel.svelte:524`), and
10.2px inside `HudPanel`'s 12px block (`HudPanel.svelte:270`) — three different
sizes from one declaration that reads as if it names one.

### 2.3 The same component, written N times

| Thing | Copies | Where |
| --- | --- | --- |
| `.shared-banner` — byte-identical | **4** | `AutofillView:159-162`, `KeybindsView:187-190`, `OverviewView:478-481`, `ProbeFormationsView:704-707`. A fifth variant with different words is `HudPanel:272-279` `.account-legend`; a sixth, compressed to 10px, is `ChatSplit:45` `.legend` |
| `button.danger { border-color: #a33 }` | **4** | `AutofillView:158`, `OverviewFiltersTab:320`, `OverviewView:504`, `ProbeFormationsView:658` |
| "Give the native control explicit dark colours" | **28 rules in 15 files** | 20 set `background`+`color`, 7 set only `accent-color`, 1 sets a disabled colour. Listed in full in §4.6 |
| Search / filter boxes | **5** | `routes/+page.svelte:611-623`, `KeybindsView:101`, `AutofillView:99`, `OverviewFiltersTab:237`, `WindowPanel:361`. Two verbs ("Search", "Filter"), three placeholder conventions (bare, trailing `…`, trailing `(Ctrl+F)`), three different style blocks |
| Tab strips | **3 visual styles** | `.viewtabs` (`app.css:119-121`), `.subtabs` (`OverviewView:545-550` and `OverviewAppearanceTab:180-185`, identical), `.tree-file` (`app.css:122`) |
| `.badge` — four unrelated meanings | **4 definitions** | `app.css:59-62` (file status), `HudPanel:335` (scope tag), `NeocomButtons:117` (a child count), `WindowPanel:609` (a warning) |
| Empty-state class names | **3, one unstyled** | `.hint` (17 sites), `.muted` (`BatchView`), and `.empty` — used at `KeybindsView:84` and `:90` and **defined nowhere in the codebase**, so it renders as a bare `<p>` |
| Inline-message class names | **7** | `hint`, `error`, `muted`, `field-error`, `flash`, `err`, `empty` |
| The `.flash` toast and its 2000 ms timer | **3 hand-rolled copies** | `Sidebar:85-88`, `Sidebar:102-104`, `ProbeFormationsView:304-306` |

The `.empty` finding is the sharpest single piece of evidence for this phase:
two empty states in a shipped view have been rendering with no styling at all,
and nobody noticed, because there is no shared thing whose absence would show.

### 2.4 The dark-WebView2 tax

`app/src-tauri/tauri.conf.json` sets no theme override, and `app.css:2` declares
`color-scheme: dark`. A native `<select>`, `<option>`, `<input>` or `<input
type="checkbox">` in this shell renders light-on-light unless given explicit
colours. Fourteen files independently discovered this and independently wrote
the same rule; the comments record fourteen separate rediscoveries:

- `AutofillView:152` — "Dark native controls: the app runs in a dark WebView2 (see the memo)."
- `ChatSplit:103-104` — "an unstyled number input renders light-on-light in this theme."
- `HudPanel:322` — "Native controls render light in WebView2 unless told otherwise."
- `KeybindsView:155-156` — "…see the dark-native-controls note in the repo memory."
- `NeocomButtons:124`, `OverviewAppearanceTab:190-191`, `OverviewColumnsTab:164-165`,
  `OverviewFiltersTab:321-322` and `:349-350`, `OverviewView:505-506`,
  `PresetGroup:218-219`, `ProbeFormationsView:642-643`, `WindowPanel:677-679`,
  `LayoutView:1201-1202` — the same sentence again.

`Field` solves it once. This is the single largest deletion in the phase.

### 2.5 Contrast, measured

WCAG 2's contrast ratio is [known to give unreliable guidance for dark
interfaces](https://git.apcacontrast.com/documentation/APCA_in_a_Nutshell.html)
— it ignores polarity, size and weight, and it is why none of the following was
caught. Scored with APCA 0.1.9 (`W3 / 0.98G-4g`, implementation in §7.2):

| Pairing | Where | WCAG 2 | APCA Lc | Verdict |
| --- | --- | --- | --- | --- |
| `--fg-dim` `#8a919e` on `--bg` | 51 uses, all meta text at ~12px | 5.60 : 1 | **41.8** | Below any text threshold |
| `--fg-dim` on `--bg-panel` `#1e2128` | sidebar, panels | — | **40.6** | Worse where it is used most |
| `#666` on `--bg` | `LayoutView:1199` `.hintish` | 2.88 : 1 | **21.9** | Effectively invisible |
| `#888` on the canvas `#1b1f27` | `LayoutView:1191,1204,1222` | — | **36.7** | Fails |
| `--danger` `#e06c60` on `--bg` | all error text | 5.48 : 1 | **41.5** | Fails as text |
| `.kind-int` `#c8a1e8` on `--bg` | tree values — the actual data | 8.22 : 1 | **58.7** | Under body threshold |
| `.badge.editable` `#10240f` on `#62b268` | `app.css:60` | — | **50.7** | Fails at 12px |
| `.badge.dirty` `#33260a` on `#d9a441` | `app.css:62` | — | **55.3** | Fails at 12px |
| `.badge.read-only` `#2b100d` on `#e06c60` | `app.css:61` | — | **43.2** | Fails at 12px |
| `#94a3b8` on the furniture fill | `LayoutView:1038` | — | **46.9** | Fails |
| `--fg` `#d5d9e0` on `--bg` | body text | 12.54 : 1 | 82.2 | Passes |

Exactly one pairing in that table passes. The badge rows are the ones that
matter for the token design: they are dark text on a *saturated* fill, which
looks confident and measures terrible. §3.4 makes that shape impossible.

---

## 3 The token system

Replaces `app/src/app.css:1-14` entirely. The ramp is built in OKLCH so
lightness steps are perceptually even; each text token's lightness was then
solved by binary search against an APCA target, measured against the lightest
surface it is allowed to sit on. All 41 text pairings meet their floor — the
numbers are in §3.6 and the check is in §7.2.

```css
:root {
  color-scheme: dark;
  font-family: system-ui, "Segoe UI", sans-serif;
  font-size: 14px;

  /* --- surfaces: four evenly-stepped OKLCH elevations ------------------- */
  --bg:               #0f1216;   /* app ground */
  --surface:          #181b20;   /* panels, rails, the layout canvas */
  --surface-raised:   #22252a;   /* inputs, hover, cards, group headers */
  --surface-overlay:  #2c3034;   /* menus, popovers, sheets */

  /* --- text: each lightness solved to an APCA floor --------------------- */
  --text:             #edeff3;   /* floor 90 — body, values, primary labels */
  --text-secondary:   #d8dce4;   /* floor 78 — field labels, secondary rows */
  --text-muted:       #c3c7ce;   /* floor 65 — captions; NEVER below 12px */

  /* --- decorative only: these carry no text, so they carry no floor ----- */
  --border:           #32363b;
  --border-strong:    #4f5358;

  /* --- roles: the light tone is text/icon, the -dim is its ground ------- */
  --accent: #8dceff;  --accent-dim: #14283c;
  --danger: #ffb4ab;  --danger-dim: #3a1d1b;
  --warn:   #f3bd6e;  --warn-dim:   #33230a;
  --ok:     #8ad79b;  --ok-dim:     #152d1a;
  --info:   #5fd4f7;  --info-dim:   #052b36;

  /* --- syntax: one hue the role palette does not have ------------------- */
  --syntax-number:    #dcb2fe;   /* OKLCH L .826 C .114 h -51 — see §3.5 */

  /* --- translucent variants, for graphics drawn over other graphics ----- */
  --accent-veil:      #8dceff40; /* 25% — window rects on the layout canvas */
  --warn-veil:        #f3bd6e40; /* 25% — the selected rect */
  --ok-veil:          #8ad79b4d; /* 30% — the Shift-drag drop target */
  --muted-veil:       #c3c7ce1f; /* 12% — furniture blocks, HUD bands */
  --muted-line:       #c3c7ce73; /* 45% — HUD part outlines */
  --scrim:            #00000080; /* 50% — dialog backdrops */
  --shadow:           0 4px 12px #00000080;

  /* --- space: 4px base. A dense tool; half-steps matter. ---------------- */
  --s1: 4px;  --s2: 8px;  --s3: 12px;
  --s4: 16px; --s5: 24px; --s6: 32px;

  /* --- type: one scale, px only. em compounds; see §2.2. ---------------- */
  --t-caption: 12px;  --t-body:  13px;
  --t-ui:      14px;  --t-title: 16px;
  --t-head:    20px;

  /* --- radius: three steps, tied to element size ------------------------ */
  --r-sm:   4px;    /* chips, inputs, buttons */
  --r-md:   8px;    /* cards, panels, popovers, sheets */
  --r-pill: 999px;

  /* --- the one opacity value ------------------------------------------- */
  --o-disabled: 0.5;
}
```

### 3.1 Why the muted text is so much lighter

`#8a919e` at 12px is not readable, and no amount of taste changes that. The
hierarchy that dimming was doing badly gets rebuilt out of **size, weight and
position**, which cost no legibility. `--text-muted` at `#c3c7ce` measures
Lc 71.1 on `--surface` against the old `--fg-dim`'s Lc 40.6 in the same place.

### 3.2 `opacity` is retired as a hierarchy device

It has exactly one sanctioned use in HTML chrome: `--o-disabled` on a disabled
control, at one value. Twenty of the 33 current declarations are dimming text
and get replaced by a `--text-*` token (§4.7). Three are the `.mini` trap and
get deleted with it.

Ten survive, and the rule has to name them or the guard test in §7.1 becomes
a nuisance. They survive because they modulate a **drawing**, not the legibility
of text:

- `LayoutView:1176` `.tab.dragging` (a drag ghost) and `:1183` `.resize` (a
  corner handle) — pure graphics with no text inside.
- `ProbeViewer:739, 750, 769, 770, 780, 781, 792` — SVG strokes and fills in the
  3-D probe viewer, where opacity is the depth cue.
- `app.css:81` `@keyframes fade-out` — an animation, which moves into `Toast`.

Everywhere else in SVG, prefer `fill-opacity` / `stroke-opacity` with a token
colour over an `rgba()` literal. That is the native mechanism, and it keeps the
hue in the token block where the guard test can see it.

### 3.3 Why not `color-mix()`

It would remove the seven `*-veil` tokens. `.github/workflows/release.yml:14`
builds for `macos-latest`, `ubuntu-22.04` and `windows-latest`, and the Linux
target links `libwebkit2gtk-4.1` (`release.yml:23`). `color-mix()` needs
WebKit 16.4 / WebKitGTK 2.40; Ubuntu 22.04 can ship 2.36. Eight-digit hex has
been universal since 2017. Seven declared tokens are cheaper than a
platform-specific rendering bug nobody can reproduce on Windows.

### 3.4 The badge rule

**A badge is a light role tone on its matching `-dim` ground. Never dark text on
a saturated fill.** The current pattern measures Lc 51 / 55 / 43 (§2.5); the
replacement measures Lc 68.7 / 69.2 / 69.5 — a 20-point gain from a rule, not
from taste.

| Badge | Today | Lc | Becomes | Lc |
| --- | --- | --- | --- | --- |
| `.badge.editable` | `#10240f` on `--ok` | 50.7 | `--ok` on `--ok-dim` | **68.7** |
| `.badge.dirty` | `#33260a` on `--warn` | 55.3 | `--warn` on `--warn-dim` | **69.2** |
| `.badge.read-only` | `#2b100d` on `--danger` | 43.2 | `--danger` on `--danger-dim` | **69.5** |
| `WindowPanel .badge.warn` | `#33260a` on `--warn` | 55.3 | `--warn` on `--warn-dim` | **69.2** |

The same rule kills `LayoutView:1168-1171` `.tab.active`, which paints `#1b1f27`
on `#f59e0b` at Lc 59.6 — a drawing of EVE's own active tab, but still text
someone reads. `--warn` on `--warn-dim` takes it to Lc 69.2.

### 3.5 The one new hue, and the one collision worth naming

The tree paints six things in six colours (`app.css:87, 90-95`). The role
palette supplies five hues; numbers need a sixth. `--syntax-number` `#dcb2fe`
is today's `#c8a1e8` re-solved: the same OKLCH hue (−51°), lifted to L 0.826 at
the maximum in-gamut chroma (0.114), which puts it exactly in the band the five
role tones occupy (L 0.813–0.838, C 0.089–0.115). It measures Lc 69.6 / 68.7 /
67.2 / 65.1 across the four surfaces, against `#c8a1e8`'s 58.7.

The tree's final assignment:

| Selector | Today | Becomes | Why |
| --- | --- | --- | --- |
| `.label` (`app.css:87`) | `--accent` | `--accent` | unchanged |
| `.kind-int/-float/-long` (`:90`) | `#c8a1e8` | `--syntax-number` | same hue, solved |
| `.kind-str/-str_ucs2/-str_table` (`:91`) | `#a3c9a5` | `--ok` | same hue family, already solved |
| `.kind-bytes` (`:92`) | `#d0b47f` | `--text-secondary` | see below |
| `.kind-none/.kind-bool` (`:93`) | `#7fb2d0` | `--info` | same hue family |
| `.kind-ref/.kind-shared` (`:94`), `.shared-mark` (`:95`) | `--warn` | `--warn` | unchanged; it genuinely is a caution |

`.kind-bytes` becomes neutral rather than taking `--warn`, because `--warn` is
already booked by `.kind-ref`/`.kind-shared`, and of the two, bytes carries the
least meaning — the editor renders them as an opaque preview. Making the least
informative value the most neutral colour is the right way round.

**Named honestly:** `--accent` (#8dceff) and `--info` (#5fd4f7) sit 0.045 apart
in OKLab — close, and they land on adjacent cells of the same tree row
(`.label` then the value). They are not confusable in practice because position
disambiguates absolutely: the label always comes first and is always followed by
`: `. Today's `#4f9cf0` / `#7fb2d0` are no further apart. If it reads badly
in the app, move `.kind-none/.kind-bool` to `--text-secondary` and accept two
neutral value kinds — do not invent a seventh hue.

### 3.6 The floors, and what "the lightest surface it may sit on" means

Each text token is validated against **all four** surfaces, not just `--bg`,
because a caption inside a popover sits on `--surface-overlay`:

| Token | on `--bg` | on `--surface` | on `--surface-raised` | on `--surface-overlay` | Floor |
| --- | --- | --- | --- | --- | --- |
| `--text` | 96.8 | 95.9 | 94.4 | 92.3 | 90 |
| `--text-secondary` | 84.7 | 83.8 | 82.4 | 80.2 | 78 |
| `--text-muted` | 72.0 | 71.1 | 69.6 | 67.5 | 65 |
| `--accent` | 72.2 | 71.3 | 69.9 | 67.7 | 65 |
| `--danger` | 72.0 | 71.2 | 69.7 | 67.6 | 65 |
| `--warn` | 71.8 | 70.9 | 69.5 | 67.4 | 65 |
| `--ok` | 71.6 | 70.7 | 69.3 | 67.2 | 65 |
| `--info` | 71.6 | 70.8 | 69.3 | 67.2 | 65 |
| `--syntax-number` | 69.6 | 68.7 | 67.2 | 65.1 | 65 |

Role tone on its own `-dim` ground: accent 69.5, danger 69.5, warn 69.2, ok
68.7, info 68.8 — all above the 65 floor, which is what makes §3.4 work.

`--text` on any `-dim` ground measures 94 (accent-dim), so an `InlineMessage`
puts its *sentence* in `--text` and reserves the role tone for the rail and any
leading label. The message stays maximally readable and the colour still says
which kind it is.

`--border` `#32363b` on `--surface` measures Lc 0 and `--border-strong`
`#4f5358` measures Lc 13.7. That is correct and intentional: they carry no text.
Non-text contrast wants ~Lc 30 for an element boundary you must *find*; for a
divider between two things you are already looking at, this is the right
weight. Any border that becomes the sole indicator of a control's bounds — a
focus ring, an input outline — uses `--accent` or `--border-strong`, never
`--border`.

### 3.7 Composited grounds on the layout canvas

The canvas draws translucent rects over a grid, so the effective ground is a
composite. Measured against `--surface` (`#181b20`, the canvas background):

| Rect | Composite | `--text` | `--text-secondary` | `--text-muted` |
| --- | --- | --- | --- | --- |
| window (`--accent-veil`) | `#354858` | 85.6 | 73.5 | 60.8 |
| selected (`--warn-veil`) | `#4f4434` | 85.7 | 73.6 | 60.9 |
| furniture (`--muted-veil`) | `#2d3035` | 92.3 | 80.2 | 67.5 |
| drop target (`--ok-veil`) | `#3a5445` | 82.6 | 70.6 | 57.8 |

Canvas labels are 11px, below `--text-muted`'s 12px rule, so: **window and
selection labels use `--text`; furniture labels use `--text-secondary`.** That
preserves today's deliberate "furniture is quieter than a window" distinction
(`#94a3b8` vs `#dbeafe`) while taking furniture from **Lc 46.9 to Lc 80.2**.
`--text-muted` is never used on the canvas.

### 3.8 Disabled

`--o-disabled: 0.5` replaces `0.4`, `0.5` and `0.6`. Composited, `--text` at
0.5 over `--surface` is `#83858a` — Lc 35.7. That is deliberately below the
content floor: a disabled control is not content, and it must read as
unavailable at a glance. It stays clearly distinguishable from the background
(Lc 35.7 is well above the Lc 21.9 of today's `#666`, which was *live* text).

The rule that comes with it: **a disabled control must carry a `title` saying
why it is disabled.** Almost none do today — `WindowPanel.svelte:455-478` gives
its three stack buttons a `title` and an `aria-label`, but both describe the
*action* ("Move up in stack order"), not the reason it is unavailable, and
`KeybindsView.svelte:133-138` is the closest thing to an exception ("Reset to
EVE's default (not yet captured)"). `Button`'s and `Field`'s `disabledReason`
prop makes it a parameter rather than a convention, so the next person cannot
forget. Phase 1 fills it in where the reason is already computed at the call
site (`readOnly`, `i === 0`, a null default) and leaves it empty otherwise —
inventing new copy is Phase 5's job.

---

## 4 Migration map

Work straight down this. Every row is a specific edit at a specific line. Line
numbers are as of this branch; they shift as you edit, so work bottom-up within
a file or re-grep.

Three global rules apply to every row:

1. **Class names are not renamed.** Fourteen existing spec files query by CSS
   class — `BatchView.spec.ts:86` (`.head`), `HudPanel.spec.ts:78,209,218,262`
   (`.label`, `.row`, `.account-legend`), `KeybindsView.spec.ts:29` (`.chip`),
   `OverviewColumnsTab.spec.ts:81,83,90` (`.copy-targets`, `.copy-parts`),
   `OverviewFiltersTab.spec.ts:37,42,89,157` (`.group-grid`, `.group-cat`,
   `.group-filter`, `.cat-bulk`), `OverviewView.spec.ts:194,200`
   (`.name-entry`), `ProbeViewer.spec.ts:101,165` (`.bg`, `.probe-face`).
   Every primitive therefore accepts a `class` pass-through and the call site
   keeps its existing hook class. **Zero spec churn is a hard requirement of
   this phase.**
2. **Delete the rule and the class in the same edit.** Svelte warns about an
   unused selector but says nothing about a class left on an element whose rule
   you deleted — that silently loses styling.
3. **A colour in a `.ts` string or a `.json` file is data, not chrome.** Do not
   touch `OverviewAppearanceTab.svelte:26` `UNSET_HEX = "#808080"` (the
   placeholder swatch for an EVE overview state with no stored colour), the
   `#40ff40` / `#bf0000` / `#ff5900` values in `*.test.ts`, or
   `ProbeViewer.svelte:150`'s runtime `rgb(${…})` template. The guard test in
   §7.1 carries this allowlist.

### 4.1 `app/src/app.css`

| Line | Current | Becomes |
| --- | --- | --- |
| 1-14 | the `:root` block | the block in §3 |
| 17-21 | `button { background: var(--bg-panel); … border-radius: 4px; padding: 4px 10px }` | delete — `Button` owns it |
| 22 | `button:hover { border-color: var(--accent) }` | delete |
| 27-31 | `.rail` `background: var(--bg-panel); color: var(--fg-dim)` | `--surface` / `--text-muted`; keep the class, render via `Button variant="ghost" iconOnly` |
| 34-37 | `.backups` `background: var(--bg-panel)` | `--surface`; padding `var(--s2)` |
| 41 | `.mini-visible { font-size: 0.85em }` | delete — becomes `Button variant="ghost" size="sm"` |
| 42-45 | `.sidebar` `var(--bg-panel)` | `--surface`; padding `var(--s2)` |
| 46 | `.sidebar-actions { gap: 6px; margin-bottom: 8px }` | `gap: var(--s2); margin-bottom: var(--s2)` |
| 50 | `.sidebar .meta { color: var(--fg-dim); font-size: 0.85em }` | `--text-muted`; `font-size: var(--t-caption)` |
| 52-55 | `.filebar { gap: 10px; padding: 8px 12px }` | `gap: var(--s3); padding: var(--s2) var(--s3)` |
| 59 | `.badge { border-radius: 8px; padding: 1px 8px; font-size: 0.85em }` | delete — `Chip` |
| 60 | `.badge.editable { background: var(--ok); color: #10240f }` | `Chip tone="ok"` |
| 61 | `.badge.read-only { background: var(--danger); color: #2b100d }` | `Chip tone="danger"` |
| 62 | `.badge.dirty { background: var(--warn); color: #33260a }` | `Chip tone="warn"` |
| 63 | `.tree-area { padding: 8px 12px }` | `var(--s2) var(--s3)` |
| 64 | `.hint { color: var(--fg-dim); padding: 12px }` | delete — `EmptyState` or `InlineMessage`, per §4.5 |
| 65 | `.error { color: var(--danger); padding: 12px }` | delete — `InlineMessage variant="error"` |
| 66-69 | `.flash` | delete — `Toast` |
| 70 | `.field-error` | delete — `Field`'s `error` prop |
| 71-80 | `.searchbar`, `.searchbar .search`, `.searchbar .meta` | delete — `SearchField` |
| 81 | `@keyframes fade-out` | move into `Toast.svelte`, add a `prefers-reduced-motion` guard |
| 82 | `.hex { padding: 12px }` | `var(--s3)` |
| 84 | `.row.reveal-hit { background: rgba(79,156,240,0.28) }` | `var(--accent-dim)`; `border-radius: var(--r-sm)` |
| 85 | `.children { margin-left: 18px; border-left: 1px solid var(--border); padding-left: 6px }` | keep the border token; `margin-left: var(--s5)`, `padding-left: var(--s1)` |
| 87 | `.label { color: var(--accent) }` | unchanged |
| 90 | `.kind-int, .kind-float, .kind-long { color: #c8a1e8 }` | `var(--syntax-number)` |
| 91 | `.kind-str, … { color: #a3c9a5 }` | `var(--ok)` |
| 92 | `.kind-bytes { color: #d0b47f }` | `var(--text-secondary)` |
| 93 | `.kind-none, .kind-bool { color: #7fb2d0 }` | `var(--info)` |
| 94-95 | `.kind-ref/.kind-shared`, `.shared-mark` | unchanged (`--warn`) |
| 89 | `.display.editable:hover { outline: 1px dashed var(--fg-dim) }` | `var(--border-strong)` |
| **96-98** | **`.mini { opacity: 0 }` / `.row:hover .mini { opacity: 1 }` / `.mini.danger:hover`** | **delete all three — see §1** |
| 99-100 | `.edit { … border-radius: 3px; padding: 1px 4px }` | `Field kind="text"` |
| 101-102 | `.overlay { background: rgba(0,0,0,0.5) }` | `var(--scrim)`; moves into `Sheet` |
| 103-104 | `.modal { background: var(--bg-panel); border-radius: 6px; padding: 16px }` | `Sheet` — `--surface-overlay`, `--r-md`, `var(--s4)` |
| 110 | `.form-actions { gap: 8px; margin-top: 12px }` | `var(--s2)` / `var(--s3)` |
| 112 | `.save { border-color: var(--accent) }` | delete — `Button variant="primary"` |
| 115 | `.discard { border-color: var(--danger); font-size: 0.85em; padding: 1px 8px }` | delete — `Button variant="danger" size="sm"` |
| 118 | `button:disabled { opacity: 0.4 }` | delete — `Button` uses `--o-disabled` |
| 119-121 | `.viewtabs` | delete — `Tabs variant="segmented"` |
| 122 | `.tree-file` | delete — `Tabs variant="segmented"` |

### 4.2 The hex → token map, complete

Every one of the 36 hardcoded hex values and 25 `rgba()` literals, by file.

**`AccountsView.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 185, 189, 197 | `var(--line, #3333)` | `var(--border)` |
| 190-194 | `.chip.empty select` / `option`, `var(--bg-panel)`/`var(--fg)` | delete — `Field kind="select"` |
| 196 | `.error { color: #c0392b }` | delete — `InlineMessage variant="error"` |
| 198 | `background: var(--panel, #0001)` | `var(--surface-raised)` |
| 200 | `.unassigned h3 { opacity: 0.7 }` | `color: var(--text-muted); font-size: var(--t-caption)` |

**`AutofillView.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 151 | `.grip { opacity: 0.6 }` | `color: var(--text-muted)` (`ListRow`'s grip) |
| 152-156 | `input, button.mini, button.danger { … }` | delete — `Field` + `Button` |
| 158 | `button.danger { border-color: #a33 }` | delete — `Button variant="danger"` |
| 159-162 | `.shared-banner` | delete — `ScopeBanner` |
| 164-167 | `.pair button { … }` | delete — `Button` |
| 168-169 | `.hint, .error` / `.error { color: #e66 }` | delete — `EmptyState` / `InlineMessage` |

**`BatchView.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 417 | `label.disabled { opacity: 0.5 }` | `opacity: var(--o-disabled)` |
| 419-420 | `.linkbtn` | delete — `Button variant="ghost"` |
| 421-422 | `select, option { … }`, `input[checkbox/radio] { accent-color }` | delete — `Field` |
| 423 | `.muted { color: var(--fg-dim) }` | `var(--text-muted)` |
| 425 | `.warn { color: #d0a000 }` | `var(--warn)` |
| 426 | `.err, .fail { color: #e06c6c }` | `var(--danger)` |
| 427 | `.ok { color: #6cc06c }` | `var(--ok)` |
| 428 | `button { padding: 0.35rem 0.9rem }` | delete — `Button` |

**`ChatSplit.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 83 | `border-top: 1px solid #333` | `var(--border)` |
| 88-89 | `.legend { color: var(--warn); font-size: 10px }` | `ScopeBanner compact` — `--info` rail, `--t-caption` |
| 97, 100 | `.fields label { color: #aaa; font-size: 10px }` | `var(--text-secondary)`, `var(--t-caption)` |
| 103-112 | `.fields input { background: #11141a; border: 1px solid #444; color: #dbeafe }` | delete — `Field kind="number" width="5rem"` |
| 114-115 | `.area { color: #888; font-size: 10px }` | `var(--text-muted)`, `var(--t-caption)` |
| 119 | `.area.bad { color: var(--warn) }` | unchanged |
| 121-130 | `.stack-apply { background: #2a2f3a; border: 1px solid #444; color: #dbeafe }` | delete — `Button size="sm"` |
| 133 | `.stack-apply:disabled { opacity: 0.5 }` | delete — `Button` |

**`DetailParts.svelte`** — the `pointer-events: none` at line 37 is pinned by
`detail.test.ts:484-485`. **Do not touch it, and do not introduce
`pointer-events: auto` anywhere in this file's `<style>`.**

| Line | Current | Becomes |
| --- | --- | --- |
| 42 | `border: 1px solid rgba(148,163,184,0.45)` | `var(--muted-line)` |
| 43 | `color: #94a3b8` | `var(--text-muted)` |
| 44 | `font-size: 9px` | `var(--t-caption)` is 12px and would break the drawing; keep `9px` and note it — see §4.3 |
| 64 | `border-color: rgba(148,163,184,0.5)` | `var(--muted-line)` |
| 71-72 | `.core { background: rgba(245,158,11,0.45); border-color: rgba(245,158,11,0.7) }` | `var(--warn-veil)` / `var(--warn)` |
| 78 | `.band, .column { background: rgba(148,163,184,0.14) }` | `var(--muted-veil)` |

**`HudPanel.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 268 | `border-bottom: 1px solid var(--border)` | unchanged |
| 270 | `font-size: 12px` | `var(--t-caption)` |
| 272-279 | `.account-legend` | `ScopeBanner compact` — **keep the class name** (`HudPanel.spec.ts:262,279` queries it) |
| 286-289 | the "deliberately NOT app.css variables" comment | rewrite: both sides now reference `--warn`; the coupling is the token |
| 291-292 | `.group.selected { border-left-color: #f59e0b; background: rgba(245,158,11,0.08) }` | `var(--warn)` / `var(--warn-dim)` |
| 304 | `.group-title { font-size: 11px }` | `var(--t-caption)` |
| 310 | `.group.selected .group-title { color: #fde68a }` | `var(--warn)` |
| 322-334 | `input[type=number]`, `:disabled`, `input[type=checkbox]` | delete — `Field` |
| 335-341 | `.badge { color: var(--fg-dim); background: var(--bg-panel); font-size: 10px }` | `Chip tone="neutral" size="sm"` |

**`LayoutView.svelte`** — the canvas is a *depiction of the EVE screen*, so it
keeps its own local block. Only the colours and scales change.

| Line | Current | Becomes |
| --- | --- | --- |
| 1027 | `.canvas { background: #1b1f27 }` | `var(--surface)` — keeps "canvas is lighter than the app ground" |
| 1028-1029 | grid `linear-gradient(#2a2f3a …)` | `var(--border)` |
| 1031 | `border: 1px solid #444` | `var(--border-strong)` |
| 1036 | `.furniture { background: rgba(148,163,184,0.12) }` | `var(--muted-veil)` |
| 1037 | `border: 1px dashed #64748b` | `var(--border-strong)` |
| 1038-1039 | `color: #94a3b8; font-size: 11px` | `var(--text-secondary)` (Lc 46.9 → 80.2, §3.7); keep 11px |
| 1063-1065 | `.furniture.selected` `#f59e0b` / `rgba(245,158,11,.25)` / `#fde68a` | `var(--warn)` / `var(--warn-veil)` / `var(--text)` |
| 1076-1077 | `.anchor-dot { background: #f59e0b; border: 1px solid #1c1917 }` | `var(--warn)` / `var(--bg)` |
| 1094-1096 | `.win` `rgba(96,165,250,.25)` / `#60a5fa` / `#dbeafe` | `var(--accent-veil)` / `var(--accent)` / `var(--text)` |
| 1103-1104 | `.win.selected` `#f59e0b` / `rgba(245,158,11,.25)` | `var(--warn)` / `var(--warn-veil)` |
| 1115-1117 | `.win.droptarget` `#34d399` / `rgba(52,211,153,.3)` / `box-shadow rgba(52,211,153,.5)` | `var(--ok)` / `var(--ok-veil)` / `0 0 0 2px var(--ok)` |
| 1124 | `.guide { background: #f59e0b }` | `var(--warn)` |
| 1153 | `.tabs { background: #11141a }` | `var(--bg)` |
| 1161-1162 | `.tab { background: #2a2f3a; color: #dbeafe }` | `var(--surface-raised)` / `var(--text-secondary)` (Lc 82.4) |
| 1169-1170 | `.tab.active { background: #f59e0b; color: #1b1f27 }` | `var(--warn-dim)` / `var(--warn)` — §3.4, Lc 59.6 → 69.2 |
| 1176 | `.tab.dragging { opacity: 0.45 }` | keep — graphic (§3.2) |
| 1183 | `.resize { opacity: 0.6 }` | keep — graphic (§3.2) |
| 1191-1192 | `.ref { color: #888; font-size: 11px }` | `var(--text-muted)`, `var(--t-caption)` |
| 1199 | `.hintish { color: #666 }` | `var(--text-muted)` — Lc 21.9 → 72.0 |
| 1204 | `.det { color: #888 }` | `var(--text-secondary)` (it labels a checkbox) |
| 1208-1210 | `.det input { accent-color: var(--accent) }` | delete — `Field kind="checkbox"` |
| 1212-1219 | `.linkish` | delete — `Button variant="ghost"` |
| 1222 | `.hint { color: #888; padding: 1rem }` | `EmptyState` |

**`NeocomButtons.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 103 | `.head { color: var(--fg-dim) }` | `PanelHeader` |
| 117-120 | `.badge { color: var(--fg-dim); font-size: 10px }` | `Chip tone="neutral" size="sm"` |
| 124-131 | `select, option { border: 1px solid #444 }` | delete — `Field kind="select"` |

**`OverviewAppearanceTab.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 26 | `UNSET_HEX = "#808080"` | **unchanged — EVE data, not chrome** |
| 177-178 | `.apply-note`, `.meta` `var(--fg-dim)`, `0.85em` | `var(--text-muted)`, `var(--t-caption)` |
| 180-185 | `.subtabs` | delete — `Tabs variant="underline"` |
| 189 | `.grip { opacity: 0.6 }` | `color: var(--text-muted)` |
| 192 | `input[type=checkbox] { accent-color }` | delete — `Field` |
| 193-196 | `.swatch { background: var(--bg-panel) … }` | `Field kind="color"` |
| 199 | `.swatch.unset { opacity: 0.4 }` | `opacity: var(--o-disabled)` |
| 200 | `.default-note` `0.85em` | `var(--t-caption)` |
| 201-204 | `.reset { … }` | delete — `Button size="sm"` |

**`OverviewColumnsTab.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 163 | `.grip { opacity: 0.6 }` | `color: var(--text-muted)` |
| 164-170 | `input.w { … }` | delete — `Field kind="number" width="5rem"` |
| 171 | `.meta` `var(--fg-dim)`, `0.85em` | `var(--text-muted)`, `var(--t-caption)` |
| 173-176 | `.col-actions button, .copy-panel button` | delete — `Button` |
| 177-181 | `.copy-panel { … background: var(--bg-panel) }` | `Panel` — `var(--surface)`, `var(--r-md)` |
| 184, 187, 190 | `0.9em` / `0.85em` | `var(--t-body)` / `var(--t-caption)` |
| 195 | `input[type=checkbox] { accent-color }` | delete — `Field` |

**`OverviewFiltersTab.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 320 | `button.danger { border-color: #a33 }` | delete — `Button variant="danger"` |
| 321-326 | `select, option, optgroup, .name-entry input, .group-filter` | delete — `Field` / `SearchField` |
| 332 | `.section-heading { font-size: 0.9em }` | `var(--t-body)` |
| 335-340 | `.cat-bulk button { … var(--fg-dim) … 0.85em }` | delete — `Button variant="ghost" size="sm"` |
| 343 | `input[type=checkbox] { accent-color }` | delete — `Field` |
| 344 | `.unknown-groups { color: var(--warn) }` | `InlineMessage variant="warn"` |
| 349-353 | `.exceptions-list input[type=radio]` | delete — `Field kind="radio"` |

**`OverviewView.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 478-481 | `.shared-banner` | delete — `ScopeBanner` |
| 483-486 | `.pair button` | delete — `Button` |
| 492-502 | `.no-windows { … 0.85em … border: 1px solid var(--border) }` | `InlineMessage variant="info"` |
| 504 | `button.danger { border-color: #a33 }` | delete — `Button variant="danger"` |
| 505-510 | `select, option, optgroup, .name-entry input` | delete — `Field` |
| 511-517 | `.ov-tabs` | **keep for now** — a reorderable list with per-item controls, not a tab strip. Tokenise (`var(--border)`, `var(--r-sm)`, `var(--s1)`) and leave the structure to Phase 4 |
| 520-529 | `.swatch`, `.bold-toggle` | `Field kind="color"` / `Button pressed` |
| 530-534 | `.palette { … box-shadow: 0 4px 12px rgba(0,0,0,0.5) }` | `Popover` — gains the viewport clamp and Escape handling it lacks today |
| 536 | `.palette-grid button { border: 1px solid #0006 }` | `var(--border-strong)` — must read against an arbitrary user colour on either side |
| 538-542 | `.palette-none` | delete — `Button variant="ghost" size="sm"` |
| 543 | `.grip { opacity: 0.6 }` | `color: var(--text-muted)` |
| 545-550 | `.subtabs` | delete — `Tabs variant="underline"` |
| 554 | `.pack-actions button` | delete — `Button` |

**`PresetGroup.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 218-228 | `input:not([type])`, `input[type=checkbox]` | delete — `Field` |
| 230 | `.new label { font-size: 0.9em }` | `var(--t-body)` |
| 231 | `.new label.disabled { opacity: 0.5 }` | `opacity: var(--o-disabled)` |
| 232 | `.actions { gap: 6px; padding: 0.25rem 0.1rem }` | `var(--s2)` / `var(--s1)` |
| 233 | `.hint { opacity: 0.7; font-size: 0.85em }` | `color: var(--text-muted); font-size: var(--t-caption)` |
| 234 | `.meta` `var(--fg-dim)`, `0.85em` | `var(--text-muted)`, `var(--t-caption)` |

**`ProbeFormationsView.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 642-647 | `input, select { … }` | delete — `Field` |
| 652 | `border-right: 1px solid var(--border)` | unchanged |
| 656 | `li button.active { background: var(--accent); color: var(--bg) }` | `ListRow selected` — `var(--accent-dim)` / `var(--text)` |
| 658 | `.list-actions .danger { border-color: #a33 }` | delete — `Button variant="danger"` |
| 673, 675, 682 | `0.85em` | `var(--t-caption)` |
| 676 | `.units button.active { background: var(--accent); color: var(--bg) }` | `Button pressed` |
| 683 | `tr.selected td { background: rgba(79,156,240,0.12) }` | `var(--accent-dim)` |
| 694 | `td.u::after { color: var(--fg-dim); font-size: 0.85em }` | `var(--text-muted)`, `var(--t-caption)` |
| 699-702 | the `.mini`-is-invisible workaround comment and `td .mini-visible:hover` | delete both — `Button variant="ghost"` and the comment's premise are gone |
| 703 | `.meta { opacity: 0.7; font-size: 0.85em }` | `color: var(--text-muted); font-size: var(--t-caption)` |
| 704-707 | `.shared-banner` | delete — `ScopeBanner` |

**`ProbeViewer.svelte`** — SVG. Prefer `fill-opacity` / `stroke-opacity` over
`rgba()`, per §3.2. `ProbeViewer.spec.ts` queries `.bg`, `.probe-face`,
`line.grab` and several `aria-label`s — none of those change.

| Line | Current | Becomes |
| --- | --- | --- |
| 150 | `rgb(${rgb.map(…)})` | **unchanged — runtime formatting of EVE colour data** |
| 730 | `.bg { fill: var(--bg-panel) }` | `var(--surface)` |
| 739 | `.ring { stroke: var(--border); opacity: 0.6 }` | keep the opacity — graphic (§3.2) |
| 741 | `.cardinal { fill: var(--fg-dim); font-size: 11px }` | `var(--text-muted)`, `var(--t-caption)` |
| 745 | `.cardinal.north { fill: var(--fg) }` | `var(--text)` |
| 749 | `.scene-vol { fill: rgba(255,255,255,0.035) }` | `fill: var(--text); fill-opacity: 0.04` |
| 750-751 | `.scene-mark { fill: var(--fg) }`, `.scene-label { fill: var(--fg-dim); font-size: 10px }` | `var(--text)` / `var(--text-muted)`, `var(--t-caption)` |
| 753 | `.axis-label { fill: var(--fg-dim); font-size: 10px }` | `var(--text-muted)`, `var(--t-caption)` |
| 754 | `.range { fill: rgba(79,156,240,.06); stroke: rgba(79,156,240,.35) }` | `fill: var(--accent); fill-opacity: .06; stroke: var(--accent); stroke-opacity: .35` |
| 759 | `.probe-face { stroke: rgba(0,0,0,0.45) }` | `stroke: var(--bg); stroke-opacity: 0.6` |
| 769-770 | `.centre`, `.centre-dot` `var(--fg)` | `var(--text)` |
| 780 | `.meta { opacity: 0.7; font-size: 0.85em }` | `color: var(--text-muted); font-size: var(--t-caption)` |
| 783 | `.toggle { font-size: 0.85em; color: var(--fg-dim) }` | `var(--t-caption)`, `var(--text-muted)` |
| 786-788 | `.gx #e06c6c` / `.gy #7bc47b` / `.gz #6c9ce0` | `var(--danger)` / `var(--ok)` / `var(--accent)` — the X-red / Y-green / Z-blue convention survives, and all three are already solved |

**`Sidebar.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 188-189 | `.toggle { font-size: 0.85em; opacity: 0.75 }` | `var(--t-caption)`; `color: var(--text-muted)` |
| 195 | `.acct { color: var(--fg-dim); font-size: 0.85em }` | `var(--text-muted)`, `var(--t-caption)` |
| 198 | `.meta.not-live { color: var(--warn, #d08770) }` | `var(--warn)` — `--warn` is defined, so the fallback is dead code |

**`WindowPanel.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 524 | `font-size: 13px` | `var(--t-body)` |
| 526 | `background: var(--bg-panel)` | `var(--surface)` |
| 529-539 | `.orphans { … 0.85em … }` | `InlineMessage variant="warn"` |
| 548 | `.filters { background: var(--bg-panel) }` | `var(--surface)` |
| 551-563 | `.filters input[type=search]`, `:focus` | delete — `SearchField` |
| 568 | `.toggle { font-size: 12px }` | `var(--t-caption)` |
| 582 | `.row.selected { background: rgba(79,156,240,0.18) }` | `var(--accent-dim)` |
| 607 | `span.detail { font-size: 0.9em }` | `var(--t-body)` |
| 609-616 | `.badge.warn { background: var(--warn); color: #33260a; font-size: 11px }` | `Chip tone="warn" size="sm"` |
| 625 | `.stack-head { background: rgba(255,255,255,0.04) }` | `var(--surface-raised)` |
| 627 | `font-size: 12px` | `var(--t-caption)` |
| 634 | `.row.frame .row-head { background: rgba(255,255,255,0.04) }` | `var(--surface-raised)` |
| 639 | `.frame-label { font-size: 10px }` | `var(--t-caption)` |
| 665 | `.stack-btn { font-size: 0.85em }` | `Button size="sm"` |
| 668 | `.stack-btn:disabled { opacity: 0.4 }` | delete — `Button` |
| 677-692 | `select`, `select option` | delete — `Field kind="select"` |
| 706 | `.coords label { font-size: 11px }` | `var(--t-caption)` |
| 709-723 | `.detail input[type=number]`, `:focus` | delete — `Field kind="number"` |
| 747 | `.fam-head { background: rgba(255,255,255,0.04) }` | `var(--surface-raised)` |
| 749 | `font-size: 12px` | `var(--t-caption)` |

**`ContextMenu.svelte`**

| Line | Current | Becomes |
| --- | --- | --- |
| 84-87 | `.menu { background: var(--bg-panel); border-radius: 4px; box-shadow: 0 4px 12px rgba(0,0,0,0.5) }` | `var(--surface-overlay)`, `var(--r-md)`, `var(--shadow)` |
| 94 | `border-radius: 3px` | `var(--r-sm)` |
| 102-104 | `.menu button:hover { background: var(--accent); color: var(--bg) }` | `background: var(--accent-dim); color: var(--accent)` — §3.4 |
| 33-55 | the clamp logic | **move into `Popover`**, keep the comment; `ContextMenu` becomes a `MenuItem[]` renderer over `Popover` |

**`AboutPanel.svelte`, `FormationPicker.svelte`, `InsertForm.svelte`,
`BackupsPanel.svelte`** — no hex; only scale and class swaps:

| File:line | Current | Becomes |
| --- | --- | --- |
| `AboutPanel:46-47` | `.version`/`.meta` `var(--fg-dim)`, `0.85em` | `var(--text-muted)`, `var(--t-caption)` |
| `AboutPanel:49-50` | `.linkbtn` | `Button variant="ghost"` |
| `FormationPicker:69` | `.meta { opacity: 0.7; font-size: 0.85em }` | `var(--text-muted)`, `var(--t-caption)` |
| `InsertForm:104,115,135,140` | `.field-error` | `Field`'s `error` prop / `InlineMessage variant="error"` |
| `InsertForm:137` | `.hint` | `InlineMessage variant="info"` |
| `BackupsPanel:87-94` | `.subtitle { opacity: 0.7; font-size: 0.85em }` | `PanelHeader`'s `subtitle` — `var(--text-muted)`, `var(--t-caption)` |

### 4.3 The seven `font-size` lines that do not move to a token

`DetailParts.svelte:44` (`9px`), `LayoutView.svelte:1039` and `:1097` (`11px`),
and `ChatSplit.svelte:89, 100, 115, 127` (`10px`) are **drawings of EVE's screen
at canvas scale**, not app chrome. `--t-caption` is 12px; forcing it would make
the labels overflow the rectangles they name at typical canvas scales. Keep the
literal px and mark each with a one-line comment saying it is canvas-scale type.
The guard test in §7.1 carries exactly these seven lines as its font-size
allowlist.

`LayoutView.svelte:1192` is also `11px` but is **not** on the list — it is
`.ref`, a status line below the canvas, and it becomes `var(--t-caption)`.

### 4.4 The amber coupling, resolved

`HudPanel.svelte:286-289` currently asks a future reader to keep two hardcoded
colours in two files in sync. After this phase both sides reference `--warn`:

- `LayoutView.svelte:1063, 1076, 1103, 1124, 1169` → `var(--warn)`
- `LayoutView.svelte:1064, 1104` → `var(--warn-veil)`
- `LayoutView.svelte:1065` → `var(--text)` (the label, per §3.7)
- `HudPanel.svelte:291` → `var(--warn)`, `:292` → `var(--warn-dim)`, `:310` → `var(--warn)`

Rewrite the comment to say what is now true:

> The selected-group treatment shares `--warn` with the canvas's selected
> rectangle (`LayoutView`'s `.win.selected` / `.furniture.selected`). The panel
> and the rectangle it describes must agree; the token is what makes them.

The requirement is unchanged. The mechanism went from a comment to a
compile-time-ish guarantee, and the `no-hardcoded-hex` test now enforces it.

### 4.5 `.hint` — two meanings, two destinations

`.hint` is used 17 times for two different jobs. Do not migrate it mechanically:

- **"There is nothing here"** → `EmptyState`. `AccountsView:128`,
  `AutofillView:90,95,103`, `BackupsPanel:59`, `OverviewView:310,316,448`,
  `PresetGroup:192`, `ProbeFormationsView:465,630`, `Sidebar:143,145`,
  `routes/+page.svelte:503,637`, `LayoutView:1222`, plus the two **unstyled**
  `.empty` at `KeybindsView:84,90` and `.muted` at `BatchView:303,341`.
- **"Here is something you should know about this control"** →
  `InlineMessage variant="info"`. `PresetGroup:183` (what a preset copies),
  `InsertForm:137` (what an empty insert does), `LayoutView:931` `.hintish`
  (the canvas gesture hints), `AutofillView:85` (the pair prompt — this one
  takes `EmptyState`'s `action` snippet, because it is an empty state *with* a
  button, and `OverviewView:305` is the same shape with a different button
  treatment; unifying those two is the point).

### 4.6 The 28 native-control rules, in deletion order

All of these are deleted and replaced by `Field`'s single style block.
Twenty set `background` + `color`; seven set only `accent-color`; one sets a
disabled colour.

`app.css:106-109` · `AccountsView:190-194` (×2) · `AutofillView:153-157` ·
`BatchView:421`, `:422` · `ChatSplit:105-112` · `HudPanel:323-328`, `:329-331`,
`:332-334` · `KeybindsView:157`, `:158` · `LayoutView:1208-1211` ·
`NeocomButtons:125-131` · `OverviewAppearanceTab:192` ·
`OverviewColumnsTab:166-170`, `:195` · `OverviewFiltersTab:323-326`, `:343`,
`:351-353` · `OverviewView:507-510` · `PresetGroup:220-227`, `:228` ·
`ProbeFormationsView:644-647` · `WindowPanel:551-560`, `:561-563`, `:680-688`,
`:689-692`, `:711-720`, `:721-723`.

### 4.7 The 33 `opacity` declarations, by disposition

**Delete, replace with a `--text-*` token** (20): `AccountsView:200` ·
`AutofillView:151` · `BackupsPanel:90` · `FormationPicker:69` ·
`KeybindsView:161`, `:162`, `:163` · `OverviewAppearanceTab:189` ·
`OverviewColumnsTab:163` · `OverviewView:543` · `PresetGroup:233` ·
`ProbeFormationsView:703` · `ProbeViewer:780` · `Sidebar:189` — plus the six
`opacity` uses folded into `Button`/`Field` below.

**Becomes `--o-disabled`** (7): `app.css:118` · `BatchView:417` ·
`ChatSplit:133` · `KeybindsView:160` · `OverviewAppearanceTab:199` ·
`PresetGroup:231` · `WindowPanel:668`.

**Deleted with `.mini`** (3): `app.css:81` (×2 in the keyframes — these move to
`Toast`), `app.css:96`, `:97`.

**Kept as graphics** (10): `LayoutView:1176`, `:1183` ·
`ProbeViewer:739, 750, 769, 770, 781, 792` and the two `filter: brightness`
siblings at `:764`, `:801` that are not opacity at all.

---

## 5 The twelve primitives

All live in `app/src/lib/ui/`, one `.svelte` file each, plus `toast.svelte.ts`
and `apca.ts`. Svelte 5 runes throughout. **No dependency is added** —
`app/package.json` carries no component library today and this phase does not
change that.

Four conventions apply to every one of them:

- **`class` pass-through.** Every primitive accepts `class?: string` and merges
  it onto its root element, so a call site keeps the hook class its spec
  queries (§4, rule 1).
- **`:focus-visible`, always.** `outline: 2px solid var(--accent);
  outline-offset: 1px; border-radius: inherit`. Never `outline: none` without a
  replacement — `ProbeViewer:778` is the one sanctioned exception and its
  four-line comment explains why (the elements are `tabindex="-1"` and
  pointer-only, so the ring is pure noise).
- **No `opacity` except `--o-disabled`.**
- **`title` on any icon-only control**, doubling as its `aria-label`.

### 5.1 `Button.svelte`

```ts
type ButtonProps = {
  variant?: "default" | "primary" | "ghost" | "danger";  // default "default"
  size?: "sm" | "md";                                    // default "md"
  type?: "button" | "submit";                            // default "button"
  disabled?: boolean;
  disabledReason?: string;   // becomes `title` when disabled — §3.8
  pressed?: boolean;         // renders aria-pressed; toggle buttons
  iconOnly?: boolean;        // square padding; requires `title`
  title?: string;            // also the aria-label when iconOnly
  href?: string;             // renders <a role="button"> — external links only
  class?: string;
  onclick?: (e: MouseEvent) => void;
  oncontextmenu?: (e: MouseEvent) => void;
  children: Snippet;
};
```

| Variant | Rest | Hover | Active | Focus-visible | Disabled |
| --- | --- | --- | --- | --- | --- |
| `default` | `--surface-raised` / `--border` / `--text` | `--surface-overlay`, border `--border-strong` | `--surface` | accent ring | `--o-disabled`, `cursor: default`, no hover |
| `primary` | `--accent-dim` / `--accent` / `--accent`, weight 600 | `--accent-dim` lightened via border `--accent` | as rest | accent ring | as above |
| `ghost` | transparent / transparent / `--text-secondary` | `--surface-raised`, `--text` | `--surface` | accent ring | as above |
| `danger` | `--surface-raised` / `--danger` / `--danger` | `--danger-dim` | as rest | accent ring | as above |

`pressed` adds `border-color: var(--accent); color: var(--text)` on any variant.
Sizes: `sm` = `var(--t-caption)`, padding `2px var(--s2)`; `md` = `var(--t-ui)`,
padding `var(--s1) var(--s3)`. Radius `--r-sm` throughout.

**The ghost variant is always visible.** That is the whole reason it exists;
see §1. If a row wants its controls to recede until hover, it does that by
changing *colour* on `.row:hover`, never by hiding them.

Replaces: the global `button` rule (`app.css:17-22`), `button:disabled`
(`:118`), `.save` (`:112`), `.discard` (`:115`), `.mini` (`:96-98`),
`.mini-visible` (`:41`), `.rail` (`:27-31`), `.twisty` (`:86`), the four
`border-color: #a33` copies, `.linkbtn` (`AboutPanel:49`, `BatchView:419`),
`.linkish` (`LayoutView:1212`), `.collapse` (`BackupsPanel:84`, `Sidebar:211`),
`.x` (`AccountsView:195`), `.caret` (`WindowPanel:644`), `.stack-btn`
(`WindowPanel:662`), `.stack-apply` (`ChatSplit:121`), `.cat-bulk button`
(`OverviewFiltersTab:335`), `.reset` (`OverviewAppearanceTab:201`),
`.palette-none` (`OverviewView:538`), `.pack-actions button` (`:554`),
`.col-actions button` (`OverviewColumnsTab:173`), `.pair button`
(`AutofillView:164`, `OverviewView:483`), `.units button` and
`.list-actions .danger` (`ProbeFormationsView:675, 658`), `.group-title`
(`HudPanel:299`), `.bold-toggle` (`OverviewView:525`), `.tab-chip` (`:517`),
`button { padding: 0.35rem 0.9rem }` (`BatchView:428`), `.mv, .rm`
(`NeocomButtons:121`).

### 5.2 `Field.svelte`

```ts
type FieldProps = {
  kind?: "text" | "number" | "select" | "checkbox" | "radio" | "search" | "color";
  value?: string | number | boolean;   // $bindable
  label?: string;              // renders a <label>; otherwise ariaLabel is required
  ariaLabel?: string;
  id?: string;                 // auto-generated when absent, so label/for always pairs
  options?: { value: string; label: string; group?: string }[];  // select only
  placeholder?: string;
  disabled?: boolean;
  disabledReason?: string;
  readonly?: boolean;
  min?: number; max?: number; step?: number;   // number only
  width?: string;              // CSS width, e.g. "5rem"
  error?: string;              // renders an InlineMessage below + aria-invalid + aria-describedby
  layout?: "row" | "column";   // label beside vs above — default "row"
  class?: string;
  onchange?: (e: Event) => void;
  oninput?: (e: Event) => void;
};
```

One style block, and it is the entire reason this component exists:

```css
input, select, textarea {
  background: var(--surface-raised);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: var(--r-sm);
  padding: var(--s1) var(--s2);
  font: inherit;
  font-size: var(--t-ui);
}
select option, select optgroup { background: var(--surface-raised); color: var(--text); }
input[type="checkbox"], input[type="radio"] { accent-color: var(--accent); }
input:focus-visible, select:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
input:disabled, select:disabled { opacity: var(--o-disabled); cursor: default; }
input::placeholder { color: var(--text-muted); }
```

`options[].group` renders `<optgroup>` — `OverviewFiltersTab:323` and
`OverviewView:507` both style `optgroup` today, so the capability is required,
not speculative.

Replaces all 28 rules in §4.6.

### 5.3 `SearchField.svelte`

```ts
type SearchFieldProps = {
  value?: string;              // $bindable
  verb?: "search" | "filter";  // default "filter" — drives the placeholder grammar
  nouns: string;               // "commands and keys", "windows", "lists"
  shortcut?: string;           // "Ctrl+F" — appended as " (Ctrl+F)"
  count?: number;              // matches found
  total?: number;              // when given, renders "n of m"
  onclear?: () => void;
  class?: string;
};
```

The placeholder is *built*, not passed: `search` → `Search {nouns}`, `filter` →
`Filter {nouns}…`, then `+ " (" + shortcut + ")"`. That is what collapses two
verbs and three conventions into one rule, and it is why `nouns` is a prop
rather than `placeholder` being one.

Renders a `Field kind="search"`, a `--text-muted` `--t-caption` count when
`count` is given, and a ghost `×` clear button that appears only when
`value !== ""` — matching today's `{#if searching}` guard at
`routes/+page.svelte:617`. Escape clears and calls `onclear`.

Replaces: `routes/+page.svelte:611-623` (`.searchbar`, `.search`, the `.meta`
count and the invisible `.mini` clear button), `KeybindsView:101` + `:157`,
`AutofillView:99` + `:144`, `OverviewFiltersTab:237` + `:323`,
`WindowPanel:361` + `:551-563`.

### 5.4 `Chip.svelte`

```ts
type ChipProps = {
  tone?: "neutral" | "accent" | "ok" | "warn" | "danger" | "info";  // default "neutral"
  size?: "sm" | "md";          // --t-caption / --t-body
  title?: string;
  class?: string;
  children: Snippet;
};
```

`background: var(--{tone}-dim); color: var(--{tone}); border: 1px solid
var(--{tone}); border-radius: var(--r-pill); padding: 1px var(--s2)`.
`neutral` uses `--surface-raised` / `--text-secondary` / `--border`.

**A Chip never renders dark text on a saturated fill** (§3.4). It is a
non-interactive label; anything clickable is a `Button`.

Replaces: `.badge` and its three variants (`app.css:59-62`, rendered at
`routes/+page.svelte:510-518`), `HudPanel .badge` (`:335`, rendered at
`:188, 216, 217, 234, 249`), `NeocomButtons .badge` (`:117`, rendered at `:67`),
`WindowPanel .badge.warn` (`:609`, rendered at `:238, 242`), `AccountsView
.chip` (`:188`), `WindowPanel .stack-count` (`:630`), `ProbeFormationsView`'s
unit annotations.

Not replaced: `KeybindsView .chip` (`:158`) — that is a clickable capture
button, so it is `Button` with `pressed={listening === command}`.
`KeybindsView.spec.ts:29` queries `.chip`, so **keep the class name on it.**

### 5.5 `Tabs.svelte`

```ts
type TabsProps = {
  tabs: { id: string; label: string; disabled?: boolean; disabledReason?: string }[];
  value?: string;                        // $bindable
  variant?: "segmented" | "underline";   // default "segmented"
  ariaLabel: string;
  class?: string;
};
```

- `segmented`: a pill group; active = `--accent-dim` ground, `--accent` text,
  `--accent` border. Replaces `.viewtabs` (`app.css:119-121`) and `.tree-file`
  (`:122`).
- `underline`: flat buttons over a `1px solid var(--border)` rule; active =
  `--text` with a `2px solid var(--accent)` bottom border. Replaces `.subtabs`
  (`OverviewView:545-550` and `OverviewAppearanceTab:180-185`, which are
  identical).

Accessibility, done once: `role="tablist"` on the container, `role="tab"` +
`aria-selected` + roving `tabindex` on each, Left/Right/Home/End keyboard
movement, `aria-controls` when the caller supplies panel ids. Today
`OverviewView:452` sets `role="tablist"` but its children have no `role="tab"`,
which is an invalid ARIA tree that `svelte-check` does not catch;
`routes/+page.svelte:528` is a bare `<span>` of buttons with no roles at all.

`disabled` + `disabledReason` render the tab with `aria-disabled="true"` and a
`title`, rather than omitting it. **Phase 1 does not use this.** The call sites
keep their existing `{#if}` guards (`routes/+page.svelte:527-536`) so the strip
still shows exactly the tabs it shows today; Phase 2 removes the guards and
passes `disabled` instead. The capability ships here so Phase 2 is a
one-line-per-tab diff, and shipping it unused is cheaper than reshaping the API
later.

Not replaced: `OverviewView .ov-tabs` (`:511-517`) — a reorderable list of
overview tabs with a colour swatch and a bold toggle per item. It is a
`ListRow` set, and Phase 4 restructures it. Tokenise it in place.
`LayoutView .tabs/.tab` (`:1150-1177`) is a **drawing of EVE's own tab bar** on
the canvas and stays local.

### 5.6 `Panel.svelte` + `PanelHeader.svelte`

```ts
type PanelProps = {
  as?: "section" | "aside" | "div";   // default "section"
  padded?: boolean;                    // default true — var(--s3)
  scroll?: boolean;                    // overflow-y: auto; min-height: 0
  bordered?: boolean;                  // default true
  class?: string;
  children: Snippet;
};

type PanelHeaderProps = {
  title: string;
  subtitle?: string;                   // --t-caption / --text-muted
  level?: 2 | 3 | 4;                   // default 3 — heading rank, not size
  collapsed?: boolean;                 // $bindable; renders a chevron when defined
  oncollapse?: () => void;
  actions?: Snippet;                   // pinned right via margin-left: auto
  class?: string;
};
```

`Panel`: `background: var(--surface); border: 1px solid var(--border);
border-radius: var(--r-md)`.
`PanelHeader`: `display: flex; align-items: baseline; gap: var(--s2)`; the
title at `var(--t-title)` / weight 600 / `var(--text)`; the subtitle at
`var(--t-caption)` / `var(--text-muted)`, ellipsised.

`level` sets the heading tag independently of its size, so the document outline
can be correct without dragging the visual scale with it — the thing
`BackupsPanel:81-83` currently works around by zeroing `h3`'s margin.

The `subtitle` slot exists because `BackupsPanel:87-94` is exactly this, and it
is the panel whose *subject silently changes* (structural fault 3). Phase 1
makes it legible (`opacity: .7` at Lc 40.6 → `--text-muted` at Lc 71.1);
Phase 2 changes what it says.

Replaces the ad-hoc headers at: `BackupsPanel:73-83` (`.backups-head`),
`Sidebar:201-213` (`.sidebar-top`), `AccountsView:183` (`.accounts-head`),
`AutofillView:146` (`.af-list header`), `BatchView:415` (`.head`),
`NeocomButtons:101` (`.head`), `OverviewColumnsTab:185` (`.copy-head`),
`OverviewFiltersTab:330` (`.contents-head`), `WindowPanel:620` (`.stack-head`)
and `:742` (`.fam-head`), `HudPanel:299` (`.group-title`).
**Keep `.head` on `BatchView`'s** — `BatchView.spec.ts:86` queries it.

### 5.7 `EmptyState.svelte`

```ts
type EmptyStateProps = {
  title: string;               // --t-title / 600 / --text
  description?: string;        // --t-body / --text-secondary
  action?: Snippet;            // one or two Buttons
  class?: string;
};
```

Centred, `padding: var(--s6) var(--s4)`, `max-width: 44ch`, `gap: var(--s2)`.

The `action` snippet is the point: `AutofillView:85-89` and
`OverviewView:305-309` render *the same "pair this character" prompt with two
different button treatments*. One component, one treatment.

Replaces the "nothing here" half of `.hint` (§4.5), the **unstyled** `.empty`
(`KeybindsView:84, 90`), and `.muted`-as-empty (`BatchView:303, 341`).

### 5.8 `InlineMessage.svelte`

```ts
type InlineMessageProps = {
  variant?: "info" | "warn" | "error" | "success";   // default "info"
  title?: string;              // optional bold lead-in, in the role tone
  dismissible?: boolean;
  ondismiss?: () => void;
  role?: "status" | "alert";   // default: alert for warn/error, status otherwise
  class?: string;
  children: Snippet;
};
```

`background: var(--{role}-dim); border-left: 2px solid var(--{role});
border-radius: var(--r-sm); padding: var(--s2) var(--s3)`. The **body text is
`--text`** (Lc 94 on a `-dim` ground); only the rail and the optional `title`
carry the role tone (Lc 69). The colour says which kind; the size and weight
say it is a sentence.

`role="alert"` on warn and error is new. Today `.error` is a bare `<p>` at nine
sites with no live region, so a validation failure is silent to a screen
reader; only the three `.flash` sites have `aria-live` (`AccountsView:125`,
`ProbeFormationsView:503`, `Sidebar:140`).

Replaces: `.error` (`app.css:65` + `AccountsView:124`, `AutofillView:93`,
`BackupsPanel:57`, `KeybindsView:88`, `OverviewView:312`,
`ProbeFormationsView:471`, `Sidebar:141`, `routes/+page.svelte:642`),
`.field-error` (`app.css:70` + `InsertForm:104, 115, 135, 140`), `.err`/`.fail`
(`BatchView:426`, used at `:377, 395`), `BatchView`'s `.warn`/`.ok`
(`:425, 427`), `.orphans` (`WindowPanel:529-539`), `.no-windows`
(`OverviewView:492-502`), `.unknown-groups` (`OverviewFiltersTab:344`), and the
"note beside a control" half of `.hint` (§4.5).

### 5.9 `ScopeBanner.svelte`

```ts
type ScopeBannerProps = {
  label: string;               // renders nothing when ""
  compact?: boolean;           // --s1/--s2 padding, --t-caption — for dense panels
  action?: Snippet;            // e.g. "Manage accounts"
  class?: string;
};
```

An `InlineMessage variant="info"` with a fixed shape, and nothing else. It
renders `null` when `label` is `""`, matching today's `{#if sharedLabel}` guard.

The string is still built at `routes/+page.svelte:156-159` and threaded through
as the existing `sharedLabel` prop. **Do not move the derivation into the
component in this phase** — that changes four component signatures and their
specs, which is a Phase 4 tidy, not a token swap.

Replaces four byte-identical CSS blocks (`AutofillView:159-162`,
`KeybindsView:187-190`, `OverviewView:478-481`,
`ProbeFormationsView:704-707`), rendered at `AutofillView:97`,
`KeybindsView:96`, `OverviewView:314`, `ProbeFormationsView:477`.

Also absorbs two near-copies:
- `HudPanel:272-279` `.account-legend`, rendered at `:186-191`. Use
  `compact`, and **keep the `.account-legend` class** —
  `HudPanel.spec.ts:262,279` queries it.
- `ChatSplit:87-91` `.legend`, rendered at `:44-46`. Use `compact`. Its colour
  moves from `--warn` to the banner's `--info`: it is a scope statement, not a
  warning, and `--warn` in this panel already means "account-scoped row"
  (`HudPanel:57`), so the two were saying different things in the same colour.

### 5.10 `ListRow.svelte`

```ts
type ListRowProps = {
  selected?: boolean;
  indent?: 0 | 1 | 2;          // padding-left in --s5 steps
  onclick?: () => void;        // when given, the label becomes a real <button>
  oncontextmenu?: (e: MouseEvent) => void;
  actions?: MenuItem[];        // renders a ghost "⋯" that opens the same menu
  draggable?: boolean;         // renders a grip and forwards the 5 drag handlers
  ondragstart?: (e: DragEvent) => void;
  ondragover?: (e: DragEvent) => void;
  ondrop?: (e: DragEvent) => void;
  ondragend?: (e: DragEvent) => void;
  title?: string;
  leading?: Snippet;           // a caret, a checkbox, a swatch
  trailing?: Snippet;          // meta text, chips, buttons
  class?: string;
  children: Snippet;           // the label
};
```

`display: flex; align-items: center; gap: var(--s2); padding: var(--s1)
var(--s2); border-radius: var(--r-sm); min-width: 0`. Hover
`--surface-raised`; `selected` `--accent-dim`. The label gets `min-width: 0;
overflow: hidden; text-overflow: ellipsis; white-space: nowrap` — the
truncation `WindowPanel:590-603` and `app.css:48-49` each discovered
separately. The grip is `cursor: grab; color: var(--text-muted);
aria-hidden="true"`, replacing five `.grip { opacity: 0.6 }` copies.

**On the `⋯` and Phase 1's no-behaviour-change rule:** `actions` renders a
visible overflow button, and that is an *added* control. Phase 1 therefore
passes `actions` **only where a visible control already exists**. The three
right-click-only menus — `PresetGroup:214`, `WindowPanel:517`,
`LayoutView:1003` — keep `oncontextmenu` alone and get their visible `⋯` in
Phase 4. Shipping the prop now means Phase 4 is a one-argument change per call
site; using it now would make Phase 1 a behaviour change and break its "revert
the whole phase" property.

Replaces the hand-rolled rows at: `Sidebar:166-172` (`.file`, `app.css:48-49`),
`WindowPanel:578-608` (`.row`, `.row-head`, `.name`) — **keep `.row` and
`.row-head`**, queried by `HudPanel.spec.ts:209,218` and used by
`WindowPanel`'s own selectors — `AutofillView:114-130` (`.af-list li`),
`OverviewColumnsTab:162` (`.ov-cols li`), `OverviewAppearanceTab:187`
(`.state-list li`), `PresetGroup`'s preset rows,
`ProbeFormationsView:655` (`.formation-list li button`), `BackupsPanel`'s
backup rows (`app.css:39`), `NeocomButtons:105-116` (`.row`, `.id`).

### 5.11 `Popover.svelte` + `Sheet.svelte`

Two files, because they solve different problems, but only one of them is new
code.

```ts
type PopoverProps = {
  anchor: HTMLElement | { x: number; y: number };
  placement?: "bottom-start" | "bottom-end" | "top-start" | "point";  // "point" = a click point
  open?: boolean;              // $bindable
  onclose: () => void;
  ariaLabel: string;
  class?: string;
  children: Snippet;
};
```

The positioning, clamping and dismissal logic is **lifted verbatim** from
`ContextMenu.svelte:33-62`, including its comment about why `pos` is an
`untrack`ed snapshot rather than a `$derived`. That code is correct and hard-won;
this moves it somewhere it can be used twice. `ContextMenu.svelte` then becomes
a ~15-line `MenuItem[]` renderer over `Popover`, keeping its exported
`MenuItem` interface (`ContextMenu.svelte:1-6`) so its three importers
(`LayoutView:13`, `PresetGroup:5`, `WindowPanel:5`) do not change.

Second user, immediately: `OverviewView:530-534`'s colour palette dropdown,
which today has **no viewport clamp and no Escape handler** — it is a bare
`position: absolute` div. It gains both by being a `Popover`.

```ts
type SheetProps = {
  open?: boolean;              // $bindable
  title: string;               // the accessible name
  placement?: "center" | "end";   // default "center"
  width?: string;              // default "min(720px, 92vw)"
  onclose: () => void;
  footer?: Snippet;
  class?: string;
  children: Snippet;
};
```

`placement="center"` is the modal three call sites already render by hand
(`AboutPanel:21-24`, `FormationPicker:30-31`, `routes/+page.svelte:665-666`,
all sharing `app.css:101-110`). `placement="end"` is the right-hand sheet
Phase 3 needs for Accounts and Copy settings.

**One component, not two, deliberately.** A Sheet and a Dialog differ in
`inset` and one transform; they are identical in everything that is actually
hard — the scrim, `role="dialog"`, `aria-modal`, the focus trap, Escape,
restoring focus to the opener. Building a second component for the differing
`inset` would be the speculative kind of abstraction this codebase already has
too much of. Phase 1 ships it with three real users on `center`; Phase 3 flips
a prop.

What it adds that the current `.modal` lacks: a focus trap, focus restoration,
and `Escape` (today only `AboutPanel` and `FormationPicker` close on a backdrop
click, and nothing closes on Escape except `ContextMenu`). That is an
accessibility floor, not a feature — §"When NOT to be lazy" applies.

### 5.12 `Toast.svelte` + `toast.svelte.ts`

```ts
// toast.svelte.ts
export type ToastVariant = "info" | "success" | "warn" | "error";
export function toast(
  message: string,
  opts?: { variant?: ToastVariant; duration?: number; action?: { label: string; run: () => void } },
): void;
export const toasts: { id: number; message: string; variant: ToastVariant; action?: … }[];
```

```ts
// Toast.svelte — the host. Mounted once, in routes/+page.svelte.
type ToastHostProps = { class?: string };
```

`position: fixed; inset-block-end: var(--s4); inset-inline-end: var(--s4);
display: flex; flex-direction: column; gap: var(--s2); z-index: 60` (above
`ContextMenu`'s 50 at `ContextMenu.svelte:81`), `role="status"`
`aria-live="polite"`. Each toast is an `InlineMessage` on `--surface-overlay`
with `--shadow`.

Defaults: `duration: 4000`; `error` defaults to sticky (`duration: 0`) with a
dismiss button. Today's `.flash` uses 2000 ms
(`Sidebar:87, 104`, `ProbeFormationsView:306`), which is under the usual 5 s
reading-time guidance for a message that names a file. Four seconds for a
three-word confirmation is the compromise; anything the user must read to act
on is sticky.

The `fade-out` keyframes move here from `app.css:81` **with a
`@media (prefers-reduced-motion: reduce)` guard**, which the current animation
does not have.

Replaces `.flash` (`app.css:66-69`) and its three hand-rolled timer pairs:
`Sidebar:85-88`, `Sidebar:102-104`, `ProbeFormationsView:304-306`, plus
`AccountsView`'s `captureNote` (`:125`). Phase 5 routes the ~58 `message()`
dialogs through it; Phase 1 changes only the three existing call sites, keeping
their exact wording.

---

## 6 File-by-file change list

Work in this order. Each step leaves the app in a working, committable state.

**Step 1 — the token block.** `app/src/app.css:1-14` only. Nothing else. At
this point the app looks *wrong* (every `var(--bg-panel)` is now undefined), so
step 2 is not optional, but keeping the commit boundary here makes the palette
reviewable on its own.

Add compatibility aliases inside `:root` for the duration of the migration:

```css
  /* TEMPORARY — deleted in step 5. Lets steps 2-4 land file by file. */
  --bg-panel: var(--surface);
  --fg: var(--text);
  --fg-dim: var(--text-muted);
```

That is three lines that keep every untouched file rendering while the swap
proceeds, and their deletion in step 5 is what proves the swap is complete.

**Step 2 — the primitives.** Create `app/src/lib/ui/`:

| File | Notes |
| --- | --- |
| `apca.ts` | ~25 lines, §7.2. No dependency. |
| `Button.svelte` | §5.1 |
| `Field.svelte` | §5.2 — the largest win |
| `SearchField.svelte` | §5.3 |
| `Chip.svelte` | §5.4 |
| `Tabs.svelte` | §5.5 |
| `Panel.svelte`, `PanelHeader.svelte` | §5.6 |
| `EmptyState.svelte` | §5.7 |
| `InlineMessage.svelte` | §5.8 |
| `ScopeBanner.svelte` | §5.9 |
| `ListRow.svelte` | §5.10 |
| `Popover.svelte`, `Sheet.svelte` | §5.11 |
| `Toast.svelte`, `toast.svelte.ts` | §5.12 |

plus the specs in §7.3. `ContextMenu.svelte` is rewritten over `Popover` in
this step, keeping its module-level `MenuItem` export.

**Step 3 — `app.css`.** Work down §4.1. Delete `.mini`, `.badge`, `.hint`,
`.error`, `.flash`, `.field-error`, `.searchbar`, `.viewtabs`, `.tree-file`,
`.save`, `.discard`, the global `button` rules, `.overlay`, `.modal`.

**Step 4 — the views, in ascending order of risk.** Each is one commit.

| Order | File | Why here | Primitives it pulls in |
| --- | --- | --- | --- |
| 1 | `AboutPanel.svelte` | 8 lines of style, has a spec | `Sheet`, `Button` |
| 2 | `FormationPicker.svelte` | 8 lines, has a spec | `Sheet`, `ListRow` |
| 3 | `BackupsPanel.svelte` | 23 lines, no hex | `Panel`, `PanelHeader`, `ListRow`, `Button`, `EmptyState` |
| 4 | `Sidebar.svelte` | 32 lines, one dead fallback | `PanelHeader`, `ListRow`, `Button`, `Field`, `EmptyState`, `Toast` |
| 5 | `NeocomButtons.svelte` | 39 lines | `Field`, `Chip`, `ListRow`, `PanelHeader` |
| 6 | `ContextMenu.svelte` | already done in step 2 | — |
| 7 | `AccountsView.svelte` | the two undefined vars live here | `Field`, `Chip`, `Button`, `InlineMessage`, `EmptyState`, `Panel` |
| 8 | `AutofillView.svelte` | 2 invisible buttons | `Field`, `Button`, `ScopeBanner`, `EmptyState`, `ListRow`, `SearchField` |
| 9 | `PresetGroup.svelte` | right-click menu — leave it right-click-only | `Field`, `Button`, `ListRow`, `EmptyState` |
| 10 | `KeybindsView.svelte` | 1 invisible button; `.chip` and `.default` | `SearchField`, `Button`, `ScopeBanner`, `EmptyState`, `InlineMessage` |
| 11 | `BatchView.svelte` | spec queries `.head` | `Field`, `Button`, `InlineMessage`, `EmptyState`, `PanelHeader` |
| 12 | `ChatSplit.svelte` | second palette | `Field`, `Button`, `ScopeBanner` |
| 13 | `HudPanel.svelte` | the amber coupling; spec queries `.account-legend` | `Field`, `Chip`, `ScopeBanner`, `ListRow`, `PanelHeader` |
| 14 | `DetailParts.svelte` | `detail.test.ts` pins `pointer-events: none` | none — colour only |
| 15 | `ProbeViewer.svelte` | SVG; spec queries several classes | `Button` |
| 16 | `ProbeFormationsView.svelte` | `.mini-visible` workaround goes | `Field`, `Button`, `ScopeBanner`, `ListRow`, `EmptyState`, `Toast` |
| 17 | `WindowPanel.svelte` | 244 style lines, the biggest | `Field`, `SearchField`, `Chip`, `ListRow`, `Button`, `InlineMessage`, `PanelHeader` |
| 18 | `OverviewColumnsTab.svelte` | spec queries `.copy-targets`/`.copy-parts` | `Field`, `Button`, `Panel`, `PanelHeader`, `ListRow` |
| 19 | `OverviewAppearanceTab.svelte` | `UNSET_HEX` must not be touched | `Field`, `Tabs`, `Button`, `ListRow` |
| 20 | `OverviewFiltersTab.svelte` | 4 spec class queries | `Field`, `SearchField`, `Button`, `InlineMessage` |
| 21 | `OverviewView.svelte` | 2 spec class queries; the `.palette` popover | `Field`, `Tabs`, `Button`, `ScopeBanner`, `Popover`, `InlineMessage`, `EmptyState` |
| 22 | `TreeNode.svelte` | 3 `.mini` buttons that *do* work today | `Button` |
| 23 | `InsertForm.svelte` | 4 `.field-error` sites | `Field`, `Button`, `Sheet`, `InlineMessage` |
| 24 | `LayoutView.svelte` | 1219 lines, 219 of style, most of the second palette | `Button`, `Field`, `EmptyState`, `Popover` |
| 25 | `routes/+page.svelte` | the shell; mounts `Toast` | `Tabs`, `SearchField`, `Chip`, `Button`, `EmptyState`, `InlineMessage`, `Sheet`, `Toast` |

**Step 5 — delete the aliases.** Remove the three temporary `--bg-panel`,
`--fg`, `--fg-dim` lines from `:root`. `npm run check` and `npm test` must be
clean. If anything renders untokenised at this point, the guard tests in §7.1
say exactly where.

---

## 7 Tests

Frontend tests are vitest beside the component (`app/src/lib/test/README.md`).
Two kinds are needed, and the cheap kind is worth more.

### 7.1 Guard tests — `app/src/lib/ui/tokens.test.ts`

These read source text, exactly as `detail.test.ts:470-486` already does for
`DetailParts`'s `pointer-events: none`. They cost ~120 lines and they are what
stops this phase decaying back into 45 hex values over the next ten features.
Written with `check()` from `$lib/test/check.ts`.

1. **`no-hardcoded-hex`** — every `#rgb`/`#rrggbb`/`#rrggbbaa` in
   `app/src/**/*.svelte` and `app/src/app.css` sits inside the `:root` block, or
   is on the data allowlist: `OverviewAppearanceTab.svelte:26` (`UNSET_HEX`),
   `ProbeViewer.svelte:150` (runtime `rgb()` template). Fails on any new one.
2. **`no-rgba-literals`** — no `rgba(`/`rgb(` in a `<style>` block. Forces
   veil tokens in HTML and `fill-opacity`/`stroke-opacity` in SVG.
3. **`no-undefined-tokens`** — every `var(--x)` referenced anywhere in
   `app/src` is declared in `app.css`'s `:root`. *This is the test that would
   have caught `--line` and `--panel`.* Assert it against the current tree first
   and watch it fail — a guard test that has never failed proves nothing.
4. **`type-scale`** — every `font-size` value is one of the five `--t-*` tokens,
   except the five canvas-scale lines allowlisted in §4.3. No `em` anywhere.
5. **`radius-scale`** — every `border-radius` is `--r-sm`, `--r-md`,
   `--r-pill`, or `50%`. The three `50%` sites are circles, not corners, and
   are allowlisted: `DetailParts:55` (the round HUD buttons), `DetailParts:65`
   (the capacitor arc) and `LayoutView:1072` (the anchor dot).
6. **`space-scale`** — every `padding`, `gap` and `margin` value is a `--s*`
   token, `0`, `auto`, or a percentage/`fr`.
7. **`one-opacity`** — every `opacity` declaration is `var(--o-disabled)`, or
   on the ten-line graphics allowlist in §4.7.
8. **`mini-is-gone`** — `app/src/**` contains no `class="mini"` and no
   `.mini {` rule. The narrowest possible regression test for §1's bug.

### 7.2 The APCA floor — `app/src/lib/ui/apca.ts` + `apca.test.ts`

`apca-w3` would be a new dependency for a 25-line pure function. Write it:

```ts
// APCA 0.1.9 (W3 / 0.98G-4g). Returns |Lc| — polarity is not information here.
const [RC, GC, BC, TRC] = [0.2126729, 0.7151522, 0.0721750, 2.4];
const y = (hex: string): number => { … };            // sRGB -> Y, with the black clamp
export const lc = (text: string, bg: string): number => { … };
```

`apca.test.ts` asserts the §3.6 table: a `TOKENS` record of every text token and
every surface, and a `check()` per pairing that its Lc meets its declared floor.
Forty-one assertions, generated from two arrays. Changing a token's hex without
re-solving it then fails a named test rather than shipping.

**Do not add a WCAG 2 ratio check.** It passes the failures (§2.5: `--fg-dim`
scores 5.60 : 1 and is unreadable) and would give false confidence in exactly
the cases this phase exists to fix.

### 7.3 Component specs

One file per primitive with behaviour worth asserting. Chip, Panel,
PanelHeader and EmptyState have none beyond what §7.1 already checks — skip
them.

| Spec | Asserts |
| --- | --- |
| `Button.spec.ts` | a `ghost` button is visible without hover (`getComputedStyle(...).opacity !== "0"`) — the §1 regression; `disabled` blocks `onclick` and carries `title={disabledReason}`; `pressed` sets `aria-pressed`; `iconOnly` without `title` throws in dev |
| `Field.spec.ts` | `kind="select"` with grouped options renders `<optgroup>`; `bind:value` round-trips for text/number/checkbox; `error` renders the message, sets `aria-invalid` and wires `aria-describedby`; the generated `id` pairs `<label for>` |
| `SearchField.spec.ts` | the placeholder is built from `verb`/`nouns`/`shortcut`; the clear button is absent when empty and calls `onclear` when clicked; Escape clears |
| `Tabs.spec.ts` | `role="tab"` + `aria-selected` on every tab; Left/Right/Home/End move selection; a `disabled` tab has `aria-disabled="true"`, its `disabledReason` as `title`, and cannot be selected by click or key |
| `InlineMessage.spec.ts` | `warn` and `error` get `role="alert"`; `info`/`success` get `role="status"`; `dismissible` calls `ondismiss` |
| `ScopeBanner.spec.ts` | renders the label; renders **nothing** when the label is `""` (matching the four `{#if sharedLabel}` guards) |
| `ListRow.spec.ts` | with `onclick`, the label is a real `<button>` and is keyboard-activatable; the five drag handlers forward; `selected` sets `aria-selected` |
| `Popover.spec.ts` | clamps inside the viewport (port the `ContextMenu:48-55` case, which has no test today); Escape closes; an outside `pointerdown` closes; a `pointerdown` inside does not |
| `Sheet.spec.ts` | Escape closes; focus moves into the sheet on open and returns to the opener on close; Tab is trapped; `role="dialog"` + `aria-modal="true"` + `aria-label={title}` |
| `Toast.spec.ts` | auto-dismisses after `duration` (fake timers); `duration: 0` persists; `error` defaults to sticky; the region is `aria-live="polite"` |

### 7.4 Regression

- **All 35 existing frontend test files must pass untouched.** That is the
  acceptance criterion for "no behaviour change", and it is why §4's rule 1
  forbids renaming a class. If a spec needs editing, the change was not a
  refactor — stop and re-read.
- `detail.test.ts:484-485` pins `pointer-events: none` in
  `DetailParts.svelte`'s `<style>` and forbids `pointer-events: auto`. Both
  survive.
- `npm run check` (`svelte-check --fail-on-warnings`) must stay clean. Run it
  after `Tabs` specifically: the new `role="tab"` / roving-tabindex wiring is
  the one place in this phase that can introduce an a11y warning.
- The 40 Rust test modules are untouched — no Rust file changes in this phase.

---

## 8 Risks and rollback

**Rollback is one revert.** Every consumer references `var(--x)`, so reverting
the `:root` block restores the old palette exactly. The five steps in §6 are
independently revertable in reverse order.

| Risk | Likelihood | Mitigation |
| --- | --- | --- |
| A deleted CSS rule leaves its class on an element, silently losing styling. Svelte warns about *unused selectors*, never about *unstyled classes*. | High — this is the main failure mode | Delete the rule and the class in the same edit (§4 rule 2). Screenshot each of the 25 files before and after; the app is one window and the skill at `.claude/skills/starting-the-app/` exists for exactly this. |
| An existing spec queries a class the swap renamed | High if rule 1 is ignored | §4 rule 1, and §7.4's "all 35 pass untouched" gate. The 14 at-risk queries are listed in §4. |
| `--text-muted` at Lc 71 makes the UI look "flatter" — everything is bright now | Certain, and intended | The hierarchy is rebuilt from size/weight/position, not dimming (§3.1). If a screen reads flat after the swap, the fix is a `--t-*` or weight change, never a return to `opacity`. |
| `svelte-check` a11y warnings from the new ARIA wiring | Medium | Run `npm run check` after step 2, before any view is migrated. |
| The layout canvas stops reading as a screen once it shares the app's palette | Medium | §3.7 measured every composited label. The canvas keeps its own local `<style>` and its own 9–11px type (§4.3); only the hues are shared. |
| `LayoutView.svelte` is 1219 lines and last in the queue | Medium | It is last deliberately — by then every primitive has 20 other users and its API is settled. |
| Someone "helpfully" fixes a structural fault while in the file | Medium | §1. The tab strip, the Accounts dead end and the double Save modal are phases 2, 3 and 5. |
| A new hardcoded colour lands in the next feature | Certain over time | §7.1's guard tests. They are the durable half of this phase. |

**One thing genuinely ships unused:** `Sheet`'s `placement="end"` and `Tabs`'
`disabled`/`disabledReason`. Both are justified in §5.5 and §5.11 — they are
one prop each on a component that has real users today, and adding them later
means reshaping an API with 20+ call sites.

**Needs a decision:** whether `ChatSplit`'s scope legend (`ChatSplit.svelte:45`,
styled at `:88`, currently `--warn`) should move to `--info` as §5.9 proposes.

The argument for is stronger than "two panels disagree", and worth stating
precisely. **`--warn` carries both meanings inside this one 135-line file.**
`:88` colours the legend — *"Chat layout — account-wide"*, a statement of scope
— and `:119` colours `.area.bad`, a genuine warning that the computed split
leaves a negative area. Same token, one file, two meanings, and the reader has
no way to tell which is which.

The two panels also already disagree about how to mark account scope:
`HudPanel` badges its account-scoped rows **neutrally** (`.badge` is
`--fg-dim` on `--bg-panel`, `HudPanel.svelte:335-341`), not in amber. So amber
is not the established convention for scope — it is one file's local choice.
(An earlier draft of this section cited `HudPanel.svelte:57` as if it were a
style rule; it is a comment. The conclusion holds, but for this reason instead.)

The argument against: the whole point of the legend is that editing a chat split
changes every character on the account, and amber makes people look.

Recommendation: **`--info` for the legend, `--warn` stays on `.area.bad`.** That
leaves `--warn` meaning exactly one thing, and lets the account-scope signal be
unified with `ScopeBanner` (§5.9), which is the component that should own it.
One-line change either way.

---

## 9 Definition of done

- [ ] `app/src/app.css`'s `:root` is exactly the block in §3, with no
      compatibility aliases remaining.
- [ ] `grep -rE '#[0-9a-fA-F]{3,8}' app/src --include=*.svelte --include=*.css`
      returns only the `:root` block and the two allowlisted data sites
      (`OverviewAppearanceTab.svelte:26`, `ProbeViewer.svelte:150`).
- [ ] No `rgba(` or `rgb(` in any `<style>` block.
- [ ] `--line` and `--panel` are gone from `AccountsView.svelte`; every
      `var(--x)` in `app/src` resolves to a declaration in `:root`.
- [ ] Exactly 3 `border-radius` tokens (plus the 3 allowlisted `50%` circles)
      and 5 `font-size` tokens (plus the 7 allowlisted canvas-scale lines).
      No `em` font sizes anywhere.
- [ ] Every `padding`/`gap`/`margin` is a `--s*` token, `0`, `auto`, or
      relative.
- [ ] Exactly one `opacity` value in HTML chrome — `var(--o-disabled)` — plus
      the 10 allowlisted graphics uses.
- [ ] `.mini` is gone: no rule, no class, no `opacity: 0`. The four buttons at
      `AutofillView:110`, `AutofillView:129`, `KeybindsView:136` and
      `routes/+page.svelte:621` are visible.
- [ ] All 28 native-control style rules (§4.6) are deleted; `Field` is the only
      place that styles an `input`, `select` or `option`.
- [ ] The four `.shared-banner` copies, the four `border-color: #a33` copies,
      the three `.subtabs`/`.viewtabs`/`.tree-file` strips, the four `.badge`
      definitions and the three `.flash` timer pairs are each one component.
- [ ] `app/src/lib/ui/` holds the twelve primitives, `apca.ts`, and the ten
      specs in §7.3.
- [ ] `tokens.test.ts`'s eight guards pass — and each was demonstrated to
      **fail** against the pre-migration tree.
- [ ] `apca.test.ts`'s 41 pairings meet their floors. No WCAG 2 check exists.
- [ ] All 35 pre-existing frontend test files pass **without modification**.
- [ ] `npm test` and `npm run check` are clean.
- [ ] The app has been launched and every one of the eight views screenshotted
      against its pre-migration shot. Nothing moved.
- [ ] No feature was removed, no control was added, no dialog changed its words.

_Added 2026-08-13 (UI/UX redesign, Phase 1)._
