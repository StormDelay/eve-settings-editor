# Phase 3 — Sheets for Accounts and Copy settings

_Part of the v0.33.0 UI/UX redesign. Phase 1 (`01-tokens-and-primitives.md`)
supplies the tokens and the twelve primitives; Phase 2 (`02-shell.md`) supplies
the always-visible context bar and save cluster. This phase consumes both and
adds no tokens and no primitives of its own._

---

## 1 Goal

Two of the app's eight views are not views. They are takeovers: entering them
replaces the editor, and nothing on screen offers a way out. This phase converts
both into **sheets** — a panel over the work area that leaves the shell standing
— and in doing so retires the audit's single CRITICAL finding.

It is specified to be shippable on its own because it is small. The two views'
internals are almost untouched; the change is one state variable in the shell,
one markup branch that stops being a branch, and a header that moves from each
view into the primitive that now frames it. Everything a user can do today they
can still do, in the same order, with the same words.

Three things fall out of it that are worth naming up front, because they are the
reason it is worth doing rather than merely worth doing eventually:

- **Save stops disappearing.** Today, entering Accounts with unsaved edits hides
  the Save button and both unsaved badges. That is a one-click-from-data-loss
  state with nothing on screen admitting it.
- **Returning costs nothing to implement.** Because the editor is never
  unmounted, "restore the prior state" is not a feature — it is the absence of a
  destruction. See §6.
- **The editor becomes a witness.** Both sheets change data the editor is
  showing. Today that happens behind a curtain. With the editor visible behind
  the sheet, pairing a character visibly lights up the account-scoped tabs, and
  a batch copy that overwrites the open file can no longer do so unremarked.
  See §7.

---

## 2 The fault, with evidence

### 2.1 The dead end

`app/src/routes/+page.svelte:30` declares the takeover switch:

```ts
let mainView: "file" | "accounts" | "batch" = $state("file");
```

It is set away from `"file"` in six places — the two sidebar buttons
(`+page.svelte:485-486`, wired from `Sidebar.svelte:127-130`) and four in-view
nudges that send the user to Accounts or Copy settings from inside Overview,
Autofill, Keybinds and Probes (`+page.svelte:572`, `:582`, `:591-592`, `:601`).

It is set **back** to `"file"` in exactly two places, and both of them are inside
functions that swap the open document:

- `openFile()` — `+page.svelte:262`
- `openPresetPair()` — `+page.svelte:300`

Neither `AccountsView.svelte` nor `BatchView.svelte` renders a close, back, or
cancel control. `AccountsView`'s header holds Refresh and "Calibrate an
account…" and nothing else (`AccountsView.svelte:103-110`); `BatchView` opens
straight into an `<h2>` and a Profile selector (`BatchView.svelte:266-271`).

So: **once you enter Accounts or Copy settings, the only way back to the editor
is to open a file from the sidebar.** That path runs `openFile()`, which
discards the open document and re-reads it, resets `selectedWindowId` and
`reveal` to `null` (`+page.svelte:263-264`), resets `treeFile` (`:255`), and
keeps your view tab only if `viewAvailable()` says the new file supports it
(`:276`). Scroll position inside `.tree-area` (`app/src/app.css:63`) goes with
the unmounted DOM. Getting back to the editor therefore costs you your place in
it, which is a peculiar thing for an escape hatch to cost.

### 2.2 The hidden Save

The takeover branch is structural, not cosmetic:

```svelte
{#if mainView === "accounts"}
  <AccountsView openPath={…} />
{:else if mainView === "batch"}
  <BatchView openPath={…} />
{:else}
  <section class="editor">
    <header class="filebar">      ← +page.svelte:505
      …badges, Discard, view tabs, Save…
```
(`+page.svelte:496-542`)

Everything from `<section class="editor">` at `:501` to the closing `{/if}` at
`:678` lives in that `{:else}` branch. That is not only the editor: it is the
**whole file bar** — the editable/read-only badge (`:509-513`), *both* unsaved
badges (`:515-518`), the Discard button (`:520-526`), the view tab strip
(`:527-536`) and the Save button (`:538-541`) — plus the `BackupsPanel`
(`:646-663`) and the insert modal (`:664-677`).

Consequence: **enter Accounts with unsaved edits and every indication that you
have unsaved edits vanishes with them.** `Ctrl+S` still works, because its
handler is on `<svelte:window>` and therefore outside the branch
(`+page.svelte:461-463`) — but nothing on screen says there is anything to save,
nothing says which of the two files is dirty, and the Discard button that would
reverse them is gone too. The `.layout` grid is
`var(--col-left, 280px) 1fr auto` at `app.css:23`, so the takeover fills the
centre track and the third track collapses to zero: the backups column
disappears without so much as a rail.

### 2.3 Losing file context changes what the views show

Both views take `openPath` and both are scoped by it:

- `AccountsView` finds the profile folder containing `openPath` and shows only
  that folder's accounts and characters (`AccountsView.svelte:17-34`). No open
  file → `scope` is `null` → it falls back to the entire roster.
- `BatchView` seeds its source character from `openPath` when the name contains
  `core_char_` (`BatchView.svelte:39-41`), and warns when a chosen target is the
  open file (`:172`).

So the sidebar-round-trip escape does not merely cost you your editor state; it
also changes what the view you were just in was showing. The dead end and the
scoping are the same fact seen twice.

A related inconsistency worth recording while we are here: `openPath` is
`current.path`, and `current` is `slots[active]` (`+page.svelte:60`) where
`active` is **derived from the view tab** (`:53-59`). Open Copy settings from
the Layout tab and it seeds the source with your character; open it from the
Keybinds tab — where `active` is `"user"` — and it seeds nothing, because an
account filename does not contain `core_char_`. Today that is invisible, because
the tab you were on is gone the moment the takeover renders. With the sheet, the
tab row is still on screen behind it, and the inconsistency becomes legible.
§5.4 fixes it.

---

## 3 Sheet behaviour spec

### 3.1 What a sheet is here

A sheet is a **modal panel occupying the work area**, with the shell's context
bar and subject browser left standing outside it, legible through a scrim. It is
not a centred dialog (the app's existing `.overlay`/`.modal` pair, `app.css:101-104`,
covers the whole window and centres a small box) and it is not a route.

Geometry:

- The **scrim** is `position: fixed; inset: 0` and covers the whole window at
  low alpha, using the Phase 1 overlay treatment. It dims the chrome; it does not
  hide it.
- The **panel** is `position: fixed`, inset from the top by the context bar's
  height and from the left by the subject browser's width, so it lands exactly
  on the work-area rectangle. It reads those two distances from CSS custom
  properties the Phase 2 shell already has to know
  (`--shell-inset-top`, `--shell-inset-left`; see §3.6).

