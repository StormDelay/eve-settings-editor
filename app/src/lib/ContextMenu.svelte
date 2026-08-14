<script lang="ts" module>
  export interface MenuItem {
    label: string;
    run: () => void;
    /** Present but inert. "Hiding becomes disabling with a reason" — a command
        that vanishes when the backend would refuse it teaches nothing, and its
        row moves under the cursor. `hint` is that reason, as a tooltip. */
    disabled?: boolean;
    hint?: string;
  }
</script>

<script lang="ts">
  import Popover from "./ui/Popover.svelte";

  // A flat right-click menu. Deliberately minimal: no submenus, no icons, no
  // portal — both callers need one list, and nothing more. `WindowPanel` uses
  // it for a row's actions; `LayoutView` uses it to list the rectangles
  // stacked under a point on the canvas, where a click can only reach the top
  // one.
  //
  // The positioning and dismissal that used to live here now lives in Popover,
  // because a second caller needed it. What is left is the list.
  let {
    x,
    y,
    items,
    onClose,
  }: {
    x: number;
    y: number;
    items: MenuItem[];
    onClose: () => void;
  } = $props();

  function pick(item: MenuItem) {
    item.run();
    onClose();
  }
</script>

<Popover
  anchor={{ x, y }}
  placement="point"
  onclose={onClose}
  role="menu"
  ariaLabel="Actions"
  class="context-menu">
  {#each items as item (item.label)}
    <button role="menuitem" disabled={item.disabled} title={item.hint}
      onclick={() => pick(item)}>{item.label}</button>
  {/each}
</Popover>

<style>
  /* :global because the class lands on Popover's root, which is in Popover's
     scope, not this file's. The name is deliberately unique for that reason —
     the buttons below are authored here, so they scope normally. */
  :global(.context-menu) {
    min-width: 11rem;
    display: flex;
    flex-direction: column;
  }
  button {
    background: none;
    border: none;
    border-radius: var(--r-sm);
    color: var(--text);
    cursor: pointer;
    font: inherit;
    font-size: var(--t-ui);
    padding: var(--s1) var(--s2);
    text-align: left;
    white-space: nowrap;
  }
  /* Tone on its own dim ground, not dark text on a saturated fill (§3.4). */
  button:not(:disabled):hover {
    background: var(--accent-dim);
    color: var(--accent);
  }
  button:disabled {
    opacity: var(--o-disabled);
    cursor: default;
  }
  button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
</style>
