// Shared, app-wide character-name map. A Svelte-5 rune module so the sidebar
// and the open-file header both react to the same state. Resolution failures
// are swallowed — unresolved ids simply render as bare ids.
import { api } from "./api";

export const names = $state<Record<string, { name: string; category: string }>>({});

function usable(ids: number[]): number[] {
  return ids.filter((id) => Number.isFinite(id));
}

export async function resolveNames(ids: number[]): Promise<void> {
  const wanted = usable(ids);
  if (wanted.length === 0) return;
  try {
    Object.assign(names, await api.resolveCharacterNames(wanted));
  } catch {
    // Silent: leave ids bare.
  }
}

export async function refreshNames(ids: number[]): Promise<void> {
  const wanted = usable(ids);
  if (wanted.length === 0) return;
  try {
    Object.assign(names, await api.refreshCharacterNames(wanted));
  } catch {
    // Silent.
  }
}
