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
  import { api, errMessage, errText, type BackupInfo, type OpenOutcome, type Slot } from "./api";
  import { confirmDialog } from "./ui/confirm.svelte";
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
  // A refused restore, above the list it was refused from. Separate from the
  // listing error above, which is about not being able to READ the folder.
  let restoreError = $state<{ text: string; detail: string } | null>(null);

  /** The timestamp EVE's own backup naming puts second-from-last. It is what a
   *  user picks a backup BY, so it is what the confirmation names. */
  const stampOf = (b: BackupInfo) => b.file_name.split(".").slice(-2, -1)[0] ?? b.file_name;

  /**
   * Show the five most recent, and the rest behind a toggle.
   *
   * Not tidiness — it is what keeps the SECOND file's group reachable. History
   * renders one group per open slot, character first, and a character with
   * thirteen backups filled the popover and pushed the account's group below the
   * fold. Saving from Probes writes only the account file, so the group that had
   * just changed was the one you could not see, and the panel read as if the
   * save had not happened.
   *
   * Five, because the restore you actually want is nearly always the last save.
   */
  const CAP = 5;
  let showAll = $state(false);
  const shown = $derived(showAll ? backups : backups.slice(0, CAP));
  const hidden = $derived(Math.max(0, backups.length - CAP));

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

  // Survivor 4. It writes disk, so it keeps a confirmation — but it IS reversible
  // (the current file is backed up first), which is why it is a confirm rather
  // than one of the three that name an irreversible loss.
  //
  // Names the person and the timestamp, not two file names: the backup is the
  // half the user already picked, and the file it lands on is the half they can
  // get wrong. The raw names go to `detail`, which is `title=` only.
  async function restore(b: BackupInfo) {
    const yes = await confirmDialog({
      title: `Restore ${subjectName} from ${stampOf(b)}?`,
      body: "The file on disk is replaced. It's backed up first, so this is reversible.",
      detail: `${b.file_name} → ${fileName}`,
      confirm: "Restore",
    });
    if (!yes) return;
    restoreError = null;
    try {
      onRestored(slot, await api.restoreBackup(slot, b.path));
    } catch (e) {
      restoreError = { text: `That backup wasn't restored — ${errText(e)}`, detail: errMessage(e) };
    }
  }
</script>

<section class="group">
  <h4>{subjectName} — {fileName}</h4>
  {#if error}<InlineMessage variant="error">{error}</InlineMessage>{/if}
  {#if restoreError}
    <InlineMessage variant="error" detail={restoreError.detail}>{restoreError.text}</InlineMessage>
  {/if}
  {#if backups.length === 0 && error === null}
    <EmptyState title="No history yet" description="Every save leaves a restorable copy here." />
  {/if}
  <ul>
    {#each shown as b (b.path)}
      <li>
        <ListRow>
          <span class="stamp">{stampOf(b)}</span>
          {#snippet trailing()}
            <span class="meta">{Math.round(b.size / 1024)} KB</span>
            <Button variant="ghost" size="sm" onclick={() => restore(b)}>Restore</Button>
          {/snippet}
        </ListRow>
      </li>
    {/each}
  </ul>
  {#if hidden > 0}
    <Button variant="ghost" size="sm" onclick={() => (showAll = !showAll)}>
      {showAll ? "Show fewer" : `${hidden} older`}
    </Button>
  {/if}
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
