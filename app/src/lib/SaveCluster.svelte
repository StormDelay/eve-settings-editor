<script lang="ts">
  // One control where there were four: two dirty badges, a Discard button and a
  // fidelity badge. It is rendered even with nothing open — disabled, never
  // absent — because a control that appears and disappears is the class of
  // problem this phase exists to remove, and a permanently-placed Save teaches
  // where Save is before there is anything to save.
  //
  // It lives in the context bar, which is OUTSIDE every `mainView` branch. That
  // one placement is the whole of the fix for fault (a): entering Accounts or
  // Copy settings with unsaved edits used to take Save and both unsaved badges
  // off the screen, with no way back except opening another file.
  import { subject, discardChanges, saveFile } from "./subject.svelte";
  import Button from "./ui/Button.svelte";
  import Popover from "./ui/Popover.svelte";
  import ScopeBanner from "./ui/ScopeBanner.svelte";

  let trigger: HTMLElement | undefined = $state();
  let open = $state(false);

  const targets = $derived(subject.saveTargets);
  const willWrite = $derived(targets.filter((t) => t.blocked === null));
  const blocked = $derived(targets.filter((t) => t.blocked !== null));
  const nothingOpen = $derived(subject.slots.char === null && subject.slots.user === null);

  // Shown only when the account file is among the files about to be written and
  // it has siblings — the single most consequential fact in the app, stated as a
  // consequence ("this also changes Clea Otsada") rather than as a storage
  // location ("core_user_140.dat").
  const scopeLabel = $derived(
    willWrite.some((t) => t.slot === "user") && subject.sharedNames.length
      ? `Account settings are shared — this also changes ${subject.sharedNames.join(", ")}.`
      : "",
  );

  // Save's own predicate is `subject.canSave`, untouched: it already folds in
  // read-only through `saveable`, and the save loop applies the same one. A Save
  // button that disagrees with what the loop writes is the next bug.
  const saveReason = $derived(
    blocked.length > 0 && willWrite.length === 0
      ? `${blocked[0].fileName} is read-only`
      : nothingOpen
        ? "Open a character first"
        : "There is nothing to save",
  );
</script>

<div class="cluster">
  {#if targets.length}
    <span bind:this={trigger}>
      <Button
        size="sm"
        title="What a save would write"
        onpointerdown={(e: PointerEvent) => e.stopPropagation()}
        onclick={() => (open = !open)}>{targets.length} unsaved ▾</Button>
    </span>
  {/if}
  <Button
    variant="primary"
    disabled={!subject.canSave}
    disabledReason={saveReason}
    onclick={() => saveFile()}>Save</Button>
</div>

{#if open && trigger}
  <Popover
    anchor={trigger}
    placement="bottom-end"
    ariaLabel="Unsaved changes"
    class="save-disclosure"
    onclose={() => (open = false)}>
    {#if willWrite.length}
      <p class="section">Will write</p>
      <ul>
        {#each willWrite as t (t.slot)}
          <li>
            <span class="who">{t.subjectName}</span>
            <span class="role">{t.role}</span>
            <span class="file">{t.fileName}</span>
          </li>
        {/each}
      </ul>
    {/if}

    {#if blocked.length}
      <!-- A dirty slot that CANNOT be written still gets a row, because "your
           edits are stuck in a read-only file" is exactly what the user needs
           told; it is simply not under "will write". -->
      <p class="section">Cannot be written</p>
      <ul>
        {#each blocked as t (t.slot)}
          <li>
            <span class="who">{t.subjectName}</span>
            <span class="role">{t.role}</span>
            <span class="file">{t.fileName}</span>
            <span class="why">{t.blocked}</span>
          </li>
        {/each}
      </ul>
    {/if}

    <ScopeBanner label={scopeLabel} compact />
    <p class="note">Each file is backed up before it is written.</p>

    <div class="actions">
      <!-- No "Save both" here: Save is on the trigger, three pixels away, and a
           second one is redundancy that has to be kept in sync. -->
      <Button
        variant="danger"
        size="sm"
        onclick={() => { open = false; void discardChanges(); }}
        title="Throw the unsaved changes away and reload both files from disk. Backups are untouched."
        >Discard changes</Button>
    </div>
  </Popover>
{/if}

<style>
  .cluster {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }
  :global(.popover.save-disclosure) {
    width: min(28rem, 90vw);
    padding: var(--s2);
  }
  .section {
    margin: var(--s1) 0;
    font-size: var(--t-caption);
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  ul {
    list-style: none;
    margin: 0 0 var(--s2);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s1);
  }
  li {
    display: flex;
    align-items: baseline;
    gap: var(--s2);
    min-width: 0;
  }
  .who {
    font-weight: 600;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .role,
  .file,
  .why {
    color: var(--text-muted);
    font-size: var(--t-caption);
    white-space: nowrap;
  }
  .why {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .note {
    margin: var(--s2) 0 0;
    font-size: var(--t-caption);
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--s2);
  }
</style>
