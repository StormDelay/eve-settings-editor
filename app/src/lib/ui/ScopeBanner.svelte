<script lang="ts">
  import type { Snippet } from "svelte";
  import InlineMessage from "./InlineMessage.svelte";

  // "Editing this changes every character on the account." Four byte-identical
  // CSS blocks said it, plus two near-copies with different words.
  //
  // The tone is --info, not --warn. A statement of scope is not a warning, and
  // ChatSplit was using --warn for BOTH inside one 135-line file: the legend at
  // :88 and a genuine negative-area warning at :119. Same token, one file, two
  // meanings, no way for a reader to tell which. Now --warn means one thing.
  let {
    label,
    compact = false,
    action,
    class: klass = "",
  }: {
    label: string;
    compact?: boolean;
    action?: Snippet;
    class?: string;
  } = $props();
</script>

<!-- Renders nothing at all when the label is empty, matching the four
     {#if sharedLabel} guards it replaces. -->
{#if label}
  <div class="scope {klass}" class:compact>
    <InlineMessage variant="info">
      {label}
      {#if action}<span class="action">{@render action()}</span>{/if}
    </InlineMessage>
  </div>
{/if}

<style>
  .scope {
    min-width: 0;
  }
  .action {
    margin-left: var(--s2);
  }
  .compact :global(.msg) {
    padding: var(--s1) var(--s2);
    font-size: var(--t-caption);
  }
</style>
