<script lang="ts">
  import { api, errMessage, type Profile } from "./api";
  import { names } from "./names.svelte";
  import { accountsStore, loadRoster, setAlias, confirmPairing, unpair } from "./accounts.svelte";

  let { openPath }: { openPath: string | null } = $props();

  const MAX = 3;
  const roster = $derived(accountsStore.roster);
  let error: string | null = $state(null);

  // Scope the panel to the profile folder the open file lives in: only that
  // folder's accounts and characters (located via discovery) are shown/offered.
  let profiles = $state<Profile[]>([]);
  api.discover().then((p) => (profiles = p)).catch(() => {});
  const scope = $derived.by(() => {
    if (!openPath) return null;
    const p = profiles.find((pr) => pr.files.some((f) => f.path === openPath));
    if (!p) return null;
    const users = new Set(
      p.files.filter((f) => f.kind === "user" && f.id != null).map((f) => f.id as number),
    );
    const chars = new Set(
      p.files.filter((f) => f.kind === "char" && f.id != null).map((f) => f.id as number),
    );
    return { users, chars };
  });
  const accounts = $derived(
    scope ? roster.accounts.filter((a) => scope.users.has(a.user_id)) : roster.accounts,
  );
  const unassigned = $derived(
    scope ? roster.unassigned.filter((id) => scope.chars.has(id)) : roster.unassigned,
  );

  // Guided capture state (see Task 11 for the flow body).
  let capturing = $state(false);
  let captureNote: string | null = $state(null);

  const nameOf = (id: number) => names[id]?.name ?? `char ${id}`;

  async function onConfirm(charId: number, userId: number) {
    error = null;
    try {
      await confirmPairing(charId, userId);
    } catch (e) {
      error = errMessage(e);
    }
  }

  async function commitAlias(userId: number, value: string) {
    await setAlias(userId, value.trim() === "" ? null : value);
  }

  async function startCapture() {
    captureNote = null;
    await api.beginCapture();
    capturing = true;
  }

  async function finishCapture() {
    const r = await api.resolveCapture();
    if (r.detected) {
      const [charId, userId] = r.detected;
      try {
        await confirmPairing(charId, userId);
        captureNote = `Paired ${nameOf(charId)} ↔ account ${userId}.`;
        capturing = false;
      } catch (e) {
        captureNote = errMessage(e);
      }
    } else if (r.changed_users.length === 0) {
      captureNote =
        "The account file didn't change. Make an account-wide change (so core_user is written), fully log out, then click Done again.";
    } else if (r.changed_users.length > 1) {
      captureNote = `Several account files changed (${r.changed_users.join(", ")}). Log out of just one account and retry.`;
    } else {
      captureNote = "No matching character file changed — log in as one character, change something, log out, and retry.";
    }
    await loadRoster();
  }

  loadRoster();
</script>

<section class="accounts">
  <header class="accounts-head">
    <h2>Accounts</h2>
    <div class="head-actions">
      <button onclick={() => loadRoster()}>Refresh</button>
      <button onclick={startCapture}>Calibrate an account…</button>
    </div>
  </header>

  {#if capturing}
    <div class="capture" role="dialog" aria-label="Calibrate an account">
      <p>1. Launch EVE and log in as the character whose account you want to identify.</p>
      <p>2. Change an account-wide setting (e.g. toggle Camera Shake under Settings → Display &amp; Graphics) so the account file is written.</p>
      <p>3. Fully log out / close the client, then click Done.</p>
      <div class="capture-actions">
        <button onclick={finishCapture}>Done</button>
        <button onclick={() => (capturing = false)}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if error}<p class="error">{error}</p>{/if}
  {#if captureNote}<p class="flash" aria-live="polite">{captureNote}</p>{/if}

  {#if accounts.length === 0}
    <p class="hint">No accounts in this profile yet. Open a profile file, or run a calibration.</p>
  {/if}

  <ul class="cards">
    {#each accounts as acct (acct.user_id)}
      <li class="card">
        <input
          class="alias"
          value={acct.alias ?? ""}
          placeholder={`core_user_${acct.user_id}`}
          onblur={(e) => commitAlias(acct.user_id, e.currentTarget.value)}
          onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
        <div class="slots">
          {#each Array(MAX) as _, i (i)}
            {@const charId = acct.characters[i]}
            {#if charId != null}
              <span class="chip filled">
                {nameOf(charId)}
                <button class="x" title="Unpair" onclick={() => unpair(charId)}>✕</button>
              </span>
            {:else}
              <span class="chip empty">
                <select
                  onchange={(e) => {
                    const v = Number(e.currentTarget.value);
                    if (v) onConfirm(v, acct.user_id);
                    e.currentTarget.selectedIndex = 0;
                  }}>
                  <option value="">＋ add character</option>
                  {#each unassigned as uid (uid)}
                    <option value={uid}>{nameOf(uid)}</option>
                  {/each}
                </select>
              </span>
            {/if}
          {/each}
        </div>
      </li>
    {/each}
  </ul>

  {#if unassigned.length > 0}
    <div class="unassigned">
      <h3>Unassigned characters</h3>
      <ul>
        {#each unassigned as uid (uid)}
          <li>{nameOf(uid)}</li>
        {/each}
      </ul>
    </div>
  {/if}
</section>

<style>
  .accounts { padding: 1rem; overflow: auto; }
  .accounts-head { display: flex; justify-content: space-between; align-items: baseline; }
  .cards { list-style: none; padding: 0; display: grid; gap: 0.75rem; }
  .card { border: 1px solid var(--line, #3333); border-radius: 8px; padding: 0.6rem; }
  .alias { font-weight: 600; width: 100%; margin-bottom: 0.5rem; }
  .slots { display: flex; gap: 0.4rem; flex-wrap: wrap; }
  .chip { display: inline-flex; align-items: center; gap: 0.3em; padding: 0.15em 0.5em;
          border-radius: 999px; border: 1px solid var(--line, #3333); font-size: 0.9em; }
  .chip.empty select {
    border: none; font: inherit; cursor: pointer;
    background: var(--bg-panel); color: var(--fg);
  }
  .chip.empty option { background: var(--bg-panel); color: var(--fg); }
  .x { border: none; background: transparent; cursor: pointer; color: inherit; }
  .error { color: #c0392b; }
  .capture { border: 1px solid var(--line, #3333); border-radius: 8px; padding: 0.75rem;
             margin: 0.75rem 0; background: var(--panel, #0001); }
  .capture-actions { display: flex; gap: 0.5rem; margin-top: 0.5rem; }
  .unassigned h3 { margin: 1rem 0 0.3rem; font-size: 0.9em; opacity: 0.7; }
</style>
