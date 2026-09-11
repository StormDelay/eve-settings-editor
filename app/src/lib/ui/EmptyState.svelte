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
    variant = "empty",
    action,
    class: klass = "",
  }: {
    title: string;
    description?: string;
    /** `error` for the BROKEN empty: a view with nothing to show because
        something failed, not because there is nothing there yet. Today a
        dismissed "Layout unavailable" modal left an empty canvas with no
        explanation at all, which is the case this exists for. */
    variant?: "empty" | "error";
    action?: Snippet;
    class?: string;
  } = $props();
</script>

<div
  class="empty-state {klass}"
  class:error={variant === "error"}
  role={variant === "error" ? "alert" : undefined}>
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
  /* The tone, not the whole block: the sentence stays at --text, because a role
     colour measures ~25 points of contrast worse on this ground and would say
     what the heading already says. */
  .error .title {
    color: var(--danger);
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
