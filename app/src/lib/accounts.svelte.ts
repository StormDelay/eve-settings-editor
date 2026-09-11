// Shared, app-wide account roster: aliases + confirmed character membership.
// A Svelte-5 rune module so the sidebar, the open-file header
// and the Accounts view all react to the same state. Mirrors names.svelte.ts.
import { api, type AccountRoster, type Proposal, type Rejected } from "./api";

const empty: AccountRoster = { accounts: [], unassigned: [] };
export const accountsStore = $state<{ roster: AccountRoster }>({ roster: empty });

/** Test-only, called from the shared `afterEach`. `loadRoster` deliberately
 *  leaves the last roster in place on failure, which across tests means one
 *  suite's pairings decide another's account chips. */
export function resetRoster(): void {
  accountsStore.roster = empty;
}

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

/**
 * Guided-capture progress, which spans a trip out to the EVE client.
 *
 * It lives here rather than in `AccountsView` because the Accounts panel is a
 * dismissable sheet now, and the flow REQUIRES the user to leave the app: launch
 * EVE, change an account-wide setting, log out. A sheet that cannot be closed
 * during that is a sheet that cannot be used.
 *
 * The expensive half already survives — the baseline is a snapshot of every
 * settings file's mtime living in the backend's `AppState.capture`. If only this
 * flag were lost, the user who came back and pressed Calibrate again would
 * silently re-baseline to *after* EVE's write, and the detection would then be
 * guaranteed to find nothing.
 */
export const captureState = $state<{ active: boolean; note: string | null }>({
  active: false,
  note: null,
});

/**
 * The launcher's answer to the same question, loaded once per SESSION rather
 * than once per mount — which used to be the same thing, because you could not
 * leave the Accounts view without opening a file.
 *
 * Two things break if these die on unmount. `foundCards` is recorded at load and
 * never pruned precisely because `proposals` empties as they are accepted;
 * recomputing it from a fresh read makes the view state "your logs say nothing"
 * about accounts whose proposals were just acted on. And `dismissed` resets, so
 * every "Keep mine" is undone — session-only is the right call, but dismissing
 * the sheet is not the end of the session.
 *
 * `dismissed` stays unpersisted. That is v0.34's judgement and this phase keeps
 * it: a "keep mine" is a judgement about this sitting.
 */
export const launcherState = $state<{
  proposals: Proposal[];
  loaded: boolean;
  known: number;
  foundCards: number[];
  dismissed: number[];
}>({ proposals: [], loaded: false, known: 0, foundCards: [], dismissed: [] });

/**
 * The only place the backend baseline is discarded.
 *
 * Both ENDINGS of the flow come through here — cancelled, and resolved into a
 * confirmed pairing — so `captureState` and `AppState.capture` cannot disagree.
 * Dismissing the sheet is not an ending and does not call it (§4.4.4): doing so
 * would destroy the very thing that outliving the sheet exists to protect.
 *
 * The flag clears BEFORE the await, so a rejected `invoke` cannot strand the
 * wizard on screen. No try/catch: `clear_capture` returns `()` and cannot fail,
 * and if the IPC bridge itself is gone the baseline is gone with it.
 */
export async function endCapture(note: string | null = null): Promise<void> {
  captureState.active = false;
  captureState.note = note;
  await api.clearCapture();
}

/** Test-only, called from the shared `afterEach`. Both runes above outlive
 *  `cleanup()`, which resets the DOM but not a module's state. */
export function resetAccountsSession(): void {
  captureState.active = false;
  captureState.note = null;
  launcherState.proposals = [];
  launcherState.loaded = false;
  launcherState.known = 0;
  launcherState.foundCards = [];
  launcherState.dismissed = [];
}
