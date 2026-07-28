# Overview Filter List Speed Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the overview preset-contents checklist rendering 649 live checkboxes at all times, so opening the Filters tab and ticking a group stops dragging.

**Architecture:** Two changes in one component. A category's checkbox rows are rendered only while its `<details>` is actually open — native `<details>` hides its children but Svelte still builds and reactively tracks every one of them, which is where the cost is. Then the filter box is debounced so a broad query re-renders the expanded matches once per pause instead of once per keystroke. No backend change, no new dependency, no virtualisation.

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest + Testing Library for components (`*.spec.ts`).

## Global Constraints

- **No new dependencies.** No virtual-list library. The whole point is that native `<details>` already provides the toggle.
- **Do not change what the checklist does** — which groups are shown, how filtering matches, or what a click writes. This is a rendering change only; every existing behaviour must survive.
- **Pure logic goes in `.test.ts` (node --test), component behaviour in `.spec.ts` (Vitest).** This work is component behaviour, so it is a `.spec.ts`.
- Run the full suite with `npm test` from `app/` — it runs both runners. Type-check with `npm run check` from `app/`.
- Commit after each task.

---

## Background: the ledger's numbers were wrong, and it matters

The ledger entry says the tab renders "every one of the **1,605** groups in `overview-groups.json`". That is not what happens, and the real shape changes which fix is right.

`overview-groups.json` has two different things in it:

| Field | Count | What it is |
|---|---|---|
| `all_group_ids` | 1,605 | Every group id known to the ESI sync — used only to ask the backend for additions |
| `categories[].groups` | **649** | The tree actually rendered as checkboxes, across 15 categories |

So the checklist paints 649 rows, not 1,605. It is still far too many, and the distribution is the interesting part:

```
Entity 400   Celestial 53   Ship 50   Asteroid 48   Starbase 26   Deployable 19
Structure 15  Drone 13  Charge 8  Fighter 6  Sovereignty 4  Orbitals 3
Planetary Industry 2   Commodity 1   Station 1
```

**One category holds 62% of the rows.** That is why "render only what is open" is the right lever: with everything collapsed the tab renders zero rows instead of 649, and a user who opens `Ship` pays for 50, not 649. Only someone who deliberately opens `Entity` pays for 400 — and they asked for it.

This also settles the "measure before picking one" note in the ledger. The three costs it lists are not equal:

1. **Filter re-runs `filterCatalog` per keystroke.** 649 `String.includes` calls is microseconds — this is *not* a real cost. The real cost on the same keystroke is `open={!!groupFilter.trim()}` force-expanding every matching category, so the DOM materialises. Task 2 debounces the input for that reason, not because the filter function is slow.
2. **Every toggle is a backend round trip that replaces `data`.** Real, and unavoidable — the backend owns the document. But it invalidates `presetGroupSet`, which re-evaluates a `checked` expression for every rendered row. Task 1 shrinks "every rendered row" from 649 to whatever is open, which is the whole fix for this cost too.
3. **`presetGroupSet` is rebuilt from scratch each time.** Building a `Set` from a preset's group list (tens to a few hundred ids) is negligible. Leave it alone.

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `app/src/lib/OverviewFiltersTab.svelte` | The preset-contents checklist | Modify: per-category open state; render rows only when open; debounce the filter |
| `app/src/lib/OverviewFiltersTab.spec.ts` | Component tests for the above | Create |
| `docs/small-tasks.md` | The ledger | Modify: close the entry, with the 1,605 → 649 correction |

---

### Task 1: Render a category's rows only while it is open

**Files:**
- Modify: `app/src/lib/OverviewFiltersTab.svelte:38` (state), `:207-220` (the category loop)
- Test: `app/src/lib/OverviewFiltersTab.spec.ts` (create)

**Interfaces:**
- Consumes: `filterCatalog`, `mergeCatalog`, `type Category` from `./groups` (unchanged); `OverviewColumns` from `./api`.
- Produces: nothing other tasks depend on.

- [ ] **Step 1: Write the failing test**

