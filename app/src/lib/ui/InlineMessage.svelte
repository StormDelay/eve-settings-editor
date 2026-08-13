<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";

  // One inline message, replacing nine class names that all meant "a sentence
  // about the thing above it": hint, error, muted, field-error, flash, err,
  // empty, and v0.34's conflict and from-launcher.
  //
  // The body is --text, not the role tone. A role tone measures ~Lc 69 on its
  // -dim ground and --text measures ~94 there, so putting the sentence in the
  // colour would cost 25 points of contrast to say something the rail and the
  // border already say. Colour says which kind; size and weight say it is a
  // sentence.
  //
  // role="alert" on warn and error is new. Today `.error` is a bare <p> at nine
  // sites with no live region, so a validation failure is silent to a screen
  // reader.
  let {
    variant = "info",
    title,
    dismissible = false,
    ondismiss,
    role,
    id,
    class: klass = "",
    children,
  }: {
    variant?: "info" | "warn" | "error" | "success";
    title?: string;
    dismissible?: boolean;
    ondismiss?: () => void;
    role?: "status" | "alert";
    /** So a Field can point `aria-describedby` at its own error message. */
    id?: string;
    class?: string;
    children: Snippet;
  } = $props();

  const liveRole = $derived(role ?? (variant === "warn" || variant === "error" ? "alert" : "status"));
</script>

<div
  class="msg {klass}"
  class:warn={variant === "warn"}
  class:error={variant === "error"}
  class:success={variant === "success"}
  role={liveRole}
  {id}>
  <div class="body">
    {#if title}<strong class="lead">{title}</strong>{/if}
    {@render children()}
  </div>
  {#if dismissible}
    <Button variant="ghost" size="sm" iconOnly title="Dismiss" onclick={ondismiss}>×</Button>
  {/if}
</div>

<style>
  .msg {
    display: flex;
    align-items: flex-start;
    gap: var(--s2);
    background: var(--info-dim);
    border-left: 2px solid var(--info);
    border-radius: var(--r-sm);
    padding: var(--s2) var(--s3);
    color: var(--text);
    font-size: var(--t-body);
  }
  .body {
    flex: 1;
    min-width: 0;
  }
  .lead {
    color: var(--info);
    margin-right: var(--s1);
  }
  .warn {
    background: var(--warn-dim);
    border-left-color: var(--warn);
  }
  .warn .lead {
    color: var(--warn);
  }
  .error {
    background: var(--danger-dim);
    border-left-color: var(--danger);
  }
  .error .lead {
    color: var(--danger);
  }
  .success {
    background: var(--ok-dim);
    border-left-color: var(--ok);
  }
  .success .lead {
    color: var(--ok);
  }
</style>
