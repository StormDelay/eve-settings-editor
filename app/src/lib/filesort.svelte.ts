//! The file list's naming and ordering rule, shared so every list of settings
//! files reads the same way. Lives here rather than in one component because the
//! sidebar and the batch-apply target list must not drift apart.
//!
//! `.svelte.ts` because these read rune state (`names`, the accounts roster).

import { aliasFor } from "./accounts.svelte";
import { names } from "./names.svelte";

/** Displayed name for a settings file: the resolved character name for char
 *  files, the account alias for user files — null while it is still a bare id
 *  (name unresolved / no alias), which callers render as the file name. */
export function resolvedName(kind: string, id: number | null): string | null {
  if (id == null) return null;
  if (kind === "char") return names[id]?.name ?? null;
  if (kind === "user") return aliasFor(id) ?? null;
  return null;
}

/** Alphabetical by resolved name; files still showing a bare id sort below the
 *  named ones, ordered among themselves by file name. */
export function byResolvedName(
  a: { kind: string; id: number | null; file_name: string },
  b: { kind: string; id: number | null; file_name: string },
): number {
  const na = resolvedName(a.kind, a.id);
  const nb = resolvedName(b.kind, b.id);
  if (na && nb) return na.localeCompare(nb);
  if (na) return -1;
  if (nb) return 1;
  return a.file_name.localeCompare(b.file_name);
}