Both live outside the `.layout` grid flow. This matters mechanically: `.layout`
declares three explicit columns (`app.css:23`), so a fourth in-flow child would
create a fourth track and shove the layout sideways. Fixed positioning takes no
track, which is exactly how the existing `.overlay` already coexists with the
grid at `+page.svelte:665`.

### 3.2 Why modal, when the point is to keep the shell visible

There is a real tension here and it deserves a stated resolution rather than a
shrug. The fault being fixed is "Save is not visible". The obvious reading of
the fix is "make Save visible **and** clickable". But `aria-modal="true"` tells
assistive technology that everything outside the dialog is inert — so a sheet
that leaves Save genuinely operable must not claim to be modal, and then it needs
a second interaction mode that nothing else in this app has.

**Decision: the sheet is modal. The shell behind it is legible but inert.**

Reasons, in order of weight:

1. **The fault is an information fault, not a reach fault.** "Nothing on screen
   indicates there is anything to save" is cured by *seeing* `2 unsaved`. Acting
   on it costs one `Esc` — or zero, because `Ctrl+S` still works from anywhere
   (`+page.svelte:461-463`, on `<svelte:window>`, unaffected by any of this).
   Today Save is unreachable by *any* means but the shortcut and unreadable by
   any means at all. Modal is a strict improvement over that; non-modal is a
   marginal improvement over modal.
2. **A live subject browser behind the sheet would misbehave.** Clicking a
   character there runs `openFile()`, which calls `confirmDiscardIfDirty()`
   (`+page.svelte:198-208`) — a native `ask()` dialog stacked on top of a sheet —
   and then silently re-scopes the sheet's contents underneath the user, because
   `AccountsView`'s entire scope derives from `openPath`
   (`AccountsView.svelte:17-28`). Inert is not a compromise here; it is the
   correct behaviour.
3. **One primitive, one mode.** Phase 1 builds a single `Popover / Sheet`
   primitive that also has to serve the insert form and the overview
   copy-columns panel. Giving it a modal mode and a non-modal mode doubles its
   surface for one caller's convenience.

The ARIA is therefore honest: `role="dialog"`, `aria-modal="true"`, and a real
focus trap, with the rest of the app genuinely unreachable while it is open.

### 3.3 Keyboard and focus

| Key | Behaviour |
| --- | --- |
| `Esc` | Closes the sheet. See §3.4 for the one guarded case. |
| `Tab` / `Shift+Tab` | Cycles within the sheet only. First stop on open is the first interactive control in the sheet body, not the close button — the user came here to do something, not to leave. |
| `Ctrl+S` | Unchanged. Saves the open documents. The existing window handler is outside the sheet and outside the branch that used to hide it. |
| `Ctrl+F` | Suppressed while a sheet is open. Its handler (`+page.svelte:469-476`) focuses either the tree search box or the layout window filter, both of which are behind the scrim; focusing an inert control would break the trap. |
| `Ctrl+K` | Reserved for Phase 5. When it lands, it closes the sheet before opening the palette (§6.3). |

On open, the sheet records `document.activeElement` and moves focus into itself.
On close it restores focus to that element — which for the sidebar entry points
is the Accounts / Copy settings button, and for the in-view nudges is the "Pair…"
link inside Overview / Autofill / Keybinds / Probes (`OverviewView.svelte:307`,
`AutofillView.svelte:87`, `KeybindsView.svelte:85,93`,
`ProbeFormationsView.svelte:467`). Those nudges sit inside a view that is still
mounted behind the sheet, so the restore target is guaranteed to still exist.
This is the second dividend of not unmounting the editor.

Dismissal is by `Esc`, by the header's close button, and by clicking the scrim.
The scrim click is a convenience only: `AboutPanel` already dismisses on backdrop
click (`AboutPanel.svelte:21`) and its test pins that behaviour
(`AboutPanel.spec.ts:53`), so the app's users have been taught the gesture.

### 3.4 Interaction with unsaved state

Two directions, and they are asymmetric.

**Editor edits, sheet open.** Nothing is guarded. The sheet does not touch the
open documents, so an unsaved document is in no more danger with a sheet open
than with it closed. The save cluster behind the scrim shows the count; `Ctrl+S`
writes. Opening or closing a sheet never calls `confirmDiscardIfDirty()`.

**Work in progress inside the sheet.** Only one thing in either sheet is a
multi-step commitment: the guided capture in Accounts (§4.3). Everything else is
either immediate (an alias commits on blur, a pairing commits on select) or
purely a draft that costs seconds to rebuild (the Copy settings form). So:

- `Esc` / close / scrim always dismiss immediately, with no confirmation. A
  confirm-on-close for a form that has written nothing is exactly the
  learned-to-disbelieve dialog the redesign is trying to reduce.
- The capture flow does not block dismissal either — it **survives** it, and
  dismissal does not discard its backend baseline (§4.4.4). See §4.3 for why
  that is the right answer rather than the lazy one.

### 3.5 Scroll containment

The sheet body owns its own `overflow-y: auto`. Nothing inside a sheet may scroll
the document.

This is not theoretical tidiness. `.batch` today declares
`padding: 1rem; max-width: 46rem` and no overflow rule at all
(`BatchView.svelte:413`), sitting in a grid item inside a `height: 100vh` grid
(`app.css:23`). A long target list therefore grows the grid item past the
viewport and scrolls the document — the app's only scrolling body. Putting it in
a sheet fixes that as a side effect. `.accounts` already scrolls itself
(`AccountsView.svelte:182`) and loses that rule when the sheet takes over the
job.

### 3.6 What Phase 3 needs from the Phase 1 `Sheet`

Consumed as-is: the panel/scrim structure, `--surface-overlay` for the panel
ground, `--border`, the elevation shadow, the header type scale, `Button` for
the close affordance, and the focus/`Esc`/restore behaviour described above if
Phase 1 already carries it.

Three extensions, stated exactly:

1. **`width: "default" | "wide"`.** Copy settings needs more horizontal room
   than Accounts (§5.2). One extra named value, resolved to a max-width token;
   no free-form sizing.
2. **An `actions` snippet in the header**, rendered right-aligned before the
   close button. Accounts puts Refresh and "Calibrate an account…" there
   (§4.1). Without it those two buttons have to be re-hosted in the body, below
   a header that already looks like it should hold them.
3. **`--shell-inset-top` / `--shell-inset-left` honoured for the panel's
   position.** These are Phase 2's to define (they are the context bar's height
   and the subject browser's width, both of which Phase 2 already computes for
   the grid). The `Sheet` reads them with a `0` fallback, so it degrades to a
   full-window sheet if Phase 2 has not landed — which keeps Phase 3
   independently shippable, just less pretty.

