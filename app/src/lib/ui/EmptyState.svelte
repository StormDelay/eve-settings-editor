<script lang="ts">
  import type { Snippet } from "svelte";

  // "There is nothing here", said the same way everywhere. It replaces three
  // class names for one idea — .hint, .muted, and .empty, which two shipped
  // empty states used and NO stylesheet in the repo ever defined, so both
  // rendered as bare unstyled paragraphs and nobody noticed. That is the
  // sharpest evidence for this whole phase: there was no shared thing whose
  // absence would show.
  //
  // The `action` snippet is the point. Two views render the same "pair this
  // character" prompt with two different button treatments; one component, one
  // treatment.
  let {
    title,
    description,
    action,
    class: klass = "",
  }: {
    title: string;
    description?: string;
    action?: Snippet;
    class?: string;
  } = $props();
</script>

<div class="empty-state {klass}">
  <p class="title">{title}</p>
  {#if description}<p class="description">{description}</p>{/if}
  {#if action}<div class="action">{@render action()}</div>{/if}
</div>

<style>
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--s2);
    padding: var(--s6) var(--s4);
    max-width: 44ch;
    margin: 0 auto;
  }
  .title {
    margin: 0;
    font-size: var(--t-title);
    font-weight: 600;
    color: var(--text);
  }
  .description {
    margin: 0;
    font-size: var(--t-body);
    color: var(--text-secondary);
  }
  .action {
    display: flex;
    gap: var(--s2);
    margin-top: var(--s2);
  }
</style>
