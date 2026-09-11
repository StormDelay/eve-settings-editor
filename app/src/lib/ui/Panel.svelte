<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    as = "section",
    padded = true,
    scroll = false,
    bordered = true,
    class: klass = "",
    children,
    ...rest
  }: {
    as?: "section" | "aside" | "div";
    padded?: boolean;
    scroll?: boolean;
    bordered?: boolean;
    class?: string;
    children: Snippet;
    /** Spread onto the root, so a panel can also carry a role and a name —
        AccountsView's calibration panel is a labelled dialog. */
    [key: string]: unknown;
  } = $props();
</script>

<svelte:element
  this={as}
  class="panel {klass}"
  class:padded
  class:scroll
  class:bordered
  {...rest}>{@render children()}</svelte:element>

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
