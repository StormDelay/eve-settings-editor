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
