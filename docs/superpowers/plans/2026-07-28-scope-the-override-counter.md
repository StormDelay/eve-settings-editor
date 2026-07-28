# Scope the Clutter-Override Counter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the layout view's "· N overridden · clear" counter describe the layout you are looking at, so it stops reporting overrides that apply to nobody on screen and stops offering a `clear` that wipes another character's.

**Architecture:** `overrideCount()` and `clearClutterOverrides()` become functions of the open document's window ids instead of the whole preferences file. The stored override list stays application-wide — that is deliberate and unchanged; only what the counter reports and what `clear` removes are narrowed to the windows in front of you.

**Tech Stack:** Svelte 5 (runes), TypeScript, `node --test` for the pure logic.

## Global Constraints

- **No new dependencies. No backend change.** `prefs.rs` and the `Preferences` DTO are untouched; the override list on disk keeps its current shape and remains app-wide.
- **A `clear` must never remove an override for a window that is not in the open document.** That is the data-loss half of this task.
- Run the frontend suite with `npm test` from `app/`, type-check with `npm run check` from `app/`. **Never run vitest from the repo root.**
- Commit after each task.

---

## Background: what "scoped" means here, and what it does not

`prefs.svelte.ts:21` is:

```ts
export const overrideCount = () => prefs.layout.clutter.length + prefs.layout.visible.length;
```

That is every override the user has ever set, across every character. `LayoutView.svelte:801-804` renders it beside "· showing N of M windows", in the same status line, so it reads as a fact about the layout on screen — and it is not. Open a character with no overrides at all and the line can still say "· 3 overridden", offering a `clear` that deletes three overrides belonging to a different character's windows.

**The decision (from the developer, 2026-07-28):** the counter should reflect *what is currently being filtered in the visible layout canvas*. The stored clutter list is application-wide, but what it is doing at any moment depends on the document you have open — it is not global state to report globally.

**Scope it to the windows in the open document, not to the windows currently drawn.** The distinction matters and the second reading is self-defeating: a `clutter` override's whole effect is to *hide* its window, so a counter over the drawn set could never count one. "The windows this document has" is the set the overrides actually act on, and it is stable while the user types in the filter box — a counter that moved as you typed would be worse than one that is too broad.

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `app/src/lib/prefs.svelte.ts` | Preference state and writes | Modify: both functions take the document's window ids |
| `app/src/lib/prefs.test.ts` | Pure logic tests | Create (or modify if it exists) |
| `app/src/lib/LayoutView.svelte` | The status line | Modify: pass the open document's ids |
| `docs/small-tasks.md` | The ledger | Modify: close item 6 of the names-and-noise entry |

---

### Task 1: Count and clear against the document's windows

**Files:**
- Modify: `app/src/lib/prefs.svelte.ts:21` (`overrideCount`) and `clearClutterOverrides`
- Test: `app/src/lib/prefs.test.ts`

**Interfaces:**
- Consumes: `prefs.layout.{clutter,visible}: string[]`, and a new caller-supplied set of window ids.
- Produces:
  - `overrideCount(ids: ReadonlySet<string>): number` — how many stored overrides name a window in `ids`.
  - `clearClutterOverrides(ids: ReadonlySet<string>): Promise<void>` — removes only those, leaving every other override on disk.

**Read `clearClutterOverrides` before changing it** — it goes through the same chained-write helper as `setClutterOverride` (see the comment above it about awaiting the previous write). Keep that mechanism exactly as it is; only the value being written changes, from "empty lists" to "the lists minus the ids in scope".

- [ ] **Step 1: Write the failing test**

`app/src/lib/prefs.test.ts` may not exist. If not, create it in the style of `app/src/lib/profiles.test.ts` — `node --test`, a throw-based `check()` helper, no framework. Note `prefs.svelte.ts` is a `.svelte.ts` module using runes; if importing it under `node --test` does not work, **stop and report** rather than forcing it, and say whether the countable logic should be extracted to a plain `.ts` first.

The behaviour to pin, whichever way it is expressed:

```ts
// Counting is about the document you are looking at, not the preferences file.
{
  const stored = { clutter: ["market", "chatchannel_corp"], visible: ["overview"] };
  const open = new Set(["market", "overview", "somethingElse"]);
  check(
    "counts only overrides naming a window this document has",
    countIn(stored, open) === 2,
  );
  check(
    "a document sharing no windows with the overrides counts none",
    countIn(stored, new Set(["unrelated"])) === 0,
  );
}

// The data-loss half: clearing must not touch another character's overrides.
{
  const stored = { clutter: ["market", "chatchannel_corp"], visible: ["overview"] };
  const next = withoutIn(stored, new Set(["market", "overview"]));
  check("clearing drops the in-scope clutter override", !next.clutter.includes("market"));
  check("clearing drops the in-scope visible override", !next.visible.includes("overview"));
  check(
    "clearing KEEPS an override for a window this document does not have",
    next.clutter.includes("chatchannel_corp"),
  );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `node --test "src/lib/prefs.test.ts"`
Expected: FAIL — the helpers do not exist.

- [ ] **Step 3: Write the implementation**

In `app/src/lib/prefs.svelte.ts`, replace `overrideCount` and narrow the clear. Keep the two pure helpers exported so the test can reach them without mounting anything:

```ts
/** Overrides naming a window the given document actually has. */
export const countIn = (
  stored: { clutter: string[]; visible: string[] },
  ids: ReadonlySet<string>,
): number =>
  stored.clutter.filter((id) => ids.has(id)).length +
  stored.visible.filter((id) => ids.has(id)).length;

