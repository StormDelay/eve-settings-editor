<script lang="ts">
  import { ask, message } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, type BackupInfo, type OpenOutcome } from "./api";

  let {
    savedAt,
    subtitle,
    onRestored,
  }: {
    savedAt: number;
    subtitle: string | null;
    onRestored: (outcome: OpenOutcome) => void;
  } = $props();

  let backups: BackupInfo[] = $state([]);
  let error: string | null = $state(null);

  // Refetch whenever a save happens (savedAt bumps) and on mount.
  $effect(() => {
    void savedAt;
    api.listBackups().then(
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
      onRestored(await api.restoreBackup(b.path));
    } catch (e) {
      await message(errMessage(e), { title: "Restore failed", kind: "error" });
    }
  }
</script>

<aside class="backups">
  <h3>Backups</h3>
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
  .subtitle {
    margin: -0.25rem 0 0.5rem;
    font-size: 0.85em;
    opacity: 0.7;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
