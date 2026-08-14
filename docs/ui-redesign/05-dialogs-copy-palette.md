# Phase 5 — dialogs, copy and the command palette

Depends on Phase 1 (`01-tokens-and-primitives.md`) for `InlineMessage`, `Toast`,
`EmptyState`, `Button`, `SearchField`, `Popover` and `ListRow`, and on Phase 2
(`02-shell.md`) for the context bar and the History popover. It is the first
phase that changes behaviour on purpose, and it is the one that needs the most
care, because it is the only phase that can silently swallow an error.

Undo is **not** in this phase. It is Phase 5b, it is optional, and nothing here
waits on it — see §3.3.

### What v0.34 changed

Re-verified against v0.34.0 (launcher-log association, PR #79; overview tab
reorder, PR #77). Every line number below is re-anchored; the working-tree `†`
caveat that used to hang off `OverviewView.svelte` is gone, because those changes
are committed now.

- **The dialog audit is unchanged in substance.** A fresh raw grep still returns
  75 matches; the two false positives are still false positives, at the same
  lines; the real figure is still **73 blocking dialogs in 13 files** (§2.1).
  Nine of `OverviewView`'s fifteen call sites moved; none were added or removed.
  Re-counting turned up one off-by-one of this spec's own: the distinct dialog
  titles are **46**, not 45 (§2.2). v0.34 added none of them.
- **`AccountsView` makes no blocking dialogs at all** — 170 new lines, a whole
  proposals UI, zero `message`/`confirm`/`ask`. It is this phase's argument,
  already shipped and already tested. §2.1 and §3.1 point at it.
- **It also invented two more inline-message classes** (`.conflict`,
  `.from-launcher`) and a twenty-second `.hint`. Seven names for four jobs is now
  nine (§2.3) — the drift this phase exists to stop is still happening.
- **Eight new strings** join the rename table (§5.1), and R5 gains one
  clarification the launcher UI forced: *a count is not a name.*
- **The registry grows by one** — `accounts.acceptAll` — to 72.

---

## 1. Goal

Three problems, one phase, because they are the same problem seen from three
angles: **the app has no idea how loudly to talk.**

1. **Seventy-three blocking native dialogs**, in thirteen files, for everything
   from "you saved a file" to "this deletes a preset for ever". A modal that
   fires on success is a modal the user learns to dismiss without reading, and
   once that reflex is trained the five dialogs that *matter* are dismissed the
   same way. The fix is not fewer dialogs for their own sake; it is **friction
   proportional to consequence**, which then makes the survivors readable again.
2. **Forty-six distinct dialog titles, two incompatible error grammars, one
   action under three labels, and an ellipsis that means nothing.** Labels
   drifted exactly the way the colours did, for the same reason — nothing was
   shared, so nothing agreed.
3. **Phase 2 empties the sidebar toolbar.** Six global actions have to land
   somewhere. They land in an app menu *and* a command palette, in that order of
   importance: the palette is an accelerator over a complete menu, never a
   replacement for one.

Success looks like: **six modal confirmations survive**, every one of them
guarding something the backup chain cannot walk back within the same session;
every failure is reported next to the control that failed; every success is a
toast or nothing at all; and everything the toolbar used to hold is reachable by
mouse without knowing a shortcut exists.

Non-goals: no new backend commands, no change to what any command *does*, no
undo stack, no light theme, no removal of any feature.

---

## 2. Dialog audit

### 2.1 The counts, verified

The proposal's headline figure is 75 blocking dialogs — 58 `message()`, 13
`confirm()`, 4 `ask()`. A raw `\b(message|confirm|ask)\(` across `app/src` still
returns exactly 75, in **13** files (not the proposal's 15), and the same two
matches are still not dialogs:

| Call | Raw matches | Verified count |
| --- | --- | --- |
| `message()` | 58 | **58** |
| `confirm()` | 13 | **11** — see below |
| `ask()` | 4 | **4** |
| **Total** | **75** | **73** |

Two of the thirteen `confirm(` matches are not dialogs. Both survived v0.34 at
their original line numbers:

- `app/src/lib/NeocomButtons.svelte:50` is a *comment* — "The Tauri dialog, not
  the bare browser `confirm()` — titled and iconed like every other destructive
  prompt in this app". The real call is on line 54.
- `app/src/lib/ProbeFormationsView.svelte:417` is `await p.confirm(...)`, the
  picker's own callback prop (`app/src/lib/ProbeFormationsView.svelte:354`).
  That file does not import `confirm` at all
  (`app/src/lib/ProbeFormationsView.svelte:6`).

So the real number is **73 blocking dialogs in 13 files**. The proposal's 75/15
is the grep, not the behaviour; the difference does not change a single decision
below, but the disposition table has to be right, so it is stated.

`app/src/lib/Sidebar.svelte:2` imports only `open as openDialog` — an OS file
picker, which is not a message dialog and is not counted here or touched by this
phase. `AutofillView.spec.ts`, `NeocomButtons.spec.ts` and
`ProbeFormationsView.spec.ts` mock the module; they are tests, not call sites.

**v0.34 added none.** PR #77 shifted `OverviewView.svelte` by twenty lines and
nine of its fifteen call sites with it; the set is identical.
`AccountsView.svelte` grew by
170 lines — a whole launcher-proposals UI, a batch write, a partial-failure
report — and **makes no blocking dialogs at all.** Every failure lands in one
`error` string rendered as `<p class="error">` at
`app/src/lib/AccountsView.svelte:214`; every proposal is an inline `.conflict`
line or a ghost chip with its own buttons; the partial-failure report from a
batch accept is inline prose built by `rejectionText`
(`app/src/lib/AccountsView.svelte:95-97`), not a modal.

That is this phase's whole argument, already in the repo. It is also *tested* the
way §11 asks for: `app/src/lib/AccountsView.spec.ts` is 219 lines, covers accept,
dismiss, conflict routing, scoping and rejection, mocks no dialog module, and
asserts the failure by its text —
`screen.getByText(/Alpha could not join Main/i)`
(`app/src/lib/AccountsView.spec.ts:145`). Implementers doing the 48
`InlineMessage` conversions in §2.7 should read that file before the first one,
and test 13 in §11 should be written in its shape rather than invented.

One thing it gets wrong, and this phase inherits: the launcher-log read at
`app/src/lib/AccountsView.svelte:184` is `.catch(() => {})`. A failed read is
indistinguishable from "the logs say nothing", and the hint at `:218` then
asserts the latter. Inline is not the same as reported; §3.1's rule that the
message clears only when the operation *succeeds* is what stops this, and the
`catch` becomes an `InlineMessage` at the Accounts header like every other
failure.

### 2.2 Distinct dialog titles

`grep -o 'title: "[^"]*"'` across `app/src` returns 47 distinct literals, but
five of them are `HudPanel` *section headings*, not dialogs
(`app/src/lib/HudPanel.svelte:60,64,70,71,79`). Adding the titles passed as
arguments rather than literals — `"Preset not created"`, `"Rename failed"`,
`"Delete failed"` via `run(fn, title)` at
`app/src/lib/PresetGroup.svelte:37-47,69,117,125`; `"Export failed"` is passed at
`:105` but already counted as a literal — plus the one template literal at
`app/src/routes/+page.svelte:450` gives **46 distinct dialog titles**. The
proposal says 47.

(This spec said 45 on the strength of 46 literals. Re-counting at v0.34 gives 47,
and v0.34 introduced none of them — `AccountsView` passes no dialog title at all.
It was an off-by-one here, corrected: 46, not 45. Nothing downstream turns on it
except the arithmetic in §5.5.)

The two grammars are exactly as reported:

- **14 distinct `"<X> failed"` titles**: `Edit failed` (21 call sites),
  `Open failed`, `Import failed`, `Export failed`, `Save failed`,
  `Restore failed`, `Rebind failed`, `Discard failed`, `Copy failed`,
  `Clear all failed`, `Stack edit failed`, `HUD edit failed`,
  `Chat layout edit failed`, `Neocom edit failed`. Sixteen once
  `PresetGroup`'s argument-passed `Rename failed` and `Delete failed` are
  included.
- **5 `"Could not <verb> the <noun>"` strings**:
  `app/src/lib/ProbeFormationsView.svelte:171,276,308,325` plus
  `The clipboard could not be read` at
  `app/src/lib/ProbeFormationsView.svelte:340`.

One file — `ProbeFormationsView` — uses the second grammar exclusively, and no
other file uses it at all. It is not drift within a view; it is one view that was
written later, by someone applying a better rule to their own file only.

### 2.3 The nine inline-message classes

Verified by grepping `class="…"` in `app/src/**/*.svelte`:

| Class | Count | Where the style lives |
| --- | --- | --- |
| `hint` | 20 exact + 2 as `hint pair` = **22** | `app/src/app.css:64` |
| `error` | 8 | `app/src/app.css:65` |
| `muted` | 7 | `BatchView` local styles only |
| `field-error` | 4 | `app/src/app.css:70` |
| `flash` | 3 | `app/src/app.css:66-69` |
| `err` | 2 | `BatchView` local styles only |
| `empty` | 2 | `KeybindsView` local styles only |
| `from-launcher` | 2 | `AccountsView` local styles only — **new in v0.34** |
| `conflict` | 1 | `AccountsView` local styles only — **new in v0.34** |

Nine names for four jobs: *this is fine but quiet* (`hint`, `muted`,
`from-launcher`), *this failed* (`error`, `err`, `field-error`), *this worked,
briefly* (`flash`), *there is something to decide* (`conflict`), and *there is
nothing here* (`empty`, and most of the 22 `hint`s). All of them collapse into
two Phase 1 primitives — `InlineMessage` and `EmptyState` — plus `Toast`. §5.4
lists every call site.

It was seven when this spec was written. v0.34 added two more class names and one
more `hint` in a single feature, without anyone deciding to — which is the
finding, not an aside. Nothing shared, so nothing agreed; the rate is roughly one
new message class per feature and it will not stop on its own.

`.flash` is worth calling out: `app/src/app.css:66-69` gives it a 2-second
CSS `fade-out` animation, and two of its three users pair it with a `setTimeout`
that nulls the state (`app/src/lib/Sidebar.svelte:87,104`,
`app/src/lib/ProbeFormationsView.svelte:306`). The third —
`app/src/lib/AccountsView.svelte:215` — does not, so `captureNote` fades to
invisible and then stays in the accessibility tree, under `aria-live="polite"`,
until the next capture. That is a hand-rolled toast, invented three times, got
subtly wrong once. `Toast` is not a new idea in this codebase — it is the third
copy, promoted, with the timer in one place so there is nothing left to forget.

### 2.4 The rule

| Consequence | Becomes |
| --- | --- |
| In-memory edit, reversible by Discard (delete a tab, delete empty stack frames, clear a list, reset the neocom) | Do it; **toast**. The toast carries an Undo action once 5b lands; until then it is informational and Discard is the escape. |
| Recoverable failure (an edit the backend refused, an ESI lookup that failed) | **`InlineMessage`** at the control that failed |
| Success (saved, imported, exported, copied) | **One toast** |
| Writes disk, reversible from the backup chain (save, restore, batch copy) | **In-app confirm** naming the files and the affected characters; the result stays on screen |
| Irreversible within the session (delete a preset, overwrite a file changed on disk, discard unsaved changes, export a full preset) | **Keep a confirm** — this is what confirms are for |

### 2.5 Decision: the six survivors are in-app, not native

The proposal says "keep the confirm" without saying *which kind*. Deciding:
**all six survivors become one in-app `ConfirmDialog`; none stay native.**

The reasoning is reuse, not taste. The app already ships a focus-managed
overlay — `.overlay` / `.modal` at `app/src/app.css:101-104`, used by
`InsertForm` at `app/src/routes/+page.svelte:665-676` — and a second full
in-app modal in `FormationPicker` (`app/src/lib/FormationPicker.svelte:32`).
Keeping native dialogs for six cases would leave the app with *three* modal
mechanisms where one already suffices, which is precisely the fault this whole
redesign is retiring. `ConfirmDialog` is `Popover`'s sibling: overlay, title,
body, two `Button`s.

The one thing native gives free and in-app must be built deliberately:

- **Focus trap** while open, restoring focus to the invoking control on close.
- **`Esc` cancels**, `Enter` activates the *safe* button.
- **Initial focus lands on the safe button**, never the destructive one.
- `role="alertdialog"`, `aria-modal="true"`, `aria-labelledby` on the title.
- The destructive `Button` uses the `danger` variant and names the verb and the
  object ("Delete preset", not "OK").

`ConfirmDialog` returns `Promise<boolean>`, the same shape `confirm()`/`ask()`
return today, so every call site is a one-line import swap and nothing about the
surrounding control flow moves.

### 2.6 Decision: the six, and why these six

1. **Discard unsaved changes** — two call sites
   (`app/src/routes/+page.svelte:204,223`), one component, two bodies. Loses
   in-memory work with no backup, because nothing was written.
2. **Overwrite a file that changed on disk** (`app/src/routes/+page.svelte:435`)
   — the EVE client may have written it since load; overwriting discards
   whatever it wrote.
3. **Delete a library preset** (`app/src/lib/PresetGroup.svelte:121`) — removes
   a directory outside the backup chain.
4. **Restore a backup** (`app/src/lib/BackupsPanel.svelte:36`) — writes disk.
   Reversible (the current file is backed up first, which its copy already says
   correctly), so it is a confirm rather than a toast, per the rule's fourth row.
5. **Clear all remembered text** (`app/src/lib/AutofillView.svelte:69`) — this
   one is a judgement call, because by the letter of the rule it is an in-memory
   edit that Discard reverses. Keeping it anyway: its blast radius is *every*
   list in the file, most of which are not on screen at the moment of the click,
   and Discard is all-or-nothing — reversing it also throws away every other
   unsaved edit made in the same session. That asymmetry is not covered by the
   rule table, and it is the whole reason a confirm exists. Revisit once 5b
   ships a per-step undo; at that point this can become a toast like the rest.
6. **Export a full preset** (`app/src/lib/PresetGroup.svelte:99`) — a privacy
   disclosure, not a data risk. A `full` preset carries the account's autofill
   history: station names, searches and typed text
   (`app/src/lib/PresetGroup.svelte:100`). Once the file is shared it cannot be
   unshared. Nothing in the app can walk that back.

**Three dialogs that look like they belong on the list, and do not:**

- **Set up per-window tabs** (`app/src/lib/OverviewView.svelte:111`) — the
  dialog's own text is good, and it is *already on screen*. The `no-windows`
  band at `app/src/lib/OverviewView.svelte:413-422` explains the same thing one
  line above the button that opens the dialog. Move the dialog's one extra
  sentence ("The editor can't undo this — it can't remove the last overview
  window…") into that band and delete the dialog. This is a deletion, not a
  redesign.
- **Replace an existing preset** (`app/src/lib/PresetGroup.svelte:67`) — becomes
  an `InlineMessage` at the name field of the create form, with the submit button
  relabelled from `Save preset` to `Replace preset`. That is the proposal's own
  rule — *say what an action costs before it is taken, in the control's own
  words, not in a dialog after the click* — applied literally. The form is
  already open and the name field is already focused; the app knows the collision
  the moment the name is typed, so it can say so then.
- **The batch copy** (`BatchView`) — the rule's fourth row asks for an in-app
  confirm naming the files and the affected characters. `BatchView` already *is*
  one: `app/src/lib/BatchView.svelte:368,379` state "Will write N file(s) — each
  is backed up first", `:381` warns about resolution mismatch per target, `:384`
  names the collateral characters an account write also changes, and `:387` lists
  exclusions with reasons — all above the `Copy` button at `:394`. It needs
  copy edits (§5), not a dialog. Adding one would be a second confirmation for
  one action.

### 2.7 Complete call-site disposition table

All 73, re-anchored against v0.34.0. Nine `OverviewView` sites moved (PR #77);
nothing was added or removed anywhere. `AccountsView` contributes no rows —
that is not an omission, it is §2.1's point.

**Legend for *Becomes*:** `toast` · `inline` (an `InlineMessage` at the named
control) · `confirm` (the in-app `ConfirmDialog`) · `delete` (no replacement
surface — the information already exists on screen) · `EmptyState`.

#### `app/src/lib/AutofillView.svelte` — 3

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 45 | `message` | `errMessage(e)` / "Edit failed" | **inline** under the list whose `commit()` failed: "That list wasn't changed — {reason}" |
| 69 | `confirm` | "Clear ALL remembered text in this account file? Every autofill list will be emptied. A backup is taken on save." / "Clear all remembered text" | **confirm** (survivor 5), reworded — §5.3 |
| 75 | `message` | `errMessage(e)` / "Clear all failed" | **inline** at the `Clear all remembered text` button: "The lists weren't cleared — {reason}" |

#### `app/src/lib/BackupsPanel.svelte` — 2

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 36 | `ask` | "Replace the current file with this backup?\n\n{file_name}\n\nThe current file is backed up first, so this is reversible." / "Restore backup" | **confirm** (survivor 4), reworded to name the character rather than the backup filename — §5.3 |
| 45 | `message` | `errMessage(e)` / "Restore failed" | **inline** in the History popover, above the list: "That backup wasn't restored — {reason}" |

#### `app/src/lib/KeybindsView.svelte` — 1

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 66 | `message` | `errMessage(e)` / "Rebind failed" | **inline** in the row that failed, in the cell beside the chip (the `stolenFrom` notice at `:128-131` already proves a per-row message slot renders there): "That binding wasn't changed — {reason}" |

#### `app/src/lib/LayoutView.svelte` — 7

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 122 | `message` | `errMessage(e)` / "Layout unavailable" | **inline** replacing the canvas, as an `EmptyState` with `variant="error"`: "The window layout couldn't be read — {reason}". Today a dismissed modal leaves an empty canvas with no explanation. |
| 207 | `message` | `errMessage(e)` / "Edit failed" | **inline** in the layout status bar (Phase 2): "That window wasn't moved — {reason}" |
| 285 | `message` | `errMessage(e)` / "Stack edit failed" | **inline** in the window panel, above the stack list: "The stack wasn't changed — {reason}" |
| 299 | `confirm` | "Delete {n} empty stack frame{s}? Each is a leftover container whose windows were unstacked. EVE does not re-create them. The change is applied to the open file — save to write it to disk." / "Delete empty stack frames" | **toast** — the mutation is in-memory and Discard reverses it. The `WindowPanel` band at `:398-401` already explains what an empty frame is *before* the click. Toast: "Deleted {n} empty stack frame{s}. Save to write it to disk." |
| 315 | `message` | `errMessage(e)` / "HUD edit failed" | **inline** in `HudPanel`, under the row that failed: "That value wasn't changed — {reason}" |
| 326 | `message` | `errMessage(e)` / "Chat layout edit failed" | **inline** in `ChatSplit`, under the field that failed. Note `:329` already re-reads the real value, so the inline message is the only thing missing. |
| 347 | `message` | `errMessage(e)` / "Neocom edit failed" | **inline** in `NeocomButtons`, above the button list: "The neocom wasn't changed — {reason}" |

#### `app/src/lib/NeocomButtons.svelte` — 1

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 54 | `confirm` | "Reset the neocom to the client's original buttons?" / "Reset neocom" | **toast** — in-memory, Discard reverses it, and `:91` already puts the consequence on the button's own tooltip. Toast: "Neocom reset to the client's original buttons." |

#### `app/src/lib/OverviewAppearanceTab.svelte` — 1

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 58 | `message` | `errMessage(e)` / "Edit failed" | **inline** at the top of the Appearance sub-tab: "That appearance setting wasn't changed — {reason}" |

#### `app/src/lib/OverviewColumnsTab.svelte` — 4

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 14 | `message` | `errMessage(e)` / "Edit failed" | **inline** at the column row: "That column wasn't shown/hidden — {reason}" |
| 20 | `message` | `errMessage(e)` / "Edit failed" | **inline** at the width field: "That width wasn't stored — {reason}" |
| 32 | `message` | `errMessage(e)` / "Edit failed" | **inline** above the column list: "The columns weren't reordered — {reason}" |
| 85 | `message` | `errMessage(e)` / "Copy failed" | **inline** inside the copy panel, above its buttons, so the panel stays open with the ticks intact: "The columns weren't copied — {reason}". On success: **toast** "Columns copied to {n} tab{s}." (there is no success message today at all). |

#### `app/src/lib/OverviewFiltersTab.svelte` — 7

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 110 | `message` | `errMessage(e)` / "Edit failed" | **inline** above the group grid: "Those groups weren't changed — {reason}" |
| 134 | `message` | `errMessage(e)` / "Edit failed" | **inline** at the exception row: "That exception wasn't changed — {reason}" |
| 170 | `message` | `errMessage(e)` / "Edit failed" | **inline** at the rename field, which must stay open on failure: "The preset wasn't renamed — {reason}" |
| 175 | `message` | `errMessage(e)` / "Edit failed" | **inline** at the preset select: "The tab's preset wasn't changed — {reason}" |
| 190 | `message` | `errMessage(e)` / "Edit failed" | **inline** at the `Duplicate preset` button: "The preset wasn't duplicated — {reason}" |
| 199 | `confirm` | `Delete preset "{X}"? Tabs using it will move to "{Y}".` / "Delete preset" | **toast** — in-memory, Discard reverses. The toast is *more* informative than the dialog it replaces, because it can count: "Deleted “{X}”. {n} tab{s} now use “{Y}”." The dialog can only name the neighbour, not the count. |
| 205 | `message` | `errMessage(e)` / "Edit failed" | **inline** at the `Delete preset` button: "The preset wasn't deleted — {reason}" |

#### `app/src/lib/OverviewView.svelte` — 15

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 92 | `message` | `errMessage(e)` / "Edit failed" (`setNameFormat`) | **inline** at the colour swatch / bold toggle: "The tab name wasn't changed — {reason}" |
| 111 | `confirm` | "Put all {n} tab{s} in one overview window? …" / "Set up per-window tabs" | **delete** — fold the irreversibility sentence into the `no-windows` band at `:413-422`, which is already on screen next to the button. Then **toast**: "All {n} tabs are now in one overview window." |
| 125 | `message` | `errMessage(e)` / "Edit failed" | **inline** in the `no-windows` band: "The windows weren't set up — {reason}" |
| 167 | `message` | `errMessage(e)` / "Edit failed" (`submitPending`) | **inline** under the name-entry row, which stays open: "That wasn't saved — {reason}" |
| **171** | `confirm` | `Delete tab "{name}"? This can't be undone.` / "Delete tab" | **toast** — **the false claim, fixed.** See §2.8. Toast: "Deleted “{name}”. Save to write it to disk." |
| 182 | `message` | `errMessage(e)` / "Edit failed" (`deleteTab`) | **inline** at the tab actions row: "That tab wasn't deleted — {reason}" |
| 192 | `message` | `errMessage(e)` / "Edit failed" (`moveTab`) | **inline** at the *Move to window* select: "That tab wasn't moved — {reason}" |
| 196 | `confirm` | `Remove Overview {n}? Its tabs move to Overview 1.` / "Remove overview window" | **toast** — in-memory. Toast: "Removed Overview {n}. Its {k} tab{s} moved to Overview 1." Again the toast counts and the dialog cannot. |
| 208 | `message` | `errMessage(e)` / "Edit failed" (`removeWindow`) | **inline** at the tab actions row: "That window wasn't removed — {reason}" |
| 237 | `message` | `errMessage(e)` / "Edit failed" (`dropTab`) | **inline** above the tab chip strip: "The tabs weren't reordered — {reason}" |
| 276 | `confirm` | "This pack contains: {what}. Each of those replaces your account's current overview settings.{columnsNote}{ignored}" / "Import overview pack" | **confirm-shaped, but not a survivor** — this is a *preview* the user has not seen anywhere else, so it cannot become a toast. It stays a modal, but as the in-app `ConfirmDialog` and reworded per §5.3. Counted as a seventh modal surface, not a seventh *confirmation*: it is disclosure, and it appears only when a pack was picked. |
| 290 | `message` | "Pack imported. Save to write it to the account file.{warnings}" / "Import overview pack" | **toast**, with warnings as a second line in the same toast; if `warnings.length > 3` the toast says "{n} warnings" and links to History. |
| 292 | `message` | `errMessage(e)` / "Import failed" | **inline** at the `Import overview pack…` button: "The pack wasn't imported — {reason}" |
| 308 | `message` | "Exported {n} section(s).{warnings}" / "Export overview pack" | **toast**: "Exported {n} section{s} to {basename}." |
| 310 | `message` | `errMessage(e)` / "Export failed" | **inline** at the `Export overview pack…` button: "The pack wasn't exported — {reason}" |

#### `app/src/lib/prefs.svelte.ts` — 1

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 75 | `message` | `errMessage(e)` / "Preferences not saved" | **toast**, `variant="warning"`: "Your view preferences weren't saved — {reason}". `prefs` writes are fire-and-forget from a queue (`:70-78`) with no rollback by design (`:60-69`), so there is no single control to attach an inline message to. A toast is the honest surface: it is the one failure in the app with no owning control. |

#### `app/src/lib/PresetGroup.svelte` — 8

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 43 | `message` | `errMessage(e)` / `title` argument — "Preset not created" (`:69`), "Export failed" (`:105`), "Rename failed" (`:117`), "Delete failed" (`:125`) | **inline** in the Presets group header, one slot, four sentences: "The preset wasn't created / exported / renamed / deleted — {reason}". `run()` gains a `noun` parameter instead of a `title`. |
| 58 | `message` | `“{name}” is currently open — close it first, then save over it.` / "Preset is open" | **inline** at the create form's name field, live as the name is typed — the app knows this before submit (`:57`), so it should say so before submit. Submit stays disabled while it holds. |
| 67 | `confirm` | `Replace the existing preset “{name}”?` / "Preset exists" | **delete** → **inline** warning at the name field + submit relabelled `Replace preset`. See §2.6. |
| 84 | `message` | `Imported as “{result.name}”.` / "Imported" | **toast** |
| 86 | `message` | `errMessage(e)` / "Import failed" | **inline** at the `Import preset…` button |
| 99 | `confirm` | "This preset is a complete copy of both settings files. It carries everything the editor does not model, including your autofill history — station names, searches and typed text. Share it anyway?" / "Share a full preset?" | **confirm** (survivor 6), reworded — §5.3 |
| 121 | `confirm` | `Delete the preset “{name}”? This cannot be undone.` / "Delete preset" | **confirm** (survivor 3), reworded — §5.3 |
| 138 | `message` | `“{name}” is currently open — close it first to rename or delete it.` / "Preset is open" | **delete** — the menu label at `:149,152` already carries the reason ("Rename… (close first)"), so this dialog fires only when the user clicks a row that has already told them why it will not work. Make the row non-actionable instead: `ContextMenu` gains a `disabled` item state, and `explainOpen` goes away. The label change is in §5. |

#### `app/src/lib/ProbeFormationsView.svelte` — 11

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 171 | `message` | `errMessage(e)` / "Could not save the formation" | **inline** above the formation editor: "That formation wasn't saved — {reason}". The recovery at `:172-178` stays exactly as it is. |
| 276 | `message` | `errMessage(e)` / "Could not delete the formation" | **inline** at the `Delete formation` button |
| 308 | `message` | `errMessage(e)` / "Could not copy the formation" | **inline** in the list-actions row (where `flash` already renders, `:503`) |
| 320 | `message` | "That text contains no formations." / "Paste formations" | **toast**, `variant="warning"` — the clipboard is invisible, so there is no control to anchor to |
| 325 | `message` | `errMessage(e)` / "Could not paste the formation" | **inline** in the list-actions row |
| 339 | `message` | "Press Ctrl+V to paste a formation instead." / "The clipboard could not be read" | **inline** at the `Paste` button, with the accelerator rendered per platform (`⌘V` on macOS) rather than hardcoded — see §9.1 |
| 374 | `message` | `Exported {n} formation(s).` / "Export formations" | **toast**: "Exported {n} formation{s} to {basename}." |
| 389 | `message` | `errMessage(e)` / "Import failed" | **inline** at the `Import formations…` button |
| 393 | `message` | "That file contains no formations." / "Import formations" | **inline** at the `Import formations…` button, `variant="warning"` |
| 402 | `message` | `Imported {n} formation(s). Save to write them to the account file.` / "Import formations" | **toast** |
| 419 | `message` | `errMessage(e)` / `p.title` | **inline** in the list-actions row. `picker.title` stops being an error title and goes back to being only the picker's heading. |

#### `app/src/routes/+page.svelte` — 12

| Line | Kind | Says today | Becomes |
| --- | --- | --- | --- |
| 204 | `ask` | "You have unsaved changes to the {which} {noun}. Discard them and open another file?" / "Unsaved changes" | **confirm** (survivor 1a), reworded — §5.3 |
| 223 | `ask` | "Discard your unsaved changes and reload from disk? Both the character and the account file are reloaded, and your backups are untouched." / "Discard changes" | **confirm** (survivor 1b), reworded — §5.3 |
| 235 | `message` | `errMessage(e)` / "Discard failed" | **inline** in the context bar's save cluster: "Your changes weren't discarded — {reason}" |
| 278 | `message` | `errMessage(e)` / "Open failed" (`openFile`) | **inline** in the sidebar file list, above the rows: "{name} wasn't opened — {reason}" |
| 310 | `message` | `errMessage(e)` / "Open failed" (`openPresetPair`) | **inline** in the Presets group: "“{preset}” wasn't opened — {reason}" |
| 381 | `message` | `errMessage(e)` / "Open failed" (`loadCharacter`) | **inline** at the Overview character selector: "{name} wasn't opened — {reason}" |
| 397 | `message` | `errMessage(e)` / "Edit failed" (`runMutation`) | **inline** at the tree node that failed. `runMutation` already takes `rethrow` (`:387`) for callers with a better place to put the error — this makes the inline path the default and `rethrow` the special case. |
| 413 | `message` | `errMessage(e)` / "Edit failed" (`runMutations`) | **inline**, same slot |
| **431** | `message` | `Saved {bytes} bytes to {file_name}.\nBackup: {backup_path}` / "Saved" | **one toast for the whole save, outside the loop.** See §2.9. |
| 435 | `ask` | "{file_name} changed on disk after it was loaded (the EVE client may have written it). Overwrite anyway?…" / "File changed on disk" | **confirm** (survivor 2), reworded — §5.3 |
| 446 | `message` | `errMessage(e)` / "Save failed" | **inline** in the save cluster |
| 450 | `message` | `errMessage(e)` / `Save failed — {file_name} untouched` | **inline** in the save cluster, naming the character: "{name} wasn't saved — {reason}. Nothing was written." |

#### Totals

| Becomes | Sites |
| --- | --- |
| `InlineMessage` | 48 |
| `Toast` | 13 |
| `ConfirmDialog` (6 survivors + the pack-import preview) | 8 call sites |
| Deleted outright — information already on screen (`OverviewView:111`, `PresetGroup:67`, `PresetGroup:138`) | 3 |
| Blocking dialogs `AccountsView` makes | 0 — §2.1 |
| Full-pane `EmptyState variant="error"` (`LayoutView:122` — layout unavailable) | 1 |
| Unchanged file/save pickers (`openDialog` / `saveDialog`) | not counted — untouched |
| **Total** | **73** ⇒ **7 modal surfaces, 6 of them confirmations** |

The proposal's target was "roughly 75 down to about 6". The measured result is
73 down to 7 surfaces, 6 of which are confirmations.

### 2.8 The false "This can't be undone"

`app/src/lib/OverviewView.svelte:171`:

```
Delete tab "{name}"? This can't be undone.
```

It can. `api.tabDelete` mutates the in-memory document and the handler sets the
dirty flag (`app/src/lib/OverviewView.svelte:177` `onUserDirty()`, `:181`
`onCharDirty()`). `discardChanges()` at `app/src/routes/+page.svelte:219-237`
then re-opens both slots from disk — the doc comment at `:216-218` is explicit
that it is "a RE-READ, not a restore" and that "exactly the files that were open
are the files reopened". The delete is reversed exactly, up to the moment of
Save.

Meanwhile the genuinely comparable mutation, thirty lines of logic away, gets it
right. `app/src/lib/LayoutView.svelte:300-302`:

> "…The change is applied to the open file — save to write it to disk."

Two dialogs, opposite claims, identical mechanism. That is the worst possible
outcome: a user who reads both learns that the app's warnings are decoration.

**Both become toasts**, and both carry the same true sentence:

- `Deleted “{name}”. Save to write it to disk.`
- `Deleted {n} empty stack frame{s}. Save to write it to disk.`

Once 5b lands, both toasts grow an `Undo` action and the sentence stays true
either way.

There is one place in the app where "can't be undone" is *nearly* true and the
copy says so well — `app/src/lib/OverviewView.svelte:116-118`: "The editor can't
undo this — it can't remove the last overview window." That is a statement about
a missing inverse command, not about persistence, and it survives verbatim into
the `no-windows` band (§2.6).

### 2.9 One Save, one toast

`app/src/routes/+page.svelte:422-454`:

```
for (const slot of ["char", "user"] as const) {
  ...
  const report = await api.save(slot, force);
  ...
  await message(note, { title: "Saved", kind: "info" });   // :431 — inside the loop
}
```

Saving a character whose overview was edited marks **both** slots dirty — see
`app/src/lib/OverviewView.svelte:162-163` (`onUserDirty()` then `onCharDirty()`),
and `:177,181` for the delete path — so a single Save writes two files and stacks
two native modals, each naming a raw filename and a backup path, each needing its
own dismissal.

**Spec:**

1. Move reporting out of the loop. Collect `{ slot, name, report }` per
   successful write into a local array; emit one toast after the loop.
2. Toast text names **people, not files**: `Saved Baguette Commander and
   stormdelay2.` Single slot: `Saved Baguette Commander.` Names come from the
   already-derived `openCharName` (`app/src/routes/+page.svelte:101-107`) and
   `openUserAlias` (`:110-114`), falling back to `file_name` exactly as
   `openDisplay` (`:118-122`) already does.
3. **Byte counts and backup paths move to the History popover** (Phase 2). They
   are the two facts in the current message that no one reads at save time and
   everyone wants at restore time. `savedAt` already bumps on each write
   (`:429`), which is already what History refetches on
   (`app/src/lib/BackupsPanel.svelte:22-33`), so the plumbing exists.
4. Failures stay per-slot and per-control (rows 446/450 above), because a
   two-slot save can half-fail and the user needs to know *which* half.
5. The conflict `ask` at `:435` stays inside the loop: it is per-file by nature,
   and a two-file conflict is two genuinely separate decisions.

### 2.10 `errMessage` leaks a machine code into every error

`app/src/lib/api.ts:573-576` (v0.34 added 23 lines to `api.ts` — the launcher
types and two commands — all of them above this function, which is untouched;
only its line numbers moved):

```ts
export function errMessage(e: unknown): string {
  const err = e as ErrDto;
  return err && err.code ? `[${err.code}] ${err.message}` : String(e);
}
```

Every one of the 58 error dialogs therefore shows the user a bracketed code —
`[conflict] …`, `[io] …`. That is diagnostic text in a user-facing sentence, and
it is why the error grammar problem looked cosmetic: the *shape* of the message
was never the app's to control.

**Spec:** split it.

- `errText(e): string` — `err.message` only. This is what `InlineMessage` and
  `Toast` render.
- `errMessage(e)` keeps its current bracketed form and is used only where a
  human is *diagnosing*: the History popover's detail line, and the `title`
  attribute of the `InlineMessage` so the code is one hover away.

This is a three-line change in one file that fixes 58 strings at once, which is
why it is here and not in the rename table. It fixes a fifty-ninth for free:
`AccountsView`'s two inline error paths (`:119`, `:129`) both assign
`errMessage(e)` straight into the `error` string that renders at `:214`, so the
bracketed code already leaks into an inline message, not just into a dialog.
Moving inline does not fix this on its own — that is the point of doing the
split first (§10, step 2).

---

## 3. Message surfaces

Four surfaces. Everything the app says goes through exactly one of them, and the
choice is mechanical.

### 3.1 `InlineMessage` — the default

**Use when:** something the user just did failed, and there is a control that
owns the failure.

Rendered inside the view, adjacent to (usually directly below) the control that
was operated. It does not shift layout on appearance in a way that moves the
control itself — reserve the row, or render as an overlay band above the control
group, never below it in a way that pushes a button out from under the cursor.

Props (from Phase 1): `variant: "error" | "warning" | "info"`, `text`,
optional `detail` (goes to `title=`), optional `action`.

**`action` takes up to two `Button`s, not one.** v0.34 shipped the case: the
launcher-conflict line at `app/src/lib/AccountsView.svelte:290-298` is an inline
message with two mutually exclusive answers — `Move it` and `Keep mine` — and a
one-`Button` prop would force that one site back into bespoke markup, which is
how the ninth message class got written in the first place. Two is the ceiling:
a third answer is a form, not a message.

**The shipped model.** `app/src/lib/AccountsView.svelte` is what this section
describes, already merged. Read it before converting anything: one `error` string
per view, nulled at the top of every handler (`:100`, `:124`) — which is the
"one live message per owning control" rule, implemented; the failure rendered
adjacent, not modally (`:214`); a *partial* failure of a batch reported as prose
that names each casualty (`rejectionText`, `:95-97`). It gets three of the four
rules below right without having been told them.

Rules:

- **One live message per owning control.** A second failure replaces the first.
- **Dismissed by fixing it**, not by a close button: the message clears when the
  same operation next succeeds, when the control's value changes, or when the
  view unmounts. An error with a close button trains the same reflex the modals
  did.
- `role="alert"` on error, `role="status"` on warning/info.
- It never contains a stack trace or a bracketed code (§2.10).

### 3.2 `Toast` — the success surface

**Use when:** something succeeded and left no visible trace, or an in-memory
change happened that the user should be able to walk back.

The app has already invented this three times — `.flash` at `app/src/app.css:66-69`
plus `setTimeout` at `app/src/lib/Sidebar.svelte:87,104`,
`app/src/lib/ProbeFormationsView.svelte:306`, and the untimed state at
`app/src/lib/AccountsView.svelte:215`. Two of the three already carry
`aria-live="polite"`; the third has the `aria-live` and no timer (§2.3). This is
a promotion, not an invention.

Spec:

- One stack, bottom-right of the work area (not over the sidebar, not over the
  inspector), owned by a single `<ToastHost>` mounted once in
  `app/src/routes/+layout.svelte`.
- API: `toast(text, opts?)` where `opts` is
  `{ variant?: "success" | "warning", action?: { label, run }, timeout?: number }`.
- Default timeout **5s**; **8s** when an `action` is present, because a toast
  with a button you cannot reach in time is worse than no button.
- **Max 3 visible**; a fourth replaces the oldest. Never a scrolling stack.
- Hovering or focusing a toast pauses its timer.
- `aria-live="polite"`, `role="status"`. A toast is never the only place an
  error appears — errors are `InlineMessage`, with the two named exceptions
  (`prefs`, clipboard paste) where no control owns the failure.
- Dismissible by click and by `Esc` when focused, but **never required** — the
  information in a toast is always recoverable elsewhere (History, or the state
  itself).
- The three existing `flash` states and their `setTimeout` pairs are deleted and
  replaced by `toast()` calls. `.flash` leaves `app.css`.

**The Undo relationship.** `opts.action` is how 5b attaches Undo without
touching a single call site written in this phase: the toast call passes
`action: undoAction()` and, until 5b lands, `undoAction()` returns `undefined`.
Every in-memory toast in §2.7 is written that way from the start.

### 3.3 `EmptyState`

**Use when:** a view has nothing to show, whether that is normal (no formations
yet), blocked (no account paired), or broken (the layout could not be read).

Structure: a short heading (a noun phrase — what is missing), one sentence of
body (why, or what to do), and at most one `Button` (the thing to do). Variant
`error` for the broken case.

The 21 `hint`, 2 `empty` and several `muted` paragraphs listed in §2.3 are
almost all this. §5.4 maps every one.

The launch state is the important instance, because it is discovery rule 4 for
the palette:

> **No file open**
> Open a character from the list on the left, or press `Ctrl+K` to search
> characters, presets and commands.
> [ Open file… ]

That replaces `app/src/routes/+page.svelte:503` — `"Open a settings file to
begin."`

### 3.4 `ConfirmDialog`

Spec'd in §2.5. Six survivors plus the pack-import preview. It is the only
modal in the app after this phase apart from `InsertForm` and
`FormationPicker`, which are forms, not confirmations.

---

## 4. The copy standard

Seven rules. They are mechanical on purpose — the point is that a reviewer can
apply them without taste, the same way Phase 1's tokens can be checked without
taste.

### R1 — Sentence case everywhere

"Remove window", not "Remove Window". Proper nouns keep their capitals (EVE,
Overview 1, Ctrl, ESI, AU). Acronyms stay uppercase (HUD, UI, ESI).

The one carve-out: **single-letter axis and coordinate labels are symbols, not
words**, and stay as they are — `x`/`y`/`w`/`h` at
`app/src/lib/WindowPanel.svelte:146`, `app/src/lib/HudPanel.svelte:65-66,72-73,80-81`,
`X`/`Y`/`Z` at `app/src/lib/ProbeViewer.svelte:159-161`. Column headers in the
probe table are words and do get sentence case.

### R2 — An ellipsis means "this will not finish without more input"

`Import…` yes. `Delete` no — a confirmation is not more input, it is the same
action asking twice.

Two clarifications the rule needs, because the codebase contains both cases and
neither is covered by the one-line version:

- **Navigation takes no ellipsis.** Opening a place you inhabit and leave —
  `Accounts` — completes on click. Opening a *form whose only purpose is to
  finish one command* — `Copy settings…` — does not. That is why
  `app/src/lib/Sidebar.svelte:130` `Copy settings` gains an ellipsis and `:128`
  `Accounts` does not.
- **A progress label is not an action label**, and its ellipsis means
  "in progress". `Refreshing…` (`app/src/lib/Sidebar.svelte:126`), `Copying…`
  (`app/src/lib/BatchView.svelte:394`), `Loading layout…`
  (`app/src/lib/LayoutView.svelte:827`), `press a key…`
  (`app/src/lib/KeybindsView.svelte:125`) all keep theirs.

Always `…` (U+2026), never `...`. Verified: the codebase already has zero
three-dot ellipses.

### R3 — One verb per concept

- **Filter** narrows a list you can already see.
- **Search** looks through a document you cannot see all of.
- **Clear** empties a container that keeps existing. **Delete** removes the
  container. **Remove** takes one thing out of a container. **Reset** restores a
  value the app did not choose (EVE's default, the client's original).
- **All / None** for bulk selection, everywhere. Not `Select all`/`Clear`,
  not `Select all`/`Select none`, not `All`/`None` in one panel and something
  else in another — the codebase currently has all three
  (`app/src/lib/OverviewColumnsTab.svelte:113-114`,
  `app/src/lib/BatchView.svelte:336-337`,
  `app/src/lib/OverviewFiltersTab.svelte:261-264`,
  `app/src/lib/FormationPicker.svelte:51`).

Consequence: the Raw view's box is the only true **Search** in the app — the
tree is collapsed, so you cannot see what you are searching
(`app/src/routes/+page.svelte:616`). Every other box narrows a visible list and
becomes **Filter**, including the keybinds box at
`app/src/lib/KeybindsView.svelte:101`. The palette is the second **Search**,
because the command set is not on screen either.

### R4 — One error grammar

> **&lt;Thing&gt; wasn't &lt;verbed&gt;** — then what to do, or why.

Not "Edit failed". Not "Could not delete the formation". The subject is the
user's object, not the app's operation, because the user is not thinking about
operations.

```
That formation wasn't saved — a formation needs at least one probe.
The keybinding wasn't changed — EVE hasn't written a keybinding table for this
account yet. Open the in-game keybinding screen once, then reopen this file.
Baguette Commander wasn't saved — the file is read-only. Nothing was written.
```

The `{reason}` half is `errText(e)` (§2.10) — backend prose, unbracketed. The
app owns the first half; the backend owns the second. Where the backend's
sentence is already a full explanation, the app's half is still written, because
it is what names *which* thing failed when three controls can fail the same way.

**The one shipped example, and its one-word gap.** `rejectionText`
(`app/src/lib/AccountsView.svelte:95-97`) builds:

```
Alpha could not join Main — account already has 3 characters. Unpair one there
and try again.
```

Its own comment says why it is shaped that way: *"'Account already has 3
characters' does not say WHICH account, and the user has to know that to fix
it."* That is R4's second half — *then what to do* — and R5 — *name people, not
files* — arrived at independently, in the app layer, over a backend string
(`app/src-tauri/src/accounts.rs:72`) that owns only the reason. It is the model,
and §5.1 changes exactly one clause of it: `could not join` is the
`Could not <verb>` grammar this rule retires, and becomes `wasn't paired with`.
Everything else about it stays, including the sentence after the full stop, which
is the half most of the 48 new error strings will be tempted to omit.

### R5 — Name people, not files

`Baguette Commander`, not `core_char_95465499.dat`. `stormdelay2`, not
`core_user_13036531.dat`. The app already resolves both
(`app/src/routes/+page.svelte:101-114`) and already has the fallback chain
(`:118-122`); it just does not use it in most of the places that print a name.

Paths and filenames belong in exactly three places: a `title=` tooltip, the
History popover, and an OS file dialog. They never appear in a heading, a toast,
or an error sentence.

**A count is not a name either.** v0.34 forced this clarification and the owner's
live test found it immediately. `Accept all — {n} character{s}`
(`app/src/lib/AccountsView.svelte:194`) passes R1 and passes R5's letter — it
names no file — but it fails R5's purpose. Three named ghost chips are on screen;
the button that acts on exactly those three refuses to say which three, and the
one thing a bulk action must disclose is its blast radius. "3 characters" is also
the *scoped* count (`:82-84` filters to the cards on screen), so the number is
right and still says nothing.

> **Rule.** When a bulk action's objects are known, named and few, the label
> names them. Fall back to a count only above three, or when the names are
> unresolved. Never both.

So: `Accept Alpha, Bravo and Charlie` at n ≤ 3; `Accept all 5 characters` above
it; `Accept all — 3 characters` never. The same rule already governs
`WindowPanel:401` `Delete them` → `Delete empty frames` (§5.1) and
`BatchView:403` (§5.1) — an action label that needs the sentence above it to be
understood is not a label.

This rule is the *wording* half. The interaction half — whether a three-name
label at a fixed button width is the right control at all, versus naming them
under it — belongs to `03-sheets.md`, which owns the Accounts sheet. Whatever it
ships must satisfy the rule above. Note that
`app/src/lib/AccountsView.spec.ts:201` pins the current string exactly
(`/^accept all — 1 character$/i`), so that assertion is part of the change.

### R6 — Say what an action costs before it is taken, in the control's own words

Not in a dialog after the click. This is what retires the "Preset exists"
dialog (§2.6) and what keeps the Set-up-per-window-tabs explanation while
deleting its dialog.

### R7 — Never bake a shortcut into a string

`"Search labels and values (Ctrl+F)"` (`app/src/routes/+page.svelte:616`) is
wrong on macOS, where the app also ships. Shortcuts are rendered by the
component from a platform-aware `Accel` value — see §9.1. The same applies to
`"Copy this formation to the clipboard (Ctrl+C)"`
(`app/src/lib/ProbeFormationsView.svelte:497`), `"…(Ctrl+V)"` (`:498`),
`"Press Ctrl+V to paste a formation instead."` (`:339`) and
`"Clear search (Esc)"` (`app/src/routes/+page.svelte:621`).

### Spelling

The app is written in British English — `colour`
(`app/src/lib/OverviewView.svelte:369,372,382`), `licence`
(`app/src/lib/Sidebar.svelte:131`), `centre`
(`app/src/lib/ProbeViewer.svelte:634`), `Unrecognised`
(`app/src/lib/KeybindsView.svelte:118`). One outlier:
`"Unrecognized groups (not in the catalog)"` at
`app/src/lib/OverviewFiltersTab.svelte:244`. Fixed in §5.

---

## 5. Complete string rename table

Every user-facing string that changes. Strings that already comply are absent —
their absence is the statement that they were checked. Line numbers are as of
v0.34.0, committed; the old text is given in full so every site is findable
regardless.

Checked and left alone from v0.34: `Calibrate an account…` (R2 — a three-step
guided flow *is* more input), `Accept anyway` (matches §5.3's `Export anyway`),
the ghost chip's `Accept {name}` / `Dismiss {name}` tooltips (R5), and
`From your launcher log.` (R1). They are absent from the table because they
passed, which is what the table's absences mean.

### 5.1 Control labels, headings and placeholders

#### `app/src/lib/Sidebar.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 122 | `⟳` (icon-only, title "Rescan standard EVE locations") | `Rescan` — labelled, in the file-list header (Phase 2 moves it there) | discoverability |
| 126 | `Refresh names` | `Refresh character names` | R3 — "names" alone collides with nothing else, but the object matters |
| 126 | `Refreshing…` | `Refreshing character names…` | R2 (progress label keeps its ellipsis) |
| 130 | `Copy settings` | `Copy settings…` | R2 |
| 143 | `No EVE profiles found in standard locations. Use “Open file…”.` | `EmptyState` — heading `No EVE profiles found` / body `Nothing was found in the standard locations. Open a settings file directly instead.` / Button `Open file…` | §3.3 |
| 147 | `No character files with EVE's own names in these profiles. Untick “Hide non-standard files”, or use “Open file…”.` | `EmptyState` — heading `No character files here` / body `These profiles hold no files with EVE's own names.` / Buttons `Show non-standard files` + `Open file…` | §3.3 |
| 148 | `These profiles hold no character files. Use “Open file…” to open an account file directly.` | `EmptyState` — heading `No character files here` / body `These profiles hold only account files.` / Button `Open file…` | §3.3 |

#### `app/src/lib/AccountsView.svelte`

Eight of these thirteen rows are v0.34 strings, marked **new**. The file is the
one this phase holds up as the model (§2.1, §3.1); it earns that on structure,
not on wording, and the wording is where it drifted.

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 90 | `accountLabel` falls back to `core_user_{userId}` | fall back to `account {userId}` | **new** — R5. This is the one function that puts a filename-shaped token into two user-facing sentences (`:96`, `:292`). The `<input>` placeholder at `:237` keeps `core_user_{id}`: it is naming the file the alias replaces, which is the field's whole job. |
| 95-97 | `{name} could not join {account} — {reason}. Unpair one there and try again.` | `{name} wasn't paired with {account} — {reason}. Unpair one there and try again.` | **new** — R4. One clause; see the note under R4. Everything else about this string is the model. |
| 153 | `Paired {name} ↔ account {userId}.` | `Paired {name} with {alias}.` (→ toast) | R5 |
| 194 | `Accept all — {n} character{s}` | `Accept {a}, {b} and {c}` at n ≤ 3; `Accept all {n} characters` above it | **new** — R5, *a count is not a name*. `03-sheets.md` owns the control; this owns the rule. Breaks `AccountsView.spec.ts:201`. |
| 197 | `Refresh` | `Refresh accounts` | R3 — bare `Refresh` collides with `Refresh character names` |
| 219-220 | `Your EVE launcher logs say nothing about these accounts — use “Calibrate an account…” to pair a character by hand.` | `Your EVE launcher logs say nothing about these accounts. Calibrate an account to pair a character by hand.` | **new** — the em-dash clause is R4's *error* grammar on a non-error, and quoting a control label inside prose means the §5.1 rename has to be made twice. Name the action, don't quote the button. |
| 225 | `No accounts in this profile yet. Open a profile file, or run a calibration.` | `EmptyState` — heading `No accounts here yet` / body `Open a profile file, or calibrate an account to identify one.` / Button `Calibrate an account…` | R3 — "run a calibration" and "Calibrate an account…" are one concept in two verb forms, and v0.34 put them two lines apart |
| 246 | title `Unpair` | title `Unpair {name} from this account` | R5 |
| 269 | `＋ add character` | `Add a character` | R1, and drop the fullwidth `＋` |
| 286 | `Your launcher log also puts {name} here, but all three slots are full.` | `Your launcher log also puts {name} here, but this account is full.` | **new** — R3 reserves `all` for bulk selection (as at `ProbeFormationsView:516` and `ChatSplit:76`), and "three" hardcodes `MAX` (`:18`) into prose |
| 292 | `Your launcher log puts {name} on {account}.` | unchanged text; `{account}` gains R5's fallback via the `:90` fix | **new** — listed so the `:90` change is traceable to its two readers |
| 294 | `Move it` | `Move to {account}` — and delete the `aria-label="Move {name}"` at `:293`, which now duplicates the visible label | **new** — R3. "It" needs the sentence above it to parse, exactly like `WindowPanel:401`'s `Delete them`; and a visible label that disagrees with its accessible name is `WindowPanel:474-475`'s fault a second time |
| 296 | `Keep mine` | `Keep here` — and delete the `aria-label="Keep {name}"` at `:295` | **new** — R3, same two faults. "Mine" is not a word this app uses for anything; the answer the button gives is *which account holds this character*, so it says that |
| 305 | `Unassigned characters` | `Characters not on any account` | plain language |

#### `app/src/routes/+page.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 503 | `Open a settings file to begin.` | `EmptyState` — see §3.3, and it is where the palette is named once | discovery rule 4 |
| 507 | `{charName} — {alias} — {file_name}` | `{charName} · {alias}`, filename to `title=` | R5 |
| 510 | `read-only` | `Read-only` | R1 |
| 512 | `editable` | `Editable` | R1 |
| 515 | `preset: unsaved` | `Unsaved` | R1 (Phase 2 reshapes the cluster; this is the word only) |
| 517 | `character: unsaved` | `Unsaved — character` | R1 |
| 518 | `account: unsaved` | `Unsaved — account` | R1 |
| 524 | title `Throw the unsaved changes away and reload both files from disk. Backups are untouched.` | `Discard every unsaved change and reload both files from disk. Your backups are untouched.` | R1/plain |
| 529 | `Tree` | `Raw` | the tab is named for its widget; every other tab is named for its content |
| 607 | `Character file` | `{charName}` when resolved, else `Character`; filename to `title=` | R5 |
| 608 | `Account file` | `{alias}` when resolved, else `Account`; filename to `title=` | R5 |
| 616 | placeholder `Search labels and values (Ctrl+F)` | placeholder `Search labels and values`, shortcut rendered by `SearchField` | R7 |
| 621 | title `Clear search (Esc)` | title `Clear search`, shortcut rendered as `<kbd>` | R7 |
| 637 | `Nothing in this file matches “{query}”.` | `EmptyState` — heading `No matches` / body `Nothing in this file matches “{query}”.` | §3.3 |
| 642 | `Cannot edit: {message} (offset {offset})` | `EmptyState variant="error"` — heading `This file can't be edited` / body `{message} (byte {offset})` | R4 |
| 661 | title `Show backups` | title `Show history` | Phase 2 renames the panel |

#### `app/src/lib/BackupsPanel.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 52 | title/aria `Hide backups` | `Hide history` | Phase 2 |
| 54 | `Backups` | `History` | Phase 2 |
| 59 | `No backups yet. Every save creates one.` | `EmptyState` — heading `No history yet` / body `Every save leaves a restorable copy here.` | §3.3 |
| 66 | `restore` | `Restore` | R1 |

#### `app/src/lib/OverviewView.svelte`

PR #77 shifted this file's template by twenty lines; the strings are unchanged,
so every row below is the same finding at a new anchor.

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 326 | `Link this character to an account to edit shared settings — overview columns live in the account file.` | `EmptyState` — heading `No account paired` / body `Overview columns live in the account file.` / Button below | §3.3 |
| 327 | `Pair…` | `Pair this character…` | R3 — one label, four sites |
| 330 | `Open a character or account file to edit overview columns.` | `EmptyState` — heading `No file open` / body `Open a character or an account file to edit its overview.` | §3.3 |
| 336 | `This account file has no overview tabs.` | `EmptyState` — heading `No overview tabs` / body `This account file holds none. Importing an overview pack adds some.` | §3.3 |
| 365 | `+ New` | `New tab…` | R1, R2 (a name is required) |
| 366 | `Rename` | `Rename tab…` | R2, R3 |
| 367 | `Delete` | `Delete tab` | R2 (no ellipsis — it just happens now), R3 |
| 369 | title `Tab name colour` | unchanged | — |
| 407 | `+ Window` | `Add window…` | R1, R2, R3 |
| 410 | `Remove Window` | `Remove window` | **R1 — the headline casing bug** |
| 410 | title `Remove this (last) overview window` | `Remove the last overview window. Its tabs move to Overview 1.` | R6 |
| 420 | `Set up per-window tabs` | `Assign tabs to windows` | R3 ("set up" is not a verb this app uses anywhere else) |
| 426 | placeholder `First tab name` / `Tab name` | unchanged | — |
| 432 | `Add window` / `Rename` / `Add tab` | `Add window` / `Rename tab` / `Add tab` | R3 |
| 437 | `Character (for widths)` | `Column widths from` | R5 — "Character" here is EVE's word for the wrong thing; the field selects a *source of widths* |
| 439 | `Select…` | `Choose a character` | R2 — a `<select>` placeholder is not an action label |
| 468 | `No characters associated with this account yet — pair one in Accounts to edit widths.` | `No characters are paired with this account yet. Pair one to edit column widths.` | plain |
| 480 | `Import pack…` | `Import overview pack…` | R3 — "pack" alone is ambiguous next to Presets |
| 481 | `Export pack…` | `Export overview pack…` | R3 |

#### `app/src/lib/OverviewColumnsTab.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 106 | `Widths (no character open)` | `Widths — no character open` | R1 punctuation |
| 113 | `Select all` | `All` | R3 |
| 114 | `None` | unchanged | — |

#### `app/src/lib/OverviewFiltersTab.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 228 | `Rename preset` | `Rename preset…` | R2 (an inline field opens) |
| 237 | placeholder `Filter groups…` | placeholder `Filter groups`, shortcut rendered by `SearchField` | R2, R7 |
| 240 | `Types Shown` | `Types shown` | R1 |
| 244 | `Unrecognized groups (not in the catalog):` | `Unrecognised groups — not in the catalogue` | R1, spelling |

#### `app/src/lib/AutofillView.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 86 | `Link {charName ?? "this character"} to an account to edit shared settings.` | `{name}'s remembered text lives in the account file.` | R5, plain |
| 87 | `Pair…` | `Pair this character…` | R3 |
| 90 | `Open a character to edit its account's remembered text.` | `EmptyState` — heading `No file open` / body `Open a character to edit its account's remembered text.` | §3.3 |
| 95 | `No remembered text in this account file yet.` | `EmptyState` — heading `Nothing remembered yet` / body `EVE stores what you type into station, search and fitting boxes here.` | §3.3 |
| 99 | placeholder `Filter lists…` | placeholder `Filter lists`, shortcut rendered by `SearchField` | R2, R7 |
| 103 | `No lists match “{query}”.` | `EmptyState` — heading `No matches` | §3.3 |
| 110 | `Clear` | `Clear list` | R3 — bare `Clear` sat two rows from `Clear all remembered text` |
| 129 | title `Remove` | `Remove this entry` | R3 |
| 133 | placeholder `+ add remembered text…` | placeholder `Add remembered text` | R1, R2 |

#### `app/src/lib/KeybindsView.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 84 | `No account file open. [Pair this character…]` | `EmptyState` — heading `No account paired` / body `Keybindings live in the account file.` / Button `Pair this character…` | §3.3 |
| 90 | `This account has no keybinding table yet. EVE only writes one once you have opened the in-game keybinding screen at least once on this account.` | `EmptyState` — heading `No keybindings yet` / body kept verbatim / Button `Copy bindings from another account…` | §3.3 |
| 101 | placeholder `Search commands and keys` | placeholder `Filter keybindings`, shortcut rendered by `SearchField` | R3 — the table is on screen, so it is Filter; and "commands" now means palette commands |
| 98-100 | the comment explaining why there is no `(Ctrl+F)` hint | delete — `Ctrl+F` focuses this field after §9 | — |
| 138 | title `Reset to EVE's default (not yet captured)` — static, shown even when a default exists | enabled: `Reset to EVE's default ({keys})`; disabled: `EVE's default for this command hasn't been captured yet` | R6 — the tooltip currently lies on every row that has a default |

#### `app/src/lib/ProbeFormationsView.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 465-469 | `Probe formations live in the account file. [Pair this character with its account] to edit them.` | `EmptyState` — heading `No account paired` / body `Probe formations live in the account file.` / Button `Pair this character…` | §3.3, R3 |
| 488 | `New` | `New formation` | R3 |
| 489 | `Duplicate` | `Duplicate formation` | R3 |
| 490 | `Delete` | `Delete formation` | R3 |
| 497 | title `Copy this formation to the clipboard (Ctrl+C)` | `Copy this formation to the clipboard`, shortcut rendered as `<kbd>` | R7 |
| 498 | title `Add a formation from the clipboard (Ctrl+V)` | `Add a formation from the clipboard`, shortcut rendered as `<kbd>` | R7 |
| 500 | `Export…` | `Export formations…` | R3 |
| 501 | `Import…` | `Import formations…` | R3 |
| 516 | `Range (all probes)` | `Range (every probe)` | R3 — `all` is reserved for bulk selection |
| 529 | `probe positions in` | `Probe positions in` | R1 |
| 540 | `distance` / `azimuth` / `elevation` | `Distance` / `Azimuth` / `Elevation` | R1 |
| 541 | `range` | `Range` | R1 |
| 614 | `+ probe` | `Add probe` | R1, R3 |
| 630 | `This account has no custom probe formations yet.` | `EmptyState` — heading `No custom formations` / body `EVE's built-in formations aren't stored in this file. Create one to get started.` / Button `New formation` | §3.3 |

#### `app/src/lib/ProbeViewer.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 532 | aria `the formation in 3D` | `Formation in 3D` | R1 |
| 694 | `Vectors` | `Show vectors` | R3 — it is a toggle, so it names what it turns on |
| 697-698 | `drag to orbit · right-drag to pan · wheel to zoom · double-click a probe or the centre to orbit it, empty space to flip view` | `Drag to orbit · Right-drag to pan · Wheel to zoom · Double-click a probe or the centre to orbit it, or empty space to flip the view` | R1 |

#### `app/src/lib/WindowPanel.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 253, 267 | title `right-click for actions` | `Right-click for actions` | R1 |
| 362 | placeholder `Filter windows…` | placeholder `Filter windows`, shortcut rendered by `SearchField` | R2, R7 |
| 401 | `Delete them` | `Delete empty frames` | R3 — "them" needs the sentence above it to make sense |
| 477 | `unstack` | `Unstack` | R1 |
| 474-475 | title/aria `Remove from stack` | `Remove this window from the stack` | R3 — the visible label and the tooltip named the same action two ways |

#### `app/src/lib/LayoutView.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 931 | `· Shift-drag onto another window to stack · drag a tab to reorder or pull out` | `· Shift-drag onto another window to stack · Drag a tab to reorder or pull it out` | R1 |
| 940 | `reset` | `Reset filters` | R1, R3 |
| 946 | `clear` | `Clear overrides` | R1, R3 |

#### `app/src/lib/HudPanel.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 64 | `Fighter UI` | `Fighter panel` | R3 — `app/src/lib/BatchView.svelte:95` already calls it "fighter panel"; one thing, one name |

#### `app/src/lib/ChatSplit.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 65 | `history area {w} × {h}` | `History area {w} × {h}` | R1 |
| 76 | `Apply to all {n} channels in this stack` | `Apply to every channel in this stack ({n})` | R3 — `all` is reserved for bulk selection |

#### `app/src/lib/PresetGroup.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 17-22 | `Window layout` / `Overview` / `Autofill` / `Keybindings` / `Probe formations` / `Everything` | replaced by a shared `ASPECT_LABELS` constant using `BatchView`'s fuller labels (`app/src/lib/BatchView.svelte:95-100`) | R3 — two label sets for one six-item concept; this deletes one of them |
| 149 | `Rename…` / `Rename… (close first)` | `Rename…` / `Rename (close the preset first)` | R2 — the disabled variant does nothing, so it takes no ellipsis |
| 152 | `Delete…` / `Delete… (close first)` | `Delete` / `Delete (close the preset first)` | R2 |
| 164 | `New from open character…` | `New preset from this character…` | R3, R5 |
| 165 | `Import…` | `Import preset…` | R3 |
| 185 | `Save` | `Save preset`, or `Replace preset` when the name collides (§2.6) | R3, R6 |
| 192 | `No presets yet. Open a character and save one.` | `EmptyState` — heading `No presets yet` / body `Save the open character's settings as a preset to reuse them.` / Button `New preset from this character…` | §3.3 |

#### `app/src/lib/BatchView.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 336 | `Select all` | `All` | R3 |
| 337 | `Clear` | `None` | R3 — `Clear` here means "untick", which collides with `Clear list` and `Clear all remembered text` |
| 403 | `{✓/✗} {basename}` | `{✓/✗} {character or account name}`, path to `title=` | R5 |

#### `app/src/lib/FormationPicker.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 51 | `Select all` / `Select none` | `All` / `None` | R3 |

#### `app/src/lib/TreeNode.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 110 | title `double-click to edit` | `Double-click to edit` | R1 |
| 114 | title `inside a shared object: edits apply everywhere it is referenced` | `Inside a shared object — edits apply everywhere it is referenced` | R1 |
| 117 | title `add entry` | `Add entry…` | R1, R2 |
| 120 | title `remove entry` | `Remove entry` | R1 |
| 123 | title `show here in the full tree` | `Show this in the full tree` | R1 |

#### `app/src/lib/InsertForm.svelte`

| Line | Old | New | Rule |
| --- | --- | --- | --- |
| 96 | `key` | `Key` | R1 |
| 102 | placeholder `key` | drop (the label says it) | — |
| 107 | `index` | `Index` | R1 |
| 118 | `value` | `Value` | R1 |
| 132 | placeholder `value` | drop | — |

### 5.2 The four `Pair` sites — one label

| Site | Old | New |
| --- | --- | --- |
| `app/src/lib/AutofillView.svelte:87` | `Pair…` | `Pair this character…` |
| `app/src/lib/OverviewView.svelte:327` | `Pair…` | `Pair this character…` |
| `app/src/lib/KeybindsView.svelte:85` | `Pair this character…` | *unchanged* — this one was already right |
| `app/src/lib/ProbeFormationsView.svelte:467` | `Pair this character with its account` | `Pair this character…` |

The command registry entry is `accounts.pair`, label `Pair this character with an
account…` — the longer form is what the palette and app menu show, where there is
room and no surrounding sentence; the button form is what the four inline sites
show, where the `EmptyState` body has already said "with an account".

### 5.3 The six confirmations, reworded

All six use `ConfirmDialog`. Each names the object, states the consequence in one
sentence, and labels its destructive button with the verb — never `OK`.

**1a — Discard and open something else** (`app/src/routes/+page.svelte:204`)

> **Discard unsaved changes?**
> {Baguette Commander} and {stormdelay2} have edits that haven't been saved.
> Opening another file throws them away.
> `[ Keep editing ]` `[ Discard and open ]`

The current text — "You have unsaved changes to the character and account file"
(`:200-205`) — names slot roles, not people. `which`/`noun` are replaced by the
resolved names, falling back to the role words only when unresolved.

**1b — Discard and reload** (`app/src/routes/+page.svelte:223`)

> **Discard unsaved changes?**
> {Baguette Commander} and {stormdelay2} are reloaded from disk as they were at
> the last save. Your backups aren't touched.
> `[ Keep editing ]` `[ Discard changes ]`

**2 — Overwrite a file changed on disk** (`app/src/routes/+page.svelte:435`)

> **{Baguette Commander} changed on disk**
> The EVE client may have written it since you opened it. Saving replaces what's
> there now. A backup of the on-disk file is taken either way.
> `[ Cancel ]` `[ Overwrite ]`

**3 — Delete a preset** (`app/src/lib/PresetGroup.svelte:121`)

> **Delete “{name}”?**
> The preset's files are removed. This one isn't covered by the backup chain.
> `[ Cancel ]` `[ Delete preset ]`

"This cannot be undone" is replaced by the *reason* it cannot — which is the
one place in the app where that sentence is true, and it earns its keep by
saying why.

**4 — Restore a backup** (`app/src/lib/BackupsPanel.svelte:36`)

> **Restore {Baguette Commander} from {timestamp}?**
> The file on disk is replaced. It's backed up first, so this is reversible.
> `[ Cancel ]` `[ Restore ]`

The raw `b.file_name` moves to the dialog's detail line and to `title=`.

**5 — Clear all remembered text** (`app/src/lib/AutofillView.svelte:69`)

> **Clear every remembered list?**
> {n} lists in {stormdelay2} are emptied. Nothing is written until you save, and
> Discard puts them back — along with any other unsaved edits.
> `[ Cancel ]` `[ Clear everything ]`

The last clause is the honest version of the asymmetry that keeps this dialog
alive (§2.6).

**6 — Export a full preset** (`app/src/lib/PresetGroup.svelte:99`)

> **This preset carries your typing history**
> A full preset copies both settings files whole, including autofill —
> station names, searches and anything you've typed into a box.
> `[ Cancel ]` `[ Export anyway ]`

**7 (disclosure, not confirmation) — Import an overview pack**
(`app/src/lib/OverviewView.svelte:276`)

> **Import {basename}?**
> It replaces: {sections}.
> {Per-tab column widths are discarded.}
> {Ignored, not understood: {ignored}.}
> `[ Cancel ]` `[ Import pack ]`

### 5.4 Inline-message class migration

Every site from §2.3, and what it becomes. This is the table that lets the nine
classes be deleted from `app.css` and from four sets of local styles.

| Site | Class | Becomes |
| --- | --- | --- |
| `+page.svelte:503` | `hint` | `EmptyState` (launch) |
| `+page.svelte:637` | `hint` | `EmptyState` |
| `+page.svelte:642` | `error` | `EmptyState variant="error"` |
| `BackupsPanel.svelte:57` | `error` | `InlineMessage variant="error"` |
| `BackupsPanel.svelte:59` | `hint` | `EmptyState` |
| `AutofillView.svelte:85` | `hint pair` | `EmptyState` + Button |
| `AutofillView.svelte:90` | `hint` | `EmptyState` |
| `AutofillView.svelte:93` | `error` | `InlineMessage variant="error"` |
| `AutofillView.svelte:95` | `hint` | `EmptyState` |
| `AutofillView.svelte:103` | `hint` | `EmptyState` |
| `AccountsView.svelte:214` | `error` | `InlineMessage variant="error"` |
| `AccountsView.svelte:215` | `flash` | `toast()` (and the missing timer stops mattering — §2.3) |
| `AccountsView.svelte:218` | `hint` | `InlineMessage variant="info"` at the Accounts header — **new in v0.34**. Not an `EmptyState`: the cards below it are not empty, and this explains only why none of them carry ghosts. |
| `AccountsView.svelte:225` | `hint` | `EmptyState` |
| `AccountsView.svelte:282` | `from-launcher` | **new in v0.34** — not a message; it is provenance for the chips beside it, so it becomes the chip group's `meta` text, exactly like `BatchView:348` |
| `AccountsView.svelte:285` | `from-launcher` | `InlineMessage variant="info"` with one action (`Accept anyway`) — **new in v0.34** |
| `AccountsView.svelte:291` | `conflict` | `InlineMessage variant="warning"` with **two** actions (`Move to {account}`, `Keep here`) — **new in v0.34**, and the site that fixes `action`'s arity in §3.1 |
| `KeybindsView.svelte:84` | `empty` | `EmptyState` + Button |
| `KeybindsView.svelte:88` | `error` | `InlineMessage variant="error"` |
| `KeybindsView.svelte:90` | `empty` | `EmptyState` + Button |
| `InsertForm.svelte:104,115,135,140` | `field-error` | `InlineMessage variant="error"` (4 sites) |
| `InsertForm.svelte:137` | `hint` | `InlineMessage variant="info"` |
| `PresetGroup.svelte:183` | `hint` | `InlineMessage variant="info"` |
| `PresetGroup.svelte:192` | `hint` | `EmptyState` + Button |
| `Sidebar.svelte:140` | `flash` | `toast()` |
| `Sidebar.svelte:141` | `error` | `InlineMessage variant="error"` |
| `Sidebar.svelte:143,145` | `hint` | `EmptyState` (2 sites) |
| `OverviewView.svelte:325` | `hint pair` | `EmptyState` + Button |
| `OverviewView.svelte:330,336,468` | `hint` | `EmptyState` (3 sites) |
| `OverviewView.svelte:332` | `error` | `InlineMessage variant="error"` |
| `ProbeFormationsView.svelte:465` | `hint` | `EmptyState` + Button |
| `ProbeFormationsView.svelte:471` | `error` | `InlineMessage variant="error"` |
| `ProbeFormationsView.svelte:503` | `flash` | `toast()` |
| `ProbeFormationsView.svelte:626` | `hint` | `InlineMessage variant="warning"` |
| `ProbeFormationsView.svelte:630` | `hint` | `EmptyState` + Button |
| `BatchView.svelte:303,341` | `muted` | `EmptyState` (2 sites) |
| `BatchView.svelte:313,349,387,404` | `muted` | `InlineMessage variant="info"` (4 sites) |
| `BatchView.svelte:348` | `muted` | not a message — it is secondary row text; becomes `ListRow`'s `meta` slot |
| `BatchView.svelte:377,395` | `err` | `InlineMessage variant="error"` (2 sites) |
| `LayoutView.svelte:827` | `hint` | `EmptyState` (loading) |

Then delete from `app/src/app.css`: `.hint` (`:64`), `.error` (`:65`), `.flash`
(`:66-69`), `.field-error` (`:70`), and `@keyframes fade-out` (`:81`). Delete
`.muted`/`.err` from `BatchView`'s local styles, `.empty` from `KeybindsView`'s,
and `.conflict`/`.from-launcher` from `AccountsView`'s
(`app/src/lib/AccountsView.svelte:331-332`).

### 5.5 Count

| What changes | Strings |
| --- | --- |
| Labels, headings, placeholders and tooltips renamed (§5.1) | **127** — 119, plus 8 v0.34 strings |
| The `Pair` action, unified (§5.2) | **3** of 4 sites; the fourth was already right |
| Dialog bodies + titles collapsed into 7 confirmation surfaces (§5.3) | **8** bodies + **8** titles → 7 |
| Error sentences newly written at the failing control (§2.7) | **48** |
| Success/reversal sentences newly written as toasts (§2.7) | **13** |
| Inline-message class sites re-homed onto primitives (§5.4) | **51** — 47, plus 4 in `AccountsView` |
| **Distinct dialog titles remaining** | **0** — a `ConfirmDialog` title is part of its copy, not a reusable string. Forty-six go away; seven bespoke ones replace them. |

**198 user-facing strings change in total.** The largest single block is the 48
error sentences, which exist because the current app has *one* sentence
(`errMessage(e)`) for 58 different failures.

Eight of the 198 were written *after* this spec was, in v0.34. That is the cost
of not having the standard yet: one feature, by someone who got the *structure*
right without being told it (§3.1), still needed eight wording fixes and still
invented two class names. The standard is not for the careless.

---

## 6. The command registry

One module, `app/src/lib/commands.ts`, holding a plain array. No registration
API, no plugin system, no dynamic mutation — the command set is known at build
time, and a `const` array is the whole design.

```ts
export type Group =
  | "File" | "Go" | "Presets" | "Accounts"
  | "Layout" | "Overview" | "Autofill" | "Keybinds" | "Probes"
  | "Raw" | "History" | "Help";

/** Where a command is reachable WITHOUT the palette. Never empty — a test
 *  enforces it, which is discovery rule 1 made mechanical. */
export type Home =
  | { at: "app-menu" }
  | { at: "view-menu"; view: View }        // a view's ⋯ menu
  | { at: "control"; where: string }       // a visible button/toggle; `where` is prose
  | { at: "context-menu"; where: string }  // right-click, which always has a ⋯ twin
  | { at: "empty-state"; where: string };

export interface Command {
  id: string;                  // stable, dot-namespaced. Never rendered.
  label: string;               // sentence case, R1–R7 apply
  group: Group;
  keywords?: string[];         // extra fuzzy terms: synonyms, EVE's own words
  accel?: Accel;               // §9.1
  /** true, or the reason it is unavailable — rendered as the disabled tooltip
   *  in menus and as the greyed subtitle in the palette. One predicate, two
   *  consumers; Phase 2's disabled-with-a-reason tabs use the same shape. */
  enabled: (ctx: Ctx) => true | string;
  homes: Home[];               // length >= 1, enforced by test
  run: (ctx: Ctx) => void | Promise<void>;
}
```

`Ctx` is the already-existing app state, passed in rather than imported, so
`commands.ts` stays a pure module and is testable without mounting anything: the
open slots, dirty flags, current view, `layoutAvailable`, `openCharId`,
`openUserId`, and the handful of action callbacks `+page.svelte` already owns.

### 6.1 The registry — 72 commands

`Accel` column shows the Windows/Linux form; §9.1 covers the macOS rendering.
`Enabled when` is prose for the predicate; the string it returns when false is
the reason shown.

#### File — 8

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `file.open` | Open file… | `Ctrl+O` | always | app menu; launch `EmptyState` |
| `file.rescan` | Rescan profiles | — | always | sidebar file-list header |
| `file.refreshNames` | Refresh character names | — | profiles discovered | app menu; sidebar list header ⋯ |
| `file.hideNonStandard` | Hide non-standard files *(toggle)* | — | always | sidebar list header ⋯ |
| `file.save` | Save | `Ctrl+S` | `canSave` — else "Nothing has changed" / "This file is read-only" | context bar; app menu |
| `file.discard` | Discard changes | — | either slot dirty — else "Nothing has changed" | context bar; app menu |
| `file.history` | Show file history | `Ctrl+H` | a file is open | context bar History button; app menu |
| `file.about` | About EVE Settings Editor | — | always | app menu |

#### Go — 9

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `go.raw` | Go to Raw | `Ctrl+1` | a file is open | tab row |
| `go.layout` | Go to Layout | `Ctrl+2` | `layoutAvailable` — else "This file has no window layout" | tab row (disabled with reason, Phase 2) |
| `go.overview` | Go to Overview | `Ctrl+3` | char or account open | tab row |
| `go.autofill` | Go to Autofill | `Ctrl+4` | char or account open | tab row |
| `go.keybinds` | Go to Keybinds | `Ctrl+5` | char or account open | tab row |
| `go.probes` | Go to Probes | `Ctrl+6` | char or account open | tab row |
| `go.accounts` | Accounts | — | always | app menu |
| `go.copySettings` | Copy settings… | — | always | app menu |
| `go.rawFile` | Show the account file in Raw *(toggle char/account)* | — | both slots open | Raw view's segmented control (Phase 2) |

#### Search — 2

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `palette.open` | Search or run a command | `Ctrl+K` | always | the context bar entry control; app menu |
| `view.find` | Find in this view | `Ctrl+F` | the current view has a search field | every `SearchField`'s own `<kbd>` |

#### Presets — 6

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `preset.new` | New preset from this character… | — | char or account open — else "Open a character first" | Presets group ⋯ |
| `preset.import` | Import preset… | — | always | Presets group ⋯; app menu |
| `preset.rename` | Rename preset… | — | a preset is selected and not open — else "Close the preset first" | preset row ⋯ + right-click |
| `preset.export` | Export preset… | — | a preset is selected | preset row ⋯ + right-click |
| `preset.delete` | Delete preset | — | selected and not open — else "Close the preset first" | preset row ⋯ + right-click |
| `preset.open` | Open preset | — | a preset is selected | the preset row itself |

#### Accounts — 5

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `accounts.pair` | Pair this character with an account… | — | a character is open and unpaired | Accounts sheet; four inline `EmptyState` buttons (§5.2) |
| `accounts.unpair` | Unpair a character… | — | any pairing exists | Accounts sheet chip `✕` |
| `accounts.acceptAll` | Accept the launcher's pairings | — | at least one undisputed proposal is on screen — else "Your launcher logs propose nothing for these accounts" | Accounts sheet header button (`AccountsView.svelte:193`) |
| `accounts.calibrate` | Calibrate an account… | — | always | Accounts sheet header; app menu |
| `accounts.refresh` | Refresh accounts | — | always | Accounts sheet header |

`accounts.calibrate` and `accounts.refresh` were already here and v0.34 shipped
both with labels this spec had already chosen — `Calibrate an account…`
verbatim, `Refresh` pending §5.1's rename. Only `accounts.acceptAll` is new.

Its registry label names no characters, and R5's *a count is not a name* still
holds: that rule binds a label whose objects are **on screen beside it**. In the
palette they are not — the sheet may not even be open — so the registry takes the
general form and the sheet button takes the naming one, the same split `accounts.pair`
already makes (§5.2).

**What deliberately stays out**, because homes are not the only test — a command
also needs a subject the palette can name. The per-ghost `Accept {name}` /
`Dismiss {name}` chip buttons (`:254-259`), `Accept anyway` (`:287`) and the
conflict pair (`:293-296`) all act on *one proposal picked from a list*, with no
selection model behind them. `preset.rename` and `keybinds.reset` are in the
registry because "the selected preset" and "the focused row" are things `Ctx`
already knows; "the ghost you meant" is not. The registry's label is what the
palette shows, and `Accept Alpha` × 30 is not a command list. `accounts.acceptAll`
is the one that generalises, so it is the one that is here.

#### Layout — 11

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `layout.find` | Filter windows | `Ctrl+F` *(via `view.find`)* | on Layout | the filter field |
| `layout.openOnly` | Show only open windows *(toggle)* | — | on Layout | window panel toggle |
| `layout.hideClutter` | Hide clutter windows *(toggle)* | — | on Layout | window panel toggle |
| `layout.envAll` | Show windows in every environment | — | on Layout | environment segmented control |
| `layout.envDocked` | Show docked windows only | — | on Layout | same |
| `layout.envSpace` | Show in-space windows only | — | on Layout | same |
| `layout.resetFilters` | Reset window filters | — | a filter is active | status bar |
| `layout.clearOverrides` | Clear clutter overrides | — | overrides exist | status bar |
| `layout.detail` | Show window detail *(toggle)* | — | on Layout | status bar checkbox |
| `layout.deleteOrphans` | Delete empty stack frames | — | orphan frames exist — else "This file has none" | window panel band; Layout ⋯ |
| `layout.neocomReset` | Reset the neocom to the client's original | — | an original bar was recorded — else "This character has no original bar recorded" | neocom panel button |

#### Overview — 15

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `overview.newTab` | New tab… | — | the account has tabs | tab actions row |
| `overview.renameTab` | Rename tab… | `F2` | a tab is selected | tab actions row |
| `overview.deleteTab` | Delete tab | — | a tab is selected | tab actions row |
| `overview.tabColour` | Set tab name colour… | — | a tab is selected | swatch control |
| `overview.tabBold` | Bold tab name *(toggle)* | — | a tab is selected | `B` toggle |
| `overview.moveTab` | Move tab to another window… | — | more than one window | the *Move to window* select |
| `overview.addWindow` | Add overview window… | — | at least one window exists | tab actions row |
| `overview.removeWindow` | Remove overview window | — | more than one, and the last is selected | tab actions row |
| `overview.assignWindows` | Assign tabs to windows | — | the account has no window mapping | `no-windows` band |
| `overview.importPack` | Import overview pack… | — | an account is open | Overview ⋯ |
| `overview.exportPack` | Export overview pack… | — | an account is open | Overview ⋯ |
| `overview.copyColumns` | Copy columns to other tabs… | — | more than one tab | Columns sub-tab button |
| `overview.duplicatePreset` | Duplicate this filter preset | — | a tab is selected | Filters sub-tab |
| `overview.renamePreset` | Rename this filter preset… | — | the preset is user-made | Filters sub-tab |
| `overview.deletePreset` | Delete this filter preset | — | user-made and not the last | Filters sub-tab |

#### Autofill — 3

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `autofill.find` | Filter lists | `Ctrl+F` *(via `view.find`)* | on Autofill | the filter field |
| `autofill.clearList` | Clear this list | — | the focused list is non-empty | per-list button |
| `autofill.clearAll` | Clear all remembered text | — | any list is non-empty | Autofill toolbar |

#### Keybinds — 3

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `keybinds.find` | Filter keybindings | `Ctrl+F` *(via `view.find`)* | on Keybinds | the filter field |
| `keybinds.reset` | Reset this binding to EVE's default | — | a default was captured for the focused row | per-row `↺` |
| `keybinds.unbind` | Unbind this command | `Backspace` *(while capturing)* | a row is capturing | the capture bar's own hint |

#### Probes — 9

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `probes.new` | New formation | — | an account is open | list actions |
| `probes.duplicate` | Duplicate formation | — | one is selected | list actions |
| `probes.delete` | Delete formation | — | one is selected | list actions |
| `probes.copy` | Copy formation to the clipboard | `Ctrl+C` | one is selected | list actions |
| `probes.paste` | Add a formation from the clipboard | `Ctrl+V` | an account is open | list actions |
| `probes.export` | Export formations… | — | at least one exists | list actions |
| `probes.import` | Import formations… | — | an account is open | list actions |
| `probes.addProbe` | Add probe | — | fewer than 8 probes — else "A formation holds at most 8 probes" | editor button |
| `probes.units` | Show distances in km / in AU *(toggle)* | — | on Probes | AU/km segmented control |

#### Raw — 3

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `raw.find` | Search labels and values | `Ctrl+F` *(via `view.find`)* | on Raw | the search field |
| `raw.addEntry` | Add entry… | — | a container node is focused | the node's `+` |
| `raw.removeEntry` | Remove entry | — | an entry is focused and editable | the node's `×` |

#### Help — 2

| id | Label | Accel | Enabled when | Also appears |
| --- | --- | --- | --- | --- |
| `help.shortcuts` | Keyboard shortcuts | `Ctrl+/` | always | app menu |
| `help.repo` | Source and issues on GitHub | — | always | About sheet |

**Total: 72.** `help.shortcuts` is the visible home for the shortcuts that are
not commands (`Esc`, `Ctrl+Z`, the nudge arrows, `Enter` to commit) — it is one
static table rendered in a sheet, and it is what keeps §9's map honest.

### 6.2 What else is searchable

The palette searches four sources. Only the first is the registry.

| Source | Rows come from | Row shows | Enter does |
| --- | --- | --- | --- |
| **Commands** | the 72 above, filtered to `enabled(ctx) === true` first, then the disabled ones below a divider with their reason | label · group · accelerator | `run(ctx)` |
| **Characters** | `profiles` (`app/src/routes/+page.svelte:75`) × `names` (`app/src/lib/names.svelte`) | character name · account alias · profile label | `openFile(path)` |
| **Presets** | `allPresets()` (`app/src/lib/presetLibrary.svelte`) | preset name · `summarise(p)` | `openPresetPair(p)` |
| **Views** | the six tabs | view name | `go.<view>` |

Views are in the registry *and* here; that is deliberate duplication in the UI
layer, not in the data — typing "layout" should find the view whether you think
of it as a place or a command, and both rows resolve to the same `go.layout`.

Entities are **never** disabled-with-a-reason: a character you cannot open is a
character that is not in the list.

### 6.3 Fuzzy matching

Hand-rolled, ~40 lines, no dependency. A fuzzy library is not worth a dependency
for a candidate set that peaks around 72 commands plus a few dozen characters
and presets; at that size an O(n·m) subsequence scan is free.

```
score(query, candidate):
  fold both to lowercase; strip the query of spaces
  walk the query as a subsequence of `haystack` = label + " " + keywords + " " + group
  fail (score = -inf) if any query char is unmatched
  +  8 per matched char
  + 10 extra if the match is contiguous with the previous one
  + 12 extra if the match lands on a word boundary (start, or after a space/-/.)
  + 20 if the whole query is a prefix of the label
  -  1 per skipped char before the first match
  ×  0.6 if every match landed in `keywords`/`group` rather than in `label`
```

Notes:

- Matching against `group` is what makes typing `overv` surface every Overview
  command, which is the proposal's own worked example.
- `keywords` carries EVE's vocabulary for things the app renamed: `tabSetup`,
  `neocom`, `hangar`, `pack`, `preset`, plus the old label of anything renamed in
  §5, so muscle memory keeps working for a release or two. This is the cheapest
  possible mitigation for the rename risk in §12.
- Empty query = no scoring; show recent, then frequent, then registry order.
- Case-sensitivity: never. Diacritics: EVE character names can carry them, so
  fold with `String.prototype.normalize("NFD")` + strip combining marks before
  comparing.

### 6.4 Grouping and ordering

- **Empty query:** section `Recent` (up to 5, most recent first), then `Frequent`
  (up to 5, by count, excluding anything already in Recent), then every group in
  registry order with its commands. Characters and presets are not listed under
  an empty query — the list would be dominated by whichever install has the most
  characters, and the sidebar already lists them.
- **Non-empty query:** one flat ranked list, section headers inserted between
  runs of the same source (`Commands`, `Characters`, `Presets`). Sorting is by
  score; recency breaks ties, then registry order.
- **Disabled commands sort last**, always, below a divider, each showing its
  reason as the row's secondary text. They are shown rather than hidden for the
  same reason Phase 2 shows disabled tabs: a command that vanishes teaches
  nothing, and "Save — nothing has changed" is an answer.
- Group headers are the `Group` names verbatim; they are also matchable (§6.3).

### 6.5 Recent and frequent

Stored in the existing preferences store — `app/src/lib/prefs.svelte.ts` — not in
a new mechanism. It already persists through `api.setPreferences` with a write
queue (`:70-78`) and already tolerates a failed write without rollback by design
(`:60-69`), which is exactly the right durability for a MRU list.

```ts
prefs.palette = {
  recent: string[],                  // command ids, most recent first, capped at 8
  counts: Record<string, number>,    // id -> times run
}
```

- Only successful `run()`s are recorded. A command that threw is not "used".
- Entity rows (characters, presets) are **not** recorded: their ordering already
  comes from `filesort.svelte.ts` and recency there would fight the sidebar's
  own ordering.
- `counts` is never pruned — 72 keys is nothing — but an id no longer in the
  registry is ignored on read, so a removed command cannot resurrect.

---

## 7. The app menu

A single `☰` button at the left of the context bar (Phase 2), opening a
`Popover`. It is the complete, mouse-only route to everything global. Someone who
never learns `Ctrl+K` loses nothing — that is discovery rule 1, and this menu is
what makes it true.

```
☰
├─ Open file…                         Ctrl+O
├─ ─────────────────────────
├─ Save                               Ctrl+S
├─ Discard changes
├─ Show file history                  Ctrl+H
├─ ─────────────────────────
├─ Presets              ▸
│   ├─ New preset from this character…
│   └─ Import preset…
├─ Accounts
├─ Copy settings…
├─ ─────────────────────────
├─ Rescan profiles
├─ Refresh character names
├─ Calibrate an account…
├─ ─────────────────────────
├─ Search or run a command…           Ctrl+K
├─ Keyboard shortcuts                 Ctrl+/
└─ About EVE Settings Editor
```

Rules:

- **Every item shows its accelerator, right-aligned, rendered per platform**
  (§9.1). That is discovery rule 3: people learn the shortcut at the moment they
  use the slow path.
- **Items are disabled with a reason, never hidden.** `Save` when nothing has
  changed reads "Save — nothing has changed" on hover, greyed. The reason string
  comes from the same `enabled(ctx)` predicate the palette uses.
- Exactly one submenu (`Presets`), and only because two preset commands are
  global while the rest are per-row. Two levels is the ceiling.
- The menu is built by mapping over the registry filtered by
  `homes.some(h => h.at === "app-menu")` — the menu cannot drift from the
  registry because it *is* the registry.
- `Esc` closes; arrow keys move; `Enter` activates; focus returns to `☰`.
- The `Copy settings…` and `Accounts` items open sheets (Phase 3), so they close
  the menu and do not nest.

**View `⋯` menus** carry the per-view commands: everything in §6.1's Layout,
Overview, Autofill, Keybinds, Probes and Raw sections whose `homes` include
`{ at: "view-menu" }`. Same rules — accelerators shown, disabled with reasons.
Right-click menus that exist today (`WindowPanel:97-126`, `PresetGroup:141-155`)
keep working and gain a visible `⋯` twin built from the same list, which is the
`00-overview.md` decision "right-click stops being the *only* route to anything".

---

## 8. The command palette

`Ctrl+K` (`⌘K` on macOS) opens a centred overlay over everything, ~640px wide,
anchored a third of the way down. Input at the top, results below, max ~10 rows
visible with scroll.

### 8.1 The four discovery rules, and how each is enforced

**1 — Nothing is palette-only.** Every command carries `homes: Home[]` and a
vitest test asserts `homes.length >= 1` for all 72 (§11) — including v0.34's
`accounts.acceptAll`, whose home is the header button that already exists at
`app/src/lib/AccountsView.svelte:193`. This is the rule the
other three rest on: it means the palette can be missed entirely with zero cost,
which is the only honest basis for shipping one.

**2 — A visible, clickable entry point.** A bordered control in the context bar,
reading `Search or run a command` with the accelerator on its right, styled as a
`SearchField` rather than a bare `⌘K` hint. Clicking it opens the palette. It is
a control, not a caption, because discovery should cost a glance rather than a
guess.

**3 — Menus teach their own accelerators.** §7. Every app-menu and `⋯`-menu row
shows its shortcut, right-aligned, per platform.

**4 — The launch empty state names it once.**
`app/src/routes/+page.svelte:503` becomes the `EmptyState` in §3.3, whose body
is *"Open a character from the list on the left, or press `Ctrl+K` to search
characters, presets and commands."* One line, every first-run user, and it is
gone the moment a file opens.

### 8.2 Behaviour

| Key | Does |
| --- | --- |
| `Ctrl+K` / `⌘K` | Open. Pressing it again while open closes — a toggle, not a stack. |
| Typing | Filters live; the first enabled row is always selected |
| `↑` `↓` | Move selection; wraps at both ends |
| `Home` `End` | First / last row |
| `Enter` | Run the selected row |
| `Esc` | Close, restore focus to the invoking element |
| Click outside | Close |
| `Tab` | Nothing — the palette is a single-stop focus trap |

- Opening while text was selected in a field does not steal that selection; the
  palette's input starts empty every time. **Not** pre-filled from the last
  query: the previous query is almost never the next one, and a pre-filled box
  costs a `Ctrl+A` on every open.
- Running a command **closes the palette first, then runs**, in that order —
  the same ordering `ProbeFormationsView.runPicker` already documents at
  `app/src/lib/ProbeFormationsView.svelte:410-421`, and for the same reason: a
  dialog raised by the action would otherwise stack on a modal that is no longer
  reachable.
- A command that opens a `ConfirmDialog` or an OS picker therefore behaves
  identically whether it was run from the palette, the menu or a button.
- `aria-modal="true"`, `role="dialog"`, the input is `role="combobox"` with
  `aria-expanded`, the list is `role="listbox"` and rows are `role="option"` with
  `aria-selected`. Row count is announced via `aria-live="polite"` on a visually
  hidden node, debounced 200ms so it does not read on every keystroke.
- The palette does **not** register a global handler of its own: `Ctrl+K` is one
  entry in the single keyboard map in §9.

### 8.3 Row shape

Built from `ListRow`:

```
[group chip]  Label                                  Ctrl+K
              secondary text (reason, or preset summary, or account alias)
```

- **Command row:** label, group chip, accelerator right-aligned. Disabled rows
  are greyed with the `enabled()` reason as secondary text.
- **Character row:** character name, `Character` chip, account alias as secondary
  text, profile label after it when more than one profile has that character.
- **Preset row:** preset name, `Preset` chip, `summarise(p)` as secondary text.
- Matched characters are marked with `<mark>` — the same span the tree search
  already produces, so the highlighting rule is not invented twice.

---

## 9. The keyboard map

### 9.1 Platform-aware accelerators

The app ships on Windows, Linux and macOS. Today `Ctrl` is written into three
user-facing strings (`app/src/routes/+page.svelte:616,621`,
`app/src/lib/ProbeFormationsView.svelte:339,497,498`), all of which are wrong on
macOS.

```ts
export interface Accel { key: string; mod?: "primary" | "shift" | "alt"; }
export const IS_MAC = /mac/i.test(navigator.platform ?? navigator.userAgent);
export const accelLabel = (a: Accel): string => ...   // "Ctrl+K" | "⌘K"
```

- `primary` renders `Ctrl` on Windows/Linux, `⌘` on macOS, and matches
  `e.ctrlKey || e.metaKey` — which is exactly what
  `app/src/routes/+page.svelte:462,469` already do, so the *matching* half is
  already right and only the *rendering* half is missing.
- Every rendered shortcut goes through `accelLabel`. **No user-facing string
  contains the literal `Ctrl`** after this phase; a test asserts it (§11).
- On macOS `Ctrl+F` is a text-navigation binding in some contexts; `⌘F` is
  correct there and is what `primary` gives.

### 9.2 One handler, one map

Today the global handler lives inline in `svelte:window` at
`app/src/routes/+page.svelte:459-479`, and a second one lives in
`app/src/lib/ProbeFormationsView.svelte:462`, and a third in
`app/src/lib/LayoutView.svelte:824`. The three do not know about each other,
which is how `Ctrl+F` came to mean two different things
(`app/src/routes/+page.svelte:469-476`).

**Spec:** one `useKeymap()` in `app/src/lib/keymap.ts`, mounted once, dispatching
through the registry. View-local handlers keep only what is genuinely local and
positional — the layout nudge arrows
(`app/src/lib/LayoutView.svelte:781-819`), the keybind capture
(`app/src/lib/KeybindsView.svelte:72-80`), and inline field `Enter`/`Escape`.
The rest becomes registry lookups.

### 9.3 The map

**Global — identical in every view**

| Keys | Command | Notes |
| --- | --- | --- |
| `Ctrl+K` | `palette.open` | toggle |
| `Ctrl+O` | `file.open` | new |
| `Ctrl+S` | `file.save` | exists (`+page.svelte:462`) |
| `Ctrl+F` | `view.find` | **behaviour change — see below** |
| `Ctrl+H` | `file.history` | new |
| `Ctrl+/` | `help.shortcuts` | new |
| `Ctrl+1`…`Ctrl+6` | `go.raw` … `go.probes` | new; disabled views are a no-op, not an error |
| `Ctrl+Z` | undo | **reserved by this phase, implemented in 5b.** Until then it is unbound and falls through to the webview's native field undo, which is the current behaviour. It is listed in `help.shortcuts` only once 5b lands. |
| `Esc` | close the topmost dismissible thing | ordered: palette → `ConfirmDialog` → sheet → popover/menu → clear the focused search field → cancel an inline name entry. Exactly one layer per press. |

**`Ctrl+F` — the behaviour change**

Today: `app/src/routes/+page.svelte:469-476` focuses the tree search box, except
on Layout where it calls `layoutFocusFilter?.()` — a callback bound down two
component levels (`:96`, `:556`) purely to make that one exception work. On
Overview, Autofill, Keybinds and Probes it focuses the *tree* box, which is not
rendered, so it silently does nothing. Five views have a search or filter field
and `Ctrl+F` reaches two of them (Raw, Layout).

Re-verified at v0.34: unchanged, at the same lines. The count of divergent boxes
is still **five** — `+page.svelte:616` (the only one that advertises its
shortcut, by hardcoding `Ctrl` into the placeholder), `WindowPanel:362`,
`OverviewFiltersTab:237`, `AutofillView:99`, `KeybindsView:101`. Three of the five
put a trailing `…` on a filter placeholder and two do not. `AccountsView` added no
sixth: its only `<input>` is the alias field (`:234-239`), which edits rather than
narrows, so `Ctrl+F` on Accounts is a no-op like Probes.

After: **`Ctrl+F` always focuses the current view's search field.** Each view
registers its field with the shell; the shell focuses and selects whatever the
current view registered, or does nothing when the view has none (Probes). The
`layoutFocusFilter` prop chain (`app/src/routes/+page.svelte:96,556`,
`app/src/lib/LayoutView.svelte`, `app/src/lib/WindowPanel.svelte`) is deleted —
the general mechanism replaces the special case, which is a smaller diff than the
one it removes.

Every field is a `SearchField`, and **every `SearchField` renders its own
accelerator** as a trailing `<kbd>`, so the five boxes stop advertising
inconsistently (today only `app/src/routes/+page.svelte:616` does, and it does it
by hardcoding `Ctrl` into the placeholder):

| View | Field | Placeholder after §5 |
| --- | --- | --- |
| Raw | `app/src/routes/+page.svelte:612-616` | `Search labels and values` |
| Layout | `app/src/lib/WindowPanel.svelte:362` | `Filter windows` |
| Overview → Filters | `app/src/lib/OverviewFiltersTab.svelte:237` | `Filter groups` |
| Autofill | `app/src/lib/AutofillView.svelte:99` | `Filter lists` |
| Keybinds | `app/src/lib/KeybindsView.svelte:101` | `Filter keybindings` |
| Probes | — | *(no list to narrow; `Ctrl+F` is a no-op)* |

**Contextual — only where the context is live**

| Keys | Does | Where |
| --- | --- | --- |
| `Ctrl+C` | `probes.copy` | Probes, when nothing is selected and focus is not in a field (`app/src/lib/ProbeFormationsView.svelte:440-447` — the existing guards are correct and stay) |
| `Ctrl+V` | `probes.paste` | Probes, same guards (`:450-457`) |
| `←↑→↓` | nudge the selected window | Layout, unless a field swallows arrows (`app/src/lib/LayoutView.svelte:781-801`) |
| `Shift`/`Ctrl`/`Alt` + arrows | nudge by a different step | Layout (`:783-785`) |
| `F2` | `overview.renameTab` | Overview, when a tab is selected |
| any key | capture a binding | Keybinds, while a chip is listening (`app/src/lib/KeybindsView.svelte:72-80`) |
| `Backspace` | unbind | Keybinds, while listening (`:76`) |
| `Esc` | stop listening | Keybinds (`:75`) — folds into the global `Esc` ladder |
| `Enter` | commit an inline name entry | Overview (`:428`), Overview→Filters (`:302`), Presets (`:199`), Autofill (`:134`), Accounts (`:239`), Raw (`TreeNode:100`) |
| `Esc` | cancel an inline name entry | same six sites — the last rung of the `Esc` ladder |

**Reserved, never bound:** `Ctrl+W`, `Ctrl+Q`, `Ctrl+N`, `Ctrl+P`, `F5`,
`Ctrl+R`. The webview owns some of these and the OS owns the rest; binding them
produces a shortcut that works on one platform.

---

## 10. File-by-file change list

Ordered so each step leaves the app working.

### New files

| File | What |
| --- | --- |
| `app/src/lib/ui/Toast.svelte`, `ToastHost.svelte` | Phase 1 primitive + the single host (mounted in `+layout.svelte`) |
| `app/src/lib/ui/ConfirmDialog.svelte` | §2.5 — overlay, focus trap, `Promise<boolean>` |
| `app/src/lib/toast.svelte.ts` | `toast(text, opts)` and the queue state |
| `app/src/lib/commands.ts` | the 72-command registry, `Command`/`Home`/`Group` types |
| `app/src/lib/accel.ts` | `Accel`, `IS_MAC`, `accelLabel` |
| `app/src/lib/keymap.ts` | the single global handler, §9.2 |
| `app/src/lib/CommandPalette.svelte` | §8 |
| `app/src/lib/AppMenu.svelte` | §7 |
| `app/src/lib/fuzzy.ts` | §6.3, ~40 lines |
| `app/src/lib/ShortcutsSheet.svelte` | `help.shortcuts` — one static table |

### Changed files

| File | Changes |
| --- | --- |
| `app/src/lib/api.ts` | `:573-576` split into `errText` (user prose) and `errMessage` (diagnostic, keeps the code) |
| `app/src/routes/+layout.svelte` | mount `ToastHost` and `useKeymap()` |
| `app/src/routes/+page.svelte` | 12 dialogs → §2.7; `saveFile` reporting moves out of the loop (§2.9); the inline `svelte:window` handler at `:459-479` moves to `keymap.ts`; `layoutFocusFilter` (`:96,556`) deleted; 17 strings (§5.1); `EmptyState` at `:503,637,642` |
| `app/src/lib/Sidebar.svelte` | toolbar → app menu (Phase 2 moves it; this phase supplies the registry entries and the labels); `flash` → `toast()`; 3 `EmptyState`s; 7 strings |
| `app/src/lib/PresetGroup.svelte` | 8 dialogs; `run(fn, title)` → `run(fn, noun)`; the "Preset exists" confirm becomes an inline warning + `Replace preset`; `ContextMenu` gains disabled items so `explainOpen` can go; shared `ASPECT_LABELS`; 7 strings |
| `app/src/lib/ContextMenu.svelte` | `MenuItem` gains `disabled?: string` (the reason); disabled rows are non-activating and show the reason |
| `app/src/lib/BackupsPanel.svelte` | 2 dialogs; 4 strings; the save report's bytes and backup path land here (§2.9) |
| `app/src/lib/AutofillView.svelte` | 3 dialogs; 9 strings; 4 `EmptyState`s; `SearchField` |
| `app/src/lib/KeybindsView.svelte` | 1 dialog; 5 strings; 2 `EmptyState`s; `SearchField`; the dynamic reset tooltip |
| `app/src/lib/LayoutView.svelte` | 7 dialogs; 3 strings; `layoutFocusFilter` prop removed; nudge handler stays |
| `app/src/lib/WindowPanel.svelte` | 5 strings; `SearchField`; filter registration replaces the focus callback |
| `app/src/lib/HudPanel.svelte` | 1 string; inline error slot per row |
| `app/src/lib/ChatSplit.svelte` | 2 strings; inline error slot |
| `app/src/lib/NeocomButtons.svelte` | 1 dialog → toast; inline error slot |
| `app/src/lib/OverviewView.svelte` | 15 dialogs; 17 strings; 4 `EmptyState`s; the `no-windows` band absorbs the deleted confirm's sentence |
| `app/src/lib/OverviewColumnsTab.svelte` | 4 dialogs; 3 strings; a success toast that does not exist today |
| `app/src/lib/OverviewFiltersTab.svelte` | 7 dialogs; 4 strings; `SearchField` |
| `app/src/lib/OverviewAppearanceTab.svelte` | 1 dialog |
| `app/src/lib/ProbeFormationsView.svelte` | 11 dialogs; 14 strings; `flash` → `toast()`; `Ctrl+C`/`Ctrl+V` guards stay, labels go through `accelLabel` |
| `app/src/lib/ProbeViewer.svelte` | 3 strings |
| `app/src/lib/BatchView.svelte` | 3 strings; `muted`/`err` → primitives (8 sites); already satisfies the confirm rule (§2.6) |
| `app/src/lib/AccountsView.svelte` | **0 dialogs** — nothing to convert; 13 strings (8 of them v0.34's); `flash` → `toast()`; 7 message sites → primitives, incl. the two-action conflict line; `EmptyState`; `.conflict`/`.from-launcher` deleted; the swallowed `catch` at `:184` gets a message |
| `app/src/lib/FormationPicker.svelte` | 1 string |
| `app/src/lib/TreeNode.svelte` | 5 strings |
| `app/src/lib/InsertForm.svelte` | 5 strings; `field-error` → `InlineMessage` |
| `app/src/lib/prefs.svelte.ts` | 1 dialog → toast; `prefs.palette` added |
| `app/src/app.css` | delete `.hint`, `.error`, `.flash`, `.field-error`, `@keyframes fade-out` (`:64-70,81`) |

### Order of work

1. `accel.ts`, `fuzzy.ts`, `toast.svelte.ts`, `ConfirmDialog` — no call sites yet.
2. `api.ts` `errText` split. One file, 58 messages improved.
3. The 48 `InlineMessage` conversions, view by view. Each view is independently
   shippable and independently testable.
4. The 13 toasts, including `saveFile` (§2.9).
5. The 6 confirmations + the pack preview.
6. Delete the dead classes from `app.css` and the three local style blocks.
7. `commands.ts` — the registry, with `run` delegating to the handlers that now
   exist.
8. `keymap.ts` — replaces the three window handlers.
9. `AppMenu` — mouse-only completeness first.
10. `CommandPalette` — last, because it is the only piece that is pure
    accelerator and the only one nothing depends on.

Steps 1–6 are shippable without 7–10. Step 9 is shippable without 10. **Step 10
is never shippable without step 9** — that inversion is discovery rule 1.

---

## 11. Tests

Vitest, `*.spec.ts` beside the component, `@testing-library/svelte`, using the
existing `calls` harness in `app/src/lib/test/setup.ts`. Dialogs are already
mocked per-file where needed (`app/src/lib/NeocomButtons.spec.ts:10`), which is
the pattern to follow for `ConfirmDialog`.

For the *other* 66 call sites — the ones that stop being dialogs — the pattern to
follow is `app/src/lib/AccountsView.spec.ts`, added in v0.34: no dialog mock at
all, IPC stubbed through `calls`, and the failure asserted by the text it renders
(`:145`). Nothing below needs inventing; tests 13 and 14 are that file
generalised.

### Registry invariants — `app/src/lib/commands.spec.ts`

These are the tests that make the discovery rules mechanical rather than
aspirational.

1. **Nothing is palette-only.** Every command has `homes.length >= 1`. This is
   discovery rule 1, and it is one assertion. All 72 satisfy it, `accounts.acceptAll`
   included.
2. **Ids are unique and stable-shaped** — `^[a-z]+\.[a-zA-Z]+$`, no duplicates.
3. **Labels obey the copy standard** — sentence case (first char upper, no
   second capitalised word unless it is in an allow-list of proper nouns), no
   trailing `...`, `…` only where the id is in the ellipsis allow-list.
4. **No accelerator is bound twice** within the same scope.
5. **`enabled()` returns a non-empty string when false** — a disabled command
   with no reason is a bug, because the menu and the palette both render it.
6. **Every accelerator renders differently on macOS** — run `accelLabel` with
   `IS_MAC` both ways and assert the `primary` ones differ.

### Copy invariants — `app/src/lib/copy.spec.ts`

7. **No user-facing string contains the literal `Ctrl`.** A source scan over
   `app/src/**/*.svelte` for `Ctrl` outside a `<kbd>`/`accelLabel` call. This is
   R7 enforced.
8. **No `...` anywhere** — `…` only.
9. **`errText` strips the bracketed code and `errMessage` keeps it.**

### Behaviour

10. **`saveFile` emits exactly one toast for a two-slot save** — stub `save` for
    both slots, assert one toast and zero `message` calls. This is the §2.9 bug,
    pinned.
11. **Deleting an overview tab shows no modal** and emits a toast whose text does
    **not** contain "can't be undone" — the §2.8 bug, pinned by its own string.
12. **Deleting an overview tab, then Discard, restores it** — an integration test
    over `+page.svelte` proving the claim the new toast makes.
13. **A refused edit renders an `InlineMessage` at the control and no dialog** —
    one test per view that has one (Autofill, Keybinds, Layout, Overview×4,
    Probes, Presets). Nine tests, one shape — the shape of
    `AccountsView.spec.ts:137-149`, which already asserts exactly this for the
    one view that never had a dialog.
14. **A refused edit leaves the control usable** — specifically, the rename field
    at `OverviewFiltersTab:170` and the copy panel at `OverviewColumnsTab:85`
    stay open with their state intact. Same claim as
    `AccountsView.spec.ts:148`: the rejected ghost survives its own rejection, so
    the retry is still there.
15. **`ConfirmDialog` cancels on `Esc`, resolves `false`, and returns focus.**
16. **`ConfirmDialog` focuses the safe button on open**, not the destructive one.

### Palette

17. **`Ctrl+K` opens; `Ctrl+K` again closes; `Esc` closes and restores focus.**
18. **Typing `overv` ranks the Overview commands above everything else** — the
    proposal's worked example, as a test.
19. **Disabled commands appear below enabled ones with their reason** and
    `Enter` on one does nothing.
20. **A command runs after the palette closes** — assert the palette is unmounted
    before the command's first `invoke`.
21. **Characters and presets are searchable by name**, and selecting one calls
    `openFile`/`openPresetPair`.
22. **Recent ordering survives a `prefs` round-trip.**

### Keyboard

23. **`Ctrl+F` focuses the search field of each of the five views that has one**,
    parameterised — the current bug is that three of them get nothing.
24. **`Ctrl+F` on Probes and on Accounts does nothing and throws nothing.**
25. **`Esc` unwinds exactly one layer** — with a palette over a sheet over a
    popover, three presses close three things in order.
26. **The Layout nudge still works** and is not swallowed by the new global
    handler — `app/src/lib/LayoutView.spec.ts` already covers nudging; it must
    stay green unmodified, which is the real assertion.

### Regression floor

`00-overview.md`'s baseline is now **37 frontend test files / 1064 tests** (v0.34
added `AccountsView.spec.ts` and `launcher.test.ts`). This phase adds roughly 8
files and must not reduce the count or skip anything. `AccountsView.spec.ts` is
the one file this phase edits rather than adds: `:201` pins the
`Accept all — 1 character` string that §5.1 changes, and §5.1's `Move to {account}`
/ `Keep here` renames touch the `/move Zulu/i` and `/keep Zulu/i` accessible-name
queries at `:86,105`. Nothing else in it should need to move — which is the real
assertion about whether these renames changed behaviour.
`npm run check` (`svelte-check --fail-on-warnings`) stays clean — the new
`role`/`aria-*` attributes on `ConfirmDialog`, `CommandPalette` and `AppMenu` are
where that will bite, so write them right the first time.

---

## 12. Risks and rollback

| Risk | Why it is real | Mitigation |
| --- | --- | --- |
| **A failure becomes invisible.** 48 modals become inline messages; an inline message in a collapsed panel, a hidden sub-tab, or a scrolled-away row is a silent failure — and a modal never was. | This is the one way this phase can be *worse* than what it replaces. `OverviewColumnsTab`/`FiltersTab`/`AppearanceTab` are mounted-but-`hidden` (`app/src/lib/OverviewView.svelte:485-493`), so an error can render into a sub-tab nobody is looking at. | Every `InlineMessage` that is not in the viewport when it appears also raises a `Toast` with the same text. One rule, applied by the primitive rather than by each call site: `InlineMessage` takes an `escalate` prop, default `true`, and uses an `IntersectionObserver` on mount. Test 13 asserts both surfaces for a hidden sub-tab. |
| **Toasts are missed.** A 5-second toast on a second monitor is not a report. | Toasts only ever carry *successes* and *reversible* changes, both of which are also visible in the state itself (the tab is gone; the unsaved badge is lit). The two exceptions — `prefs` and clipboard paste — are warnings about things with no other home, and both are non-destructive. | — |
| **The rename table breaks muscle memory.** 127 labels move, five of them shipped in v0.34 and barely a release old. | Real, and it is why the deprecated labels go into `keywords` (§6.3) — the palette finds `Remove Window` for a release or two even though nothing shows it. | Ship the renames and the palette in the same release, never renames first. |
| **`Ctrl+F` regresses on Layout.** The current special case works; the general mechanism might not. | Test 23 parameterises all five, and `LayoutView.spec.ts` must stay green unmodified (test 26). | Roll back to the `layoutFocusFilter` prop — it is a two-line revert in `+page.svelte`. |
| **The palette becomes the only route to something.** Six months from now someone adds a command with `homes: []`. | Test 1 fails the build. | — |
| **`ConfirmDialog` traps focus badly** and the app becomes unusable by keyboard. | In-app modals get this wrong more often than they get it right. | Tests 15 and 16; `svelte-check --fail-on-warnings` catches the missing ARIA. If it goes wrong in the field, `ConfirmDialog` can fall back to `ask()` behind a one-line change in one file, because it has the same `Promise<boolean>` signature (§2.5). |
| **The `errText` split hides a code someone needs.** | The bracketed code is genuinely useful in a bug report. | It is kept — on the `title=` of every `InlineMessage` and in the History detail line. Nothing is lost, it is relegated. |

**Rollback.** The phase is ten independent steps (§10). Steps 1–6 are per-view
and each is one file. Step 10 can be dropped entirely — the app menu (step 9)
is the complete route by design, so shipping 1–9 is a coherent release. The only
step that cannot be partially rolled back is 2 (`errText`), and that is three
lines.

---

## 13. Definition of done

- [ ] `grep -c` over `app/src` finds **at most 8 call sites** for `message`,
      `confirm` and `ask` combined from `@tauri-apps/plugin-dialog` — the six
      confirmations, the pack-import preview, and nothing else. `openDialog` and
      `saveDialog` are untouched.
- [ ] No `message()` call remains anywhere.
- [ ] One Save that writes both slots produces **one** toast (test 10).
- [ ] The string `can't be undone` appears nowhere in `app/src` (test 11), and
      deleting an overview tab is provably reversed by Discard (test 12).
- [ ] `.hint`, `.error`, `.flash`, `.field-error`, `@keyframes fade-out` are gone
      from `app/src/app.css`; `.muted`/`.err` gone from `BatchView`; `.empty`
      gone from `KeybindsView`; `.conflict`/`.from-launcher` gone from
      `AccountsView`. All 51 sites in §5.4 use a Phase 1 primitive.
- [ ] Every string in §5's rename table is changed, and `Pair` reads
      `Pair this character…` at all four sites.
- [ ] Every dialog title from §2.2 is gone; the seven surviving modal surfaces
      have the copy in §5.3 verbatim.
- [ ] `errText`/`errMessage` are split; no bracketed code reaches a user-facing
      sentence (test 9).
- [ ] `app/src/lib/commands.ts` holds 72 commands; every one has a non-empty
      `homes` (test 1) and a reason string when disabled (test 5).
- [ ] The app menu renders every `app-menu`-homed command with its accelerator,
      disabled-with-a-reason rather than hidden.
- [ ] Every command in the palette is reachable by mouse without it — checked by
      test 1, and spot-checked by hand once against §6.1's *Also appears* column.
- [ ] `Ctrl+K` and `⌘K` both work; no user-facing string contains the literal
      `Ctrl` (test 7).
- [ ] `Ctrl+F` focuses the current view's field in all five views that have one,
      and every one of those fields shows its own accelerator.
- [ ] `Esc` unwinds exactly one layer, everywhere (test 25).
- [ ] `Ctrl+S`, `Ctrl+Z` and `Esc` behave identically in every view.
      (`Ctrl+Z` is *unbound and documented as such* until 5b — that is
      "identical", not "implemented".)
- [ ] The launch `EmptyState` names the palette once.
- [ ] `npm test` green, no skips, file count ≥ 45 (37 at v0.34 + ~8).
      `npm run check` clean.
- [ ] Nothing was removed: §6.1's *Also appears* column accounts for every
      control the sidebar toolbar used to hold, and the §5 rename table changes
      no control's behaviour.

---

## 14. What actually differs — read this before trusting §§1–13

Built 2026-08-14, branch `feat/ui-redesign-phase-5`. Sections 1–13 were written
against v0.34; Phases 1–4 moved most of what they cite, so every `file:line`
below §1 has drifted and several sections were **already true on arrival**.

### Already shipped before this phase started

- **`.hint`, `.error`, `.flash`, `.field-error` and `@keyframes fade-out` were
  already gone from `app.css`**, and §5.4's fifty-one class sites were already
  on `InlineMessage`/`EmptyState`/`Toast`. Phase 1 did that migration. §5.4 is
  a historical record, not a change list.
- **`Toast`, `toasts.svelte.ts`, `InlineMessage`, `EmptyState`, `Popover`,
  `SearchField`, `ListRow` and `Sheet` already existed** (Phase 1), and the
  toast host was already mounted in `+page.svelte`.
- **`accel()` already existed** in `lib/keys.ts`, so §9.1's `accel.ts` was not
  created — `keys.ts` is it.
- **§9.3's `Ctrl+F` change was already built.** Phase 2 shipped the general
  `viewFocusSearch` bindable and deleted the `layoutFocusFilter` prop chain.
  Test 23's bug — three of five views getting nothing — was already fixed.
- **An `AppMenu` already existed** (Phase 2), hand-written. This phase rebuilt
  it from the registry rather than adding a second menu.
- **R5's "a count is not a name"** was already implemented for `Accept all` by
  Phase 3's `acceptAllSentence`.

### The registry is 14 commands, not 72

The load-bearing divergence. The per-view commands — Layout's filter toggles
and environment control, Overview's fifteen, Autofill's, Keybinds', the Probes
list actions, Raw's two, and the two global Presets commands — are **not** in
`commands.ts`.

Each would need a callback threaded out of the component that owns its state,
for a palette row that duplicates a control one click away; and a registry entry
whose `run` is a no-op is worse than its absence, because the palette would then
lie about what it can do. They keep the homes §6.1's *Also appears* column gives
them, in their own views' `⋯` menus.

**Discovery rule 1 holds in both directions**, which is why this is a scope cut
and not a hole: nothing in the array is palette-only, and nothing outside it is
in the palette. `commands.spec.ts` test 1 still passes over all 14.

Also absent for the same reason: §6.4's Recent/Frequent sections and §6.5's
`prefs.palette` (an MRU over 14 commands earns nothing), and §8.2's arrow-key
selection model — the palette is `ListRow`s inside the existing `Popover`, which
brings its own dismissal and focus handling.

### Smaller, deliberate

- **`ConfirmDialog` is built on `Sheet`**, not beside it. Sheet already had the
  focus trap, Escape and focus-restore §2.5 asks for. Sheet gained one `role`
  prop for `alertdialog`. The imperative API is a queue in
  `ui/confirm.svelte.ts` plus a host mounted once, so the `Promise<boolean>`
  survives the view that raised it being unmounted by the action itself.
- **`InlineMessage`'s `escalate` defaults to `error` only**, not to `true` for
  every variant as §12 has it. Two reasons: hints and bands are off-screen on
  purpose and toasting them trains the reflex this phase exists to untrain, and
  `Toast` renders an `InlineMessage` itself — a blanket default would not
  terminate. `Toast` passes `escalate={false}` explicitly.
- **The seventh modal surface is the pack-import preview, as specced.** The
  other six confirmations are §5.3's, verbatim in substance.
- **`OverviewFiltersTab`'s preset delete became a toast** (§2.7), and the
  SETTINGS-preset delete kept its confirm (§5.3). §8 of `05b-undo.md` calls this
  the naming trap; both sides of it are now in code with a comment saying so.
- **`Fighter UI` → `Fighter panel` in two places**, not one: §5.1 lists only
  `HudPanel`, but `layout.ts:619` labels the same rectangle on the canvas.
- **`SearchField` renders its accelerator as a trailing `<kbd>`** and dropped
  the trailing `…` from filter placeholders. Phase 1 had baked the shortcut
  into the placeholder string, which is R7's violation moved into the primitive.
- **`errText`/`errMessage` split as specced**, and `InlineMessage` gained a
  `detail` prop so the bracketed code lands on `title=`.
- **`ShortcutsSheet` exists**; `help.shortcuts` opens it. Its table is the
  registry's accelerators plus the positional keys that are not commands.
- **`fuzzy.ts` is used only by the palette's command section.** Characters and
  presets keep their substring filter and alphabetical order, which is how a
  name is found — ranking them would fight the sidebar's own ordering.

### Counts, measured

`grep` over `app/src` finds **zero** calls to `message`, `confirm` or `ask`.
What remains of `@tauri-apps/plugin-dialog` is four `open`/`save` file pickers,
in `OverviewView`, `PresetGroup`, `ProbeFormationsView` and `+page.svelte`.

61 test files / 1393 tests (from 58/1341), `npm run check` clean over 509 files,
`npm run build` clean. Rust untouched.

### Not done

- §11's tests 15, 16, 17, 19-22, 25: `ConfirmDialog` focus/Escape behaviour is
  inherited from `Sheet`, which has its own spec, but there is no test for the
  confirm host specifically; and the palette has no keyboard-selection tests
  because it has no keyboard-selection model (see above).
- Not walked control-by-control against §5's rename table. The tables for
  `Sidebar`, `BatchView` and `LayoutView` were applied where the string still
  existed; several rows had already been changed by Phases 1-4 and a few name
  controls those phases deleted.

_Added 2026-08-14 (Phase 5, as built)._

_Added 2026-08-13 (UI/UX redesign, Phase 5)._