If Phase 1 has already shipped and cannot absorb these, all three are additive
and none changes an existing call site.

---

## 4 Accounts as a sheet

### 4.1 Shape

`<Sheet title="Accounts" width="default">`, rendered by `AccountsView` itself
rather than by the shell. The alternative — the shell renders the `Sheet` and
passes a header-actions snippet down — means plumbing Refresh and "Calibrate an
account…" up through `+page.svelte`, which owns neither `loadRoster()` nor
`startCapture()`. Letting each view frame itself keeps the shell's job to one
line per sheet.

The view's own `<header class="accounts-head">` (`AccountsView.svelte:104-110`)
is deleted. Its `<h2>Accounts</h2>` becomes the sheet's `title`; its two buttons
become the sheet's `actions` snippet. Without this the word "Accounts" appears
twice, six pixels apart.

Everything below the header is **unchanged in structure**: the alias input
(`:134-139`), the three character slots with their unpair `✕` and their
add-character `<select>` (`:140-164`), the empty-roster hint (`:127-129`) and the
unassigned-characters list (`:169-178`). Phase 1's `Chip`, `Field`, `ListRow`
and `EmptyState` restyle them; no markup logic moves. `MAX = 3`
(`AccountsView.svelte:9`) and the hard cap it mirrors in the backend are
untouched.

The `error` and `captureNote` paragraphs (`:124-125`) become `InlineMessage`
(error) and `Toast` (success) per Phase 1, keeping `aria-live="polite"` on the
latter.

### 4.2 Scoping now that the editor is visible

`openPath` keeps feeding the scope derivation at `AccountsView.svelte:17-28`,
but it should be `slots.char?.path ?? slots.user?.path` rather than
`current.path`. The scope is a *profile folder*, both slots are always in the
same folder by construction (`reconcileUserSlot` / `reconcileCharSlot`,
`+page.svelte:327-367`), and taking it from the tab-derived `active` slot makes
the sheet's contents depend on which editor tab happened to be selected — a
dependency that is now visible on screen and reads as a bug.

Since both slots resolve to the same folder, this is a no-op in every real case
and a correctness fix in the case where only the user slot is filled.

### 4.3 The capture flow survives dismissal

"Calibrate an account" is a three-step wizard: launch EVE, change an
account-wide setting, log out, click Done (`AccountsView.svelte:112-122`). Steps
one to three happen **outside this application**, and they take minutes.

The state is split across the process boundary, and that is the key fact:

- The **baseline** — a snapshot of every settings file's mtime, excluding the
  open documents — lives in the backend, in `AppState.capture`
  (`app/src-tauri/src/ops.rs:34-43`), written by `begin_capture`
  (`ops.rs:381-390`) and diffed by `resolve_capture` (`ops.rs:392-403`).
- The **frontend** holds only two booleans-worth of UI: `capturing` and
  `captureNote` (`AccountsView.svelte:48-50`).

So the expensive half already survives anything the frontend does. If the sheet
unmounts on dismissal, only `capturing` is lost — and the user who dismissed the
sheet, alt-tabbed to EVE, did the three steps, came back and reopened Accounts
would find the wizard gone and a **Calibrate** button that, pressed, calls
`begin_capture` again and re-baselines to *after* EVE's write. The detection is
then guaranteed to find nothing. That is a silent, reproducible failure created
by making the sheet dismissable.

**Decision: the capture survives dismissal.** `capturing` and `captureNote` move
out of the component and into `app/src/lib/accounts.svelte.ts`, which already
owns the shared roster rune (`accounts.svelte.ts:7`) and is where every other
piece of cross-component account state lives. Four lines:

```ts
// Guided capture spans a trip out to the EVE client, so its progress must
// outlive the Accounts sheet being dismissed — the baseline it pairs with
// lives in the backend (`AppState.capture`) and does.
export const captureState = $state<{ active: boolean; note: string | null }>({
  active: false,
  note: null,
});
```

`startCapture` additionally becomes a no-op when `captureState.active` is
already true, so a second press cannot re-baseline over a capture in flight. The
button label reflects it.

Blocking dismissal instead was considered and rejected: the flow **requires** the
user to leave the application, so a sheet that cannot be closed during it is a
sheet that cannot be used. Cancelling on dismissal was rejected for the same
reason, plus it would throw away a baseline the user cannot cheaply recreate.

No chrome outside the sheet announces an in-flight capture in this phase. The
sheet is one click from the app menu and it remembers its own state; a persistent
badge in the context bar is Phase 2's real estate and would need its own
justification. If it turns out to be needed, `captureState.active` is now a
module-level rune and any component can read it.

### 4.4 `clear_capture` — the baseline gets an end as well as a beginning

`Cancel` clears the frontend flag and nothing else. The baseline sits in
`AppState.capture` (`app/src-tauri/src/ops.rs:38`) until the next
`begin_capture` overwrites it, because there is no command that discards it:
`app/src-tauri/src/lib.rs:178-186` exposes `begin_capture` and
`resolve_capture` and nothing more.

