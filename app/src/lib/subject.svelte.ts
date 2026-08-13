//! The subject: which character (and its account) is open, what is unsaved, and
//! who else an edit reaches.
//!
//! A Svelte-5 rune module for the same reason `accounts.svelte.ts` is one, one
//! level up — its own header says it best: "so the sidebar, the open-file header
//! and the Accounts view all react to the same state". Here the readers are the
//! context bar, the save cluster, the History popover, the subject list, the
//! launch empty state and four views, and threading that many props through a
//! context bar that exists only to hold them is what `+page.svelte` growing to
//! 718 lines already looks like.
//!
//! A CLASS rather than `accountsStore`'s object literal, and the reason is
//! mechanical rather than stylistic: `$derived` cannot cross a module boundary as
//! a bare `export const`. The compiler rewrites *reads*, not the binding, and the
//! only read it can rewrite in another module is a property access. So every
//! derived here is a class field, and `subject.canSave` is a getter.
//!
//! What is NOT here is as deliberate: `view`, `treeFile`, `mainView` and the
//! selection are facts about where the user is LOOKING, read only by the shell.
//! See `02-shell.md` §6.2.

import { api, errMessage, type ErrDto, type OpenOutcome, type Profile, type Slot } from "./api";
import { names, resolveNames } from "./names.svelte";
import { accountsStore, aliasFor, loadRoster } from "./accounts.svelte";
import { byResolvedName } from "./filesort.svelte";
import { primaryProfileDir } from "./profiles";
import {
  accountOf,
  associatedCharacters,
  charSlotFor,
  pairedFilePath,
  sharedWith,
  slotsToReload,
  userSlotFor,
} from "./overview";
import { ask, message } from "@tauri-apps/plugin-dialog";

/** The id EVE encoded in a slot's file name, or null if that slot holds
 *  something else (an unparsed file, a non-standard name, or nothing). */
function idIn(o: OpenOutcome | null, kind: "char" | "user"): number | null {
  if (o?.status !== "opened") return null;
  const m = o.file_name.match(kind === "char" ? /^core_char_(\d+)\.dat$/ : /^core_user_(\d+)\.dat$/);
  return m ? Number(m[1]) : null;
}

/**
 * The account chip on a character row, or null for no chip. THE only source, on
 * every surface — `02-shell.md` §5.7.2 rule 6, which is the clause that keeps
 * the other five honest.
 *
 * It reads `accountOf()` and nothing else, so a chip means a CONFIRMED pairing
 * and absence means no account. A launcher proposal is not an account: it leaves
 * `accountOf()` at null, draws no chip, and so renders exactly as an unpaired
 * character does — which is truthful, because a proposed character can do
 * exactly what an unpaired one can (nothing account-scoped) until someone
 * accepts it. No list may derive a chip from `Proposal.user_id`.
 */
export function accountAliasOf(f: { id: number | null }): string | null {
  const userId = f.id === null ? null : accountOf(f.id, accountsStore.roster);
  return userId === null ? null : aliasFor(userId);
}

/** The ONE predicate for "saving would write this slot". Save's disabled state,
 *  the save disclosure's file list and the save loop itself all ask it, because a
 *  Save button that disagrees with what the loop writes is the next bug. */
export function saveable(o: OpenOutcome | null, dirty: boolean): boolean {
  return dirty && o?.status === "opened" && o.fidelity.state === "editable";
}

class Subject {
  /** Two editing slots: a character file and its account (user) file. */
  slots = $state<{ char: OpenOutcome | null; user: OpenOutcome | null }>({ char: null, user: null });
  dirty = $state<{ char: boolean; user: boolean }>({ char: false, user: false });
  /** Set while a PRESET (rather than a character) occupies the two slots. */
  preset = $state<string | null>(null);
  /** Discovered profiles, for resolving an id to its file path within the same
   *  profile folder as an already-open file (see `pairedFilePath`). */
  profiles = $state<Profile[]>([]);
  /** Why the scan failed, if it did. Here rather than in the sidebar because the
   *  scan now has two callers — mount and the app menu's Rescan — and a sidebar
   *  that is empty for an unstated reason is the thing this replaces. */
  profilesError = $state<string | null>(null);
  /** Whether the open document has any saved window layout. A property of the
   *  document, which is why it lives here and not beside `view`. */
  layoutAvailable = $state(false);
  /** Bumped after every save, open, discard and restore. History refetches on
   *  it, and it is every view's `refreshToken`. */
  savedAt = $state(0);

