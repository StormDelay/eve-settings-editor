# Character-centric entry point Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the tool character-centric — the sidebar lists characters, opening one auto-loads its account file, and account (`core_user`) files are edited only incidentally through a character.

**Architecture:** A frontend-only re-framing over the existing two-slot (`char`/`user`) backend. The old Character/Account header toggle is deleted; the active document becomes a `$derived` of the current view. The sidebar lists char files only. Account-scoped editors (Autofill, Overview) gate on the character being paired and carry a "shared account settings" banner. No backend, codec, or Tauri-command changes.

**Tech Stack:** SvelteKit 5 (runes), TypeScript, Tauri 2, `node --test` (zero-dep, throw-based) for pure helpers.

## Global Constraints

- Commit messages: **sentence-case, no attribution trailers** (repo convention).
- `blue-marshal` and `settings-model` crates stay **dependency-free** — not touched here anyway (frontend-only work).
- `npm`/`gh` are **not on the Bash PATH** — run `npm` commands via the PowerShell tool, from `app/`.
- Dark native controls: any new `<button>`/`<input>`/`<select>` must get explicit dark colors (`var(--bg-panel)`/`var(--fg)`/`var(--border)`) or it renders light in the WebView2.
- `npm run check` (svelte-check) must stay green — **unused CSS selectors are warnings**, so delete CSS for any markup you remove.
- Design spec: `docs/superpowers/specs/2026-07-20-character-centric-entry-point-design.md`.

---

### Task 1: `sharedWith` helper for the shared-account banner

The only new pure logic: given the open character's account and the roster, list the *other* characters an account-scoped edit also affects, mapped to display names. Lives in `overview.ts` beside the existing roster helpers; unit-tested with the existing `node --test` file.

**Files:**
- Modify: `app/src/lib/overview.ts` (add `sharedWith` after `associatedCharacters`)
- Test: `app/src/lib/overview.test.ts` (add cases + import)

**Interfaces:**
- Consumes: `associatedCharacters(userId, roster)` (already in `overview.ts`), `AccountRoster` (from `api.ts`).
- Produces: `sharedWith(userId: number | null, currentCharId: number | null, roster: AccountRoster, nameOf: (id: number) => string): string[]` — the account's characters minus `currentCharId`, each mapped through `nameOf`. Empty when `userId` is null.

- [ ] **Step 1: Write the failing test**

Add to `app/src/lib/overview.test.ts` — extend the import on line 3-9 to include `sharedWith`, then append these checks (the existing `roster` const has `user_id: 456, characters: [123, 124]`):

```ts
check(
  "sharedWith lists the account's OTHER characters by name",
  sharedWith(456, 123, roster, (id) => (id === 124 ? "Bravo" : String(id))).join(",") === "Bravo",
);
check(
  "sharedWith returns empty when the character has no known account",
  sharedWith(null, 123, roster, String).length === 0,
);
check(
  "sharedWith excludes only the current character",
  sharedWith(456, 999, roster, (id) => (id === 123 ? "A" : id === 124 ? "B" : String(id))).join(",") === "A,B",
);
```

- [ ] **Step 2: Run the test to verify it fails**

Run (PowerShell, from `app/`): `npm test`
Expected: FAIL — `sharedWith` is not exported (`SyntaxError`/`undefined is not a function`).

- [ ] **Step 3: Add the implementation**

In `app/src/lib/overview.ts`, after the `associatedCharacters` function:

