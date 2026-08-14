<script lang="ts">
  // A subject browser and nothing else.
  //
  // Its top block used to hold six buttons of four different kinds — Open file…,
  // rescan, Refresh names, Accounts, Copy settings, About — in a `flex-wrap:
  // wrap` container above a list none of them were about. Five moved to the app
  // menu. "Open file…" stayed, because it IS a file-list operation and it is the
  // only route to an account file directly.
  //
  // The per-profile `<details>` loop became ONE flat list of the selected
  // profile's characters, in exactly the order it had before. Account grouping
  // was proposed and rejected: browsing is this column's whole job, it happens
  // every session, and grouping breaks alphabetical order and makes a character
  // harder to find. Knowing which characters share settings matters at EDIT
  // time, and is stated at both moments it bites — the save disclosure names the
  // siblings at the moment of writing, and `ScopeBanner` says so on every
  // account-scoped view before the edit.
  import { subject, accountAliasOf, noCharactersHint } from "./subject.svelte";
  import { resolvedName } from "./filesort.svelte";
  import { profileLabels, primaryProfileDir, profileNote } from "./profiles";
  import PresetGroup from "./PresetGroup.svelte";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";
  import type { PresetInfo } from "./api";

  let {
    onOpen,
    onPickFile,
    onCollapse,
    onOpenPreset,
    onShowAccounts,
  }: {
    onOpen: (path: string) => void;
    /** The OS file dialog lives in the shell, because the launch empty state
     *  offers the same button and two copies of it would be two copies. */
    onPickFile: () => void;
    onCollapse: () => void;
    onOpenPreset: (p: PresetInfo) => void;
    /** Kept, though §7 lists it among the toolbar's deleted props: §5.7 gives
     *  every chip-less row a `Link…` action, and this is where it goes. Since
     *  0.34 that is often a one-click fix rather than a manual pairing chore,
     *  because the launcher's own proposal is waiting there with an Accept. */
    onShowAccounts: () => void;
  } = $props();

  // The scan itself belongs to the shell now — it fires once on mount, for the
  // store. This column used to run its own `api.discover()` beside it, so the
  // app sent `discover_profiles` twice on every start.
  const labels = $derived(profileLabels(subject.profiles));
  const primaryDir = $derived(primaryProfileDir(subject.profiles));
  const isPrimary = $derived(subject.profileDir !== null && subject.profileDir === primaryDir);

  // The selector carries the folder count, so it is never a secret that other
  // folders exist — which is the accepted cost of single-select.
  const options = $derived(
    subject.profiles.map((p) => ({ value: p.dir, label: labels.get(p.dir) ?? p.dir })),
  );

  const openPath = $derived(
    subject.slots.char?.status === "opened" ? subject.slots.char.path : null,
  );
</script>

