<script lang="ts" module>
  export interface MenuItem {
    label: string;
    run: () => void;
  }
</script>

<script lang="ts">
  // A flat right-click menu. Deliberately minimal: no submenus, no icons, no
  // portal — the panel is the only caller and it needs one list of actions.
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

  // The list sits flush against the app's right edge and a row's clickable
  // name area runs to the row's edge, so a right-click near the right or
  // bottom of the window opens a menu that would otherwise render partly
  // off-screen — potentially clipping "Copy window id", the escape hatch for
  // reading a raw id. Clamp once the element exists and can be measured;
  // starts at the raw click point and snaps onscreen a frame later.
  let menuEl: HTMLDivElement | undefined = $state();
  let pos = $state({ x, y });
  $effect(() => {
    if (!menuEl) return;
    const r = menuEl.getBoundingClientRect();
    pos = {
      x: Math.max(0, Math.min(x, window.innerWidth - r.width)),
      y: Math.max(0, Math.min(y, window.innerHeight - r.height)),
    };
  });
</script>

<svelte:window
  onpointerdown={onClose}
  onkeydown={(e) => {
    if (e.key === "Escape") onClose();
  }} />

<!-- stopPropagation so a click INSIDE the menu doesn't trip the window handler
     above and close it before the button's own onclick runs. -->
<div
  class="menu"
  role="menu"
  tabindex="-1"
  bind:this={menuEl}
  style="left: {pos.x}px; top: {pos.y}px;"
  onpointerdown={(e) => e.stopPropagation()}>
  {#each items as item (item.label)}
    <button role="menuitem" onclick={() => pick(item)}>{item.label}</button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 50;
    min-width: 11rem;
    padding: 0.2rem;
    background: var(--bg-panel);
    border: 1px solid var(--border);
    border-radius: 4px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
  }
  .menu button {
    background: none;
    border: none;
    border-radius: 3px;
    color: var(--fg);
    cursor: pointer;
    font: inherit;
    padding: 0.25rem 0.5rem;
    text-align: left;
    white-space: nowrap;
  }
  .menu button:hover {
    background: var(--accent);
    color: var(--bg);
  }
</style>
