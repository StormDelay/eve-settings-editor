<script lang="ts">
  // History replaces the permanent 280px backups column.
  //
  // How it makes its subject unambiguous: it stops having one. `list_file_backups`
  // is per-slot server-side, so this asks for EVERY open slot and renders one
  // titled group each. Nothing here is derived from `view`, so the content is
  // identical on every tab — fault (b) closed by construction, on top of the
  // deletion of `active`'s first clause in the shell.
  import { subject } from "./subject.svelte";
  import type { OpenOutcome, Slot } from "./api";
  import BackupsPanel from "./BackupsPanel.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Popover from "./ui/Popover.svelte";

  let {
    anchor,
    onclose,
    onRestored,
  }: {
    anchor: HTMLElement;
    onclose: () => void;
    onRestored: (slot: Slot, outcome: OpenOutcome) => void;
  } = $props();

  // A slot with no open document contributes NO group, rather than an empty one.
  const groups = $derived(
    (["char", "user"] as const)
      .map((slot) => {
        const o = subject.slots[slot];
        if (o?.status !== "opened") return null;
        return {
          slot,
          subjectName: (slot === "char" ? subject.charName : subject.userAlias) ?? o.file_name,
          fileName: o.file_name,
        };
      })
      .filter((g) => g !== null),
  );
</script>

<Popover {anchor} placement="bottom-end" {onclose} ariaLabel="History" class="history">
  <p class="section">History</p>
  {#if groups.length === 0}
    <EmptyState title="Nothing open." description="Backups are listed per open file." />
  {:else}
    {#each groups as g (g.slot)}
      <BackupsPanel
        slot={g.slot}
        subjectName={g.subjectName}
        fileName={g.fileName}
        savedAt={subject.savedAt}
        {onRestored} />
    {/each}
    <p class="note">
      Every save writes a backup first. Restoring one also backs up the file it replaces.
    </p>
  {/if}
</Popover>

<style>
  :global(.popover.history) {
    width: min(30rem, 90vw);
    max-height: min(30rem, 80vh);
    overflow-y: auto;
    padding: var(--s2);
  }
  .section {
    margin: 0 0 var(--s2);
    font-size: var(--t-caption);
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .note {
    margin: 0;
    font-size: var(--t-caption);
    color: var(--text-muted);
  }
</style>