<aside class="sidebar">
  <div class="sidebar-top">
    <Field
      kind="select"
      label="Profile"
      class="profile"
      layout="column"
      width="100%"
      options={options}
      bind:value={subject.selectedProfileDir}
      title={subject.profileDir ?? undefined} />
    <Button variant="ghost" size="sm" iconOnly title="Hide file list" onclick={onCollapse}>«</Button>
  </div>
  {#if subject.profiles.length > 0}
    <!-- Was a `<span class="meta">` inside the profile's `<summary>`, so it
         wrapped with the label and repeated once per folder. One selection, one
         chip. A non-live profile is a real hazard, not a detail: editing one
         looks like it worked and changes nothing the game reads. -->
    <p class="status">
      <Chip tone={isPrimary ? "ok" : "warn"} size="sm">{profileNote(isPrimary)}</Chip>
      {#if subject.profiles.length > 1}
        <span class="count">{subject.profiles.length} profiles</span>
      {/if}
    </p>
  {/if}

  {#if subject.profilesError}<InlineMessage variant="error">{subject.profilesError}</InlineMessage>{/if}
  <!-- InlineMessage rather than EmptyState, though §4.5 files these under
       "there is nothing here". Two reasons: content follows them (the preset
       group and any character rows), so EmptyState's centred 32px block would
       push that down, and Sidebar.spec asserts that the "no character files"
       hint is ONE element whose text also names "Open file…" — splitting it
       across EmptyState's title and description would break that. -->
  {#if subject.profiles.length === 0}
    <InlineMessage>No EVE profiles found in standard locations. Use “Open file…”.</InlineMessage>
  {:else if subject.characters.length === 0}
    <InlineMessage>{noCharactersHint()}</InlineMessage>
  {/if}

  <ul>
    {#each subject.characters as f (f.path)}
      {@const alias = accountAliasOf(f)}
      <li>
        <!-- The KB per row is gone from the row and folded into its tooltip,
             beside the file name it belongs to. Every `core_char_*.dat` in a
             profile is within a few KB of every other, so the number never
             separated two rows, never indicated health, and never answered a
             question anyone brought to the app — pure per-row noise in the one
             list that has to be scannable. Sizes that DO answer a question stay
             where they do it: `bytes_written` in the save result, and per-backup
             size in History. -->
        <ListRow
          class={alias ? "paired" : ""}
          onclick={() => onOpen(f.path)}
          title="{f.file_name} · {Math.round(f.size / 1024)} KB{alias ? ` · account ${alias}` : ''}">
          {#snippet leading()}
            <span class="dot" class:open={f.path === openPath} aria-hidden="true">●</span>
          {/snippet}
          {resolvedName(f.kind, f.id) ?? f.file_name}
          <!-- Chip and action both in `trailing`, NOT beside the name, so the
               chip cannot be clipped away by a long name — "no chip means no
               account" has to be a rule rather than a coin flip.

               But the chip does not win the row either. It was drawn `nowrap`,
               which made the CHARACTER NAME the thing that gave way, and an
               account whose characters share a prefix ("Storm Hold…" four times
               over) is exactly where that hurts most. Owner's call,
               2026-08-14: the name has priority, the chip ellipsises first, and
               the full account is in the row's tooltip. See the flex rules
               below — this is the whole of that decision.

               A Chip is a CONFIRMED pairing and nothing else. No chip means no
               account — including for a character the launcher merely proposes,
               which is truthful, because a proposed character can do exactly
               what an unpaired one can until someone accepts it. -->
          {#snippet trailing()}
            {#if alias}
              <Chip size="sm" title={alias}><span class="alias">{alias}</span></Chip>
            {:else}
              <Button
                variant="ghost"
                size="sm"
                class="link-btn"
                title="Pair this character with an account"
                onclick={onShowAccounts}>Link…</Button>
            {/if}
          {/snippet}
        </ListRow>
      </li>
    {/each}
  </ul>

  <PresetGroup
    {onOpenPreset}
    charOpen={subject.slots.char?.status === "opened"}
    userOpen={subject.slots.user?.status === "opened"}
    openPresetName={subject.preset} />

  <div class="foot">
    <div class="toggle" title="Show only EVE's own core_char_<id>.dat files">
      <Field kind="checkbox" label="Hide non-standard files" bind:value={subject.hideNonStandard} />
    </div>
    <Button onclick={onPickFile}>Open file…</Button>
  </div>
</aside>

<style>
  .sidebar-top {
    display: flex;
    align-items: flex-start;
    gap: var(--s2);
    margin-bottom: var(--s2);
  }
  .sidebar-top :global(.profile) {
    flex: 1;
    min-width: 0;
  }
  .status {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin: 0 0 var(--s2);
  }
  .count {
    color: var(--text-muted);
    font-size: var(--t-caption);
  }
  ul {
    list-style: none;
    margin: var(--s1) 0;
    padding: 0;
  }
  li {
    list-style: none;
  }
  /* The open marker occupies its slot on every row, so a row does not shift
     sideways when it becomes the open one. */
  .dot {
    color: transparent;
    font-size: var(--t-caption);
  }
  .dot.open {
    color: var(--accent);
  }
  /* Recedes by COLOUR until the row is hovered, never by hiding: `.mini`'s
     `opacity: 0` shipped four permanently-invisible but still-clickable buttons,
     and Phase 1 retired that pattern. Most rows in a real install are unpaired,
     so at full strength this reads as a column of buttons rather than a column
     of names. */
  li :global(.link-btn) {
    color: var(--text-muted);
  }
  li :global(.row:hover .link-btn) {
    color: var(--accent);
  }
  /* Which of the two gives up its width first. ListRow's `.trailing` is
     `nowrap`, so by default the chip kept every pixel it wanted and the name
     ellipsised — the wrong way round, because the name is what identifies the
     row and the account is a qualifier on it. A shrink factor this large means
     the chip is down to a stub before the name loses a character; past that the
     name ellipsises too, and the row's tooltip has both in full. */
  /* ONLY on a row that carries a chip. An unpaired row's trailing content is the
     `Link…` BUTTON, and a squeezed button is worse than a truncated name — it
     was being clipped against the panel edge. Those rows keep ListRow's own
     rules, where the label has a zero basis, grows into what is left and
     ellipsises on its own.

     On a paired row the two do compete, and `flex: 1` is why the chip used to
     win by default: it is `flex: 1 1 0%`, a ZERO basis, so the label never took
     part in shrinking at all — it just grew into whatever the chip left over.
     A content basis is what puts them in competition; only then does the shrink
     factor decide, and 999 means the chip is a stub before the name gives up a
     character. */
  li :global(.row.paired .label) {
    flex: 1 1 auto;
  }
  li :global(.row.paired .trailing) {
    flex-shrink: 999;
    min-width: 0;
  }
  li :global(.row.paired .trailing .chip) {
    min-width: 0;
  }
  .alias {
    display: block;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .foot {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--s2);
    margin-top: var(--s3);
    padding-top: var(--s2);
    border-top: 1px solid var(--border);
  }
  .toggle {
    cursor: pointer;
  }
</style>
