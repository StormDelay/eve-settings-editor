<script lang="ts">
  import Button from "./Button.svelte";
  import Sheet from "./Sheet.svelte";
  import { answer, pending } from "./confirm.svelte";

  // The host. Mounted once, beside the toast host, so a confirmation outlives
  // whichever view raised it — a dialog unmounted by the action it is asking
  // about would resolve nothing and hang its caller's `await`.
  const current = $derived(pending[0] ?? null);
</script>

{#if current}
  <!-- Keyed so a second request gets a fresh Sheet, and therefore a fresh
       focus-restore target, rather than inheriting the first one's. -->
  {#key current.id}
    <Sheet
      title={current.title}
      role="alertdialog"
      width="min(30rem, 92vw)"
      onclose={() => answer(current.id, false)}
      data-testid="confirm-dialog">
      <h2 class="title">{current.title}</h2>
      <p class="body" title={current.detail}>{current.body}</p>
      {#snippet footer()}
        <!-- Cancel FIRST in the DOM, which is what puts initial focus on it:
             Sheet focuses its first focusable, and the safe answer is the only
             one that may be one Enter away. -->
        <Button onclick={() => answer(current.id, false)}>{current.cancel ?? "Cancel"}</Button>
        <Button
          variant={current.danger ? "danger" : "primary"}
          onclick={() => answer(current.id, true)}>{current.confirm}</Button>
      {/snippet}
    </Sheet>
  {/key}
{/if}

<style>
  .title {
    margin: 0 0 var(--s2);
    font-size: var(--t-title);
    font-weight: 600;
  }
  .body {
    margin: 0;
    color: var(--text-secondary);
    font-size: var(--t-body);
    /* A consequence you have to read is a consequence set at a reading measure. */
    max-width: 56ch;
    white-space: pre-line;
  }
</style>
