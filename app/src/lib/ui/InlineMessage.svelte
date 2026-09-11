<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";
  import { toast } from "./toasts.svelte";

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
    detail,
    dismissible = false,
    escalate,
    ondismiss,
    role,
    id,
    class: klass = "",
    children,
  }: {
    variant?: "info" | "warn" | "error" | "success";
    title?: string;
    /** The diagnostic form of an error — `[conflict] …` — parked on `title=`.
        `errText` owns the sentence and `errMessage` owns this, so the bracketed
        machine code is one hover away from a bug report and nowhere near the
        prose. */
    detail?: string;
    /** Raise a toast with the same words if this message is NOT on screen when
        it appears. Defaults on for `error` only.

        This is the one way replacing modals with inline messages could be WORSE
        than what it replaced: a modal was never invisible, and an inline error
        in a collapsed panel, a hidden sub-tab or a scrolled-away row is a silent
        failure. The rule lives in the primitive rather than at forty-eight call
        sites, because a rule each caller has to remember is a rule that gets
        forgotten exactly where it matters.

        Not on for info/warn: those are bands and hints that are frequently
        off-screen on purpose, and toasting them would train the same
        dismiss-without-reading reflex this phase exists to untrain. `Toast`
        renders one of these itself, so it passes `false` and cannot recurse. */
    escalate?: boolean;
    dismissible?: boolean;
    ondismiss?: () => void;
    role?: "status" | "alert";
    /** So a Field can point `aria-describedby` at its own error message. */
    id?: string;
    class?: string;
    children: Snippet;
  } = $props();

  const liveRole = $derived(role ?? (variant === "warn" || variant === "error" ? "alert" : "status"));

  let el: HTMLDivElement | undefined = $state();

  // One observation, on mount, then disconnect: the question is "was this
  // reported where the user could see it", which is answered once. Re-checking
  // on scroll would toast a message the user has simply scrolled past, which is
  // the opposite of the point.
  $effect(() => {
    if (!(escalate ?? variant === "error") || !el) return;
    if (typeof IntersectionObserver === "undefined") return; // jsdom, and SSR
    const node = el;
    const io = new IntersectionObserver((entries) => {
      io.disconnect();
      if (entries.some((e) => e.isIntersecting)) return;
      toast(node.textContent?.trim() ?? "", { variant: "error" });
    });
    io.observe(node);
    return () => io.disconnect();
  });
</script>

<div
  class="msg {klass}"
  class:warn={variant === "warn"}
  class:error={variant === "error"}
  class:success={variant === "success"}
  role={liveRole}
  title={detail}
  bind:this={el}
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
