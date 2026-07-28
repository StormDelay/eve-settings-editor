# Two UI Honesty Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the keybinds "taken by" note escaping its row, and stop the Layout aspect's label implying it carries screen furniture it leaves behind.

**Architecture:** Two unrelated, small frontend corrections from the same live session, batched because each is a few lines and both are pure presentation. No backend, no behaviour change to what is copied or written.

**Tech Stack:** Svelte 5, CSS, Vitest for the one behavioural assertion.

## Global Constraints

- **No new dependencies.** No backend change. **Task 2 must not change which fields a Layout copy carries** — that is a deliberate scope boundary, see its Background.
- Run the frontend suite with `npm test` from `app/`, type-check with `npm run check` from `app/`. **Never run vitest from the repo root** — it resolves none of the `$lib` aliases.
- Commit after each task.

---

### Task 1: Keep the "taken by" note inside its row

**Files:**
- Modify: `app/src/lib/KeybindsView.svelte` (the `.meta` span around line 129, and the `.meta` rule around line 162)

**Interfaces:**
- Consumes / produces: nothing. Presentation only.

**Background.** `KeybindsView` renders `<span class="meta">taken by {stolenFrom[e.command]}</span>` inline beside the binding button. The table is `table-layout: fixed` with `.c-combo { width: 16rem }`, and `.chip` has `min-width: 7rem`, so a long command name ("Activate High Power Slot 4") does not fit the remainder and the note escapes the row box, overlapping the row beneath.

The ledger offers two remedies: constrain and ellipsise, or let the row grow. **Take the ellipsis.** It bounds the row height deterministically whatever the command name, which a growing row does not, and the full name stays available on the `title`. A keybinding table is scanned vertically — rows of uneven height are harder to scan than a truncated note.

- [ ] **Step 1: Write the failing test**

`app/src/lib/KeybindsView.spec.ts` may not exist. If it does not, create it, following the style of `app/src/lib/OverviewFiltersTab.spec.ts` (which mounts a component with `calls.stub` for its backend calls). Read `KeybindsView.svelte`'s props and the commands it fires on mount first, and stub whatever it needs.

The assertion is that the note carries the full text on `title`, which is the half a CSS-only change cannot express:

```ts
test("a long 'taken by' note keeps the full command on its title", async () => {
  // The visible text is ellipsised to keep the row's height fixed, so the
  // untruncated name has to remain reachable — otherwise constraining the row
  // silently destroys the information it was showing.
  mountWithStolen({ SomeCommand: "Activate High Power Slot 4" });
  const note = await screen.findByText(/taken by/);
  expect(note.getAttribute("title")).toBe("Activate High Power Slot 4");
});
```

If mounting this component turns out to need more scaffolding than the assertion is worth — it takes a large projected keybinds DTO — **say so in your report and skip the spec**, doing Steps 3-5 only. Do not build a fixture larger than the change.

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `npx vitest run src/lib/KeybindsView.spec.ts`
Expected: FAIL — no `title` attribute today.

- [ ] **Step 3: Add the title**

In `app/src/lib/KeybindsView.svelte`, give the note its full text on hover:

```svelte
              {#if stolenFrom[e.command]}
                <span class="meta" title={stolenFrom[e.command]}
                  >taken by {stolenFrom[e.command]}</span>
              {/if}
```

- [ ] **Step 4: Constrain it**

Replace the `.meta` rule:

```css
  /* Ellipsised, not wrapped: the combo column is a fixed 16rem in a
     `table-layout: fixed` table, so a long command name ("Activate High Power
     Slot 4") used to spill out of the row and overlap the one beneath. The full
     name is on the `title`. */
  .meta {
    opacity: 0.7;
    font-size: 0.85em;
    margin-left: 0.5rem;
    display: inline-block;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    vertical-align: bottom;
  }
```

- [ ] **Step 5: Verify**