Create `app/src/lib/OverviewFiltersTab.spec.ts`. Two things the fixture has to get right or the checklist does not render at all:

- The component fires `sync_group_catalog` on mount. Stub it with `[]` so the catalog settles to the bundled tree. (Unstubbed it resolves `undefined`, `mergeCatalog` throws on it, and the `.catch` falls back to the same tree — it works, but by accident. Stub it.)
- The checklist renders only when `editable`, i.e. the tab's preset resolves to a **stored** preset or a **bundled default**. Give it a stored one. Do *not* use `preset: "All"` — "All" is only the display *label*; the real legacy keys are `defaultall`, `defaultpvp` and friends, so `"All"` resolves to neither and the whole block stays unrendered. A stored preset also keeps the test independent of what is in `default-presets.json`.

```ts
// Component test: run with `npm run test:ui` (vitest + jsdom).
//
// The checklist renders 649 group checkboxes across 15 categories, 400 of them
// in `Entity` alone. Every one is a live reactive `checked` expression, so the
// backend round trip behind each tick re-evaluated all 649 for a one-bit
// change. Collapsed categories must therefore cost nothing at all — a
// `<details>` hides its children but Svelte still builds and tracks them.
import { describe, expect, test } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import OverviewFiltersTab from "$lib/OverviewFiltersTab.svelte";
import { calls } from "$lib/test/setup";
import type { OverviewColumns } from "$lib/api";

const data: OverviewColumns = {
  tabs: [{ index: 0, name: "Default", preset: "Mine", inherits: false, columns: [] }],
  windows: [{ index: 0, tab_indices: [0] }],
  // A stored preset with no groups: the checklist is editable, and nothing is
  // pre-ticked, so a row count is a clean count of what got rendered.
  presets: [{ name: "Mine", groups: [], filtered_states: [], always_shown_states: [] }],
  appearance: {
    background: { enabled: [], order: [] },
    flag: { enabled: [], order: [] },
    colors: [],
    bools: [],
    defaulted: false,
  },
};

const noop = () => {};

function mount() {
  calls.stub("sync_group_catalog", []);
  render(OverviewFiltersTab, { data, tabIndex: 0, onChanged: noop, onUserDirty: noop });
}

/// Group rows only — the Exceptions block below uses radios, not checkboxes.
const groupBoxes = () => document.querySelectorAll(".group-grid input[type='checkbox']");

const categoryNamed = (name: string): HTMLElement => {
  const s = [...document.querySelectorAll(".group-cat summary")].find((e) => e.textContent?.trim() === name);
  if (!s) throw new Error(`no category summary "${name}"`);
  return s as HTMLElement;
};

describe("the group checklist", () => {
  test("renders no checkbox rows while every category is collapsed", async () => {
    mount();
    // The categories themselves must still be listed — this is about their rows.
    await waitFor(() => expect(categoryNamed("Ship")).toBeTruthy());
    expect(groupBoxes().length).toBe(0);
  });

  test("opening one category renders that category's rows and no others", async () => {
    mount();
    await waitFor(() => expect(categoryNamed("Ship")).toBeTruthy());

    const ship = categoryNamed("Ship").closest("details") as HTMLDetailsElement;
    ship.open = true;
    await fireEvent(ship, new Event("toggle"));

    // Ship holds 50 of the catalog's 649 groups; Entity's 400 stay unrendered.
    await waitFor(() => expect(groupBoxes().length).toBe(50));
  });

  test("closing it again releases the rows", async () => {
    mount();
    await waitFor(() => expect(categoryNamed("Ship")).toBeTruthy());
    const ship = categoryNamed("Ship").closest("details") as HTMLDetailsElement;

    ship.open = true;
    await fireEvent(ship, new Event("toggle"));
    await waitFor(() => expect(groupBoxes().length).toBe(50));

    ship.open = false;
    await fireEvent(ship, new Event("toggle"));
    await waitFor(() => expect(groupBoxes().length).toBe(0));
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `npx vitest run src/lib/OverviewFiltersTab.spec.ts`
Expected: FAIL on the first test — 649 checkboxes are rendered while everything is collapsed.

**Note:** run this from `app/`, never the repo root. The root has its own vitest, and it resolves none of the `$lib` aliases.

- [ ] **Step 3: Write the implementation**

In `app/src/lib/OverviewFiltersTab.svelte`, add beside the existing `groupFilter` state (line 38):

```ts
  // Which categories are expanded. A `<details>` hides its children but Svelte
  // still builds every one and tracks it reactively, so 649 checkboxes stayed
  // live at all times — and each backend round trip behind a tick re-evaluated
  // all of them. Rows are rendered only while their category is open.
  //
  // Unset means "follow the filter": a query expands its matches, which is what
  // makes filtering useful. Once the user toggles a category by hand that choice
  // sticks, so a deliberately-collapsed Entity (400 rows) stays collapsed even
  // while a broad query matches it.
  let openCats = $state<Record<number, boolean>>({});
  const isOpen = (id: number) => openCats[id] ?? !!groupFilter.trim();
