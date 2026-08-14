<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";

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
    subtitle,
    placement = "center",
    width = "min(720px, 92vw)",
    titled = false,
    role = "dialog",
    onclose,
    actions,
    footer,
    class: klass = "",
    children,
    ...rest
  }: {
    open?: boolean;
    title: string;
    /** A second line under the title, for a sheet whose subject varies while its
        identity does not — Copy settings changes what it copies, not what it is. */
    subtitle?: string;
    /** `work` insets the panel to the shell's work area, leaving the context bar
        and the tab row visible above it and the subject browser beside it. The
        scrim still covers the whole window, because the sheet is modal: a live
        subject browser behind it would re-scope the sheet's own contents
        underneath the user. */
    placement?: "center" | "end" | "work";
    width?: string;
    /** `alertdialog` for a confirmation: it tells a screen reader the content is
        a consequence to weigh, not a form to fill in. One prop rather than a
        second component — everything else about the two is identical. */
    role?: "dialog" | "alertdialog";
    /** Renders a visible header — title, subtitle, `actions`, close button.
        Opt-in because the three Phase-1 callers each draw their own heading and
        would otherwise show two. */
    titled?: boolean;
    onclose: () => void;
    /** Right-aligned in the header, before the close button. */
    actions?: Snippet;
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
  <div
    class="overlay"
    class:end={placement === "end"}
    class:work={placement === "work"}
    onclick={onclose}
    {...rest}>
    <div
      class="sheet {klass}"
      class:end={placement === "end"}
      class:work={placement === "work"}
      {role}
      aria-modal="true"
      aria-label={title}
      tabindex="-1"
      bind:this={el}
      style={placement === "work" ? undefined : `width: ${width}`}
      onclick={(e) => e.stopPropagation()}>
      {#if titled}
        <header class="head">
          <div class="titles">
            <h2>{title}</h2>
            {#if subtitle}<p class="sub">{subtitle}</p>{/if}
          </div>
          {#if actions}<div class="actions">{@render actions()}</div>{/if}
          <Button variant="ghost" iconOnly title="Close" onclick={onclose}>✕</Button>
        </header>
      {/if}
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
  /* The scrim still covers everything — the sheet is modal, and the shell behind
     it is legible but inert. Only the PANEL is inset, onto the work-area
     rectangle, using the two distances the shell already knows: its own left
     column and the height of everything above row 3. Both fall back to 0, so a
     sheet degrades to full-window rather than to broken. */
  /* The inset is STRUCTURE, not spacing: a 2x2 grid whose first row and column
     are the two distances the shell already knows, and whose second cell is
     therefore exactly the work-area rectangle. Padding would say the same thing
     less clearly, and would owe the 4px space scale an explanation it does not
     have — these are measured offsets, not steps on a ramp. */
  .overlay.work {
    place-items: stretch;
    grid-template-columns: var(--shell-inset-left, 0) 1fr;
    grid-template-rows: var(--shell-inset-top, 0) 1fr;
  }
  .sheet.work {
    grid-column: 2;
    grid-row: 2;
    width: auto;
    max-height: 100%;
    border-radius: 0;
    border-width: 1px 0 0 1px;
  }

  .head {
    display: flex;
    align-items: flex-start;
    gap: var(--s3);
    padding-bottom: var(--s2);
    border-bottom: 1px solid var(--border);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  .head h2 {
    margin: 0;
    font-size: var(--t-title);
    font-weight: 600;
  }
  .sub {
    margin: var(--s1) 0 0;
    font-size: var(--t-body);
    color: var(--text-secondary);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--s2);
    flex-wrap: wrap;
    justify-content: flex-end;
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
