<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    as = "section",
    padded = true,
    scroll = false,
    bordered = true,
    class: klass = "",
    children,
  }: {
    as?: "section" | "aside" | "div";
    padded?: boolean;
    scroll?: boolean;
    bordered?: boolean;
    class?: string;
    children: Snippet;
  } = $props();
</script>

<svelte:element
  this={as}
  class="panel {klass}"
  class:padded
  class:scroll
  class:bordered>{@render children()}</svelte:element>

<style>
  .panel {
    background: var(--surface);
    border-radius: var(--r-md);
    min-width: 0;
  }
  .bordered {
    border: 1px solid var(--border);
  }
  .padded {
    padding: var(--s3);
  }
  .scroll {
    overflow-y: auto;
    /* Without this a scrolling panel inside a flex or grid parent refuses to
       shrink below its content and the scrollbar never appears. */
    min-height: 0;
  }
</style>
