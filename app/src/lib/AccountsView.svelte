<script lang="ts">
  import { api, errMessage, type Profile, type Rejected } from "./api";
  import { names, resolveNames } from "./names.svelte";
  import { resolvedName } from "./filesort.svelte";
  import {
    accountsStore,
    captureState,
    endCapture,
    launcherState,
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
  import Sheet from "./ui/Sheet.svelte";

  // The view frames ITSELF rather than letting the shell do it. The alternative
  // means plumbing Refresh and Calibrate up through `+page.svelte`, which owns
  // neither `loadRoster()` nor the capture flow.
  let { openPath, onClose }: { openPath: string | null; onClose: () => void } = $props();

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

  const nameOf = (id: number) => names[id]?.name ?? `char ${id}`;

  // Capture progress and the launcher's proposals both live in
  // `accounts.svelte.ts` now, because this panel is a DISMISSABLE sheet: the
  // capture flow requires a trip out to the EVE client, and a dismissal is not
  // the end of the session. See that module for the full reasoning.
  const dismissedSet = $derived(new Set(launcherState.dismissed));
  const byCard = $derived(proposalsByCard(launcherState.proposals, dismissedSet));
  // Exactly the cards on screen, and nothing else. `accounts` is scoped to the
  // profile folder the open file lives in; an unscoped accept would write
  // pairings for accounts the user has no card for, never saw a ghost for, and
  // had no way to dismiss — the one thing this feature may never do.
  const onScreen = $derived(new Set(accounts.map((a) => a.user_id)));
  const allPairs = $derived(
    acceptAllPairs(launcherState.proposals, dismissedSet).filter(([, userId]) =>
      onScreen.has(userId),
    ),
  );
  // Scoped for the same reason: proposals for accounts outside this profile
  // folder render no card and no Accept all, so counting them would suppress the
  // hint and leave a blank state that explains nothing.
  const everFound = $derived(launcherState.foundCards.some((u) => onScreen.has(u)));

  const accountLabel = (userId: number) => aliasFor(userId) ?? `core_user_${userId}`;

  /**
   * The sentence that names what `Accept all` will pair.
   *
   * A count is not an answer to "which characters am I about to assign?", and
   * this is the panel's most consequential control — one click writes up to nine
   * pairings. It names characters, not accounts: each ghost already shows which
   * account it lands on, on the card one line below.
   *
   * It reads the same scoped `allPairs` the click sends, so it can never name a
   * character the click would not pair, nor miss one it would.
   */
  const acceptAllSentence = $derived.by(() => {
    const n = allPairs.map(([charId]) => nameOf(charId));
    if (n.length === 0) return "";
    // Scoped to one profile folder with MAX = 3, nine is the ceiling and every
    // name fits. The "and N more" cap exists only for the unscoped fallback, and
    // that trailing number is the one place a count is allowed — by then the
    // sentence has already named eight.
    const list =
      n.length > 8
        ? `${n.slice(0, 8).join(", ")} and ${n.length - 8} more`
        : n.length === 1
          ? n[0]
          : `${n.slice(0, -1).join(", ")} and ${n[n.length - 1]}`;
    return `Your launcher log pairs ${list}.`;
  });

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
      launcherState.proposals = launcherState.proposals.filter((p) => !accepted.has(p.char_id));
      if (rejected.length > 0) error = rejected.map(rejectionText).join(" ");
    } catch (e) {
      error = errMessage(e);
    }
  }

  async function onConfirm(charId: number, userId: number) {
    error = null;
    try {
      await confirmPairing(charId, userId);
      launcherState.proposals = launcherState.proposals.filter((p) => p.char_id !== charId);
    } catch (e) {
      error = errMessage(e);
    }
  }

  async function commitAlias(userId: number, value: string) {
    await setAlias(userId, value.trim() === "" ? null : value);
  }

  async function startCapture() {
    // A second press must never re-baseline a capture already in flight: it
    // would snapshot the files as they are NOW — after EVE's write — and the
    // detection would then be guaranteed to find nothing.
    if (captureState.active) return;
    captureState.note = null;
    await api.beginCapture();
    captureState.active = true;
  }

  async function finishCapture() {
    const r = await api.resolveCapture();
    if (r.detected) {
      const [charId, userId] = r.detected;
      try {
        await confirmPairing(charId, userId); // already refreshes the roster
        // Same pruning `onConfirm` does: a character the launcher also proposed
        // would otherwise stay a ghost on the very card it now fills, and still
        // count towards Accept all. It has to run BEFORE the note is set.
        launcherState.proposals = launcherState.proposals.filter((p) => p.char_id !== charId);
        // An ending, so the baseline is spent and gets discarded. The note names
        // the account by alias rather than by its bare id, for the reason
        // `rejectionText` already gives: a raw `core_user` number does not tell
        // the user which account they just paired.
        await endCapture(`Paired ${nameOf(charId)} ↔ account ${accountLabel(userId)}.`);
      } catch (e) {
        // NOT an ending: the cap was hit, the wizard stays open, and the next
        // press must diff against the same baseline.
        captureState.note = errMessage(e);
      }
      return;
    }
    // Every branch below is a RETRY, and each retry diffs against the baseline
    // that is still sitting in the backend. None of them ends the capture.
    if (r.changed_users.length === 0) {
      captureState.note =
        "The account file didn't change. Make an account-wide change (so core_user is written), fully log out, then click Done again.";
    } else if (r.changed_users.length > 1) {
      captureState.note = `Several account files changed (${r.changed_users.join(", ")}). Log out of just one account and retry.`;
    } else if (r.changed_chars.length > 1) {
      captureState.note =
        "Several character files changed — log in as just one character, change something, log out, and retry.";
    } else {
      captureState.note = "No matching character file changed — log in as one character, change something, log out, and retry.";
    }
    await loadRoster();
  }

  loadRoster();
  // Once per SESSION, not once per mount. The sheet is dismissable, so
  // re-mounting must not undo a "Keep mine", must not re-parse every launcher
  // log, and must not recompute `foundCards` from a list the accepted proposals
  // have already left — which is what makes the panel call its own logs liars.
  if (!launcherState.loaded) {
    api
      .launcherProposals()
      .then(async (p) => {
        launcherState.proposals = p;
        // A disputed proposal shows on the card that holds the chip today, not
        // on the one the launcher names — same routing as `proposalsByCard`.
        launcherState.foundCards = p.map((x) => x.conflict ?? x.user_id);
        await resolveNames(p.map((x) => x.char_id));
      })
      .catch(() => {})
      .finally(() => (launcherState.loaded = true));
  }
