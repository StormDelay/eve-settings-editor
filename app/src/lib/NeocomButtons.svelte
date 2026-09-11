<script lang="ts">
  import type { NeocomBar } from "$lib/api";
  import { addableButtons, type CatalogButton } from "$lib/neocom";
  import { neocomLabel } from "$lib/windowLabels";
  import CATALOG from "$lib/data/neocom-buttons.json";
  import { toast } from "./ui/toasts.svelte";
  import { undoAction } from "./undo.svelte";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";

  let { bar, readOnly, busy = false, error = null, onReorder, onRemove, onAdd, onReset }: {
    bar: NeocomBar;
    readOnly: boolean;
    /** A refused neocom edit, owned by LayoutView (which runs the command) and
     *  rendered here (which owns the control). */
    error?: { text: string; detail: string } | null;
    /** True while a neocom command is in flight. Commands key by index, so a
     * second click before the re-projection lands could reuse a now-stale
     * index — disable the whole list for the round trip rather than risk it. */
    busy?: boolean;
    /** A full permutation of the current indices — the backend rejects anything else. */
    onReorder: (order: number[]) => void;
    onRemove: (index: number) => void;
    onAdd: (id: string, btnType: number, iconPath: string) => void;
    onReset: () => void;
  } = $props();

  const disabled = $derived(readOnly || busy);
  const addable = $derived(addableButtons(bar.buttons, bar.original, CATALOG as CatalogButton[]));
  // Reset needs a baseline to reset TO; a character whose client never wrote
  // one gets a disabled button rather than a backend error.
  const canReset = $derived(bar.original.length > 0);

  /** Swap two neighbours and send the whole ordering. */
  function move(index: number, delta: number) {
    const order = bar.buttons.map((b) => b.index);
    const to = index + delta;
    if (to < 0 || to >= order.length) return;
    [order[index], order[to]] = [order[to], order[index]];
    onReorder(order);
  }

  let addChoice = $state("");
  function doAdd() {
    const pick = addable.find((a) => a.id === addChoice);
    if (!pick) return;
    onAdd(pick.id, pick.btnType, pick.iconPath);
  }
  // Clearing the pick inside `doAdd` threw it away before the command had run,
  // so a failed add lost the user's choice and they had to find it again. The
  // add itself is what clears it: a button that landed is no longer addable. A
  // failure leaves the bar — and so the dropdown — exactly as it was.
  $effect(() => {
    if (addChoice !== "" && !addable.some((a) => a.id === addChoice)) addChoice = "";
  });

  // No confirmation. The mutation is in-memory, Discard reverses it exactly, and
  // the button's own tooltip already carries the consequence BEFORE the click —
  // which is the rule (say what an action costs in the control's own words, not
  // in a dialog after the fact). Friction proportional to consequence: this one
  // costs a click to put back.
  function resetBar() {
    onReset();
    toast("Neocom reset to the client's original buttons.", { action: undoAction() });
  }
</script>

<div class="buttons">
  <h4 class="head">Buttons</h4>
  {#if error}
    <InlineMessage variant="error" detail={error.detail}>{error.text}</InlineMessage>
  {/if}
  {#each bar.buttons as b (b.index)}
    {@const label = neocomLabel(b.id)}
    <ListRow class="row">
      <!-- The raw id stays in the tooltip: it is what the file says, so it is
           what a reader checks the friendly name against. -->
      <span class="id" title="{b.id} — {b.icon_path}">{label}</span>
      {#snippet trailing()}
        {#if b.children > 0}<Chip tone="neutral" size="sm">{b.children}</Chip>{/if}
        <Button
          variant="ghost"
          size="sm"
          iconOnly
          disabled={disabled || b.index === 0}
          disabledReason={b.index === 0 ? "Already first" : "The bar is read-only"}
          title="Move {label} up"
          onclick={() => move(b.index, -1)}>↑</Button>
        <Button
          variant="ghost"
          size="sm"
          iconOnly
          disabled={disabled || b.index === bar.buttons.length - 1}
          disabledReason={b.index === bar.buttons.length - 1 ? "Already last" : "The bar is read-only"}
          title="Move {label} down"
          onclick={() => move(b.index, 1)}>↓</Button>
        <Button
          variant="ghost"
          size="sm"
          iconOnly
          {disabled}
          disabledReason="The bar is read-only"
          title="Remove {label}"
          onclick={() => onRemove(b.index)}>✕</Button>
      {/snippet}
    </ListRow>
  {/each}

  {#if addable.length > 0}
    <div class="add">
      <Field
        kind="select"
        bind:value={addChoice}
        {disabled}
        disabledReason="The bar is read-only"
        ariaLabel="Add a neocom button"
        class="add-pick"
        options={[{ value: "", label: "Add…" }, ...addable.map((a) => ({ value: a.id, label: neocomLabel(a.id) }))]} />
      <Button
        disabled={disabled || addChoice === ""}
        disabledReason={addChoice === "" ? "Pick a button first" : "The bar is read-only"}
        onclick={doAdd}>Add</Button>
    </div>
  {/if}

  <Button
    class="reset"
    disabled={disabled || !canReset}
    title={canReset ? "Replace the bar with the client's own original" : "This character has no original bar recorded"}
    onclick={resetBar}>
    Reset to original
  </Button>
</div>

<style>
  .buttons {
    margin: var(--s1) 0;
  }
  /* Deliberately not PanelHeader, which §5.6 nominates. This is a sub-list
     label inside HudPanel's already-headed, 12px-dense column; PanelHeader's
     fixed --t-title would render it at 16px bold and dominate the panel it sits
     inside. What it needed was to stop using dimness for rank — that is what
     weight is for, and it costs no legibility. */
  .head {
    margin: 0 0 var(--s1);
    font-size: var(--t-caption);
    font-weight: 600;
    color: var(--text-secondary);
  }
  .id {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .add {
    display: flex;
    align-items: center;
    gap: var(--s1);
    margin-top: var(--s1);
  }
  .add :global(.add-pick) {
    flex: 1;
    min-width: 0;
  }
  .add :global(.add-pick select) {
    width: 100%;
  }
  /* Scoped through .buttons, which is authored here — a bare :global(.reset)
     would style every .reset in the app. */
  .buttons :global(.reset) {
    margin-top: var(--s1);
    width: 100%;
  }
</style>
