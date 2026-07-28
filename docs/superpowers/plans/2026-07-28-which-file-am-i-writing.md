# "Which file am I writing?" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the editor unambiguous about which settings file an action writes to, so a user cannot spend an hour editing a stale backup folder or a document the editor has silently invalidated underneath them.

**Architecture:** Four small, independent corrections in the frontend. Two fix which *profile* the UI recommends (`profiles.ts`, `Sidebar.svelte`); two fix which *character files* a batch apply offers and warns about (`BatchView.svelte`). No Rust changes and no format changes — every fact these need is already in the `Profile`/`SettingsFile` DTOs.

**Tech Stack:** Svelte 5 (runes), TypeScript, `node --test` for pure logic (`*.test.ts`, throw-based `check()` helper, no framework), Vitest + Testing Library for components (`*.spec.ts`).

## Global Constraints

- **No new dependencies.** The frontend dependency list stays as scaffolded; `profiles.test.ts` says so explicitly and there is deliberately no `@types/node`.
- **Pure logic goes in `.test.ts` (node --test), component behaviour in `.spec.ts` (Vitest).** Follow whichever the file you are touching already has.
- **Never write to the live EVE directory from tests.** Only `tools/sync-corpus.ps1` may read it; nothing may write it.
- Run the full suite with `npm test` from `app/` — it runs both runners (`node --test` then `vitest run`).
- Commit after each task.

---

## Background: why these four

All four came out of the 2026-07-27/28 live verification sessions, and one of them cost a full round of staging work. The user staged four separate edits into `settings_Default - USE THIS ONE`, a stale backup, believing it was live — and the editor had *recommended* that folder.

The evidence that drives Task 1 is worth stating because it is not obvious: across the sessions' captures, **the anonymous per-profile files are written only by EVE, on every run.**

| Capture kind | Count | `core_char__.dat` / `core_user__.dat` / `core_public__.yaml` touched |
|---|---|---|
| Editor-only staging | 11 | **0 every time** |
| After a client run | 4 | **3 every time** |

The editor writes `core_char_<id>.dat` and `core_user_<id>.dat` and never the anonymous ones. So "newest anonymous file" identifies the profile EVE actually uses, and — unlike the current heuristic — the editor's own writes cannot move it.

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `app/src/lib/profiles.ts` | Which profile is live, and how profiles are labelled | Modify: rank on EVE-written files |
| `app/src/lib/profiles.test.ts` | Pure logic tests for the above | Modify: add ranking cases |
| `app/src/lib/Sidebar.svelte` | Profile list, file list | Modify: say "in use", warn on the others |
| `app/src/lib/BatchView.svelte` | Batch source/target picking | Modify: target filter + open-file warning |
| `app/src/lib/BatchView.spec.ts` | Component tests for the batch view | Modify: add cases for both |

---

### Task 1: Rank the live profile on files only EVE writes

**Files:**
- Modify: `app/src/lib/profiles.ts:19-32` (`primaryProfileDir`)
- Test: `app/src/lib/profiles.test.ts`

**Interfaces:**
- Consumes: `Profile` and `SettingsFile` from `./api` — `SettingsFile` has `file_name: string` and `modified_unix: number | null`.
- Produces: `primaryProfileDir(profiles: Profile[]): string | null` — unchanged signature, changed ranking. `Sidebar.svelte` and `BatchView.svelte` both already call it.

- [ ] **Step 1: Write the failing test**

Add to `app/src/lib/profiles.test.ts`. Note the existing `profile()` helper builds `files: []`; these cases need files, so build them inline.

