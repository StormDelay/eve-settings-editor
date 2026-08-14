<script lang="ts">
  import ContextMenu, { type MenuItem } from "../ContextMenu.svelte";
  import Button from "./Button.svelte";

  // A visible "⋯" opening the same flat menu right-click opens. Five places
  // needed it once Phase 4 stopped hiding commands: a tab row, a window-group
  // header, the Overview view header, and WindowPanel's three row heads.
  //
  // Positioned from the button's own rect rather than from the pointer event,
  // so the menu lands under the control whether it was opened by mouse or by
  // keyboard. Popover already clamps it to the viewport from there.
  // The defaults are the DENSE-ROW treatment: a small ghost that recedes into a
  // list row it shares with a name and its chips. A menu that is the only
  // control in a header has the opposite problem — nothing around it to belong
  // to — so those call sites pass a real button.
  let {
    items,
    title = "More actions",
    label = "⋯",
    variant = "ghost",
    size = "sm",
  }: {
    /** A thunk, so a menu whose disabled reasons depend on current state is
        built when it opens rather than on every render of the row. */
    items: MenuItem[] | (() => MenuItem[]);
    title?: string;
    label?: string;
    variant?: "default" | "ghost";
    size?: "sm" | "md";
  } = $props();

  let at = $state<{ x: number; y: number } | null>(null);
</script>

<Button
  {variant}
  {size}
  iconOnly={label === "⋯"}
  {title}
  aria-haspopup="menu"
  onclick={(e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    at = { x: r.left, y: r.bottom };
  }}>{label}</Button>

{#if at}
  <ContextMenu
    x={at.x}
    y={at.y}
    items={typeof items === "function" ? items() : items}
    onClose={() => (at = null)} />
{/if}
