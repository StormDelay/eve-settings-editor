<script lang="ts">
  import { api, errMessage, type Profile, type Proposal, type Rejected } from "./api";
  import { names, resolveNames } from "./names.svelte";
  import { resolvedName } from "./filesort.svelte";
  import {
    accountsStore,
    loadRoster,
    setAlias,
    confirmPairing,
    confirmMany,
    unpair,
    aliasFor,
  } from "./accounts.svelte";
  import { proposalsByCard, acceptAllPairs } from "./launcher";

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
  // Order the character pickers the same way the sidebar and batch view do:
  // named characters alphabetically, bare ids after them.
  const sortedUnassigned = $derived(
    [...unassigned].sort((a, b) => {
      const na = resolvedName("char", a);
      const nb = resolvedName("char", b);
      if (na && nb) return na.localeCompare(nb);
      if (na) return -1;
      if (nb) return 1;
      return a - b;
    }),
  );

  // Guided capture state (see Task 11 for the flow body).
  let capturing = $state(false);
  let captureNote: string | null = $state(null);

  const nameOf = (id: number) => names[id]?.name ?? `char ${id}`;

  // Launcher-log proposals. Loaded once on mount: unlike the roster, this does
  // not change when the user edits an alias, and re-reading the logs on every
  // roster refresh would be waste.
  let proposals = $state<Proposal[]>([]);
  let proposalsLoaded = $state(false);
  // The cards the logs ever said anything about. Recorded at load and never
  // pruned, because `proposals` empties as they are accepted and "your logs say
  // nothing" is a lie once they have been acted on.
  let foundCards = $state<number[]>([]);
  // Session-only, like the M3b suggestion dismissals: a "keep mine" is a
  // judgement about this sitting, not something to persist.
  let dismissed = $state<number[]>([]);
  const dismissedSet = $derived(new Set(dismissed));
  const byCard = $derived(proposalsByCard(proposals, dismissedSet));
  // Exactly the cards on screen, and nothing else. `accounts` is scoped to the
  // profile folder the open file lives in; an unscoped accept would write
  // pairings for accounts the user has no card for, never saw a ghost for, and
  // had no way to dismiss — the one thing this feature may never do.
  const onScreen = $derived(new Set(accounts.map((a) => a.user_id)));
  const allPairs = $derived(
    acceptAllPairs(proposals, dismissedSet).filter(([, userId]) => onScreen.has(userId)),
  );
  // Scoped for the same reason: proposals for accounts outside this profile
  // folder render no card and no Accept all, so counting them would suppress the
  // hint and leave a blank state that explains nothing.
  const everFound = $derived(foundCards.some((u) => onScreen.has(u)));

  const accountLabel = (userId: number) => aliasFor(userId) ?? `core_user_${userId}`;

  // Name the character and the account rather than echoing a bare cap message —
  // "Account already has 3 characters" does not say WHICH account, and the user
  // has to know that to fix it.
  const rejectionText = (r: Rejected) =>
    `${nameOf(r.char_id)} could not join ${accountLabel(r.user_id)} — ` +
    `${r.reason.charAt(0).toLowerCase()}${r.reason.slice(1)}. Unpair one there and try again.`;

  async function acceptAll() {
    error = null;
    // Capture before the await: `allPairs` is derived from `proposals` and
    // `dismissed`, either of which the user can change while the request is in
    // flight (dismiss a ghost, click "Keep mine" elsewhere). Re-reading the
    // derived value afterwards would drop a different set than was actually
    // sent, leaving the confirmed character in `proposals` to re-render as a
    // duplicate ghost — the exact bug this filtering exists to prevent.
    const pairs = allPairs;
    try {
      const rejected = await confirmMany(pairs);
      // Drop what actually landed — NOT everything sent. `proposalsByCard`
      // cannot see the roster, so a proposal left in the list re-renders as a
      // ghost in the next empty slot; and a rejected one must stay, because its
      // ghost is the affordance for retrying after an unpair.
      const failed = new Set(rejected.map((r) => r.char_id));
      const accepted = new Set(pairs.map(([charId]) => charId).filter((c) => !failed.has(c)));
      proposals = proposals.filter((p) => !accepted.has(p.char_id));
      if (rejected.length > 0) error = rejected.map(rejectionText).join(" ");
    } catch (e) {
      error = errMessage(e);
    }
  }

  async function onConfirm(charId: number, userId: number) {
    error = null;
    try {
      await confirmPairing(charId, userId);
      proposals = proposals.filter((p) => p.char_id !== charId);
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
        await confirmPairing(charId, userId); // already refreshes the roster
        // Same pruning `onConfirm` does: a character the launcher also proposed
        // would otherwise stay a ghost on the very card it now fills, and still
        // count towards Accept all.
        proposals = proposals.filter((p) => p.char_id !== charId);
        captureNote = `Paired ${nameOf(charId)} ↔ account ${userId}.`;
        capturing = false;
      } catch (e) {
        captureNote = errMessage(e);
      }
      return;
    }
    if (r.changed_users.length === 0) {
      captureNote =
        "The account file didn't change. Make an account-wide change (so core_user is written), fully log out, then click Done again.";
    } else if (r.changed_users.length > 1) {
      captureNote = `Several account files changed (${r.changed_users.join(", ")}). Log out of just one account and retry.`;
    } else if (r.changed_chars.length > 1) {
      captureNote =
        "Several character files changed — log in as just one character, change something, log out, and retry.";
    } else {
      captureNote = "No matching character file changed — log in as one character, change something, log out, and retry.";
    }
    await loadRoster();
  }

  loadRoster();
  api
    .launcherProposals()
    .then(async (p) => {
      proposals = p;
      // A disputed proposal shows on the card that holds the chip today, not on
      // the one the launcher names — same routing as `proposalsByCard`.
      foundCards = p.map((x) => x.conflict ?? x.user_id);
      await resolveNames(p.map((x) => x.char_id));
    })
    .catch(() => {})
    .finally(() => (proposalsLoaded = true));