```ts
const file = (file_name: string, modified_unix: number | null): SettingsFile => ({
  path: `/roots/x/${file_name}`,
  file_name,
  kind: file_name.startsWith("core_user") ? "user" : "char",
  id: null,
  modified_unix,
});

const withFiles = (p: Profile, files: SettingsFile[]): Profile => ({ ...p, files });

// The exact failure from the 2026-07-28 session: the user had edited a stale
// backup that morning, so it was the most recently *touched* profile, and the
// editor pinned it to the top and opened it. EVE had not written it in weeks.
{
  const live = withFiles(profile("g_eve", "tranquility", "Default"), [
    file("core_char_93622368.dat", 100),
    file("core_public__.yaml", 500), // only EVE writes this
  ]);
  const backup = withFiles(profile("g_eve", "tranquility", "Default - USE THIS ONE"), [
    file("core_char_93622368.dat", 900), // the user's own later edit
  ]);
  check(
    "the profile EVE last wrote wins over one the user edited more recently",
    primaryProfileDir([live, backup]) === live.dir,
  );
}

// Fallback: when nothing carries an EVE-only file, any file is better than
// giving up — a profile the editor has written is still a real profile.
{
  const a = withFiles(profile("g_eve", "tranquility", "A"), [file("core_char_1.dat", 10)]);
  const b = withFiles(profile("g_eve", "tranquility", "B"), [file("core_char_1.dat", 20)]);
  check(
    "with no EVE-only file anywhere, the newest file still decides",
    primaryProfileDir([a, b]) === b.dir,
  );
}

{
  check("no profiles means no answer", primaryProfileDir([]) === null);
  const t = withFiles(profile("g_eve", "tranquility", "A"), [file("core_char_1.dat", null)]);
  check("no usable timestamp means no answer", primaryProfileDir([t]) === null);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run from `app/`: `node --test "src/lib/profiles.test.ts"`
Expected: FAIL — `FAIL: the profile EVE last wrote wins over one the user edited more recently`. The other three should already pass against the current implementation.

- [ ] **Step 3: Write the implementation**

Replace `primaryProfileDir` in `app/src/lib/profiles.ts`:

```ts
/**
 * Files only EVE writes. The editor writes `core_char_<id>.dat` and
 * `core_user_<id>.dat`; it never touches the anonymous ones. Verified over the
 * 2026-07-27/28 live captures: 11 editor-only captures touched none of these,
 * and all 4 captures taken after a client run touched three of them.
 */
const EVE_WRITTEN = /^core_(char|user)__\.dat$|^core_public__\.yaml$/;

/**
 * The profile actually in use: the one EVE wrote most recently.
 *
 * Ranking on ANY file — which this used to do — is wrong in the case that
 * matters. Players keep hand-made backups beside the live folder (one machine
 * had nine under a single profile), and editing one through this very editor
 * makes it the most recently touched. On 2026-07-28 that pinned a weeks-stale
 * backup to the top of the sidebar and a full round of work went into it.
 * Ranking on files only EVE writes cannot be moved by our own saves.
 *
 * `null` when there are no profiles, or none carries a usable timestamp —
 * callers then have nothing better to guess with. Ties keep the first, which is
 * discovery's alphabetical order.
 */
