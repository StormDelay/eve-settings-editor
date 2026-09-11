<script lang="ts">
  import type { Snippet } from "svelte";
  import type { MenuItem } from "../ContextMenu.svelte";
  import MenuButton from "./MenuButton.svelte";

  // One row. It absorbs eight hand-rolled row treatments, five copies of
  // `.grip { opacity: 0.6 }`, and the min-width/ellipsis truncation that two
  // files each discovered separately.
  let {
    selected,
    indent = 0,
    onclick,
    disabled = false,
    disabledReason,
    oncontextmenu,
    actions,
    draggable = false,
    ondragstart,
    ondragover,
    ondrop,
    ondragend,
    title,
    leading,
    trailing,
    class: klass = "",
    children,
  }: {
    selected?: boolean;
    indent?: 0 | 1 | 2;
    onclick?: () => void;
    /** A row that exists but cannot be opened — a preset whose folder failed to
        read still has to be visible, and has to say why. */
    disabled?: boolean;
    disabledReason?: string;
    oncontextmenu?: (e: MouseEvent) => void;
    /** Renders a visible "⋯" opening the same menu as right-click. Phase 1
        passes this ONLY where a visible control already exists — using it on the
        three right-click-only menus would make Phase 1 a behaviour change and
        break its "revert the whole phase" property. */
    actions?: MenuItem[];
    draggable?: boolean;
    ondragstart?: (e: DragEvent) => void;
    ondragover?: (e: DragEvent) => void;
    ondrop?: (e: DragEvent) => void;
    ondragend?: (e: DragEvent) => void;
    title?: string;
    leading?: Snippet;
    trailing?: Snippet;
    class?: string;
    children: Snippet;
  } = $props();
</script>

<div
  class="row {klass}"
  class:selected
  class:indent1={indent === 1}
  class:indent2={indent === 2}
  role={selected === undefined ? undefined : "option"}
  aria-selected={selected === undefined ? undefined : selected}
  {title}
  draggable={draggable ? "true" : undefined}
  {ondragstart}
  {ondragover}
  {ondrop}
  {ondragend}
  {oncontextmenu}>
  <!-- aria-hidden: the grip is a texture, and the drag it affords is not
       keyboard-operable anyway. Announcing it would only add noise. -->
  {#if draggable}<span class="grip" title="Drag to reorder" aria-hidden="true">⠿</span>{/if}
  {#if leading}{@render leading()}{/if}
  {#if onclick}
    <button
      type="button"
      class="label"
      {disabled}
      title={disabled ? disabledReason : undefined}
      {onclick}>{@render children()}</button>
  {:else}
    <span class="label">{@render children()}</span>
  {/if}
  {#if trailing}<span class="trailing">{@render trailing()}</span>{/if}
  {#if actions}<MenuButton items={actions} />{/if}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: var(--s1) var(--s2);
    border-radius: var(--r-sm);
    min-width: 0;
  }
  .row:hover {
    background: var(--surface-raised);
  }
  .selected {
    background: var(--accent-dim);
  }
  .indent1 {
    padding-left: var(--s5);
  }
  .indent2 {
    padding-left: var(--s6);
  }
  .grip {
    cursor: grab;
    color: var(--text-muted);
  }
  /* The truncation WindowPanel and app.css each worked out for themselves. */
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
  button.label {
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    padding: 0;
    cursor: pointer;
  }
  button.label:disabled {
    opacity: var(--o-disabled);
    cursor: default;
  }
  button.label:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    border-radius: var(--r-sm);
  }
  .trailing {
    display: flex;
    align-items: center;
    gap: var(--s2);
    color: var(--text-muted);
    font-size: var(--t-caption);
    white-space: nowrap;
  }
</style>