</script>

<Sheet
  title="Accounts"
  titled
  placement="work"
  onclose={onClose}
  class="accounts-sheet"
  data-testid="accounts-backdrop">
  <!-- Refresh and Calibrate belong to the frame; `Accept all` deliberately does
       NOT, because a count is not an answer to "which characters am I about to
       assign?" — it moves into the body with the names (§4.8). -->
  {#snippet actions()}
    <Button onclick={() => loadRoster()}>Refresh</Button>
    <Button onclick={startCapture} disabled={captureState.active}
            disabledReason="A calibration is already in progress">Calibrate an account…</Button>
  {/snippet}

  <!-- One wrapper so this file's scoped rules still have an element of its own
       to hang off: everything below used to sit inside `<section class="accounts">`,
       and the Sheet's own root belongs to the Sheet's scope, not this one. -->
  <div class="accounts">
  {#if captureState.active}
    <!-- A plain Panel, not `role="dialog"`: inside a sheet that would be a
         dialog within a dialog. -->
    <Panel class="capture" as="div">
      <h3>Calibrate an account</h3>
      <p>1. Launch EVE and log in as the character whose account you want to identify.</p>
      <p>2. Change an account-wide setting (e.g. toggle Camera Shake under Settings → Display &amp; Graphics) so the account file is written.</p>
      <p>3. Fully log out / close the client, then click Done.</p>
      <p class="capture-note">You can close this panel and come back — the calibration keeps running.</p>
      <div class="capture-actions">
        <Button variant="primary" onclick={finishCapture}>Done</Button>
        <Button onclick={() => endCapture()}>Cancel</Button>
      </div>
    </Panel>
  {/if}

  {#if error}<InlineMessage variant="error">{error}</InlineMessage>{/if}
  <!-- captureNote stays in place rather than becoming a toast, though §5.12
       nominates it. It never auto-cleared — it only ever borrowed `.flash`'s
       class — and it explains what to do next about the calibration panel
       directly above it. Moving it to a corner, or giving it a timer, would be
       a behaviour change. -->
  {#if captureState.note}<InlineMessage>{captureState.note}</InlineMessage>{/if}

  <!-- Directly above the cards the names appear on, so the sentence and the
       ghosts are one glance apart. This is the sheet's only primary button. -->
  {#if allPairs.length > 0}
    <InlineMessage variant="info" class="accept-all">
      {acceptAllSentence}
      <Button variant="primary" onclick={acceptAll}>
        {allPairs.length === 1 ? "Accept" : "Accept all"}
      </Button>
    </InlineMessage>
  {/if}

  <!-- InlineMessage, not EmptyState: the account cards follow it. -->
  {#if launcherState.loaded && !everFound}
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
                    <!-- Not symmetric, and they should stop looking it: Accept
                         writes to the store, Dismiss is session-only and undone
                         by reopening the app. A bare ✓ beside a bare ✕ at equal
                         weight is part of why the ghost read as decoration
                         rather than as a question. The accessible name is
                         unchanged, and the visible text is a prefix of it. -->
                    <Button variant="primary" size="sm" title="Accept {nameOf(gid)}"
                            aria-label="Accept {nameOf(gid)}"
                            onclick={() => onConfirm(gid, acct.user_id)}>Accept</Button>
                    <Button variant="ghost" size="sm" iconOnly title="Dismiss {nameOf(gid)}"
                            onclick={() => (launcherState.dismissed = [...launcherState.dismissed, gid])}>✕</Button>
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
                    onclick={() => (launcherState.dismissed = [...launcherState.dismissed, c.charId])}>Keep mine</Button>
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
  </div>
</Sheet>

<style>
  /* `--line` and `--panel` are gone. They were referenced four times in this
     file and declared in no stylesheet in the repo, so every card, chip and
     panel border here fell back to #3333 — a colour no other view used. The
     no-undefined-tokens guard is what would have caught it. */
  /* `.accounts`'s own padding and overflow are gone with it: the Sheet owns
     both now, and a scroller inside a scroller is how a sheet ends up with two
     scrollbars. */
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
