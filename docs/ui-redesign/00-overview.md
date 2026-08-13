# UI/UX redesign — implementation specs

These specs turn the redesign proposal into work that can be picked up and
executed without re-deriving anything. The proposal itself — the audit, the
reasoning, the mockups — lives at
<https://claude.ai/code/artifact/5ea152b7-bdb0-4c65-b087-386b4bd9ab4c>. This
directory is the *how*.

**Phases 1 and 2 are implemented** (`feat/ui-redesign-phase-1`,
`feat/ui-redesign-phase-2`); each spec carries a note listing where the shipped
result differs from the plan. Phases 3–5b are still plans.

**Current as of v0.34.0.** Every spec was re-reviewed against that release on
2026-08-13: counts re-measured, `file:line` citations re-anchored, and the
launcher-log account rework folded in. Each spec carries a "What v0.34 changed"
note. 0.34 fixed **none** of the seven pre-existing bugs below and added two of
its own (§"Pre-existing bugs"), so every phase's premise still holds.

## Why

The app is not badly designed; it is **undesigned**. Eight views were built one
at a time, each locally sensible, none sharing a vocabulary. What reads as
"haphazard, unaligned, inconsistent" has mechanical causes that can be counted:

| Measured across `app/src` | Found | Should be |
| --- | --- | --- |
| Distinct hex literals (36 of them outside `:root`) | 45 | 0 outside tokens |
| `rgba()` literals | 25 | 0 |
| CSS variables referenced but never defined | 2 (`--line`, `--panel`) | 0 |
| CSS classes used but never defined | 1 (`.empty`) | 0 |
| Hand-rolled "dark native control" style rules | 28, in 16 files | 1 primitive |
| Border radii | 8 | 3 |
| Font sizes (mixing `px` and `em`) | 10 | 5 |
| Padding values | 55 | 6 |
| Gap values | 23 | 6 |
| `opacity` declarations used as hierarchy | 35, at 10 values | 1, disabled only |
| Meanings carried by the class name `chip` | 5 | 1 |
| Blocking native dialogs | 73 | 7 |
| Class names for inline messages | 9 | 1 |
| Disabled-state treatments | 3 | 1 |
| Distinct dialog titles | 46 | ~10 |

Re-measured at v0.34.0. Most held exactly; the ones that moved all moved the
**wrong way** — `gap` 22→23, `opacity` 33→35 declarations across 9→10 distinct
values, `chip` meanings 4→5, and inline-message classes 7→**9** (0.34 added
`.conflict` and `.from-launcher`). That is the case for doing Phase 1 rather
than deferring it again: absent a shared vocabulary, every feature adds to the
pile, and 0.34 was a well-reviewed feature written by people who knew better.

Three root causes produce nearly all of it, and each phase below attacks a cause
rather than a symptom:

1. **There is a token system and the code routes around it.** `app.css` declares
   nine custom properties; the codebase then hardcodes 45 hex values, including
   four different reds, four ambers, three blues and five greys that all mean the
   same thing. `LayoutView`, `HudPanel`, `ChatSplit` and `DetailParts` run a
   second, Tailwind-derived palette, so the canvas half of the app is a different
   colour scheme from the shell around it.
2. **Hierarchy is built entirely out of dimming** — a `--fg-dim` token, then
   `#888`, then `#666`, then `opacity` at `.7`/`.6`/`.5`/`.4` stacked on top.
   Because size and weight never rank anything, dimming carries the whole load and
   gets pushed past legibility.
3. **Every view invented its own vocabulary** — nothing was shared, so nothing
   agrees. Each is a missing shared *thing*: a token, a scale, a component.

### The contrast finding that matters

This is a dark UI, and WCAG 2's ratio is known to give unreliable guidance for
dark mode — it ignores polarity, size and weight. Scored with APCA instead:

| Colour | Where | WCAG 2 | APCA Lc | Verdict |
| --- | --- | --- | --- | --- |
| `--fg-dim` `#8a919e` | 51 uses — all meta text, at ~12px | 5.60 : 1 | **42** | Below any text threshold |
| `#666` | `LayoutView` canvas hints | 2.88 : 1 | **21** | Effectively invisible |
| `--danger` `#e06c60` | Error text | 5.48 : 1 | **42** | Fails as text |
| `.kind-int` `#c8a1e8` | Tree values — the actual data | 8.22 : 1 | **59** | Under body threshold |
| `--fg` `#d5d9e0` | Body text | 12.54 : 1 | 82 | Passes |