  /** The profile folder in scope, when the user has picked one. Single-select,
   *  and the reason is a hazard rather than a preference: one account id can
   *  exist in several profile folders at once (`docs/small-tasks.md:32-45`
   *  records an install with ten holding the same `core_user_13036531.dat`), so
   *  a list spanning folders puts indistinguishable duplicates next to each
   *  other in alphabetical order with nothing to tell them apart. */
  selectedProfileDir = $state<string | null>(null);
  /** Hide user-made backups and anomalous names, keeping only EVE's own working
   *  file names. Shared rather than sidebar-local, because the switcher and the
   *  launch empty state have to list exactly what the sidebar lists. */
  hideNonStandard = $state(true);

  charId = $derived(idIn(this.slots.char, "char"));
  userId = $derived(idIn(this.slots.user, "user"));

  /**
   * Falls back to the profile EVE itself wrote last, which is the one the
   * sidebar already pinned open — and then to the first discovered profile.
   *
   * That last fallback is load-bearing now in a way it was not before.
   * `primaryProfileDir` returns null when NO profile carries a usable
   * timestamp, and says so: "callers then have nothing better to guess with".
   * A list of `<details>` did not care, because it drew every folder. A
   * single-select list with a null selection draws nothing at all — an empty
   * sidebar for a user whose files simply have no mtime. The first profile is
   * discovery's alphabetical order, which is a guess, but a visible one.
   */
  profileDir = $derived(
    this.selectedProfileDir ?? primaryProfileDir(this.profiles) ?? this.profiles[0]?.dir ?? null,
  );
  profile = $derived(this.profiles.find((p) => p.dir === this.profileDir) ?? null);

  /**
   * THE character list. One derived, read by the sidebar, the subject switcher
   * and the launch empty state, so "same characters, same order, same chips" is
   * true by construction rather than by three copies of a filter agreeing.
   *
   * Order is `byResolvedName` and does not change: named characters
   * alphabetically, files still showing a bare id after them, ordered among
   * themselves by file name. Alphabetical is how a name is found, and finding a
   * character is what these lists are for — which is why account grouping was
   * proposed and rejected (`02-shell.md` §5.7).
   */
  characters = $derived.by(() =>
    (this.profile?.files ?? [])
      .filter(
        (f) =>
          f.kind === "char" &&
          (!this.hideNonStandard || /^core_(char|user)_\d+\.dat$/.test(f.file_name)),
      )
      .sort(byResolvedName),
  );

  charName = $derived(this.charId === null ? null : names[this.charId]?.name ?? null);
  userAlias = $derived(this.userId === null ? null : aliasFor(this.userId));

  /**
   * The subject, named once. Same precedence `openDisplay` implemented, but
   * resolved against the SUBJECT rather than against `slots[active]` — which is
   * the whole of the fix for the OS window title flipping when you change tab.
   *
   * The raw file name is the LAST resort here, not the headline. It keeps three
   * homes that each have a reason to carry it: the save disclosure, the History
   * popover, and the switcher row's tooltip.
   */
  subjectName = $derived.by(() => {
    if (this.preset !== null) return this.preset;
    if (this.charName) return this.charName;
    if (this.slots.char?.status === "opened") return this.slots.char.file_name;
    if (this.userAlias) return this.userAlias;
    if (this.slots.user?.status === "opened") return this.slots.user.file_name;
    return null;
  });

  /** The same name for the OS window title, where "(preset)" earns its place
   *  because there is no Chip beside it to say so. */
  subjectLabel = $derived(
    this.subjectName === null
      ? null
      : this.preset !== null
        ? `${this.preset} (preset)`
        : this.subjectName,
  );

  /**
   * What a Save would touch, for the disclosure. Dirty slots only, each carrying
   * why it cannot be written if it cannot — a read-only slot is LISTED (so the
   * user learns their edit is stuck) but is not "will write".
   *
   * `blocked` is derived from the same `saveable` the button and the save loop
   * ask, so the three cannot disagree.
   */
  saveTargets = $derived.by(() => {
    const rows: {
      slot: Slot;
      subjectName: string;
      role: "character" | "account";
      fileName: string;
      blocked: string | null;
    }[] = [];
    for (const slot of ["char", "user"] as const) {
      const o = this.slots[slot];
      if (!this.dirty[slot] || o?.status !== "opened") continue;
      rows.push({
        slot,
        subjectName:
          (slot === "char" ? this.charName : this.userAlias) ?? o.file_name,
        role: slot === "char" ? "character" : "account",
        fileName: o.file_name,
        blocked: o.fidelity.state === "editable" ? null : o.fidelity.reason,
      });
    }
    return rows;
  });

  /** The roster's characters for the open account — the width selector's list. */
  accountCharacters = $derived(
    this.userId === null ? [] : associatedCharacters(this.userId, accountsStore.roster),
  );

