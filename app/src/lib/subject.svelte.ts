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
import { accountsStore } from "./accounts.svelte";
import {
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
  /** Whether the open document has any saved window layout. A property of the
   *  document, which is why it lives here and not beside `view`. */
  layoutAvailable = $state(false);
  /** Bumped after every save, open, discard and restore. History refetches on
   *  it, and it is every view's `refreshToken`. */
  savedAt = $state(0);

  charId = $derived(idIn(this.slots.char, "char"));
  userId = $derived(idIn(this.slots.user, "user"));

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
  subject.layoutAvailable = false;
  subject.savedAt = 0;
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