APCA wants **Lc 75** for body text and **Lc 90** comfortably, with *more*
required as text gets smaller. The app's most-used text colour sits at half that,
and WCAG 2's comfortable-looking 5.6 : 1 is exactly why it was never caught.

**Do not use WCAG 2 ratios to check this app's palette.** Phase 1 carries an APCA
floor per token and a way to verify it.

## The five structural faults

These are bugs, not opinions. They are reachable in normal use and each costs
real work. Every one is fixed by a phase below.

1. **Accounts and Copy settings are dead ends** *(critical)* — `mainView` is only
   reset to `"file"` inside `openFile()`/`openPresetPair()`, and neither view has
   a close control, so the only way back is re-opening a file. The whole file bar
   sits in the `{:else}` branch, so entering either view **hides Save and both
   unsaved badges** while edits are pending. → Phases 2 and 3.
2. **One Save produces two blocking modals** — `saveFile()` calls `message()`
   inside its per-slot loop. → Phase 5.

   A related root cause turned up while specifying it: `errMessage()`
   (`app/src/lib/api.ts:550-553`) prefixes a bare `[code]` to every one of the 58
   error strings. Splitting it into `errText`/`errMessage` repairs all 58 in
   three lines, which is why the grammar problem looked merely cosmetic.
3. **The Backups panel silently changes subject** — the active slot is derived
   from the current view, so the docked column lists a different file's backups
   depending on which tab you are on. → Phase 2.
4. **The tab row rearranges under the cursor** — tabs render conditionally, so
   the strip changes width and membership as files load and accounts pair. →
   Phase 2.
5. **"This can't be undone" is false** — deleting an overview tab is an in-memory
   mutation that Discard reverses exactly, while the genuinely comparable "Delete
   empty stack frames" dialog correctly says the opposite. → Phase 5.

## Pre-existing bugs found while specifying

These are not redesign work. They are defects in the shipping app that surfaced
because several people read the code closely at once. Each is fixable on its
own, today, without any of the phases below — and each is specified in the phase
that found it.

**All seven were re-checked against v0.34.0 and all seven are still live.** The
files carrying them — `+page.svelte`, `BatchView.svelte`, `app.css`,
`BackupsPanel.svelte` — are byte-identical to v0.33.

**v0.34 added two more:**

- **A failed launcher-log read is reported as an empty one.**
  `AccountsView.svelte:184` is `.catch(() => {})`, and `.finally` still sets
  `proposalsLoaded`. So when the read *breaks*, `proposals` and `foundCards`
  stay empty, `everFound` is false, and the view states *"Your EVE launcher logs
  say nothing about these accounts"* — which is false. Broken and empty are
  indistinguishable to the user. → `05-dialogs-copy-palette.md`.
- **Which editor tab you are on decides what `Accept all` writes.** The
  pre-existing `active`-derived-from-`view` fault (bug 3 below) now reaches
  further than the Backups panel: `AccountsView`'s folder scope gates
  `onScreen`/`allPairs` (`AccountsView.svelte:81-84`), so `slots[active]`
  determines *which pairings a bulk accept commits*, not merely which cards are
  listed. A write whose scope depends on an unrelated tab selection.
  → `02-shell.md`.

- **`refreshToken` reaches only two of six views.** Autofill, Keybinds and Probes
  key their reload `$effect` on `userOpen`/`userId` alone, so they never see a
  token bump. **Discard and backup-restore therefore leave those three views
  showing stale data right now.** Undo would inherit the same gap. → `05b-undo.md`.
- **`targetsOpenFile` cannot see the open account file.** The batch view's
  "one target is the file open in the editor" warning tests only the char path;
  account writes exist solely in `plan.account_writes[].path`, and the char case
  is also missed whenever the active tab is user-scoped. The warning silently
  fails to fire in exactly the cases that matter most. → `03-sheets.md`.
- **`Ctrl+F` is a suppressed no-op on four of six tabs.** The window handler
  calls `preventDefault()` unconditionally, but `searchBox` only exists in the
  Raw branch, so on Overview, Autofill, Keybinds and Probes the shortcut kills
  the webview's own find and then does nothing. → `02-shell.md`.
