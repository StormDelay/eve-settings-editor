<script lang="ts">
  // The view strip: all six, always, in row order, disabled rather than hidden.
  //
  // Fault (c) was that the strip changed membership and width as files loaded
  // and pairings landed — each tab behind its own `{#if}`, and the whole strip
  // behind a seventh. With nothing qualifying it disappeared entirely, so the
  // user was given no indication the other five views existed.
  import Tabs from "./ui/Tabs.svelte";
  import { VIEWS, viewAvailable, type View } from "./views";

  let {
    value = $bindable(),
    height = $bindable(0),
    onpick,
  }: {
    value: View;
    /** Measured for the same reason the context bar's is: a sheet insets past
     *  both, so that the tabs stay visible above it. */
    height?: number;
    onpick?: () => void;
  } = $props();

  const tabs = $derived(
    VIEWS.map((v) => {
      const reason = viewAvailable(v.id);
      return {
        id: v.id,
        label: v.label,
        disabled: reason !== null,
        disabledReason: reason ?? undefined,
      };
    }),
  );
</script>

<!-- Row 2, columns 2-3: the tabs govern the work area and its inspector, and
     govern nothing about the subject list, so spanning the full width would put
     a control above a panel it does not control. -->
<div class="tabrow" bind:clientHeight={height}>
  <Tabs {tabs} ariaLabel="Editor view" bind:value onpick={() => onpick?.()} />
</div>

<style>
  .tabrow {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: var(--s1) var(--s3);
    border-bottom: 1px solid var(--border);
    min-width: 0;
    /* Never wraps, never scrolls. §4.3 rule 3 reaches this by swapping in short
       labels below 900px; that mechanism is not built, because there is no
       configured minimum window width to fit them to (`tauri.conf.json` sets a
       1200px default and no `minWidth`) and these six labels already fit a
       window far narrower than anyone runs. Instead the tabs shrink and
       ellipsis, which holds the property at EVERY width rather than at one
       chosen threshold — and keeps six readable words instead of six
       abbreviations nobody asked for. */
    overflow: hidden;
  }
  .tabrow :global(.tabs) {
    flex: 1;
    min-width: 0;
    flex-wrap: nowrap;
  }
  .tabrow :global(.tab) {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* The `⋯` view menu (§5.5) is not rendered. Phase 2 fills it with nothing —
     every view contributes zero actions until Phase 4/5 — and a slot that is
     always hidden is not a slot, it is dead markup. It arrives with its first
     item. */
</style>
