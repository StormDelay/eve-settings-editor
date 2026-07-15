<script lang="ts">
  import { api, errMessage, type Suggestion } from "./api";
  import { names } from "./names.svelte";
  import { accountsStore, loadRoster, setAlias, confirmPairing, unpair } from "./accounts.svelte";

  const MAX = 3;
  const roster = $derived(accountsStore.roster);
  let error: string | null = $state(null);

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

  loadRoster();
</script>

<section class="accounts">
  <header class="accounts-head">
    <h2>Accounts</h2>
    <div class="head-actions">
      <button onclick={() => loadRoster()}>Refresh</button>
      <button onclick={() => (capturing = true)}>Calibrate an account…</button>
    </div>
  </header>

  {#if error}<p class="error">{error}</p>{/if}
  {#if captureNote}<p class="flash" aria-live="polite">{captureNote}</p>{/if}

  {#if roster.accounts.length === 0}
    <p class="hint">No accounts discovered yet. Open a profile, or run a calibration.</p>
  {/if}

  <ul class="cards">
    {#each roster.accounts as acct (acct.user_id)}
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
              {@const sugg = acct.suggestions[i - acct.characters.length]}
              {#if sugg}
                <span class="chip ghost" title={`${sugg.basis} (${sugg.confidence})`}>
                  probably {nameOf(sugg.char_id)}?
                  <button class="ok" onclick={() => onConfirm(sugg.char_id, acct.user_id)}>✓</button>
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
                    {#each roster.unassigned as uid (uid)}
                      <option value={uid}>{nameOf(uid)}</option>
                    {/each}
                  </select>
                </span>
              {/if}
            {/if}
          {/each}
        </div>
      </li>
    {/each}
  </ul>

  {#if roster.unassigned.length > 0}
    <div class="unassigned">
      <h3>Unassigned characters</h3>
      <ul>
        {#each roster.unassigned as uid (uid)}
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
  .chip.ghost { opacity: 0.7; font-style: italic; }
  .chip.empty select { border: none; background: transparent; }
  .x, .ok { border: none; background: transparent; cursor: pointer; }
  .error { color: #c0392b; }
  .unassigned h3 { margin: 1rem 0 0.3rem; font-size: 0.9em; opacity: 0.7; }
</style>
