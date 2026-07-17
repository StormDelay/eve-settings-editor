<script lang="ts">
  import { ask, message } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, type BackupInfo, type OpenOutcome, type Slot } from "./api";

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
  <div class="backups-head">
    <button class="collapse" onclick={onCollapse} title="Hide backups" aria-label="Hide backups"
      >»</button>
    <h3>Backups</h3>
  </div>
  {#if subtitle}<p class="subtitle" title={subtitle}>{subtitle}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
  {#if backups.length === 0}
    <p class="hint">No backups yet. Every save creates one.</p>
  {/if}
  <ul>
    {#each backups as b (b.path)}
      <li>
        <span class="stamp">{b.file_name.split(".").slice(-2, -1)[0]}</span>
        <span class="meta">{Math.round(b.size / 1024)} KB</span>
        <button class="mini-visible" onclick={() => restore(b)}>restore</button>
      </li>
    {/each}
  </ul>
</aside>

<style>
  .backups-head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
  }
  /* The default h3 top margin pushes the whole head (and its chevron) down;
     zero it so the chevron pins to the top-left, symmetric with the sidebar's. */
  .backups-head h3 {
    margin: 0;
  }
  .collapse {
    padding: 0 6px;
  }
  .subtitle {
    margin: -0.25rem 0 0.5rem;
    font-size: 0.85em;
    opacity: 0.7;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
