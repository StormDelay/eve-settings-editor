// Shared, app-wide account roster: aliases + confirmed character membership.
// A Svelte-5 rune module so the sidebar, the open-file header
// and the Accounts view all react to the same state. Mirrors names.svelte.ts.
import { api, type AccountRoster, type Rejected } from "./api";

const empty: AccountRoster = { accounts: [], unassigned: [] };
export const accountsStore = $state<{ roster: AccountRoster }>({ roster: empty });

/// Alias for an account id, or null if unnamed/unknown.
export function aliasFor(userId: number): string | null {
  return accountsStore.roster.accounts.find((a) => a.user_id === userId)?.alias ?? null;
}

export async function loadRoster(): Promise<void> {
  try {
    accountsStore.roster = await api.accountRoster();
  } catch {
    // Silent: leave the last roster in place.
  }
}

export async function setAlias(userId: number, alias: string | null): Promise<void> {
  accountsStore.roster = await api.setAccountAlias(userId, alias);
}

// Throws on the hard-cap rejection so the caller can surface it.
export async function confirmPairing(charId: number, userId: number): Promise<void> {
  accountsStore.roster = await api.confirmPairing(charId, userId);
}

export async function unpair(charId: number): Promise<void> {
  accountsStore.roster = await api.unpairCharacter(charId);
}

// Several pairings in one round trip. `confirmPairing` rebuilds the whole roster
// per call, and "accept everything the launcher proposes" is thirty of them.
// Applies what fits; the pairs it could not are returned, not thrown.
export async function confirmMany(pairs: [number, number][]): Promise<Rejected[]> {
  const batch = await api.confirmPairings(pairs);
  accountsStore.roster = batch.roster;
  return batch.rejected;
}