  /** The account's OTHER characters, which an account-scoped edit also changes.
   *  The single most consequential fact in the app, so it is stated at both
   *  moments it bites: `ScopeBanner` before the edit, the save disclosure at the
   *  moment of writing. */
  sharedNames = $derived(
    sharedWith(this.userId, this.charId, accountsStore.roster, (id) => names[id]?.name ?? String(id)),
  );

  canSave = $derived(
    saveable(this.slots.char, this.dirty.char) || saveable(this.slots.user, this.dirty.user),
  );
}

export const subject = new Subject();

/**
 * Clear every field back to launch state.
 *
 * MANDATORY in `afterEach` of any suite that mounts the shell, and not a
 * nicety: `page.spec.ts` already documents what module-level rune state does to
 * this suite — "a load still in flight when `afterEach` clears the stubs
 * resolves to `undefined` and poisons that state for the next test". This store
 * is the third such module and much the largest.
 */
export function resetSubject(): void {
  subject.slots.char = null;
  subject.slots.user = null;
  subject.dirty.char = false;
  subject.dirty.user = false;
  subject.preset = null;
  subject.profiles = [];
  subject.profilesError = null;
  subject.selectedProfileDir = null;
  subject.hideNonStandard = true;
  subject.layoutAvailable = false;
  subject.savedAt = 0;
}

/**
 * Why the selected profile lists no characters. One function, because the
 * sidebar and the launch empty state must say it in the same words — and one of
 * the two wordings names the "Hide non-standard files" filter as the cause,
 * which is the only actionable thing about it.
 */
export function noCharactersHint(): string {
  return subject.hideNonStandard
    ? "No character files with EVE's own names in these profiles. Untick “Hide non-standard files”, or use “Open file…”."
    : "These profiles hold no character files. Use “Open file…” to open an account file directly.";
}

/** Every character id across ALL discovered profiles — the set whose names are
 *  resolved. Deliberately not scoped to the selected profile: a resolved name is
 *  cached app-wide, and re-fetching per folder would be the same call twice. */
export function allCharIds(): number[] {
  return subject.profiles
    .flatMap((p) => p.files)
    .filter((f) => f.kind === "char" && f.id != null)
    .map((f) => f.id as number);
}

/**
 * Rescan the standard EVE locations. Returns the profile count, or null if the
 * scan failed (the reason lands on `profilesError`).
 *
 * One caller on mount and one in the app menu, where the sidebar's `⟳` went.
 * It used to be two independent scans — `+page.svelte` fetched profiles for pair
 * resolution and `Sidebar` fetched them again for its list — so the app sent
 * `discover_profiles` twice on every start.
 */
export async function rescanProfiles(): Promise<number | null> {
  try {
    subject.profiles = await api.discover();
    subject.profilesError = null;
    // Land the selection on a folder that exists. The default is the profile
    // EVE itself wrote last, which is the one the sidebar already pinned open;
    // holding a stale dir across a rescan would show an empty list with no
    // explanation.
    if (!subject.profiles.some((p) => p.dir === subject.selectedProfileDir)) {
      subject.selectedProfileDir =
        primaryProfileDir(subject.profiles) ?? subject.profiles[0]?.dir ?? null;
    }
    void resolveNames(allCharIds());
    void loadRoster();
    return subject.profiles.length;
  } catch (e) {
    subject.profilesError = errMessage(e);
    return null;
  }
}

/** Empty a slot: close its backend document and clear the frontend state. */
export async function clearSlot(slot: Slot): Promise<void> {
  if (subject.slots[slot] === null) return;
  try {
    await api.close(slot);
  } catch { /* best-effort */ }
  subject.slots[slot] = null;
  subject.dirty[slot] = false;
}

// After a character lands in the char slot, make the user slot its paired
// account file — or empty it. Never keep a stale, unrelated account file (the
// Overview view shows the Accounts nudge when the user slot is empty).
export async function reconcileUserSlot(charOutcome: OpenOutcome): Promise<void> {
  const charId =
    charOutcome.status === "opened"
      ? charOutcome.file_name.match(/^core_char_(\d+)\.dat$/)?.[1] ?? null
      : null;
  const action = userSlotFor(
    charOutcome.status === "opened" ? charOutcome.path : "",
    charId === null ? null : Number(charId),
    subject.slots.user?.status === "opened" ? subject.slots.user.path : null,
    accountsStore.roster,
    subject.profiles,
  );
  if (action.kind === "keep") return;
  if (action.kind === "clear") return clearSlot("user");
  try {
    subject.slots.user = await api.open("user", action.path);
    subject.dirty.user = false;
  } catch {
    await clearSlot("user"); // couldn't load the pair -> don't keep a stale one
  }
}

