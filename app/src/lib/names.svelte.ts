// Shared, app-wide character-name map. A Svelte-5 rune module so the sidebar
// and the open-file header both react to the same state. Resolution failures
// are swallowed — unresolved ids simply render as bare ids.
import { api, type NameMap } from "./api";

export const names = $state<NameMap>({});

/** Test-only, called from the shared `afterEach`. `resolveNames` merges with
 *  `Object.assign`, so without this one suite's fixture names survive into the
 *  next and quietly reorder any list sorted by resolved name. */
export function resetNames(): void {
  for (const k of Object.keys(names)) delete names[k];
}

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
