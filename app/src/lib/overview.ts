// Pure helpers for the overview editor's char↔user loading: roster lookups and
// same-folder file resolution. No Svelte/Tauri deps so this is node --test-able.
import type { AccountRoster, Profile } from "./api";

export function associatedCharacters(userId: number, roster: AccountRoster): number[] {
  return roster.accounts.find((a) => a.user_id === userId)?.characters ?? [];
}

export function accountOf(charId: number, roster: AccountRoster): number | null {
  return roster.accounts.find((a) => a.characters.includes(charId))?.user_id ?? null;
}

function dirOf(path: string): string {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i < 0 ? "" : path.slice(0, i);
}

export function pairedFilePath(
  profiles: Profile[],
  anchorPath: string,
  id: number,
  kind: "char" | "user",
): string | null {
  const dir = dirOf(anchorPath);
  for (const p of profiles) {
    for (const f of p.files) {
      if (f.kind === kind && f.id === id && dirOf(f.path) === dir) return f.path;
    }
  }
  return null;
}

// What to do with the *other* slot when a file is opened: leave it, swap in the
// correct pair, or empty it. The two slots must always be a matching char/user
// pair (or one empty) — never a stale, unrelated file that the Overview editor
// would then misread.
export type SlotAction = { kind: "keep" } | { kind: "load"; path: string } | { kind: "clear" };

// On opening a CHARACTER, decide what the USER slot becomes: the character's
// paired account file (unique — a character belongs to one account), else empty.
export function userSlotFor(
  charPath: string,
  charId: number | null,
  currentUserPath: string | null,
  roster: AccountRoster,
  profiles: Profile[],
): SlotAction {
  const userId = charId === null ? null : accountOf(charId, roster);
  const userPath = userId === null ? null : pairedFilePath(profiles, charPath, userId, "user");
  if (userPath === null) return currentUserPath === null ? { kind: "keep" } : { kind: "clear" };
  if (userPath === currentUserPath) return { kind: "keep" };
  return { kind: "load", path: userPath };
}

// On opening an ACCOUNT file, decide what the CHAR slot becomes: keep it only if
// it holds one of this account's characters, else empty it. There is no single
// character to auto-load (an account has several); the selector picks one.
export function charSlotFor(
  userId: number | null,
  currentCharId: number | null,
  roster: AccountRoster,
): SlotAction {
  if (currentCharId === null) return { kind: "keep" };
  const belongs = userId !== null && accountOf(currentCharId, roster) === userId;
  return belongs ? { kind: "keep" } : { kind: "clear" };
}
