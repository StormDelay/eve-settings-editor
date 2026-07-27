<script lang="ts">
  import type { NeocomBar } from "$lib/api";
  import { addableButtons, type CatalogButton } from "$lib/neocom";
  import CATALOG from "$lib/data/neocom-buttons.json";
  import { confirm } from "@tauri-apps/plugin-dialog";

  let { bar, readOnly, busy = false, onReorder, onRemove, onAdd, onReset }: {
    bar: NeocomBar;
    readOnly: boolean;
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
    addChoice = "";
    onAdd(pick.id, pick.btnType, pick.iconPath);
  }

  // The Tauri dialog, not the bare browser confirm() — titled and iconed like
  // every other destructive prompt in this app (OverviewView's deleteTab,
  // AutofillView's clearAll, ...), not the unstyled default.
  async function resetBar() {
    const ok = await confirm(
      "Reset the neocom to the client's original buttons?",
      { title: "Reset neocom", kind: "warning" },
    );
    if (ok) onReset();
  }
</script>

<div class="buttons">
  <p class="head">Buttons</p>
  {#each bar.buttons as b (b.index)}
    <div class="row">
      <span class="id" title={b.icon_path}>{b.id}</span>
      {#if b.children > 0}<span class="badge">{b.children}</span>{/if}
      <button class="mv" disabled={disabled || b.index === 0} onclick={() => move(b.index, -1)} aria-label="Move {b.id} up">↑</button>
      <button class="mv" disabled={disabled || b.index === bar.buttons.length - 1} onclick={() => move(b.index, 1)} aria-label="Move {b.id} down">↓</button>
      <button class="rm" disabled={disabled} onclick={() => onRemove(b.index)} aria-label="Remove {b.id}">✕</button>
    </div>
  {/each}

  {#if addable.length > 0}
    <div class="row">
      <!-- Native select: give it explicit dark colours, or it renders light in
           this WebView2 app (standing project note). -->
      <select bind:value={addChoice} disabled={disabled} aria-label="Add a neocom button">
        <option value="">Add…</option>
        {#each addable as a (a.id)}
          <option value={a.id}>{a.id}</option>
        {/each}
      </select>
      <button disabled={disabled || addChoice === ""} onclick={doAdd}>Add</button>
    </div>
  {/if}

  <button
    class="reset"
    disabled={disabled || !canReset}
    title={canReset ? "Replace the bar with the client's own original" : "This character has no original bar recorded"}
    onclick={resetBar}>
    Reset to original
  </button>
</div>

<style>
  .buttons {
    margin: 0.3rem 0 0.2rem;
  }
  .head {
    margin: 0 0 0.2rem;
    color: var(--fg-dim);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.2rem;
    margin-bottom: 1px;
  }
  .id {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    color: var(--fg-dim);
    font-size: 10px;
  }
  .mv, .rm {
    padding: 0 0.25rem;
  }
  /* Native controls render light in WebView2 unless told otherwise. */
  select, option {
    background: var(--bg-panel);
    color: inherit;
    border: 1px solid #444;
    flex: 1;
    min-width: 0;
  }
  .reset {
    margin-top: 0.3rem;
    width: 100%;
  }
</style>