Run from `app/`: `npm test` then `npm run check`
Expected: PASS, 0 type errors. (Four pre-existing `state_referenced_locally` warnings in `ContextMenu.svelte`, `InsertForm.svelte` and `TreeNode.svelte`; the count must not grow.)

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/KeybindsView.svelte
git commit -m "Keep the keybinds taken-by note inside its row"
```

---

### Task 2: Say what the Layout aspect actually carries

**Files:**
- Modify: `app/src/lib/BatchView.svelte:70` (the `ASPECTS` entry for `layout`)

**Interfaces:**
- Consumes / produces: nothing. **The `key` stays `"layout"` and the copied key set is untouched** — this is a label change only.

**Background, and the scope boundary.** `Category::Layout => &[b"windows"]` copies that whole section, but the nine HUD fields live across three sections (`hud.rs:72-101`): `windows` holds `shipuialignleftoffset` and `neocomWidth`; `ui` holds `fightersDetachedPosition`, `shipuialigntop`, `detachFighterUI`, `displayFighterUI`; `notifications` holds `notification_badge_offset`. So a Layout copy moves the ship-HUD offset and neocom width and leaves the fighter UI and badge behind. Confirmed on live files: after an A1 → A2 Layout copy, `shipuialignleftoffset` matched at `0.0` while `fightersDetachedPosition` stayed at A2's own `(326, 54)`.

The ledger asks for a decision — pull the other seven in, or split a HUD aspect out — **and** to "say which in the aspect's UI label either way".

**Only the label is in scope here.** Changing what gets copied alters what a batch apply writes to other characters' files, and splitting a new aspect is a new feature; both belong on their own branch. The label is what is actively misleading today, and correcting it costs one line and no behaviour.

Current label: `"Window layout (positions, neocom buttons)"`. It does not mention the ship-HUD offset it *does* carry, nor the fighter panel and badge it does *not*.

- [ ] **Step 1: Correct the label**

In `app/src/lib/BatchView.svelte`, in the `ASPECTS` array:

```ts
    { key: "layout", label: "Window layout (positions, neocom, ship HUD — not the fighter panel or badge)", account: false },
```

Leave every other entry, and the `key`, exactly as they are. `BatchView.spec.ts` selects aspect rows by substring (`aspect("Window layout")`), so the existing tests keep matching — **verify that rather than assuming it**, and if any test matches on the full string, update it.

- [ ] **Step 2: Verify**

Run from `app/`: `npm test` then `npm run check`
Expected: PASS, 0 type errors.

- [ ] **Step 3: Commit**

```bash
git add app/src/lib/BatchView.svelte
git commit -m "Say which screen furniture the Layout aspect leaves behind"
```

---

### Task 3: Close one entry, and re-file the decision the other still needs

**Files:**
- Modify: `docs/small-tasks.md`

**Interfaces:** none — documentation only.

- [ ] **Step 1: Close the keybinds entry**

Delete the `- [ ] **The keybinds "taken by" note overflows its row.**` entry from **Open** and add under `### Unreleased (on master)`:

```markdown
- [x] **The keybinds "taken by" note overflows its row.** Ellipsised rather than
  wrapped, with the full command name on the `title`. The combo column is a fixed
  16rem in a `table-layout: fixed` table, so a growing row was the other option —
  the ellipsis bounds the row height whatever the command name, and a keybinding
  table is scanned vertically, where uneven rows cost more than a truncated note.
  _Added 2026-07-27; done 2026-07-28._
```

- [ ] **Step 2: Replace the Layout-aspect entry with the decision it still needs**

The existing entry mixes a labelling fix (now done) with a design decision (not done, and out of scope for this branch). Replace it in **Open** with just the part that remains:

```markdown
- [ ] **Decide what the Layout aspect should mean, then make it carry that.**
  `Category::Layout => &[b"windows"]` copies that section, but the nine HUD
  fields span three (`hud.rs:72–101`): `windows` has `shipuialignleftoffset` and
  `neocomWidth`, `ui` has `fightersDetachedPosition` / `shipuialigntop` /
  `detachFighterUI` / `displayFighterUI`, and `notifications` has
  `notification_badge_offset`. So a Layout copy or preset moves the ship-HUD
  offset and neocom width and leaves the fighter UI and badge behind — half
  applied, which is more confusing than carrying none. Confirmed on live files:
  after an A1 → A2 Layout copy, `shipuialignleftoffset` matched at `0.0` while
  `fightersDetachedPosition` stayed at A2's own `(326, 54)` against A1's `(0, 0)`.
  Presets share the key sets (`presets.rs:113`), so both surfaces are affected.

  Either pull the other seven in, or split a HUD aspect out. Both change what a
  batch apply writes to other characters' files, so this wants its own branch and
  a live smoke — it is the only remaining live-session finding that is a
  behaviour change rather than a correction. **The misleading half is already
  fixed**: the aspect's label now names the ship HUD it carries and the fighter
  panel and badge it does not (2026-07-28). _Added 2026-07-27._
```

- [ ] **Step 3: Commit**

```bash
git add docs/small-tasks.md
git commit -m "Close the keybinds overflow, and re-file the Layout aspect decision"
```

---

## Self-review notes

- **Why these two together.** Both are a few lines of presentation from the same live session, and neither justifies its own plan. They share no code.
- **Task 2 is deliberately half a fix.** The ledger asked for a decision *and* a label; the label is a correction, the decision is a behaviour change. Splitting them is what lets the honest label ship now on this branch while the decision gets the branch and the live smoke it needs.
- **The test in Task 1 may not be worth its scaffolding**, and the plan says so explicitly rather than forcing a large fixture for one attribute assertion. The CSS half is not meaningfully unit-testable; the `title` half is the part that carries information.