export function primaryProfileDir(profiles: Profile[]): string | null {
  const newest = (p: Profile, eveOnly: boolean) =>
    p.files.reduce(
      (max, f) =>
        eveOnly && !EVE_WRITTEN.test(f.file_name) ? max : Math.max(max, f.modified_unix ?? 0),
      0,
    );
  // Prefer the EVE-only signal. Fall back to any file only when no profile has
  // one at all, so a profile the client has never run in is still selectable.
  for (const eveOnly of [true, false]) {
    let best: string | null = null;
    let bestTime = 0;
    for (const p of profiles) {
      const t = newest(p, eveOnly);
      if (t > bestTime) {
        bestTime = t;
        best = p.dir;
      }
    }
    if (best) return best;
  }
  return null;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run from `app/`: `node --test "src/lib/profiles.test.ts"`
Expected: PASS, all cases.

- [ ] **Step 5: Run the full suite**

Run from `app/`: `npm test`
Expected: PASS. `Sidebar` and `BatchView` consume this function and their specs must still pass.

- [ ] **Step 6: Commit**

```bash
git add app/src/lib/profiles.ts app/src/lib/profiles.test.ts
git commit -m "Rank the live profile on files only EVE writes"
```

---

### Task 2: Say which profile is live, and warn on the others

**Files:**
- Modify: `app/src/lib/Sidebar.svelte:50-56` (the `rows` derivation) and `:132-141` (the row markup)
- Test: `app/src/lib/profiles.test.ts` (the pure helper only)

**Interfaces:**
- Consumes: `primaryProfileDir` from Task 1.
- Produces: nothing other tasks depend on.

The sidebar already pins the primary profile on top, opens it, and labels it `most recent`. That label is the problem: it states a fact about timestamps when the user needs a statement about *which folder EVE uses*, and it says nothing at all about the others.

- [ ] **Step 1: Write the failing test**

Add to `app/src/lib/profiles.test.ts` — a pure helper so it can be tested without mounting the component:

```ts
{
  check("the live profile is named as in use", profileNote(true) === "in use by EVE");
  check("every other profile is called out as not", profileNote(false) === "not in use — EVE has not written here");
}
```

Add the import at the top of the file alongside the existing ones:

```ts
import { primaryProfileDir, profileLabels, profileNote } from "./profiles.ts";
```

- [ ] **Step 2: Run test to verify it fails**

Run from `app/`: `node --test "src/lib/profiles.test.ts"`
Expected: FAIL — `profileNote is not a function`.

- [ ] **Step 3: Write the implementation**

Add to `app/src/lib/profiles.ts`:

```ts
/**
 * What to show beside a profile in the list. The wording is deliberately about
 * EVE rather than about timestamps: "most recent" is what this used to say, and
 * it reads as a ranking rather than as "this is the one the game loads" — which
 * is the question a user with nine backup folders is actually asking.
 */
export function profileNote(isPrimary: boolean): string {
  return isPrimary ? "in use by EVE" : "not in use — EVE has not written here";
}
```

- [ ] **Step 4: Run test to verify it passes**

Run from `app/`: `node --test "src/lib/profiles.test.ts"`
Expected: PASS.

- [ ] **Step 5: Use it in the sidebar**

In `app/src/lib/Sidebar.svelte`, add `profileNote` to the existing import on line 8:

```ts
import { primaryProfileDir, profileLabels, profileNote } from "./profiles";
```

Then replace the note in the row markup (currently line 140):

```svelte
<span class="meta" class:not-live={!primary}>{profileNote(primary)}</span>
```

And add to the component's `<style>` block:

```css
  /* A non-live profile is a real hazard, not a detail: editing one looks like
     it worked and changes nothing the game reads. */
  .meta.not-live { color: var(--warn, #d08770); }
```

- [ ] **Step 6: Run the full suite**

Run from `app/`: `npm test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/profiles.ts app/src/lib/profiles.test.ts app/src/lib/Sidebar.svelte
git commit -m "Say which profile EVE actually uses, and flag the ones it doesn't"
```

---

### Task 3: The open character is a valid batch target when the source is a preset

**Files:**
- Modify: `app/src/lib/BatchView.svelte:97-100` (the `candidates` derivation)
- Test: `app/src/lib/BatchView.spec.ts`

**Interfaces:**
- Consumes: the existing `source` derived value, shape
  `{ kind: "character"; path: string } | { kind: "preset"; dir: string; anchor_dir: string } | null`.
- Produces: nothing other tasks depend on.

`sourcePath` is seeded from `openPath` on mount (line 38), so the open character defaults to being the batch *source* and is filtered out of the targets — correct for a character→character copy. But switching `sourceKind` to `"preset"` never clears `sourcePath`, so the exclusion outlives its reason and the open character cannot be chosen as a target at all.

- [ ] **Step 1: Write the failing test**

Add to `app/src/lib/BatchView.spec.ts`, following the existing render/query style in that file:

```ts
test("with a preset as the source, the open character is still a target", async () => {
  // Two characters in one profile; the first is the open document, so it seeds
  // sourcePath and — before this fix — vanished from the target list for good.
  const { getByLabelText, findByText } = renderBatch({
    openPath: "/roots/g_eve_tq/settings_Default/core_char_1.dat",
    profiles: [profileWith(["core_char_1.dat", "core_char_2.dat"])],
    presets: [{ name: "layout", dir: "/presets/layout", aspects: ["layout"], full: false }],
  });

  await fireEvent.change(getByLabelText("Source"), { target: { value: "preset" } });

  expect(await findByText(/char 1/)).toBeInTheDocument();
});
```

If `renderBatch`, `profileWith` or the label strings differ in the existing spec, reuse whatever that file already defines rather than introducing new helpers — the point of the test is the assertion, not the scaffolding.

- [ ] **Step 2: Run test to verify it fails**

Run from `app/`: `npx vitest run src/lib/BatchView.spec.ts`
Expected: FAIL — the open character is absent from the target list.

- [ ] **Step 3: Write the implementation**

In `app/src/lib/BatchView.svelte`, replace the first filter in `candidates`:

```svelte
  const candidates = $derived(
    chars
      // A character cannot be its own copy target — but ONLY when it is the
      // source. `sourcePath` is seeded from the open file and never cleared on
      // switching to a preset source, so filtering on it directly kept the open
      // character out of the list for the rest of the session.
      .filter((c) => !(source?.kind === "character" && c.path === source.path))
      .filter((c) => allowOtherFolders || c.dir === folder)
```

Leave the rest of the derivation (the `.slice()`, the `.sort()`) exactly as it is.

- [ ] **Step 4: Run test to verify it passes**

Run from `app/`: `npx vitest run src/lib/BatchView.spec.ts`
Expected: PASS, and the existing 13 tests still pass — one of them covers the character→character case where the source must still be excluded.

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/BatchView.svelte app/src/lib/BatchView.spec.ts
git commit -m "Offer the open character as a batch target when the source is a preset"
```

---

### Task 4: Warn when a batch apply targets the open document

**Files:**
- Modify: `app/src/lib/BatchView.svelte` (the plan-summary markup, beside the existing account-write warning at `:298`)
- Test: `app/src/lib/BatchView.spec.ts`

**Interfaces:**
- Consumes: the existing `openPath: string | null` prop (line 9) and `selectedTargets`.
- Produces: nothing other tasks depend on.

A batch apply writes to disk behind the open in-memory document. Nothing warns at apply time; re-selecting the file warns about unsaved changes but does not reload it, and only the save-time on-disk check eventually catches the divergence. No data is lost — that check is a real backstop — but the user finds out two steps later.

Warn whenever a selected target *is* the open file, regardless of dirty state: the in-memory copy goes stale either way, and this needs no dirty-tracking plumbing.

- [ ] **Step 1: Write the failing test**

Add to `app/src/lib/BatchView.spec.ts`:

```ts
test("warns when a target is the file currently open", async () => {
  const openPath = "/roots/g_eve_tq/settings_Default/core_char_2.dat";
  const { getByLabelText, findByText } = renderBatch({
    openPath,
    profiles: [profileWith(["core_char_1.dat", "core_char_2.dat"])],
  });

  // char_1 is the source (seeded elsewhere); select the OPEN file as a target.
  await fireEvent.click(getByLabelText(/char 2/));

  expect(await findByText(/open in the editor/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run from `app/`: `npx vitest run src/lib/BatchView.spec.ts`
Expected: FAIL — no such text is rendered.

- [ ] **Step 3: Write the implementation**

In `app/src/lib/BatchView.svelte`, add a derived value beside the other `$derived` declarations:

```ts
  // Applying onto the open document writes behind it: the in-memory copy goes
  // stale and the only thing that notices is the save-time on-disk check, two
  // steps later. Warn at the point of decision instead.
  const targetsOpenFile = $derived(openPath !== null && [...selectedTargets].includes(openPath));
```

And render it in the plan summary, immediately before the existing account-write warning:

```svelte
    {#if targetsOpenFile}
      <p class="warn">⚠ One target is the file open in the editor. Its on-screen
        copy will be out of date after this runs — reload it before editing
        further, or your next save will collide with what this wrote.</p>
    {/if}
```

- [ ] **Step 4: Run test to verify it passes**

Run from `app/`: `npx vitest run src/lib/BatchView.spec.ts`
Expected: PASS.

- [ ] **Step 5: Run the full suite**

Run from `app/`: `npm test`
Expected: PASS — 13 node checks and the component suites.

- [ ] **Step 6: Update the ledger**

In `docs/small-tasks.md`, mark these four closed, each with its verdict: the folder-marking task (note that the cause was `primaryProfileDir`'s ranking, not a missing marker), the batch-target task, and the batch-apply-warning task. Leave the rest of the list untouched.

- [ ] **Step 7: Commit**

```bash
git add app/src/lib/BatchView.svelte app/src/lib/BatchView.spec.ts docs/small-tasks.md
git commit -m "Warn when a batch apply targets the document you have open"
```

---

## Self-review notes

- **Coverage.** Four findings, four tasks: `primaryProfileDir` ranking (Task 1), the sidebar marker (Task 2), the batch target filter (Task 3), the open-document warning (Task 4). The ledger entry for the folder problem blamed a missing marker; Task 1's background corrects that — the marker exists and was pointing at the wrong folder.
- **Naming.** `primaryProfileDir` keeps its signature so both existing callers are untouched. `profileNote` is new and used only by `Sidebar.svelte`. `EVE_WRITTEN` is module-private.
- **Ordering.** Task 2 depends on Task 1 (the marker is only worth trusting once the ranking is right). Tasks 3 and 4 are independent of both and of each other.
- **Not in scope, deliberately.** The `tabsettings2` key, the windowless-account state, the overview filter list's performance, the HUD footprint, and the orphan-frame deletion offer are each their own subsystem and need their own plan.
