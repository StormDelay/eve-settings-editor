<script lang="ts">
  import Button from "./Button.svelte";
  import InlineMessage from "./InlineMessage.svelte";
  // `toasts.svelte.ts`, not the spec's `toast.svelte.ts`: on a case-insensitive
  // filesystem `./toast.svelte` resolves to this very component, and TypeScript
  // rejects the pair as differing only in casing.
  import { dismiss, toasts } from "./toasts.svelte";

  // The host. Mounted once, in routes/+page.svelte.
  let { class: klass = "" }: { class?: string } = $props();
</script>

<div class="toasts {klass}" role="status" aria-live="polite">
  {#each toasts as t (t.id)}
    <div
      class="toast"
      class:fading={t.duration > 0}
      style={t.duration > 0 ? `animation-duration: ${t.duration}ms` : undefined}>
      <!-- `escalate={false}` is not a preference: a toast IS the escalation, and
           one that raised another would not terminate. -->
      <InlineMessage
        variant={t.variant}
        escalate={false}
        dismissible
        ondismiss={() => dismiss(t.id)}>
        {t.message}
        {#if t.action}
          <Button
            size="sm"
            class="toast-action"
            onclick={() => {
              t.action?.run();
              dismiss(t.id);
            }}>{t.action.label}</Button>
        {/if}
      </InlineMessage>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    inset-block-end: var(--s4);
    inset-inline-end: var(--s4);
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    /* Above ContextMenu's 50 — a confirmation that a menu action worked must
       not render behind the menu that triggered it. */
    z-index: 60;
  }
  .toast {
    background: var(--surface-overlay);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow);
    max-width: 44ch;
  }

  /* Moved here from app.css, where it had no reduced-motion guard. It holds at
     full opacity for 60% of the toast's life and fades over the rest, so the
     animation and the removal timer describe the same span. A sticky toast
     never gets the class. */
  .fading {
    animation: fade-out ease-in forwards;
  }
  @keyframes fade-out {
    0%,
    60% {
      opacity: 1;
    }
    100% {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fading {
      animation: none;
    }
  }
</style>
