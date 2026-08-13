<script lang="ts">
  import type { Snippet } from "svelte";

  // One component, not a Dialog and a Sheet. They differ in `inset` and one
  // transform; they are identical in everything actually hard — the scrim,
  // role="dialog", aria-modal, the focus trap, Escape, and restoring focus to
  // whatever opened them. Building two would be the speculative abstraction this
  // codebase already has too much of. Phase 1 ships it with three real users on
  // `center`; Phase 3 flips a prop to get the right-hand sheet.
  //
  // What it adds over the `.modal` it replaces is an accessibility floor, not a
  // feature: today two of the three modals close on a backdrop click, none
  // closes on Escape, none traps focus, and none gives focus back.
  let {
    open = $bindable(true),
    title,
    placement = "center",
    width = "min(720px, 92vw)",
    onclose,
    footer,
    class: klass = "",
    children,
    ...rest
  }: {
    open?: boolean;
    title: string;
    placement?: "center" | "end";
    width?: string;
    onclose: () => void;
    footer?: Snippet;
    class?: string;
    children: Snippet;
    /** Spread onto the backdrop, which is this component's root. Two existing
        specs identify the modal they are dismissing by a data-testid on it. */
    [key: string]: unknown;
  } = $props();

  let el: HTMLDivElement | undefined = $state();

  const FOCUSABLE =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  $effect(() => {
    if (!open || !el) return;
    const opener = document.activeElement as HTMLElement | null;
    (el.querySelector<HTMLElement>(FOCUSABLE) ?? el).focus();
    // Focus goes back to whatever opened the sheet. Without this a keyboard user
    // lands at the top of the document every time a dialog closes.
    return () => opener?.focus?.();
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onclose();
      return;
    }
    if (e.key !== "Tab" || !el) return;
    const items = [...el.querySelectorAll<HTMLElement>(FOCUSABLE)];
    if (items.length === 0) return;
    const [first, last] = [items[0], items[items.length - 1]];
    const on = document.activeElement;
    if (e.shiftKey && (on === first || on === el)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && on === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<!-- Outside the {#if}: <svelte:window> may not sit inside a block. -->
<svelte:window onkeydown={(e) => open && onkeydown(e)} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="overlay" class:end={placement === "end"} onclick={onclose} {...rest}>
    <div
      class="sheet {klass}"
      class:end={placement === "end"}
      role="dialog"
      aria-modal="true"
      aria-label={title}
      tabindex="-1"
      bind:this={el}
      style="width: {width}"
      onclick={(e) => e.stopPropagation()}>
      <div class="content">{@render children()}</div>
      {#if footer}<div class="footer">{@render footer()}</div>{/if}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 55;
    background: var(--scrim);
    display: grid;
    place-items: center;
  }
  .overlay.end {
    place-items: stretch end;
  }
  .sheet {
    background: var(--surface-overlay);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--s4);
    max-height: 92vh;
    display: flex;
    flex-direction: column;
    gap: var(--s3);
    min-width: 0;
  }
  .sheet.end {
    border-radius: 0;
    max-height: 100%;
    height: 100%;
  }
  .content {
    overflow-y: auto;
    min-height: 0;
  }
  .footer {
    display: flex;
    gap: var(--s2);
    justify-content: flex-end;
  }
</style>
