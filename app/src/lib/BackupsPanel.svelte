<script lang="ts">
  import { ask, message } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, type BackupInfo, type OpenOutcome, type Slot } from "./api";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";

  let {
    slot,
    savedAt,
    subtitle,
    onRestored,
    onCollapse,
  }: {
    slot: Slot;
    savedAt: number;
    subtitle: string | null;
    onRestored: (outcome: OpenOutcome) => void;
    onCollapse: () => void;
  } = $props();

  let backups: BackupInfo[] = $state([]);
  let error: string | null = $state(null);

  // Refetch on save (savedAt bumps), on active-slot switch, and on mount.
  $effect(() => {
    void savedAt;
    void slot;
    api.listBackups(slot).then(
      (b) => {
        backups = b;
        error = null;
      },
      (e) => (error = errMessage(e)),
    );
  });

  async function restore(b: BackupInfo) {
    const yes = await ask(
      `Replace the current file with this backup?\n\n${b.file_name}\n\n` +
        "The current file is backed up first, so this is reversible.",
      { title: "Restore backup", kind: "warning" },
    );
    if (!yes) return;
    try {
      onRestored(await api.restoreBackup(slot, b.path));
    } catch (e) {
      await message(errMessage(e), { title: "Restore failed", kind: "error" });
    }
  }
</script>

<aside class="backups">
  <!-- Deliberately NOT PanelHeader, though §5.6 nominates this file as the
       reason its `subtitle` slot exists. PanelHeader lays the subtitle out
       beside the title; here it is a full file path on its own line below, and
       moving it inline would both squeeze it and break Phase 1's rule that
       nothing moves. What this file actually needed from PanelHeader was the
       legibility — opacity .7 at Lc 40.6 becoming --text-muted at Lc 71.1 — and
       that is a token, not a component. -->
  <div class="backups-head">
    <Button variant="ghost" size="sm" iconOnly title="Hide backups" onclick={onCollapse}>
      »
    </Button>
    <h3>Backups</h3>
  </div>
  {#if subtitle}<p class="subtitle" title={subtitle}>{subtitle}</p>{/if}
  {#if error}<InlineMessage variant="error">{error}</InlineMessage>{/if}
  {#if backups.length === 0}
    <EmptyState title="No backups yet." description="Every save creates one." />
  {/if}
  <ul>
    {#each backups as b (b.path)}
      <li>
        <ListRow>
          <span class="stamp">{b.file_name.split(".").slice(-2, -1)[0]}</span>
          {#snippet trailing()}
            <span class="meta">{Math.round(b.size / 1024)} KB</span>
            <Button variant="ghost" size="sm" onclick={() => restore(b)}>restore</Button>
          {/snippet}
        </ListRow>
      </li>
    {/each}
  </ul>
</aside>

<style>
  .backups-head {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin-bottom: var(--s2);
  }
  /* The default h3 top margin pushes the whole head (and its chevron) down;
     zero it so the chevron pins to the top-left, symmetric with the sidebar's. */
  .backups-head h3 {
    margin: 0;
    font-size: var(--t-title);
  }
  .subtitle {
    margin: 0 0 var(--s2);
    font-size: var(--t-caption);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    color: var(--text-muted);
  }
</style>
