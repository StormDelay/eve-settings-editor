<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  // `chip` carried five unrelated meanings and `badge` four. This is the one
  // chip: a small, non-interactive tag. Anything clickable inside it is a
  // Button, and the chip itself is never clickable.
  //
  // A Chip never renders dark text on a saturated fill. That pattern looks
  // confident and measures terrible — the three status badges it replaces
  // scored Lc 51 / 55 / 43 at 12px, where APCA wants 75. A light role tone on
  // its matching -dim ground measures ~69 for the same three, which is a
  // 20-point gain from a rule rather than from taste.
  let {
    tone,
    state = "settled",
    size = "md",
    title,
    actions,
    class: klass = "",
    children,
  }: {
    tone?: "neutral" | "accent" | "ok" | "warn" | "danger" | "info";
    state?: "settled" | "proposed";
    size?: "sm" | "md";
    title?: string;
    actions?: Snippet;
    class?: string;
    children: Snippet;
  } = $props();

  // A proposed chip defaults to `info` rather than `neutral`, so a pairing that
  // needs an answer reads LOUDER than the settled ones beside it. v0.34 did the
  // opposite — `.chip.ghost` was a settled chip minus 15% opacity — and the
  // reported symptom was that proposals are not visible enough. That is the
  // predictable result of dimming a thing to make it matter.
  const hue = $derived(tone ?? (state === "proposed" ? "info" : "neutral"));

  untrack(() => {
    if (import.meta.env.DEV && state === "proposed" && !title) {
      throw new Error("Chip: state=\"proposed\" requires a `title` naming the source, e.g. \"From your launcher log\".");
    }
  });
</script>

<span
  class="chip {klass}"
  class:accent={hue === "accent"}
  class:ok={hue === "ok"}
  class:warn={hue === "warn"}
  class:danger={hue === "danger"}
  class:info={hue === "info"}
  class:proposed={state === "proposed"}
  class:sm={size === "sm"}
  {title}>
  {@render children()}
  {#if actions}{@render actions()}{/if}
</span>

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--s1);
    background: var(--surface-raised);
    color: var(--text-secondary);
    border: 1px solid var(--border);
    border-radius: var(--r-pill);
    padding: 1px var(--s2);
    font-size: var(--t-body);
    white-space: nowrap;
  }
  .sm {
    font-size: var(--t-caption);
  }

  /* The dashed border stays: it is the honest carrier of "not committed yet" —
     a real signal that survives the no-opacity rule and reads without colour. */
  .proposed {
    border-style: dashed;
  }

  .accent {
    background: var(--accent-dim);
    color: var(--accent);
    border-color: var(--accent);
  }
  .ok {
    background: var(--ok-dim);
    color: var(--ok);
    border-color: var(--ok);
  }
  .warn {
    background: var(--warn-dim);
    color: var(--warn);
    border-color: var(--warn);
  }
  .danger {
    background: var(--danger-dim);
    color: var(--danger);
    border-color: var(--danger);
  }
  .info {
    background: var(--info-dim);
    color: var(--info);
    border-color: var(--info);
  }
</style>