```

Then replace the category loop (lines 207-220) with:

```svelte
        {#each visibleCategories as cat (cat.id)}
          <details
            class="group-cat"
            open={isOpen(cat.id)}
            ontoggle={(e) => (openCats[cat.id] = (e.currentTarget as HTMLDetailsElement).open)}>
            <summary>{cat.name}</summary>
            {#if isOpen(cat.id)}
              <div class="group-grid">
                {#each cat.groups as g (g.id)}
                  <label class="group-item">
                    <input type="checkbox" checked={presetGroupSet.has(g.id)}
                           onchange={(e) => setPresetGroup(g.id, (e.currentTarget as HTMLInputElement).checked)} />
                    {g.name}
                  </label>
                {/each}
              </div>
            {/if}
          </details>
        {/each}
```

- [ ] **Step 4: Run the test to verify it passes**

Run from `app/`: `npx vitest run src/lib/OverviewFiltersTab.spec.ts`
Expected: PASS, all three.

- [ ] **Step 5: Run the full suite and type-check**

Run from `app/`: `npm test` then `npm run check`
Expected: PASS, and 0 type errors. (Four pre-existing `state_referenced_locally` warnings in `ContextMenu.svelte`, `InsertForm.svelte` and `TreeNode.svelte` are unrelated; the count must not grow.)

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/OverviewFiltersTab.svelte app/src/lib/OverviewFiltersTab.spec.ts
git commit -m "Render a group category's checkboxes only while it is open"
```

---

### Task 2: Debounce the group filter

**Files:**
- Modify: `app/src/lib/OverviewFiltersTab.svelte:38` (state) and `:193` (the input binding)
- Test: `app/src/lib/OverviewFiltersTab.spec.ts`

**Interfaces:**
- Consumes: `isOpen` / `openCats` from Task 1.
- Produces: nothing other tasks depend on.

With Task 1 in place a keystroke costs only the *expanded matches*, but a one-character query like `a` still matches across most categories and expands all of them, on every keystroke. A short debounce collapses a burst of typing into one render.

- [ ] **Step 1: Write the failing test**

Add to `app/src/lib/OverviewFiltersTab.spec.ts`:

```ts
describe("the group filter", () => {
  test("does not expand matches until typing pauses", async () => {
    vi.useFakeTimers();
    try {
      mount();
      await vi.advanceTimersByTimeAsync(0);
      const box = document.querySelector(".group-filter") as HTMLInputElement;

      await fireEvent.input(box, { target: { value: "vexor" } });
      // Mid-burst: nothing has expanded yet.
      expect(groupBoxes().length).toBe(0);

      await vi.advanceTimersByTimeAsync(200);
      expect(groupBoxes().length).toBeGreaterThan(0);
    } finally {
      vi.useRealTimers();
    }
  });
});
```

Add `vi` to the existing vitest import at the top of the file.

- [ ] **Step 2: Run the test to verify it fails**

Run from `app/`: `npx vitest run src/lib/OverviewFiltersTab.spec.ts`
Expected: FAIL — the match expands on the keystroke itself, so the mid-burst assertion sees rows already.

- [ ] **Step 3: Write the implementation**

In `app/src/lib/OverviewFiltersTab.svelte`, replace the single `groupFilter` declaration (line 38) with the typed-vs-applied pair:

```ts
  // What the box shows, and what the list actually filters on. They are separate
  // because applying a query expands every category it matches, and doing that
  // per keystroke re-renders the whole expanded set while the user is still
  // typing. 150ms is below the threshold where a pause feels like lag.
  let typedFilter = $state("");
  let groupFilter = $state("");
  $effect(() => {
    const next = typedFilter;
    const t = setTimeout(() => (groupFilter = next), 150);
    return () => clearTimeout(t);
  });
```

And bind the input (line 193) to the typed value rather than the applied one:

```svelte
          <input class="group-filter" type="text" placeholder="Filter groups…" bind:value={typedFilter} />
```

- [ ] **Step 4: Run the test to verify it passes**

Run from `app/`: `npx vitest run src/lib/OverviewFiltersTab.spec.ts`
Expected: PASS, all four.

- [ ] **Step 5: Run the full suite and type-check**

Run from `app/`: `npm test` then `npm run check`
Expected: PASS, 0 type errors.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/OverviewFiltersTab.svelte app/src/lib/OverviewFiltersTab.spec.ts
git commit -m "Debounce the group filter so a burst of typing renders once"
```

---

### Task 3: Close the ledger entry

**Files:**
- Modify: `docs/small-tasks.md` (the "The overview filter list is slow" entry)

**Interfaces:** none — documentation only.

- [ ] **Step 1: Move the entry to Shipped**

Delete the `- [ ] **The overview filter list is slow.**` entry from the **Open** section and add under `### Unreleased (on master)` in the **Shipped** section:

```markdown
- [x] **The overview filter list is slow.** Categories now render their checkbox
  rows only while open, and the filter box is debounced 150ms. The entry's count
  was wrong in a way that mattered: the tab rendered **649** rows across 15
  categories, not 1,605 — `all_group_ids` (1,605) is the ESI sync list, not the
  rendered tree. 400 of the 649 sit in `Entity` alone, which is why "render only
  what is open" was the right lever: a collapsed tab now costs zero rows, and
  only someone who deliberately opens `Entity` pays for 400. Of the three costs
  listed, (1) was misattributed — `filterCatalog` over 649 strings is
  microseconds; the cost on that keystroke was the force-expand — and (3)
  (`presetGroupSet` rebuilds) was never worth touching. (2), the round trip per
  tick, is unavoidable but now re-evaluates only rendered rows. Virtualisation
  stayed unneeded. _Added 2026-07-27; done 2026-07-28._
```

- [ ] **Step 2: Commit**

```bash
git add docs/small-tasks.md
git commit -m "Close the filter-list speed task, and correct its row count"
```

---

## Self-review notes

- **Coverage.** The ledger named three costs and a recommended fix ("keep categories collapsed and render each one's rows only when open… and debounce the filter"). Task 1 is the first half, Task 2 the second. Costs (1) and (3) are answered in the plan's background with measurements rather than code, because both turned out to be misattributed.
- **Why no virtualisation.** The ledger offers it as "the bigger hammer if that is not enough". With everything collapsed the tab renders 0 rows and the worst single category is 400 — a size the app already renders elsewhere without complaint. Reach for it only if `Entity` alone still drags.
- **The one wart, deliberately.** A category the user has explicitly collapsed stays collapsed even when a later query matches it. The alternative — letting a filter override an explicit collapse — takes away the only way to keep `Entity`'s 400 rows out of a broad search, which is the case that hurts most. Commented in the source.
- **Naming.** `openCats` / `isOpen` (Task 1), `typedFilter` vs `groupFilter` (Task 2) — the second pair is named for which one the *list* uses, since that is the one every existing `$derived` already reads and none of them change.
- **Not in scope, deliberately.** The per-tick backend round trip, the Exceptions list below the groups, and the catalog sync on mount. None was reported as slow.
