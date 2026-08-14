<script lang="ts">
  // ONE slot's backups, headed by the subject AND the file. It used to be the
  // whole 280px right-hand column, taking a single `slot` derived from the
  // current VIEW — so switching from Overview to Autofill silently replaced the
  // character file's backup list with the account file's, and the only marker
  // was a 0.85em, 0.7-opacity subtitle measured at Lc 42. Restore is
  // destructive, and its confirm named only the backup.
  //
  // `HistoryPopover` now renders one of these per OPEN slot, so the list has no
  // single subject to get wrong. The heading is at --text because it is the
  // thing that says which file you are about to overwrite.
  import { ask, message } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, type BackupInfo, type OpenOutcome, type Slot } from "./api";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";

  let {
    slot,
    subjectName,
    fileName,
    savedAt,
    onRestored,
  }: {
    slot: Slot;
    subjectName: string;
    fileName: string;
    savedAt: number;
    /** Takes the slot from the group the entry belongs to, which is the value it
     *  should always have had — it used to write back into `slots[active]`. */
    onRestored: (slot: Slot, outcome: OpenOutcome) => void;
  } = $props();

  let backups: BackupInfo[] = $state([]);
  let error: string | null = $state(null);

  // Refetch on save (savedAt bumps) and on mount. Closing the popover unmounts
  // this, so a stale list cannot survive a save.
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
    // Names the file being REPLACED as well as the backup. The backup is the
    // half the user already picked; the file it lands on is the half they can
    // get wrong.
    const yes = await ask(
      `Replace ${fileName} (${subjectName}) with this backup?\n\n${b.file_name}\n\n` +
        "The current file is backed up first, so this is reversible.",
      { title: "Restore backup", kind: "warning" },
    );
    if (!yes) return;
    try {
      onRestored(slot, await api.restoreBackup(slot, b.path));
    } catch (e) {
      await message(errMessage(e), { title: "Restore failed", kind: "error" });
    }
  }
</script>

<section class="group">
  <h4>{subjectName} — {fileName}</h4>
  {#if error}<InlineMessage variant="error">{error}</InlineMessage>{/if}
  {#if backups.length === 0 && error === null}
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
</section>

<style>
  .group {
    margin-bottom: var(--s3);
  }
  h4 {
    margin: 0 0 var(--s1);
    font-size: var(--t-body);
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  li {
    list-style: none;
  }
  /* Was a global rule in the shell's stylesheet, beside `.backups`. It is one
     class with one user, so it comes with it. */
  .stamp {
    font-family: Consolas, monospace;
  }
  .meta {
    color: var(--text-muted);
  }
</style>