```ts
// The account's OTHER characters — the ones a shared account-scoped edit (made
// through the current character) also affects — mapped to display names. Empty
// when the character has no known account. Powers the "shared account settings"
// banner (§6).
export function sharedWith(
  userId: number | null,
  currentCharId: number | null,
  roster: AccountRoster,
  nameOf: (id: number) => string,
): string[] {
  if (userId === null) return [];
  return associatedCharacters(userId, roster)
    .filter((c) => c !== currentCharId)
    .map(nameOf);
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm test`
Expected: PASS — all `sharedWith` checks print `ok - …`; existing checks still pass.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/overview.ts app/src/lib/overview.test.ts
git commit -m "Add sharedWith helper for the shared-account banner"
```

---

### Task 2: Autofill — pairing prompt + shared banner

`AutofillView` today hides behind `userOpen` and shows a generic "open an account file" hint when the account file isn't loaded. Give it the character context so an *unpaired* character shows a pairing prompt, and add the shared-settings banner above the editor. New props are defaulted so this task is independently green before `+page` wires them (Task 5).

**Files:**
- Modify: `app/src/lib/AutofillView.svelte`

**Interfaces:**
- Consumes: `charName`, `sharedLabel`, `onShowAccounts` (wired by Task 5; defaulted here).
- Produces: props `charName?: string | null`, `sharedLabel?: string`, `onShowAccounts?: () => void` for `+page` to pass.

- [ ] **Step 1: Add the new props**

Replace the props line (line 6):

```svelte
  let { userOpen, onUserDirty }: { userOpen: boolean; onUserDirty: () => void } = $props();
```

with:

```svelte
  let { userOpen, onUserDirty, charName = null, sharedLabel = "", onShowAccounts = () => {} }:
    { userOpen: boolean; onUserDirty: () => void;
      charName?: string | null; sharedLabel?: string; onShowAccounts?: () => void } = $props();
