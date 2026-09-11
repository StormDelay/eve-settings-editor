<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";

  // `level` sets the heading rank independently of its size, so the document
  // outline can be right without dragging the visual scale with it — the thing
  // BackupsPanel currently works around by zeroing h3's margin.
  let {
    title,
    subtitle,
    level = 3,
    collapsed = $bindable(),
    oncollapse,
    actions,
    class: klass = "",
  }: {
    title: string;
    subtitle?: string;
    level?: 2 | 3 | 4;
    collapsed?: boolean;
    oncollapse?: () => void;
    actions?: Snippet;
    class?: string;
  } = $props();

  function toggle() {
    collapsed = !collapsed;
    oncollapse?.();
  }
</script>

<div class="head {klass}">
  {#if collapsed !== undefined}
    <Button
      variant="ghost"
      size="sm"
      iconOnly
      title={collapsed ? `Expand ${title}` : `Collapse ${title}`}
      onclick={toggle}>{collapsed ? "▸" : "▾"}</Button>
  {/if}
  <svelte:element this={`h${level}`} class="title">{title}</svelte:element>
  {#if subtitle}<span class="subtitle">{subtitle}</span>{/if}
  {#if actions}<span class="actions">{@render actions()}</span>{/if}
</div>

<style>
  .head {
    display: flex;
    align-items: baseline;
    gap: var(--s2);
    min-width: 0;
  }
  .title {
    margin: 0;
    font-size: var(--t-title);
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
  }
  /* Legible rather than dimmed: this is the line that says which file a panel is
     about, and at opacity .7 it measured Lc 40.6. */
  .subtitle {
    color: var(--text-muted);
    font-size: var(--t-caption);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin-left: auto;
  }
</style>