**Decision (the owner's): close it, here, in this phase.** The hole is
**pre-existing** — today's Cancel button (`AccountsView.svelte:119`) has it
exactly as written, with no sheet anywhere in sight — so this is a bug fix
riding along with the phase, not scope creep, and it does not go to
`docs/small-tasks.md`. It shares exactly one call site with the sheet
conversion, so **it can ship independently**, before or after the rest of
Phase 3, in either order.

#### 4.4.1 The command

One function beside its two siblings in `ops.rs`, one `#[tauri::command]`
one-liner beside theirs in `lib.rs`, one line in `api.ts`:

```rust
/// Discard the guided-capture baseline. Cancelling the wizard — or finishing
/// it — must leave nothing behind for the next `resolve_capture` to diff
/// against.  (ops.rs, beside begin_capture at :383)
pub fn clear_capture(state: &AppState) {
    *state.capture.lock().unwrap() = None;
}
```

```rust
// lib.rs, after resolve_capture at :183-186
#[tauri::command]
fn clear_capture(state: tauri::State<'_, AppState>) {
    ops::clear_capture(&state);
}
```

Registered by adding the name to the existing capture line of
`generate_handler!` — `begin_capture, resolve_capture, clear_capture,`
(`lib.rs:623`).

**No `ErrDto`, and no return value.** It cannot fail: it writes `None` into a
`Mutex` the caller already owns. A poisoned mutex panics on `.unwrap()` exactly
as `begin_capture` (`ops.rs:389`) and every other `AppState` accessor in the
file already does; making this the one fallible-looking capture command would
force a `catch` at a call site with nothing to do about it. It takes no `roots`
either — unlike both siblings it does no discovery.

```ts
// api.ts, after resolveCapture (:431)
clearCapture: () => invoke<void>("clear_capture"),
```

#### 4.4.2 `resolve_capture` must first learn what "no baseline" means

This is the part that makes the fix worth more than tidiness, and it must land
in the same commit. `resolve_capture` reads the slot as

```rust
let baseline = state.capture.lock().unwrap().clone().unwrap_or_default();
```
(`ops.rs:396`)

and `capture_diff` treats a path the baseline does not contain as advanced —
`None => true, // appeared since baseline` (`app/src-tauri/src/accounts.rs:173-176`).
An **empty** baseline therefore means *every* discovered file changed. In a
profile holding one character and one account file that is
`detected: Some((char_id, user_id))`: a confident, entirely fabricated pairing,
one `confirm_pairing` away from being written to the store.

Today that state is unreachable — the only caller is the wizard's Done button,
which exists only once `begin_capture` has run. `clear_capture` makes it
reachable. So:

```rust
pub fn resolve_capture(state: &AppState, roots: &[PathBuf]) -> accounts::CaptureResult {
    // No baseline is not an empty baseline: `capture_diff` reads an absent path
    // as "appeared since baseline", so diffing against `default()` would report
    // every file on disk as having changed.
    let Some(baseline) = state.capture.lock().unwrap().clone() else {
        return accounts::CaptureResult::default();
    };
    …
}
```

`CaptureResult` (`accounts.rs:157-164`) gains `Default` in its derive list — one
word, and the three fields' defaults are precisely the right answer. The
frontend needs no new branch: `r.changed_users.length === 0` already prints "The
account file didn't change…" (`AccountsView.svelte:86-88`), which is the honest
thing to say when there was nothing to compare against.

#### 4.4.3 `resolve_capture` does not clear the slot, and must not start

Checked: it clones the baseline and puts nothing back (`ops.rs:395-403`). The
slot survives the call, and that is correct, because **Done is retryable** —
every ambiguous outcome at `AccountsView.svelte:86-96` leaves the wizard open
and asks the user to try again, and each retry diffs against the same baseline.
A backend that cleared on resolve would make the second press diff against
nothing (and, before §4.4.2, against everything).

Clearing therefore belongs to the frontend, which is the only side that knows
the flow has ended.

#### 4.4.4 Which endings clear, and which dismissals preserve

| Event | Baseline | Why |
| --- | --- | --- |
| **Cancel** (`AccountsView.svelte:119`) | **cleared** | The user abandoned the flow. This is the hole being closed. |
| **Done → `detected` → `confirmPairing` succeeds** (`:75-84`) | **cleared** | Spent. The pairing is written, the wizard closes, and nothing will ever diff against this baseline again. |
| **Done → `confirmPairing` throws** (`:81-83`, the `MAX = 3` cap) | preserved | The wizard stays open; this is a retry, not an ending. |
| **Done → ambiguous or nothing detected** (`:86-96`) | preserved | The retry the message asks for diffs against it. |
| **`Esc`, the close button, the scrim** | **preserved** | §4.3. The user is on their way to EVE. |
| **Opening a document or a preset pair** (`+page.svelte:262, 300`, now `sheet = null`) | **preserved** | Same dismissal by another route, same reason. |
| **App shutdown** | nothing to do | `AppState.capture` is an in-process `Mutex` with no persistence; it dies with the process. No `Drop`, no shutdown hook, no `on_window_event`. |

The three preserved rows are the point. §4.3 argued the capture must survive
dismissal *because the baseline is expensive* — it costs the user a launch of
EVE, a settings change and a logout to recreate. Calling `clear_capture` on
dismissal would destroy exactly the thing §4.3 exists to protect, and would do
it silently: the user comes back, presses Calibrate again, re-baselines to
*after* EVE's write, and detection is then guaranteed to find nothing. That is
the same silent failure §4.3 describes, arrived at from the other side. **Only
an ending clears. A dismissal never does.**

One asymmetry left deliberately alone: `begin_capture` excludes the documents
open *then* (`ops.rs:386-388`), `resolve_capture` excludes those open *now*
(`:399-401`). Opening a different file mid-capture therefore drops it out of the
"after" snapshot, so it can no longer be reported as changed. That can only
suppress a detection, never invent one; it is pre-existing, and the user-visible
result is one of the retry messages. Not this phase's business.

#### 4.4.5 One caller, next to the state it owns

§4.3 moves the capture flags into `accounts.svelte.ts`. The clear goes with
them, so no component reaches for `api.clearCapture` directly:

```ts
// The only place the backend baseline is discarded. Both endings of the flow —
// cancelled, and resolved into a confirmed pairing — come through here, so
// `captureState` and `AppState.capture` cannot disagree. Dismissing the sheet
// is not an ending (§4.4.4) and does not call it.
export async function endCapture(note: string | null = null): Promise<void> {
  captureState.active = false;
  captureState.note = note;
  await api.clearCapture();
}
```

The flag is cleared *before* the await, so a rejected `invoke` cannot strand the
wizard on screen. There is no `try`/`catch`: `clear_capture` returns `()` and
cannot fail (§4.4.1), and if the IPC bridge itself is gone the baseline is gone
with it.

`AccountsView` then contains no `clearCapture` call of its own. Cancel (`:119`)
becomes `onclick={() => endCapture()}`, and the success branch (`:79-80`)
replaces its two assignments with one call:

```ts
await confirmPairing(charId, userId);          // already refreshes the roster
await endCapture(`Paired ${nameOf(charId)} ↔ account ${userId}.`);
```

Every other branch of `finishCapture` (`:86-96`) keeps writing
`captureState.note` directly and leaves the baseline where it is.

### 4.5 Refresh

Unchanged. `loadRoster()` refreshes the shared rune, which the editor behind the
sheet also reads — see §7.1.

---

## 5 Copy settings as a sheet

### 5.1 Sheet, not a route — the decision and its evidence

This is the one flow where a takeover is arguably defensible: it is long
(profile → source kind → source → aspects → targets → plan → apply → results),
it is the app's only destructive screen, and it wants the user's full attention.
Three options were weighed.

**A dedicated route.** Rejected. The app has exactly one page
(`app/src/routes/+page.svelte`, with `+layout.svelte`), and that page's script
*is* the application state: both slots, both dirty flags, the view tab, the
selection, the preset, the layout availability. Navigating away either destroys
all of it or forces it into a store first. That is a large, risky refactor whose
only reward is a URL nobody can type — and it reintroduces the exact problem
this phase exists to remove, because "come back to where you were" becomes
something that has to be implemented rather than something that never stopped
being true. Routing is the wrong tool for a modal task in a desktop app.

**A full-width sheet (or an unchanged takeover with a close button).**
Rejected, on the view's own evidence: `.batch` already caps itself at
`max-width: 46rem` (`BatchView.svelte:413`). It is a 46rem column today,
rendered into a full-width takeover. The takeover is not buying it room; it is
buying it whitespace on both sides while hiding the editor.

**A `wide` sheet.** Chosen. It is more than the default sheet width because the
target list carries three pieces per row — name, filename, and a folder label
when the row is out of folder (`BatchView.svelte:347-350`) — and the plan
preview's account warnings are long sentences that list collateral characters by
name (`:384`). Cramping those is how a destructive screen gets misread.

The decisive argument is not width, though. It is that **Copy settings is the
flow with the most reason to keep the editor visible.** It already warns that a
chosen target is the file open in the editor (`BatchView.svelte:172, 357-363`).
Warning a user about the document behind the curtain, while holding the curtain
shut, is the wrong way round. With the sheet, the file it is talking about is
right there.

### 5.2 Shape

`<Sheet title="Copy settings" width="wide">`, again rendered by `BatchView`
itself.

The view's `<h2>` (`BatchView.svelte:267`) is dynamic — it reads "Copy a file
onto other files" in file mode and "Copy a setup to other characters" otherwise.
That distinction is real and must not be lost. It moves to a subtitle line
directly under the sheet title, which stays the fixed "Copy settings" so the
sheet's identity does not change under the user when they click a radio button.
The subtitle keeps both strings verbatim.

The body is otherwise unchanged: the Profile selector (`:270-273`), the three
source radios (`:276-284`), the three source pickers (`:286-315`), the aspect
checkboxes with "Everything" exclusive (`:318-331`), the target list with select
all / clear / show-other-folders (`:333-353`), all four preview blocks
(`:355-391`), the apply button (`:393-396`) and the results list (`:398-408`).
Phase 1's `Field`, `Panel`, `ListRow`, `InlineMessage` and `Button` restyle
them; the four hardcoded colours at `:425-427` (`#d0a000`, `#e06c6c`, `#6cc06c`)
become `--warn`, `--danger`, `--ok` per Phase 1's palette.

### 5.3 The apply button and the results list

Both stay inside the sheet. A results list is the outcome of the task the sheet
exists for, and moving it out — to a toast, say — would either truncate a
per-file success/failure list or fire one toast per target, which is the same
mistake `saveFile()` makes today (`+page.svelte:430-431`, a `message()` inside
the slot loop).

The sheet stays open after a successful apply. Closing it automatically would
discard the results the user just asked for.

### 5.4 The seeding fix

`sourcePath` seeds from `openPath` (`BatchView.svelte:39-41`), and `openPath` is
the tab-derived `current.path` (`+page.svelte:60, 499`). Change the prop to
`openCharPath`, fed from `slots.char`, so the source seeds from the open
character regardless of which editor tab is selected. This is a one-line change
to the call site, and it removes a difference in behaviour that the sheet makes
visible (§2.3).

`openUserPath` is added alongside it, for §7.2.

---

## 6 Navigation & return semantics

### 6.1 What replaces `mainView`

```ts
// One sheet at a time; `null` is the editor. Replaces `mainView`, which had
// no third value's worth of meaning — "file" was only ever "no sheet".
let sheet = $state<"accounts" | "batch" | null>(null);
```

All six assignments retarget mechanically:
`mainView = "accounts"` → `sheet = "accounts"` at `+page.svelte:485, 572, 582,
591, 601`; `mainView = "batch"` → `sheet = "batch"` at `:486, 592`.

The two `mainView = "file"` resets at `:262` and `:300` become `sheet = null`
and **stay where they are**. They are unreachable by mouse now (the scrim makes
the sidebar inert), but they encode a rule worth keeping: opening a document is
a request to be in the editor, so it closes any sheet. Phase 5's palette will be
able to open a character while a sheet is open, and this is already the correct
behaviour for that.

The markup change is the whole point of the phase, and it is small:

```svelte
<!-- the {#if mainView === …}{:else if …}{:else} chain at :496-500 is deleted,
     as is its {/if} at :678. The editor becomes unconditional. -->
<section class="editor"> … </section>
{#if current?.status === "opened"}  … BackupsPanel … {/if}
{#if insertTarget !== null} … {/if}

{#if sheet === "accounts"}
  <AccountsView openPath={…} onClose={() => (sheet = null)} />
{:else if sheet === "batch"}
  <BatchView openCharPath={…} openUserPath={…}
             onClose={() => (sheet = null)} onApplied={…} />
{/if}
```

The `Sheet` inside each view is fixed-positioned, so these two children take no
grid track (§3.1).

### 6.2 Return semantics

**Nothing is restored, because nothing is destroyed.**

Because the editor markup is no longer inside a branch that the sheet turns off,
`<section class="editor">` and every component under it stay mounted for the
whole life of the sheet. That means:

| What must survive | How it survives |
| --- | --- |
| View tab | `view` (`+page.svelte:48`) is never reassigned by opening or closing a sheet. |
| Tree file switch | `treeFile` (`:46`) likewise. |
| Canvas selection | `selectedWindowId` (`:93`) likewise — and it is lifted to the page precisely so it survives view switches, so it already had to be durable. |
| Tree search | `query` (`:178`) and `reveal` (`:98`) likewise. |
| Scroll position | `.tree-area` (`app.css:63`) keeps its `scrollTop` because the element is never removed. This is the one item that could not be restored by re-assigning state, and it is free. |
| Per-view internal state | Every child component — `LayoutView`, `OverviewView`, `WindowPanel`'s filter, `KeybindsView`'s search — keeps its own `$state` for the same reason. |

There is deliberately **no snapshot-and-restore code**. Any such code would be a
second source of truth for state that already exists, and it would rot the first
time someone adds a seventh view.

### 6.3 One sheet at a time — agreed, with the reason

`sheet` is a single nullable variable, not a stack. I agree with the owner's
judgement, and the reason is smaller than it looks: **there is exactly one place
in the app where one sheet refers to the other, and it is prose.** A target row
that is disabled for want of a pairing reads
`— pair in the Accounts view to include` (`BatchView.svelte:349`), and the
account-write warning ends `pair them in the Accounts view to see them by name`
(`:384`). Neither is a control. Nothing can open Accounts from inside Copy
settings, so the stack has no way to form.

**Those two stay as prose.** Turning them into buttons would demand a stack (or
a swap that silently discards a half-built copy plan, since the form is
component-local `$state`), and would buy a shortcut for a case where the user's
next action is "and now come back and rebuild my selection" anyway. No
functionality is lost: Accounts is one dismissal and one click away, exactly as
it is today.

If Phase 5's palette can open one sheet while another is open, it **replaces**
rather than stacks: set `sheet` to the new value in one assignment. Focus
restore then targets the palette's caller, which is correct.

Being a single variable also makes `Esc` unambiguous (there is only one thing it
can close) and gives the focus-restore logic exactly one saved element to hold.

### 6.4 Entry points

Phase 3 changes **no entry points at all.** The two sidebar buttons stay where
they are (`Sidebar.svelte:127-130`); the four in-view nudges keep their labels
and their positions. Only what the callbacks *do* changes. This is deliberate:
Phase 2 moves those buttons into the app menu and Phase 5 adds palette commands
for them, and Phase 3 must be shippable whether or not either has landed.

---

## 7 Cross-talk with the editor behind

Both sheets change data the editor is showing. Previously that happened while
the editor was hidden and was reconciled, if at all, on the way back in. Now the
editor is on screen while it happens, so each path needs a stated answer.

### 7.1 Accounts → the editor

**This already works, and the sheet makes it visible.** The effect at
`+page.svelte:165-169`:

```ts
$effect(() => {
  const o = slots.char;
  void accountsStore.roster;                // track roster changes
  if (o?.status === "opened" && slots.user === null) void reconcileUserSlot(o);
});
```

lives in the page's script, not in the markup branch, so it has always run while
the takeover was on screen. Pairing the open character in Accounts updates the
shared roster rune (`accounts.svelte.ts:27-29`), the effect fires,
`reconcileUserSlot` (`:327-347`) opens the paired account file into the user
slot, and the account-scoped views become available.

What changes is that the user can now **see** it: the Overview / Autofill /
Keybinds / Probes tabs light up behind the sheet the instant the pairing is
confirmed, because their availability is derived from
`slots.user?.status === "opened"` (`+page.svelte:531-534`, and the
`viewAvailable` predicate at `:85-91`). No code is needed to make that happen;
it needs only to not be hidden. Same for the subject's name in the context bar:
`openUserAlias` (`:110-114`) derives through `aliasFor()`, which reads the
roster rune, so renaming an account in the sheet renames the subject behind it
live.

Two behaviours to record rather than change:

- **Unpairing does not clear the user slot**, because the effect is guarded on
  `slots.user === null` (`:168`). The account file stays open and editable,
  which is correct — it is still a real file — and the shared-scope banner
  updates anyway, because `sharedNames` (`:153-159`) recomputes from the roster.
- **Pairing a character that is not the open one** changes nothing in the
  editor, correctly.

### 7.2 Copy settings → the editor

A batch apply writes files on disk, behind the in-memory documents. Three parts.

**(a) The warning must cover both slots and the planned account writes.**
`targetsOpenFile` today is:

```ts
const targetsOpenFile = $derived(openPath !== null && effectiveTargets.includes(openPath));
```
(`BatchView.svelte:172`)

It compares one path — the tab-derived active slot — against the *character*
targets only. It therefore misses two real cases:

- The open **account** file being written. Account writes are computed by the
  backend and returned as `plan.account_writes[]`, each carrying its own `path`
  (rendered at `:383-385`; the shape is pinned in `BatchView.spec.ts:277-280`).
  A copy of Keybinds onto a paired sibling writes the account file — which may
  be the very one open in the user slot — and nothing warns.
- The open character file when the user is sitting on the Autofill / Keybinds /
  Probes tab, where `active` is `"user"` and `openPath` is therefore not the
  character's path at all.

Replace with:

```ts
// Every path this run will write, character targets and backend-planned
// account writes alike, intersected with BOTH open slots. Applying onto an
// open document leaves the on-screen copy stale, and until now only one of
// the two slots — the one the current editor tab happened to select — was
// checked at all.
const willWrite = $derived(
  fileMode ? effectiveTargets
           : [...effectiveTargets, ...(plan?.account_writes ?? []).map((w) => w.path)],
);
const openTargets = $derived(
  [openCharPath, openUserPath].filter((p): p is string => p !== null && willWrite.includes(p)),
);
```

The warning text names which file, rather than saying "one target":

> ⚠ This will rewrite **Baguette Commander**, open in the editor behind this
> sheet. Its on-screen copy will be out of date afterwards.

The name comes from the same `nameOf` helper the target rows use
(`BatchView.svelte:193-199`) — "name people, not files", per the redesign's copy
rules. The account row says "the **stormdelay2** account file". Keep the second
sentence of today's warning ("reload it before editing further, or your next
save will collide with what this wrote") only for the dirty case; §7.2(c) makes
it untrue for the clean one.

**(b) The editor reloads itself after a clean apply.** `BatchView` gains
`onApplied(writtenPaths: string[])`, called after `apply()` succeeds
(`BatchView.svelte:248-263`) with the `path` of every result where `ok` is true.
The shell handles it:

```ts
// A batch copy writes behind the open documents. Re-read any slot it wrote —
// the same api.open + savedAt bump that openFile() and discardChanges() use,
// so every projection-based view (layout, overview, keybinds, autofill,
// probes, backups) refreshes through the mechanism it already has.
async function onBatchApplied(written: string[]) { … }
```

For each slot whose path is in `written`: if the slot is **clean**, re-open it
with `api.open(slot, path)`, leave `dirtySlots[slot]` false, and bump `savedAt`
(`+page.svelte:80`) — the existing refresh token that `BackupsPanel` and
`LayoutView` already watch (`:551, 649`). Nothing new is invented.

**(c) A dirty slot is never reloaded.** Re-reading a slot with unsaved edits
would destroy them, and no amount of warning makes a silent discard acceptable.
Instead the shell raises a persistent `InlineMessage` in the save cluster:

> The account file was rewritten on disk by Copy settings. Your unsaved edits
> are still here — saving will overwrite what was just copied. Discard to take
> the copied version instead.

Both routes out already exist and both are correct: `Discard` re-reads both
files from disk (`+page.svelte:219-237`), and `Save` hits the backend's
changed-on-disk check and offers the overwrite confirmation
(`:434-448`). The message only moves the discovery forward from "two steps
later, at save time" to "now", which is precisely the improvement the existing
`targetsOpenFile` warning was written to make (`BatchView.svelte:168-172`) and
could only make prospectively.

**(d) The roster refreshes too.** `BatchView` already calls `loadRoster()` on
mount (`:11`); nothing it does changes pairings, so no further work.

---

## 8 File-by-file change list

| File | Change |
| --- | --- |
| `app/src/lib/ui/Sheet.svelte` *(Phase 1)* | Consumed. Needs the three additions in §3.6: `width` variant, `actions` header snippet, `--shell-inset-*` positioning. |
| `app/src/routes/+page.svelte` | `mainView` (`:30`) → `sheet: "accounts" \| "batch" \| null`. Retarget six assignments (`:485, 486, 572, 582, 591, 592, 601`) and the two resets (`:262, 300`). Delete the `{#if}/{:else if}/{:else}` chain (`:496-500`) and its `{/if}` (`:678`); the editor, backups panel and insert modal become unconditional. Render the two views as fixed-position siblings. Pass `openCharPath` / `openUserPath` to `BatchView` and `onClose` to both. Add `onBatchApplied`. Suppress `Ctrl+F` while `sheet !== null` (`:469-476`). |
| `app/src/lib/AccountsView.svelte` | Wrap in `<Sheet title="Accounts">`. Delete `.accounts-head` (`:104-110`); its buttons become the sheet's `actions`. Read capture state from the store instead of local `$state` (`:48-50`). Guard `startCapture` against re-entry (`:67-71`). Cancel (`:119`) and the paired-successfully branch (`:79-80`) call `endCapture()` (§4.4.5); no other branch does. Drop `.accounts`'s own `padding`/`overflow` (`:182`) — the sheet owns both. Scope from `slots.char ?? slots.user` (§4.2). |
| `app/src/lib/BatchView.svelte` | Wrap in `<Sheet title="Copy settings" width="wide">`; the dynamic `<h2>` (`:267`) becomes a subtitle. `openPath` → `openCharPath` + `openUserPath`. Replace `targetsOpenFile` (`:172`) with `willWrite` / `openTargets` and name the file in the warning (`:357-363`). Add `onApplied`. Tokenise the three hardcoded colours (`:425-427`). |
| `app/src/lib/accounts.svelte.ts` | Add the `captureState` rune (§4.3) and `endCapture()` (§4.4.5) beside it, ~14 lines with their comments. |
| `app/src/lib/api.ts` | One line: `clearCapture: () => invoke<void>("clear_capture")`, after `resolveCapture` (`:431`). |
| `app/src-tauri/src/ops.rs` | Add `clear_capture` (§4.4.1) beside `begin_capture` (`:383`). Give `resolve_capture` (`:395-403`) an early return when the slot is `None` instead of `unwrap_or_default()` (`:396`) — §4.4.2. |
| `app/src-tauri/src/lib.rs` | The `#[tauri::command] clear_capture` one-liner beside its two siblings (`:178-186`), and its name added to the capture line of `generate_handler!` (`:623`). |
| `app/src-tauri/src/accounts.rs` | One word: `Default` into `CaptureResult`'s derive (`:157`), for §4.4.2's early return. |
| `app/src/app.css` | No change. `.overlay` / `.modal` (`:101-104`) stay for the insert form until Phase 1 retires them. |
| `app/src/lib/Sidebar.svelte` | **No change.** The buttons already call `onShowAccounts` / `onShowBatch`. |
| `AutofillView` / `KeybindsView` / `OverviewView` / `ProbeFormationsView` | **No change.** Their nudges already call the same callbacks. |

Nothing is deleted from any view's feature set. The only markup removed anywhere
is `AccountsView`'s own header, whose contents move up into the sheet frame.

The `clear_capture` fix (§4.4) is the four backend/api rows plus two lines of
`AccountsView` and one function in `accounts.svelte.ts`. It shares no line with
the sheet conversion and can be committed and shipped on its own.

---

## 9 Tests

Frontend: Vitest + jsdom, `*.spec.ts`, following `BatchView.spec.ts`'s house
pattern (stub `invoke` through `calls`, drive with `@testing-library/svelte`,
assert on the IPC shape rather than on internals). Backend: one plain `#[test]`
in the module `ops.rs` already has.

### `app/src/routes/page.spec.ts` — the shell rules

This is the only place the sheet's shell-level contract can be pinned, which is
the same argument the file's own header makes (`page.spec.ts:1-7`). Add a
`describe("sheets")`:

1. **`the editor stays mounted while a sheet is open`** — open a character, open
   Accounts from the sidebar, assert the view tabs and the file bar are still in
   the document. This is the anti-regression for the whole phase.
2. **`Save and the unsaved count stay visible with a sheet open`** — open a
   character, make it dirty, open Accounts, assert `save()` (the existing helper
   at `page.spec.ts:88`) is still found and still enabled. **This test is the
   CRITICAL fault**; write it first and watch it fail against `master`.
3. **`Esc closes the sheet and returns to the editor`**.
4. **`the close button closes the sheet`**.
5. **`the view tab survives a sheet round-trip`** — select Overview, open Copy
   settings, close it, assert Overview is still the active tab. Fails on
   `master`, where the only way back re-opens the file.
6. **`opening a file closes an open sheet`** — pins the retained `:262` reset.
7. **`a batch apply that wrote the open file re-reads it`** — stub
   `setup_apply` returning the open char path with `ok: true`, assert a second
   `open_file` call for that slot and no third.
8. **`a batch apply never re-reads a dirty slot`** — same, with the slot dirty;
   assert exactly one `open_file` call in the whole run and that the warning
   text is on screen.

### `app/src/lib/AccountsView.spec.ts` — new file

There is no test for this view today, and the capture flow is the one part of
this phase with a failure mode that is silent.

1. **`a capture survives the sheet being dismissed and reopened`** — start a
   capture, unmount, remount, assert the wizard is still showing, that
   `begin_capture` has been called exactly **once**, and `calls.never
   ("clear_capture")`. This is the test that catches the re-baseline bug
   described in §4.3, and the last assertion is what stops a later
   well-meaning "clean up on unmount" from reintroducing it (§4.4.4).
2. **`Calibrate cannot re-baseline a capture already in flight`** —
   `calls.never` a second `begin_capture` on a second press.
3. **`Cancel discards the backend baseline`** — start a capture, press Cancel,
   assert exactly one `clear_capture` call and that the wizard is gone. Fails on
   `master`, where the command does not exist and Cancel touches only the flag.
4. **`Done pairs the detected character and clears the wizard`** — stub
   `resolve_capture` with a `detected` pair; assert `confirm_pairing`, one
   `clear_capture` after it, and that the note names the character.
5. **`a retryable capture keeps its baseline`** — stub `resolve_capture` with
   two `changed_users`, press Done, assert the wizard is still rendered
   (`AccountsView.svelte:90`), the message explains why, and
   `calls.never("clear_capture")`. One test for §4.4.4's two retry rows.
6. **`the sheet is labelled and dismissable`** — `role="dialog"`,
   accessible name "Accounts", `Esc` calls `onClose`.

Note for the implementer: the capture panel currently carries its own
`role="dialog"` (`AccountsView.svelte:113`). Inside a sheet that is a dialog
within a dialog and must be dropped — it becomes a plain `Panel` with a heading.

### `app/src-tauri/src/ops.rs` — `clear_capture`

One `#[test]` in the existing `mod tests` (`ops.rs:962`), beside the two capture
tests at `:1180-1200` and built the same way — a temp discovery tree with one
char and one user file:

```rust
#[test]
fn clear_capture_discards_the_baseline_and_resolve_then_detects_nothing() {
    let root = std::env::temp_dir().join(format!("app-cap-clear-{}", std::process::id()));
    …                                       // same tree setup as :1182-1189
    let state = AppState::new();
    begin_capture(&state, std::slice::from_ref(&root));
    assert!(state.capture.lock().unwrap().is_some());

    clear_capture(&state);
    assert!(state.capture.lock().unwrap().is_none(), "the slot is empty");

    // Both files advance after the discarded baseline. Without §4.4.2's early
    // return this reports a confident false pairing, because `capture_diff`
    // counts every path the baseline does not hold as "appeared".
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(&cf, b"xy").unwrap();
    fs::write(&uf, b"xy").unwrap();

    let r = resolve_capture(&state, &[root]);
    assert_eq!(r.detected, None);
    assert!(r.changed_chars.is_empty() && r.changed_users.is_empty());
}
```

The two assertions are the two halves of the fix: the first pins `clear_capture`
itself, the last three pin §4.4.2. Write the last three first and watch them
fail against a `clear_capture` that ships without the early return — that is the
only way this fix can be got half right.

### `app/src/lib/BatchView.spec.ts` — extend

Every existing test must keep passing with the prop rename; the only mechanical
edit is `mount(openPath)` → `mount(openCharPath)`.

1. **Rewrite `warns when a target is the file currently open` (`:294-312`)** to
   assert the warning names the character rather than saying "one target".
2. **New: `warns when the plan writes the open account file`** — stub
   `setup_preview` with an `account_writes` entry whose `path` is the open user
   file, tick no target that is the open char file, assert the warning appears.
   Fails on `master`: `targetsOpenFile` cannot see account writes at all.
3. **New: `apply reports every written path to the shell`** — assert `onApplied`
   receives exactly the `ok` results' paths, and is not called when `apply`
   throws.
4. **New: `the sheet stays open after a successful apply`** — assert the results
   list is rendered and `onClose` was not called.

---

## 10 Risks & rollback

**The scrim removes today's accidental escape.** Clicking a sidebar file is
currently the only way out of a takeover; the scrim makes it inert. That is the
intent — the escape cost the user their editor state — but it is a behaviour
change for anyone who learned it. Mitigation: three dismissals (`Esc`, the close
button, the scrim itself), and the scrim click lands on the gesture users have
already been taught by `AboutPanel` (`AboutPanel.svelte:21`).

**`aria-modal="true"` makes Save unclickable while a sheet is open.** Accepted
and argued in §3.2. `Ctrl+S` remains, and it is more than today offers.

**The auto-reload in §7.2(b) could surprise.** It only fires for a slot that is
both written and clean, so it can destroy nothing. The dirty path never reloads.

**A fourth child in the `.layout` grid would break the layout.** Guarded by
fixed positioning (§3.1) and worth an explicit reviewer check, because it is the
one way this diff could go visibly wrong.

**Phase 1's `Sheet` may not exist yet.** Phase 3 is blocked on the primitive, not
on Phase 2 — the `--shell-inset-*` fallbacks degrade to a full-window sheet
(§3.6), which is still closable and still keeps the editor mounted, so the
critical fault is fixed either way.

**`clear_capture` is the phase's only backend change, and it is additive.** One
new command that writes `None` into a `Mutex`; no existing command's signature,
shape or behaviour moves, with the single exception of `resolve_capture`
returning an empty result instead of a false-positive one when no baseline is
set (§4.4.2) — a state that is unreachable today and only becomes reachable
because of this fix. Nothing persists, so there is no format to migrate and
nothing to migrate back.

**Rollback** is a single revert, or two. `mainView` is one variable and the two
views' bodies are structurally unchanged; no IPC shape and no persisted format
is touched by this phase. The one piece of new shared state (`captureState`) is
additive and inert if unused. Because §4.4 shares no line with the sheet
conversion (§8), it can be reverted separately in either direction: the sheets
without the fix behave as `master` does today, and the fix without the sheets is
a strictly better Cancel button.

---

## 11 Definition of done

- [ ] `mainView` is gone from `+page.svelte`; `sheet` replaces it and the
      editor renders unconditionally.
- [ ] Accounts and Copy settings both open as sheets with the context bar, the
      save cluster and the subject browser visible behind them.
- [ ] Both close by `Esc`, by a visible close button, and by the scrim.
- [ ] Focus moves into the sheet on open and returns to the invoking control on
      close.
- [ ] `role="dialog"`, `aria-modal="true"` and an accessible name on each sheet;
      the nested `role="dialog"` on the capture panel is gone.
- [ ] With unsaved edits, opening either sheet leaves Save and the unsaved count
      visible and readable, and `Ctrl+S` still saves.
- [ ] Closing a sheet leaves the view tab, the canvas selection, the tree
      search, the tree file switch and the scroll position exactly as they were
      — verified by hand as well as by test, since scroll is not asserted.
- [ ] A capture started in the Accounts sheet survives dismissal; reopening
      shows it still in flight; `begin_capture` fires once per capture.
- [ ] `clear_capture` exists, is registered in `generate_handler!`, and is
      wrapped in `api.ts`; `AppState.capture` is empty after it runs.
- [ ] Cancel and a successfully-confirmed Done each call it exactly once;
      `Esc`, the close button, the scrim, and opening a document call it
      **never**, and a capture survives all four with its baseline intact.
- [ ] `resolve_capture` with no baseline reports nothing detected rather than
      diffing against an empty snapshot and inventing a pairing.
- [ ] Pairing in the Accounts sheet visibly enables the account-scoped tabs
      behind it, and renaming an account renames the subject in the context bar.
- [ ] Copy settings warns, by name, when a run will write either open document —
      character target or planned account write.
- [ ] After an apply, a written-and-clean slot is re-read; a written-and-dirty
      slot is not, and says so.
- [ ] Every existing test passes; the new tests in §9 pass; the two marked as
      failing on `master` were seen to fail before the fix.
- [ ] No feature listed in the redesign's parity table has moved or vanished:
      alias editing, pair, unpair, the three slots, calibrate, refresh, the
      unassigned list, profile selection, all three source kinds, aspects,
      target select-all, the plan preview and its warnings, apply, and the
      results list are all present and reachable.
