<script lang="ts">
  // Row 1, full width, ALWAYS rendered, outside every `mainView` branch.
  //
  // That single placement is the fix for fault (a). Whatever occupies the work
  // area — the editor, the Accounts takeover, the Copy-settings takeover, or a
  // Phase 3 sheet — this bar and its save cluster are above it and unaffected.
  //
  // It is the only full-width element in the app, which is what makes it read as
  // "the frame" rather than as "a toolbar belonging to something".
  import { subject } from "./subject.svelte";
  import { accel } from "./keys";
  import type { OpenOutcome, PresetInfo, Slot } from "./api";
  import type { View } from "./views";
  import AppMenu from "./AppMenu.svelte";
  import HistoryPopover from "./HistoryPopover.svelte";
  import SaveCluster from "./SaveCluster.svelte";
  import SubjectSwitcher from "./SubjectSwitcher.svelte";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";

  let {
    switcherOpen = $bindable(false),
    /** Measured, not tokenised: the bar's height is its content's, and a sheet
     *  inset by a guessed constant would either clip it or float below it. */
    height = $bindable(0),
    onOpen,
    onOpenPreset,
    onGoto,
    onShowAccounts,
    onShowBatch,
    onShowAbout,
    onRestored,
  }: {
    switcherOpen?: boolean;
    height?: number;
    onOpen: (path: string) => void;
    onOpenPreset: (p: PresetInfo) => void;
    onGoto: (v: View) => void;
    onShowAccounts: () => void;
    onShowBatch: () => void;
    onShowAbout: () => void;
    onRestored: (slot: Slot, outcome: OpenOutcome) => void;
  } = $props();

  let menuEl: HTMLElement | undefined = $state();
  let subjectEl: HTMLElement | undefined = $state();
  let historyEl: HTMLElement | undefined = $state();
  let menuOpen = $state(false);
  let historyOpen = $state(false);

  // Either open slot can be un-writable, so the chip reports the first that is
  // and names it. `editable` is deliberately NOT carried over as a chip: it is
  // the normal state, and a permanent badge announcing normality is noise. Its
  // information survives in the negative — no chip means editable, and the save
  // cluster is live.
  const readOnly = $derived.by(() => {
    for (const slot of ["char", "user"] as const) {
      const o = subject.slots[slot];
      if (o?.status === "opened" && o.fidelity.state === "read_only") {
        return { fileName: o.file_name, reason: o.fidelity.reason };
      }
    }
    return null;
  });

  // The account alias sits beside the character name, and is omitted when the
  // account is unnamed or the character unpaired. Not repeated when the alias IS
  // the subject name (an account file open on its own).
  const aliasSuffix = $derived(
    subject.preset === null && subject.slots.char?.status === "opened" ? subject.userAlias : null,
  );
</script>

<header class="context-bar" bind:clientHeight={height}>
  <span bind:this={menuEl}>
    <Button
      variant="ghost"
      iconOnly
      title="Menu"
      onpointerdown={(e: PointerEvent) => e.stopPropagation()}
      onclick={() => (menuOpen = !menuOpen)}>☰</Button>
  </span>

  <span class="subject" bind:this={subjectEl}>
    <Button
      variant="ghost"
      class="subject-btn"
      title="Switch character ({accel('K')})"
      onpointerdown={(e: PointerEvent) => e.stopPropagation()}
      onclick={() => (switcherOpen = !switcherOpen)}>
      <span class="name">{subject.subjectName ?? "No character open"}</span>
      {#if aliasSuffix}<span class="alias">{aliasSuffix}</span>{/if}
      <span aria-hidden="true">▾</span>
    </Button>
  </span>

  {#if readOnly}
    <Chip tone="danger" size="sm" title="{readOnly.fileName}: {readOnly.reason}">read-only</Chip>
  {/if}
  {#if subject.preset !== null}
    <Chip tone="info" size="sm">preset</Chip>
  {/if}

  <span class="spacer"></span>

  <!-- A bordered control, not a hint: it sits where a search field would sit, so
       it is found by looking rather than by knowing. In Phase 2 it opens the
       subject switcher with a "Go to…" section; Phase 5 turns the same component
       into the command palette, which is why it is not built twice. -->
  <Button
    class="palette"
    title="Search or run a command"
    onpointerdown={(e: PointerEvent) => e.stopPropagation()}
    onclick={() => (switcherOpen = !switcherOpen)}>
    <span aria-hidden="true">⌕</span>
    <span class="palette-label">Search or run a command</span>
    <span class="kbd">{accel("K")}</span>
  </Button>

  <SaveCluster />

  <span bind:this={historyEl}>
    <Button
      title="Backups of every open file"
      onpointerdown={(e: PointerEvent) => e.stopPropagation()}
      onclick={() => (historyOpen = !historyOpen)}>History ▾</Button>
  </span>
</header>

{#if menuOpen && menuEl}
  <AppMenu
    anchor={menuEl}
    onclose={() => (menuOpen = false)}
    {onShowAccounts}
    {onShowBatch}
    {onShowAbout} />
{/if}

{#if switcherOpen && subjectEl}
  <SubjectSwitcher
    anchor={subjectEl}
    onclose={() => (switcherOpen = false)}
    {onOpen}
    {onOpenPreset}
    {onGoto} />
{/if}

{#if historyOpen && historyEl}
  <HistoryPopover anchor={historyEl} onclose={() => (historyOpen = false)} {onRestored} />
{/if}

<style>
  .context-bar {
    grid-column: 1 / -1;
    grid-row: 1;
    display: flex;
    align-items: center;
    gap: var(--s3);
    padding: var(--s2) var(--s3);
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    /* Nothing here is allowed to wrap. Wrapping is what the old file bar did,
       and it is the fault being fixed — its own stylesheet was candid that
       "combined with the filebar's flex-wrap this keeps the Save button
       reachable on small windows".

       §4.3 sheds the alias, then the palette label, then the History label at
       fixed thresholds. This sheds continuously instead: the flexible items
       carry `min-width: 0` and ellipsis, and Save and the unsaved count carry
       `flex-shrink: 0`, so the same things give way in the same order at every
       width rather than at three chosen ones. */
    flex-wrap: nowrap;
    min-width: 0;
  }
  .spacer {
    flex: 1;
    min-width: 0;
  }
  .subject {
    min-width: 0;
    display: flex;
  }
  .context-bar :global(.subject-btn) {
    min-width: 0;
    gap: var(--s2);
  }
  .name {
    font-weight: 600;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .alias {
    color: var(--text-muted);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .context-bar :global(.palette) {
    min-width: 0;
    color: var(--text-muted);
    justify-content: flex-start;
  }
  .palette-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kbd {
    font-size: var(--t-caption);
    color: var(--text-muted);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 0 var(--s1);
    white-space: nowrap;
  }
  /* Save and the unsaved count NEVER shed. */
  .context-bar :global(.cluster) {
    flex-shrink: 0;
  }
</style>
