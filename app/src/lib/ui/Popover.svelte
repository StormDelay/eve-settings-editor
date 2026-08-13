<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  // The positioning, clamping and dismissal logic here is lifted from
  // ContextMenu, which had it right and had it alone. Its second user was
  // already in the tree and going without: OverviewView's colour palette
  // dropdown is a bare `position: absolute` div with no viewport clamp and no
  // Escape handler, so near the right edge of the window it renders partly
  // offscreen and there is no keyboard way out of it.
  let {
    anchor,
    placement = "point",
    open = $bindable(true),
    onclose,
    ariaLabel,
    role = "dialog",
    class: klass = "",
    children,
  }: {
    anchor: HTMLElement | { x: number; y: number };
    placement?: "bottom-start" | "bottom-end" | "top-start" | "point";
    open?: boolean;
    onclose: () => void;
    ariaLabel: string;
    role?: string;
    class?: string;
    children: Snippet;
  } = $props();

  const base = $derived.by(() => {
    if (typeof HTMLElement !== "undefined" && anchor instanceof HTMLElement) {
      const r = anchor.getBoundingClientRect();
      if (placement === "top-start") return { x: r.left, y: r.top };
      if (placement === "bottom-end") return { x: r.right, y: r.bottom };
      return { x: r.left, y: r.bottom };
    }
    return { x: (anchor as { x: number }).x, y: (anchor as { y: number }).y };
  });

  // A popover opened near the right or bottom edge would otherwise render
  // partly offscreen — potentially clipping the only route to an action.
  // Clamp once the element exists and can be measured: it starts at the raw
  // point and snaps onscreen a frame later.
  //
  // `pos` is a deliberate SNAPSHOT rather than a $derived of `base`: the effect
  // below owns this value once the popover can be measured, and a derived would
  // overwrite the clamp on every read. `untrack` says that to the compiler,
  // which otherwise warns that this captures only the initial value — which is
  // the point, since the effect tracks it itself and re-clamps if it changes.
  let el: HTMLDivElement | undefined = $state();
  let pos = $state(untrack(() => base));
  $effect(() => {
    if (!el) return;
    const r = el.getBoundingClientRect();
    const x = placement === "bottom-end" ? base.x - r.width : base.x;
    pos = {
      x: Math.max(0, Math.min(x, window.innerWidth - r.width)),
      y: Math.max(0, Math.min(base.y, window.innerHeight - r.height)),
    };
  });
</script>

<!-- Outside the {#if}: <svelte:window> may not sit inside a block. -->
<svelte:window
  onpointerdown={() => open && onclose()}
  onkeydown={(e) => {
    if (open && e.key === "Escape") onclose();
  }} />

{#if open}
  <!-- stopPropagation so a click INSIDE the popover doesn't trip the window
       handler above and close it before the button's own onclick runs. -->
  <div
    class="popover {klass}"
    {role}
    aria-label={ariaLabel}
    tabindex="-1"
    bind:this={el}
    style="left: {pos.x}px; top: {pos.y}px;"
    onpointerdown={(e) => e.stopPropagation()}>
    {@render children()}
  </div>
{/if}

<style>
  .popover {
    position: fixed;
    z-index: 50;
    background: var(--surface-overlay);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow);
    padding: var(--s1);
  }
</style>