/** The stored lists with every in-scope id removed, the rest untouched. */
export const withoutIn = (
  stored: { clutter: string[]; visible: string[] },
  ids: ReadonlySet<string>,
): { clutter: string[]; visible: string[] } => ({
  clutter: stored.clutter.filter((id) => !ids.has(id)),
  visible: stored.visible.filter((id) => !ids.has(id)),
});

/**
 * How many overrides are doing something in the document on screen.
 *
 * The stored list is application-wide, but what it is DOING at any moment
 * depends on the file you have open — so a global tally sat beside "showing N
 * of M windows" claiming to describe this layout while describing every
 * character's. Scoped to the open document's windows, not to the windows
 * currently drawn: a `clutter` override's whole effect is to hide its window,
 * so a count over the drawn set could never include one, and a count that moved
 * while you typed in the filter box would be worse than one that is too broad.
 */
export const overrideCount = (ids: ReadonlySet<string>): number => countIn(prefs.layout, ids);
```

And in `clearClutterOverrides`, write `withoutIn(prefs.layout, ids)` instead of empty lists, keeping the existing chained-write and error handling untouched.

- [ ] **Step 4: Run the test to verify it passes**

Run from `app/`: `node --test "src/lib/prefs.test.ts"`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/prefs.svelte.ts app/src/lib/prefs.test.ts
git commit -m "Count and clear clutter overrides against the open document"
```

---

### Task 2: Give the status line the document's windows

**Files:**
- Modify: `app/src/lib/LayoutView.svelte:801-804`

**Interfaces:**
- Consumes: `overrideCount` / `clearClutterOverrides` from Task 1; `layout.windows` (already in scope in this component — `layout` is nullable, so use the file's existing `layout?.` idiom).

- [ ] **Step 1: Pass the ids**

Add a derived set beside the other `$derived` declarations:

```ts
  // Every window this document has — NOT the filtered set. See overrideCount.
  const documentWindowIds = $derived(new Set((layout?.windows ?? []).map((w) => w.id)));
```

And use it in the status line:

```svelte
        {#if overrideCount(documentWindowIds) > 0}
          <span class="showing">
            · {overrideCount(documentWindowIds)} overridden
            <button class="linkish" onclick={() => clearClutterOverrides(documentWindowIds)}>clear</button>
          </span>
        {/if}
```

- [ ] **Step 2: Verify**

Run from `app/`: `npm test` then `npm run check`
Expected: PASS, 0 type errors. (Four pre-existing `state_referenced_locally` warnings in `ContextMenu.svelte`, `InsertForm.svelte` and `TreeNode.svelte`; the count must not grow.)

- [ ] **Step 3: Commit**

```bash
git add app/src/lib/LayoutView.svelte
git commit -m "Scope the override counter to the layout on screen"
```

---

### Task 3: Close it in the ledger

**Files:**
- Modify: `docs/small-tasks.md`

**Interfaces:** none — documentation only.

- [ ] **Step 1: Strike item 6**

In the still-open "Run the names-and-noise live in-game smoke" entry, replace item 6's text with a done note, leaving items 2-5 untouched:

```markdown
  6. ~~`overrideCount()` counts overrides across every character~~ — **done
     2026-07-28**: the counter and its `clear` are scoped to the windows the open
     document has, so the line beside "showing N of M windows" describes that
     layout rather than every character's, and `clear` can no longer remove an
     override belonging to a file you do not have open. The stored list stays
     application-wide by design; only what is reported and cleared is narrowed.
     Scoped to the document's windows rather than the drawn ones deliberately —
     a `clutter` override hides its own window, so a count over the drawn set
     could never include one.
```

- [ ] **Step 2: Commit**

```bash
git add docs/small-tasks.md
git commit -m "Close the override-counter scoping"
```

---

## Self-review notes

- **The decision was the blocker, not the code.** The entry asked to "decide from real use whether to scope it to the open layout"; the developer decided it on 2026-07-28. The change itself is small.
- **Two readings of "scoped", and why this one.** Document windows, not drawn windows — a `clutter` override removes its own window from the drawn set, so the tighter reading would make the counter structurally unable to count the most common override kind.
- **The stored list stays global on purpose.** A user who marks a window as clutter means it generally, not for one character; narrowing storage would be a different and larger change.
- **Not in scope.** The override *storage* shape, `setClutterOverride`, and the per-window context menu that writes them.
