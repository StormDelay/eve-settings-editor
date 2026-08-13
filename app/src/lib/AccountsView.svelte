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
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import Panel from "./ui/Panel.svelte";
  import PanelHeader from "./ui/PanelHeader.svelte";

  let { openPath }: { openPath: string | null } = $props();

  const MAX = 3;
  const roster = $derived(accountsStore.roster);
  let error: string | null = $state(null);
  // Per-card selection for the "＋ add character" picker, cleared as soon as the
  // pick is acted on so the control returns to its prompt.
  let addPick: Record<number, string> = $state({});

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
  <PanelHeader class="accounts-head" title="Accounts" level={2}>
    {#snippet actions()}
      {#if allPairs.length > 0}
        <Button variant="primary" onclick={acceptAll}>
          Accept all — {allPairs.length} character{allPairs.length === 1 ? "" : "s"}
        </Button>
      {/if}
      <Button onclick={() => loadRoster()}>Refresh</Button>
      <Button onclick={startCapture}>Calibrate an account…</Button>
    {/snippet}
  </PanelHeader>

  {#if capturing}
    <Panel class="capture" as="div" role="dialog" aria-label="Calibrate an account">
      <p>1. Launch EVE and log in as the character whose account you want to identify.</p>
      <p>2. Change an account-wide setting (e.g. toggle Camera Shake under Settings → Display &amp; Graphics) so the account file is written.</p>
      <p>3. Fully log out / close the client, then click Done.</p>
      <div class="capture-actions">
        <Button variant="primary" onclick={finishCapture}>Done</Button>
        <Button onclick={() => (capturing = false)}>Cancel</Button>
      </div>
    </Panel>
  {/if}

  {#if error}<InlineMessage variant="error">{error}</InlineMessage>{/if}
  <!-- captureNote stays in place rather than becoming a toast, though §5.12
       nominates it. It never auto-cleared — it only ever borrowed `.flash`'s
       class — and it explains what to do next about the calibration panel
       directly above it. Moving it to a corner, or giving it a timer, would be
       a behaviour change. -->
  {#if captureNote}<InlineMessage>{captureNote}</InlineMessage>{/if}

  <!-- InlineMessage, not EmptyState: the account cards follow it. -->
  {#if proposalsLoaded && !everFound}
    <InlineMessage>
      Your EVE launcher logs say nothing about these accounts — use “Calibrate an account…”
      to pair a character by hand.
    </InlineMessage>
  {/if}

  {#if accounts.length === 0}
    <EmptyState
      title="No accounts in this profile yet."
      description="Open a profile file, or run a calibration." />
  {/if}

  <ul class="cards">
    {#each accounts as acct (acct.user_id)}
      {@const card = byCard.get(acct.user_id)}
      {@const ghosts = card?.ghosts ?? []}
      {@const free = Math.max(0, MAX - acct.characters.length)}
      <li class="card">
        <Field
          kind="text"
          class="alias"
          ariaLabel="Account alias"
          value={acct.alias ?? ""}
          placeholder={`core_user_${acct.user_id}`}
          onblur={(e: FocusEvent & { currentTarget: HTMLInputElement }) =>
            commitAlias(acct.user_id, e.currentTarget.value)}
          onkeydown={(e: KeyboardEvent & { currentTarget: HTMLInputElement }) =>
            e.key === "Enter" && e.currentTarget.blur()} />
        <div class="slots">
          {#each Array(MAX) as _, i (i)}
            {@const charId = acct.characters[i]}
            {#if charId != null}
              <Chip class="filled">
                {nameOf(charId)}
                {#snippet actions()}
                  <Button variant="ghost" size="sm" iconOnly title="Unpair" onclick={() => unpair(charId)}>
                    ✕
                  </Button>
                {/snippet}
              </Chip>
            {:else}
              {@const slot = i - acct.characters.length}
              {#if ghosts[slot] != null}
                {@const gid = ghosts[slot]}
                <!-- The proposal is the thing on this card that needs an
                     answer, so it is drawn LOUDER than the settled chips beside
                     it: dashed border, --info tone, and no opacity at all. It
                     used to be a settled chip minus 15% opacity, which is
                     exactly why it was reported as not visible enough. -->
                <Chip state="proposed" title="From your launcher log">
                  {nameOf(gid)}
                  {#snippet actions()}
                    <Button variant="ghost" size="sm" iconOnly title="Accept {nameOf(gid)}"
                            onclick={() => onConfirm(gid, acct.user_id)}>✓</Button>
                    <Button variant="ghost" size="sm" iconOnly title="Dismiss {nameOf(gid)}"
                            onclick={() => (dismissed = [...dismissed, gid])}>✕</Button>
                  {/snippet}
                </Chip>
              {:else}
                <Field
                  kind="select"
                  class="add-char"
                  ariaLabel="Add a character to this account"
                  bind:value={addPick[acct.user_id]}
                  onchange={() => {
                    const v = Number(addPick[acct.user_id]);
                    if (v) onConfirm(v, acct.user_id);
                    addPick[acct.user_id] = "";
                  }}
                  options={[
                    { value: "", label: "＋ add character" },
                    ...sortedUnassigned.map((uid) => ({ value: String(uid), label: nameOf(uid) })),
                  ]} />
              {/if}
            {/if}
          {/each}
        </div>
        <!-- Only when a ghost actually got a slot: with the card full, every
             ghost is overflow and each carries its own line already. -->
        {#if ghosts.length > 0 && free > 0}
          <InlineMessage class="from-launcher">From your launcher log.</InlineMessage>
        {/if}
        {#each ghosts.slice(free) as gid (gid)}
          <InlineMessage class="from-launcher">
            Your launcher log also puts {nameOf(gid)} here, but all three slots are full.
            <Button size="sm" onclick={() => onConfirm(gid, acct.user_id)}>Accept anyway</Button>
          </InlineMessage>
        {/each}
        <!-- A conflict is --warn, not --info: it reports a disagreement with
             what is stored, and one of the two has to lose. -->
        {#each card?.conflicts ?? [] as c (c.charId)}
          <InlineMessage variant="warn" class="conflict">
            Your launcher log puts {nameOf(c.charId)} on {accountLabel(c.target)}.
            <Button size="sm" aria-label="Move {nameOf(c.charId)}"
                    onclick={() => onConfirm(c.charId, c.target)}>Move it</Button>
            <Button size="sm" aria-label="Keep {nameOf(c.charId)}"
                    onclick={() => (dismissed = [...dismissed, c.charId])}>Keep mine</Button>
          </InlineMessage>
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
  /* `--line` and `--panel` are gone. They were referenced four times in this
     file and declared in no stylesheet in the repo, so every card, chip and
     panel border here fell back to #3333 — a colour no other view used. The
     no-undefined-tokens guard is what would have caught it. */
  .accounts { padding: var(--s4); overflow: auto; }
  .cards { list-style: none; padding: 0; display: grid; gap: var(--s3); }
  .card { border: 1px solid var(--border); border-radius: var(--r-md); padding: var(--s2); }
  .accounts :global(.alias input) { font-weight: 600; width: 100%; }
  .accounts :global(.alias) { margin-bottom: var(--s2); }
  .slots { display: flex; gap: var(--s1); flex-wrap: wrap; align-items: center; }
  .accounts :global(.from-launcher),
  .accounts :global(.conflict) { margin: var(--s1) 0 0; }
  .accounts :global(.capture) { margin: var(--s3) 0; background: var(--surface-raised); }
  .capture-actions { display: flex; gap: var(--s2); margin-top: var(--s2); }
  /* Rank by weight, not by dimming: this is a section heading, so it is a
     heading, at caption size to suit the density. */
  .unassigned h3 {
    margin: var(--s4) 0 var(--s1);
    font-size: var(--t-caption);
    font-weight: 600;
    color: var(--text-secondary);
  }
</style>