// After an account file lands in the user slot, keep the char slot only if it
// holds one of this account's characters — otherwise empty it (the character
// selector picks which of the account's characters to load).
export async function reconcileCharSlot(userOutcome: OpenOutcome): Promise<void> {
  const userId =
    userOutcome.status === "opened"
      ? userOutcome.file_name.match(/^core_user_(\d+)\.dat$/)?.[1] ?? null
      : null;
  const currentCharId =
    subject.slots.char?.status === "opened"
      ? subject.slots.char.file_name.match(/^core_char_(\d+)\.dat$/)?.[1] ?? null
      : null;
  const action = charSlotFor(
    userId === null ? null : Number(userId),
    currentCharId === null ? null : Number(currentCharId),
    accountsStore.roster,
  );
  if (action.kind === "clear") await clearSlot("char");
}

// Shared unsaved-changes prompt for anything that swaps out an open file:
// the Open-file dialog/sidebar and the character selector alike.
export async function confirmDiscardIfDirty(): Promise<boolean> {
  if (!subject.dirty.char && !subject.dirty.user) return true;
  const which = [subject.dirty.char && "character", subject.dirty.user && "account"]
    .filter(Boolean)
    .join(" and ");
  const noun = subject.dirty.char && subject.dirty.user ? "files" : "file";
  return ask(
    `You have unsaved changes to the ${which} ${noun}. Discard them and open another file?`,
    { title: "Unsaved changes", kind: "warning" },
  );
}

/// Throw the unsaved edits away and re-read the open file(s) from disk.
///
/// Both slots, even when only one is dirty: the editors write to both — an
/// overview edit touches the account's tabs and the character's column widths
/// — so reverting one would leave a half-reverted pair. The button says so.
///
/// This is a RE-READ, not a restore: nothing in the backup chain is touched,
/// and the view, the selection and an open preset all stay where they are,
/// because exactly the files that were open are the files reopened.
export async function discardChanges(): Promise<void> {
  if (!subject.dirty.char && !subject.dirty.user) return;
  const targets = slotsToReload(subject.slots);
  if (targets.length === 0) return;
  const ok = await ask(
    "Discard your unsaved changes and reload from disk? Both the character and the account file are reloaded, and your backups are untouched.",
    { title: "Discard changes", kind: "warning" },
  );
  if (!ok) return;
  try {
    const reopened = await Promise.all(targets.map((t) => api.open(t.slot, t.path)));
    targets.forEach((t, i) => (subject.slots[t.slot] = reopened[i]));
    subject.dirty.char = false;
    subject.dirty.user = false;
    subject.savedAt += 1;
  } catch (e) {
    await message(errMessage(e), { title: "Discard failed", kind: "error" });
  }
}

// Load a selected character into the char slot (from the OverviewView selector).
export async function loadCharacter(charId: number): Promise<void> {
  if (!(await confirmDiscardIfDirty())) return;
  const anchor = subject.slots.user?.status === "opened" ? subject.slots.user.path : "";
  const charPath = pairedFilePath(subject.profiles, anchor, charId, "char");
  if (!charPath) return;
  try {
    subject.preset = null;
    subject.slots.char = await api.open("char", charPath);
    subject.dirty.char = false;
    await resolveNames([charId]);
  } catch (e) {
    await message(errMessage(e), { title: "Open failed", kind: "error" });
  }
}

export async function saveFile(force = false): Promise<void> {
  for (const slot of ["char", "user"] as const) {
    const o = subject.slots[slot];
    if (!saveable(o, subject.dirty[slot])) continue;
    // `saveable` narrows nothing for TypeScript, but it has just asserted this.
    const doc = o as Extract<OpenOutcome, { status: "opened" }>;
    try {
      const report = await api.save(slot, force);
      subject.dirty[slot] = false;
      subject.savedAt += 1;
      const note = `Saved ${report.bytes_written} bytes to ${doc.file_name}.\nBackup: ${report.backup_path}`;
      await message(note, { title: "Saved", kind: "info" });
    } catch (e) {
      const err = e as ErrDto;
      if (err.code === "conflict") {
        const overwrite = await ask(
          `${doc.file_name} changed on disk after it was loaded (the EVE client may have ` +
            `written it). Overwrite anyway?\n\nA backup of the on-disk file is taken first either way.`,
          { title: "File changed on disk", kind: "warning" },
        );
        if (overwrite) {
          try {
            await api.save(slot, true);
            subject.dirty[slot] = false;
            subject.savedAt += 1;
          } catch (e2) {
            await message(errMessage(e2), { title: "Save failed", kind: "error" });
          }
        }
      } else {
        await message(errMessage(e), { title: `Save failed — ${doc.file_name} untouched`, kind: "error" });
      }
    }
  }
}