</script>

<section class="accounts">
  <header class="accounts-head">
    <h2>Accounts</h2>
    <div class="head-actions">
      {#if allPairs.length > 0}
        <button onclick={acceptAll}>
          Accept all — {allPairs.length} character{allPairs.length === 1 ? "" : "s"}
        </button>
      {/if}
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

  {#if proposalsLoaded && !everFound}
    <p class="hint">
      Your EVE launcher logs say nothing about these accounts — use “Calibrate an account…”
      to pair a character by hand.
    </p>
  {/if}

  {#if accounts.length === 0}
    <p class="hint">No accounts in this profile yet. Open a profile file, or run a calibration.</p>
  {/if}

  <ul class="cards">
    {#each accounts as acct (acct.user_id)}
      {@const card = byCard.get(acct.user_id)}
      {@const ghosts = card?.ghosts ?? []}
      {@const free = Math.max(0, MAX - acct.characters.length)}
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
              {@const slot = i - acct.characters.length}
              {#if ghosts[slot] != null}
                {@const gid = ghosts[slot]}
                <span class="chip ghost">
                  {nameOf(gid)}
                  <button class="ok" title="Accept {nameOf(gid)}"
                          aria-label="Accept {nameOf(gid)}"
                          onclick={() => onConfirm(gid, acct.user_id)}>✓</button>
                  <button class="x" title="Dismiss {nameOf(gid)}"
                          aria-label="Dismiss {nameOf(gid)}"
                          onclick={() => (dismissed = [...dismissed, gid])}>✕</button>
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
                    {#each sortedUnassigned as uid (uid)}
                      <option value={uid}>{nameOf(uid)}</option>
                    {/each}
                  </select>
                </span>
              {/if}
            {/if}
          {/each}
        </div>
        <!-- Only when a ghost actually got a slot: with the card full, every
             ghost is overflow and each carries its own line already. -->
        {#if ghosts.length > 0 && free > 0}
          <p class="from-launcher">From your launcher log.</p>
        {/if}
        {#each ghosts.slice(free) as gid (gid)}
          <p class="from-launcher">
            Your launcher log also puts {nameOf(gid)} here, but all three slots are full.
            <button onclick={() => onConfirm(gid, acct.user_id)}>Accept anyway</button>
          </p>
        {/each}
        {#each card?.conflicts ?? [] as c (c.charId)}
          <p class="conflict">
            Your launcher log puts {nameOf(c.charId)} on {accountLabel(c.target)}.
            <button aria-label="Move {nameOf(c.charId)}"
                    onclick={() => onConfirm(c.charId, c.target)}>Move it</button>
            <button aria-label="Keep {nameOf(c.charId)}"
                    onclick={() => (dismissed = [...dismissed, c.charId])}>Keep mine</button>
          </p>
        {/each}
      </li>
    {/each}
  </ul>

  {#if unassigned.length > 0}
    <div class="unassigned">
      <h3>Unassigned characters</h3>
      <ul>
        {#each sortedUnassigned as uid (uid)}
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
  .chip.ghost { border-style: dashed; opacity: 0.85; }
  .ok { border: none; background: transparent; cursor: pointer; color: inherit; }
  .from-launcher { margin: 0.3rem 0 0; font-size: 0.85em; opacity: 0.7; }
  .conflict { margin: 0.3rem 0 0; font-size: 0.9em; }
  .x { border: none; background: transparent; cursor: pointer; color: inherit; }
  .error { color: #c0392b; }
  .capture { border: 1px solid var(--line, #3333); border-radius: 8px; padding: 0.75rem;
             margin: 0.75rem 0; background: var(--panel, #0001); }
  .capture-actions { display: flex; gap: 0.5rem; margin-top: 0.5rem; }
  .unassigned h3 { margin: 1rem 0 0.3rem; font-size: 0.9em; opacity: 0.7; }
</style>