- **`errMessage()` prefixes `[code]` to all 58 error strings**
  (`app/src/lib/api.ts:550-553`). → `05-dialogs-copy-palette.md`.
- **`try_edit_char` gained a third call site** (`tab_delete`, `ops.rs:525`, from
  PR #76) without anything enforcing the invariant it relies on. Nothing is
  broken today, but the hazard is no longer theoretical. → `05b-undo.md`.
- **`.empty` is a class nobody defines.** `KeybindsView.svelte:84` and `:90` style
  their two empty states with it, and no stylesheet in the repo declares it — both
  render unstyled today. → `01-tokens-and-primitives.md`.
- **Four `.mini` buttons are permanently invisible but still clickable**
  (`AutofillView:110,129`, `KeybindsView:136`, `+page.svelte:621`). `app.css` sets
  `.mini { opacity: 0 }`, revealed only by `.row:hover .mini`, and these four sit
  outside any `.row`. Already logged in `docs/small-tasks.md` as a repo-wide
  cascade trap; Phase 1 retires the pattern. → `01-tokens-and-primitives.md`.

## The phases

Ordered so the highest visible improvement comes first at the lowest risk.

| # | Spec | Changes behaviour? | Depends on |
| --- | --- | --- | --- |
| 1 | [Tokens and primitives](01-tokens-and-primitives.md) — **done** | No — pure refactor | — |
| 2 | [Shell and architecture](02-shell.md) — **done** | Layout only | 1 |
| 3 | [Sheets for Accounts and Copy settings](03-sheets.md) | Yes — fixes the critical fault | 1, 2 |
| 4 | [Overview consolidation and the inspector rule](04-overview-and-inspector.md) | Layout only | 1, 2 |
| 5 | [Dialogs, copy and the command palette](05-dialogs-copy-palette.md) | Yes | 1, 2 |
| 5b | [Undo](05b-undo.md) | Yes — **optional and separable** | 5 (for the toast) |

**If only one phase ships, ship Phase 1.** It is almost entirely find-and-replace,
touches no logic, and resolves the original complaint — the app looking haphazard
and unaligned — more completely than any other single change. Phases 2 and 3
together are the next best value: they retire the critical fault and two of the
three high ones.

**Phase 5b is genuinely optional.** `discardChanges()` already reloads both files
from disk and reverses every in-memory edit exactly, so the dialog diet in Phase 5
stands without undo. Undo upgrades Discard from an all-or-nothing escape to a
per-step one.

## Decisions taken, so they are not relitigated

- **No UI dependency is added.** `app/package.json` currently carries zero
  component libraries — only Tauri plugins. The twelve primitives are hand-rolled
  Svelte 5 components in `app/src/lib/ui/`. A component kit would be a large
  dependency to solve a problem that is one stylesheet and twelve small files.
- **The app stays dark-only.** A light theme is out of scope; it would double the
  palette work and nobody has asked for one. The token names are theme-neutral
  (`--surface`, `--text-muted`) so a light theme remains possible later.
- **No functionality is removed.** Controls move, get renamed, or become
  progressively disclosed — never deleted. Phases 4 and 5 each carry an explicit
  inventory table proving it for the views they touch.
- **Hiding is replaced by disabling-with-a-reason** wherever a control's absence
  would be confusing (the view tabs are the main case).
- **Right-click keeps working everywhere it works today**, but stops being the
  *only* route to anything — every such action also gets a visible `⋯`.
- **Accessibility is a floor, not a phase.** Every token carries an APCA target,
  every interactive element has a visible `:focus-visible`, and `npm run check`
  runs `svelte-check --fail-on-warnings`, so a11y warnings fail the build. Keep
  it that way.
- **`opacity` is retired as a hierarchy device**, reserved for disabled state at
  one value. Rank comes from size, weight and position.

## Decisions taken

All five open questions were settled by the repo owner on **2026-08-13**. Each
spec now reads as decided; the reasoning is recorded there so none of it gets
relitigated. Three of the five overturned the spec's own recommendation, and the
reasoning for the override is captured in each case.

| # | Question | Outcome | Spec |
| --- | --- | --- | --- |
| 1 | Sidebar: single-select profile, and group characters by account? | **Single-select profile: yes. Account grouping: no** — it breaks alphabetical sorting and makes a character harder to find. Flat alphabetical list within the selected profile, account carried as a per-row chip. *(Overrode the spec.)* | `02-shell.md` §5 |
| 2 | Should `ChatSplit`'s scope legend move from `--warn` to `--info`? | **Yes.** Owner had no preference; recommendation stood. `--warn` was carrying both meanings inside one file — the account-wide legend (`:88`) and a real negative-area warning (`:119`). | `01-tokens-and-primitives.md` §8 |
| 3 | Calibrate-capture `Cancel` leaves `AppState.capture` set, with no `clear_capture` command | **Fix it now**, rather than defer to `small-tasks.md`. *(Overrode the spec.)* | `03-sheets.md` §4 |
| 4 | Should `overview_copy_columns` undo in one press or two? | **One.** "You are undoing one action; it would feel wrong to have to Ctrl+Z multiple times to go back on a single action." This is now a governing principle: **one Tauri command = at most one undo entry.** *(Overrode the spec.)* | `05b-undo.md` §3 |
| 5 | Take the free atomicity in `apply_mutations`? | **Yes** — own commit, own test, and `ops.rs:206-208`'s doc comment rewritten in the same change. | `05b-undo.md` §13 |

### Settled during the v0.34 review

Two more, from specs disagreeing with each other. Both are recorded at the point
of the rule, not just here.

| Question | Outcome |
| --- | --- |
| What colour is a *proposed* pairing? | **`--info` on `--info-dim`**, not `--accent`. `--accent` is already the focus ring, `ListRow`'s selected state, the primary `Button` and the active tab; an accent chip carrying its own focus ring would be one colour saying two things. A dispute is `--warn`. `02-shell.md` §5.7.2 is the binding statement. |
| Does anything tell you proposals are waiting? | **Yes, one thing: a count on the `Accounts…` menu item** (`02-shell.md` §5.10.1) — computed when the menu opens, never at app start, and it counts without naming. The sidebar still shows no per-character proposal chip, because a proposed character *is* unpaired everywhere that matters. |

### Why account grouping was rejected, in full

It is worth recording because the proposal argued for it at some length. The
sidebar's primary job is **finding a character**, which happens constantly;
understanding *which characters share settings* matters at **edit** time, not at
browse time. Grouping paid for the second with the first.

Nothing is lost, because the sharing relationship is surfaced at both moments it
actually matters, and neither is the sidebar: the **save cluster's impact
disclosure** names the other characters an account write will change
(`02-shell.md`), and **`ScopeBanner`** says so inside every account-scoped view
(`01-tokens-and-primitives.md` §5.9).

The flat list also wins a case the grouped one buried: with the account as a
per-row chip, **a character with no chip is unpaired** — and an unpaired
character silently cannot receive account-scoped aspects in a batch copy.

## Conventions for these specs

Each spec is self-contained and ends with a **Definition of done** checklist.
Each carries a file-by-file change list, so an implementer can work down it.
Claims about current behaviour cite `path/file.ext:LINE`; where a spec says the
code does something, someone read the code.

**There are no open questions left.** The five that were are recorded above with
their outcomes, and each spec now reads as settled. Everything is decided with
the reasoning stated — overturn any of it, but the reasoning is there to argue
against rather than a blank.

## Working on these

- Frontend tests are vitest, beside the component (`*.spec.ts` / `*.test.ts`),
  using `@testing-library/svelte`. Baseline at v0.34.0: **37 frontend test
  files, 1064 tests passing**, and `npm run check` clean over 445 files.
  Phase 1's acceptance gate is that all 37 still pass **untouched**.
  Run `npm test` from `app/` (it does `svelte-kit sync && vitest run`).
- Rust tests are in-file `#[cfg(test)]` modules. Baseline: **40 modules** across
  `crates/` and `app/src-tauri/`.
- `npm run check` must stay clean — it is `svelte-check --fail-on-warnings`.
- Running the app: `cd app && npm run tauri dev`. Vite is pinned to port 1420
  with `strictPort`; if it is taken, cargo still launches a window pointed at
  whoever owns 1420. See `.claude/skills/starting-the-app/`.
- Per the repo's branch policy, a phase this size gets its own branch. Small
  corrective work can ride an existing one.

_Added 2026-08-13 (UI/UX redesign proposal)._