```

- [ ] **Step 2: Replace the `!userOpen` branch and add the banner**

Replace the opening of the markup block (lines 74-80, from `{#if !userOpen}` through `{:else if lists}`) with:

```svelte
{#if !userOpen}
  {#if charName}
    <div class="hint pair">
      <p>Link <strong>{charName}</strong> to an account to edit shared settings.</p>
      <button onclick={onShowAccounts}>Pair…</button>
    </div>
  {:else}
    <p class="hint">Open a character to edit its account's remembered text.</p>
  {/if}
{:else if error}
  <p class="error">{error}</p>
{:else if lists && lists.length === 0}
  <p class="hint">No remembered text in this account file yet.</p>
{:else if lists}
  {#if sharedLabel}<p class="shared-banner">{sharedLabel}</p>{/if}
```

(The `<div class="af-top">…` line that followed `{:else if lists}` stays exactly as-is, now directly after the banner line.)

- [ ] **Step 3: Add styles for the banner and pair button**

In the `<style>` block, add (the `input, button.mini, button.danger` dark rule already exists; add a plain-button + banner rule):

```css
  .shared-banner {
    margin: 0 0 0.6rem; padding: 0.3rem 0.5rem; font-size: 0.85em;
    color: var(--fg-dim); border-left: 2px solid var(--accent); background: var(--bg-panel);
  }
  .pair { display: flex; align-items: center; gap: 0.6rem; }
  .pair button {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 10px; font: inherit; cursor: pointer;
  }
```

- [ ] **Step 4: Typecheck**

Run (PowerShell, from `app/`): `npm run check`
Expected: 0 errors, 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/AutofillView.svelte
git commit -m "Show a pairing prompt and shared-account banner in Autofill"
```

---

### Task 3: Overview — shared banner + reworded pairing prompt

Add the same shared-settings banner to the account-scoped Overview editor, and align the existing "not linked" prompt wording/button with the pairing language. `sharedLabel` is defaulted so the task is standalone-green.

**Files:**
- Modify: `app/src/lib/OverviewView.svelte`

**Interfaces:**
- Consumes: `sharedLabel` (wired by Task 5; defaulted here).
- Produces: prop `sharedLabel?: string`.

- [ ] **Step 1: Add the prop**

In the props destructuring (lines 6-9), add `sharedLabel = ""` and its type. Change:

```svelte
  let { userOpen, charId, characters, refreshToken, onLoadCharacter, onUserDirty, onCharDirty, onWindowAdded, onShowAccounts }:
    { userOpen: boolean; charId: number | null; characters: number[]; refreshToken: number;
      onLoadCharacter: (id: number) => void; onUserDirty: () => void; onCharDirty: () => void;
      onWindowAdded: (windowId: string) => void; onShowAccounts: () => void } = $props();
```

to:

```svelte
  let { userOpen, charId, characters, refreshToken, onLoadCharacter, onUserDirty, onCharDirty, onWindowAdded, onShowAccounts, sharedLabel = "" }:
    { userOpen: boolean; charId: number | null; characters: number[]; refreshToken: number;
      onLoadCharacter: (id: number) => void; onUserDirty: () => void; onCharDirty: () => void;
      onWindowAdded: (windowId: string) => void; onShowAccounts: () => void; sharedLabel?: string } = $props();
```

- [ ] **Step 2: Reword the "not linked" prompt**

Replace the `!userOpen && charId !== null` branch (lines 164-169):

```svelte
{#if !userOpen && charId !== null}
  <div class="hint">
    <p>This character isn't linked to an account yet. Overview columns live in the account
      file — associate it to edit them.</p>
    <button onclick={onShowAccounts}>Open Accounts</button>
  </div>
```

with:

```svelte
{#if !userOpen && charId !== null}
  <div class="hint pair">
    <p>Link this character to an account to edit shared settings — overview columns live in the account file.</p>
    <button onclick={onShowAccounts}>Pair…</button>
  </div>
```

- [ ] **Step 3: Render the banner above the editor**

Find the `{:else if data}` branch (line 176). Insert the banner as its first child, before `<div class="ov-controls">`:

```svelte
{:else if data}
  {#if sharedLabel}<p class="shared-banner">{sharedLabel}</p>{/if}
  <div class="ov-controls">
```

- [ ] **Step 4: Add styles**

In the `<style>` block add:

```css
  .shared-banner {
    margin: 0 0 0.6rem; padding: 0.3rem 0.5rem; font-size: 0.85em;
    color: var(--fg-dim); border-left: 2px solid var(--accent); background: var(--bg-panel);
  }
  .pair { display: flex; align-items: center; gap: 0.6rem; }
  .pair button {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 10px; font: inherit; cursor: pointer;
  }
```

- [ ] **Step 5: Typecheck**

Run: `npm run check`
Expected: 0 errors, 0 warnings.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/OverviewView.svelte
git commit -m "Add the shared-account banner and pairing prompt to Overview"
```

---

### Task 4: Sidebar — list characters as the entry point

Drop the Accounts and Other file groups; render only char-kind files per profile. Keep the hide-non-standard toggle (now scoped to characters) and show each character's account alias as dim metadata. This alone makes the sidebar character-centric; the header/toggle change is Task 5.

**Files:**
- Modify: `app/src/lib/Sidebar.svelte`

**Interfaces:**
- Consumes: `accountOf` (`overview.ts`), `aliasFor` + `accountsStore` (`accounts.svelte.ts`), `resolvedName`/`byResolvedName` (`filesort.svelte.ts` — already imported), `isStandardName` (local).
- Produces: nothing new; `onOpen(path)` contract is unchanged.

- [ ] **Step 1: Add imports**

After the existing `import { loadRoster } from "./accounts.svelte";` (line 5), extend the accounts import and add `accountOf`:

```svelte
  import { loadRoster, aliasFor, accountsStore } from "./accounts.svelte";
  import { accountOf } from "./overview";
```

(Remove the now-redundant standalone `import { loadRoster } from "./accounts.svelte";` line so `loadRoster` isn't imported twice.)

- [ ] **Step 2: Replace the profile-rows markup with a character list**

Replace the whole `{#each rows as { p, label, primary } (p.dir)} … {/each}` block (lines 121-153) with:

```svelte
  {#each rows as { p, label, primary } (p.dir)}
    {@const chars = p.files
      .filter((f) => f.kind === "char" && (!hideNonStandard || isStandardName(f.file_name)))
      .sort(byName)}
    {#if chars.length > 0}
      <details open={primary}>
        <summary title={p.dir}>
          {label}
          {#if primary}<span class="meta">most recent</span>{/if}
        </summary>
        <ul>
          {#each chars as f (f.path)}
            {@const userId = f.id === null ? null : accountOf(f.id, accountsStore.roster)}
            {@const alias = userId === null ? null : aliasFor(userId)}
            <li>
              <button class="file" onclick={() => onOpen(f.path)} title={f.file_name}>
                {resolvedName(f.kind, f.id) ?? f.file_name}
                {#if alias}<span class="acct">· {alias}</span>{/if}
                <span class="meta">{Math.round(f.size / 1024)} KB</span>
              </button>
            </li>
          {/each}
        </ul>
      </details>
    {/if}
  {/each}
```

- [ ] **Step 3: Update the toggle tooltip and delete dead CSS**

Change the hide-non-standard `<label>` title (line 112) to characters-only wording:

```svelte
  <label class="toggle" title="Show only EVE's own core_char_<id>.dat files">
```

In `<style>`, delete the now-unused `.group { … }` rule (the Accounts/Other group summaries are gone — svelte-check flags it as an unused selector). Add an alias-metadata style:

```css
  .acct { color: var(--fg-dim); font-size: 0.85em; margin: 0 0.3em; }
```

- [ ] **Step 4: Typecheck**

Run: `npm run check`
Expected: 0 errors, 0 warnings. (If `svelte-check` reports `.group` or any removed selector as unused, delete that rule too.)

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/Sidebar.svelte
git commit -m "List characters as the sidebar entry point"
```

---

### Task 5: `+page.svelte` — derive active from view, drop the toggle, wire it together

The integration task: the active document becomes a `$derived` of the current view, the Character/Account toggle is deleted, the Tree view gains a Character-file/Account-file switch, the account-scoped tabs stay visible for unpaired characters, and the shared banner + Autofill pairing props get wired. A small effect loads the account file when a character is paired while open (the "roster-driven reconcile" from spec §5).

**Files:**
- Modify: `app/src/routes/+page.svelte`

**Interfaces:**
- Consumes: `sharedWith` (Task 1), the Task 2/3 props, existing `reconcileUserSlot`, `openCharName`, `openCharId`, `openUserId`, `accountsStore`, `names`.
- Produces: the finished character-centric UI. No new exports.

- [ ] **Step 1: Add `sharedWith` to the overview import**

Extend the existing import (lines 16-21) to include `sharedWith`:

```svelte
  import {
    pairedFilePath,
    associatedCharacters,
    userSlotFor,
    charSlotFor,
    sharedWith,
  } from "$lib/overview";
```

- [ ] **Step 2: Replace the `active` state with a derived, add `treeFile`, and hoist `view`**

Replace lines 37-38:

```svelte
  let active = $state<Slot>("char");
  const current = $derived(slots[active]);
```

with:

```svelte
  // Which file the raw Tree view shows; a Tree-local switch flips it to the
  // account file when one is loaded. Reset on every open.
  let treeFile = $state<Slot>("char");
  type View = "tree" | "layout" | "overview" | "autofill";
  let view: View = $state("tree");
  // The active document is a consequence of the current view — NOT a manual
  // toggle: Autofill edits the account file, the Tree view honors its file
  // switch, everything else (Layout, Overview, search, backups) follows the
  // character.
  const active = $derived<Slot>(
    view === "autofill" && slots.user?.status === "opened"
      ? "user"
      : view === "tree" && treeFile === "user" && slots.user?.status === "opened"
        ? "user"
        : "char",
  );
  const current = $derived(slots[active]);
```

Then delete the now-duplicate declaration at lines 58-59:

```svelte
  type View = "tree" | "layout" | "overview" | "autofill";
  let view: View = $state("tree");
```

- [ ] **Step 3: Update `viewAvailable` for the always-visible account tabs**

Replace the `viewAvailable` derived (lines 64-68):

```svelte
  const viewAvailable = (v: View) =>
    v === "tree" ||
    (v === "layout" && layoutAvailable) ||
    (v === "overview" && (openCharId !== null || slots.user?.status === "opened")) ||
    (v === "autofill" && (openCharId !== null || slots.user?.status === "opened"));
```

(Only the `autofill` line changes — it now also holds for an open character with no account yet, so switching characters keeps you on Autofill, which shows the pairing prompt.)

- [ ] **Step 4: Add the shared-label deriveds**

After the `openAccountCharacters` block and its `$effect` (after line 122), add:

```svelte
  // The account's other characters an account-scoped edit also touches, and the
  // banner text shown above Autofill / Overview (spec §6).
  const sharedNames = $derived(
    sharedWith(openUserId, openCharId, accountsStore.roster, (id) => names[id]?.name ?? String(id)),
  );
  const sharedLabel = $derived(
    "Shared account settings" +
      (sharedNames.length ? ` — also applies to ${sharedNames.join(", ")}` : ""),
  );
```

- [ ] **Step 5: Load the account file when a paired character is already open**

After the `sharedLabel` derived, add the roster-driven reconcile effect:

```svelte
  // If the open character becomes paired while its account slot is empty — e.g.
  // the user just paired it in the Accounts view — load the account file so the
  // account-scoped editors light up without a manual re-open (spec §5). Guarded
  // on an empty user slot, so it never re-loads an already-open account.
  $effect(() => {
    const o = slots.char;
    void accountsStore.roster; // track roster changes
    if (o?.status === "opened" && slots.user === null) void reconcileUserSlot(o);
  });
```

- [ ] **Step 6: In `openFile`, reset the Tree file switch instead of setting active**

`active` is now derived and can't be assigned. Replace line 178:

```svelte
      active = slot;
```

with:

```svelte
      treeFile = slot;
```

(Opening a character sets `treeFile = "char"`; opening a raw account file via the dialog sets `treeFile = "user"` so the Tree shows it.)

- [ ] **Step 7: Delete the Character/Account toggle**

Remove the toggle block (lines 396-401):

```svelte
        {#if slots.char && slots.user}
          <span class="viewtabs">
            <button class:active={active === "char"} onclick={() => (active = "char")}>Character</button>
            <button class:active={active === "user"} onclick={() => (active = "user")}>Account</button>
          </span>
        {/if}
```

- [ ] **Step 8: Show the Autofill tab for open characters and wire its props**

In the view-tabs block, change the Autofill tab gate (line 407) from `slots.user?.status === "opened"` to include an open character:

```svelte
            {#if openCharId !== null || slots.user?.status === "opened"}<button class:active={view === "autofill"} onclick={() => (view = "autofill")}>Autofill</button>{/if}
```

Then wire the Overview and Autofill component props. Change the `OverviewView` usage (lines 428-437) to add `sharedLabel={sharedLabel}`:

```svelte
          <OverviewView
            userOpen={slots.user?.status === "opened"}
            charId={openCharId}
            characters={openAccountCharacters}
            refreshToken={savedAt}
            sharedLabel={sharedLabel}
            onLoadCharacter={loadCharacter}
            onUserDirty={() => (dirtySlots.user = true)}
            onCharDirty={() => (dirtySlots.char = true)}
            onWindowAdded={(id) => { if (layoutAvailable) { selectedWindowId = id; view = "layout"; } }}
            onShowAccounts={() => (mainView = "accounts")} />
```

Change the `AutofillView` usage (lines 441-443) to:

```svelte
          <AutofillView
            userOpen={slots.user?.status === "opened"}
            charName={openCharName}
            sharedLabel={sharedLabel}
            onShowAccounts={() => (mainView = "accounts")}
            onUserDirty={() => (dirtySlots.user = true)} />
```

- [ ] **Step 9: Add the Tree-view file switch**

In the Tree branch (the final `{:else}` around line 445), add a switch above the searchbar, shown only when the account file is loaded. Change:

```svelte
      {:else}
        <div class="searchbar">
```

to:

```svelte
      {:else}
        {#if slots.user?.status === "opened"}
          <span class="viewtabs tree-file">
            <button class:active={treeFile === "char"} onclick={() => (treeFile = "char")}>Character file</button>
            <button class:active={treeFile === "user"} onclick={() => (treeFile = "user")}>Account file</button>
          </span>
        {/if}
        <div class="searchbar">
```

(`viewtabs` styling already exists in this file's `<style>`; `.tree-file` just needs a little spacing.) Add to `+page.svelte`'s `<style>`:

```css
  .tree-file { display: inline-flex; margin: 0 0 0.5rem; }
```

- [ ] **Step 10: Typecheck**

Run: `npm run check`
Expected: 0 errors, 0 warnings. (Watch for: any remaining `active =` assignment — all must be gone; unused `associatedCharacters` import — it is still used by `openAccountCharacters`, keep it.)

- [ ] **Step 11: Commit**

```bash
git add app/src/routes/+page.svelte
git commit -m "Drive the active document from the view and drop the Character/Account toggle"
```

---

### Task 6: Build, run, and live smoke

Full green build plus a manual smoke against real profiles — this is frontend-only, so the gate is `npm run build` succeeding and the dev app behaving. The live checks mirror spec §10.

**Files:** none (verification only).

- [ ] **Step 1: Run the unit tests and typecheck**

Run (PowerShell, from `app/`): `npm test` then `npm run check`
Expected: tests all `ok - …`; check reports 0 errors, 0 warnings.

- [ ] **Step 2: Production build**

Run: `npm run build`
Expected: build completes with no errors.

- [ ] **Step 3: Launch the dev app and smoke the flows**

Run: `npm run tauri dev` (leave it running; observe the window). Walk the spec §10 checklist:

- Sidebar lists **characters only** (no Accounts/Other groups); the most-recent profile is expanded; a paired character shows its account alias (`Name · Alias`).
- Toggle "hide non-standard" off → backup/anomalous char files appear; on → only `core_char_<id>.dat`.
- Open a **paired** character → Layout, Overview, and Autofill all populate; there is **no** Character/Account toggle in the header; Overview and Autofill show `Shared account settings — also applies to …` naming the right siblings.
- Open an **unpaired** character → Layout and Tree work; Overview and Autofill show the `Pair…` prompt. Click `Pair…`, pair the character in Accounts, return → the account editors light up without re-opening (roster-driven reconcile).
- In the Tree view of a paired character, the **Character file / Account file** switch appears and flips the raw tree (and search) between the two documents.
- **Open file…** still opens a raw account `.dat` directly (Tree shows the account file).
- Save with edits in both the character and account files → both write; per-slot "unsaved" badges behave as before.

- [ ] **Step 4: Commit (if any smoke fixes were needed)**

Only if Step 3 surfaced fixes:

```bash
git add -A
git commit -m "Fix <what the smoke surfaced>"
```

---

## Self-Review

**Spec coverage:**
- §2 view→active mechanic → Task 5 Step 2 (derived `active`), toggle removal Step 7. ✓
- §3 sidebar character list, hide-non-standard kept, alias metadata, escape hatches → Task 4. ✓ (Accounts/Batch buttons and Open file… are untouched in `Sidebar.svelte`, so they remain.)
- §4 opening a character reuses `openFile` + `reconcileUserSlot` → no code change needed; Task 5 Step 6 only swaps the `active`-assignment for `treeFile`. ✓
- §5 unpaired gating + prompts + roster-driven reconcile → Task 2 (Autofill prompt), Task 3 (Overview prompt), Task 5 Steps 3/5/8. ✓
- §6 shared banner → Task 1 (helper), Tasks 2/3 (render), Task 5 Step 4 (label). ✓
- §7 Tree-local file switch → Task 5 Steps 2/6/9. ✓
- §9 edge cases: anomalous `id==None` char (never pairable → prompt stays) handled by `accountOf(f.id)` guarded on `f.id === null` (Task 4) and `sharedWith`/`openUserId` null-paths (Task 1/5); absent account file → existing `reconcileUserSlot` clears the slot (unchanged). ✓
- §10 testing → Task 1 unit test; Task 6 build + live smoke. ✓

**Placeholder scan:** none — every step shows the exact code/command. ✓

**Type consistency:** `sharedWith(userId, currentCharId, roster, nameOf)` defined in Task 1 is called with `(openUserId, openCharId, accountsStore.roster, (id) => names[id]?.name ?? String(id))` in Task 5 Step 4 — matches. `sharedLabel` prop typed `string` in Tasks 2/3, passed a `string` derived in Task 5. `treeFile`/`active` are `Slot` throughout. ✓
