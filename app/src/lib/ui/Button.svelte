<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  // The one button. It replaces the global `button` rule plus twenty-odd local
  // copies, but the reason it exists is a bug rather than tidiness:
  // `app.css` set `.mini { opacity: 0 }` and revealed it only through
  // `.row:hover .mini`, so the four `.mini` buttons that sit outside any `.row`
  // were permanently invisible and permanently clickable. Two of them destroy
  // data (Clear a list, remove an entry). `variant="ghost"` is the replacement
  // for that pattern and it is ALWAYS visible: a row that wants its controls to
  // recede until hover does that by changing colour on `.row:hover`, never by
  // hiding them.
  let {
    variant = "default",
    size = "md",
    type = "button",
    disabled = false,
    disabledReason,
    pressed,
    iconOnly = false,
    title,
    href,
    class: klass = "",
    onclick,
    oncontextmenu,
    children,
    ...rest
  }: {
    variant?: "default" | "primary" | "ghost" | "danger";
    size?: "sm" | "md";
    type?: "button" | "submit";
    disabled?: boolean;
    disabledReason?: string;
    pressed?: boolean;
    iconOnly?: boolean;
    title?: string;
    href?: string;
    class?: string;
    onclick?: (e: MouseEvent) => void;
    oncontextmenu?: (e: MouseEvent) => void;
    children: Snippet;
    [key: string]: unknown;
  } = $props();

  // An icon-only button whose glyph is its only label is unreachable by screen
  // reader and unguessable by anyone else. Failing loudly in dev is the only
  // thing that reliably stops it shipping — a lint rule cannot see the glyph.
  // `untrack` because this is a construction-time contract check, not a
  // reactive one: a call site does not start and stop being icon-only.
  untrack(() => {
    if (import.meta.env.DEV && iconOnly && !title) {
      throw new Error("Button: iconOnly requires a `title` — it is also the accessible name.");
    }
  });

  // A disabled control that does not say why is a dead end (§3.8). Where the
  // reason is already computed at the call site, it costs one prop to say it.
  const tip = $derived(disabled && disabledReason ? disabledReason : title);
</script>

{#if href}
  <a
    {href}
    class="btn {klass}"
    class:primary={variant === "primary"}
    class:ghost={variant === "ghost"}
    class:danger={variant === "danger"}
    class:sm={size === "sm"}
    class:icon={iconOnly}
    title={tip}
    aria-label={iconOnly ? title : undefined}
    target="_blank"
    rel="noreferrer"
    {...rest}>{@render children()}</a>
{:else}
  <button
    {type}
    class="btn {klass}"
    class:primary={variant === "primary"}
    class:ghost={variant === "ghost"}
    class:danger={variant === "danger"}
    class:sm={size === "sm"}
    class:icon={iconOnly}
    class:pressed
    {disabled}
    title={tip}
    aria-label={iconOnly ? title : undefined}
    aria-pressed={pressed === undefined ? undefined : pressed}
    {onclick}
    {oncontextmenu}
    {...rest}>{@render children()}</button>
{/if}

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--s1);
    background: var(--surface-raised);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: var(--s1) var(--s3);
    font: inherit;
    font-size: var(--t-ui);
    line-height: 1.2;
    text-decoration: none;
    cursor: pointer;
  }
  .btn:hover {
    background: var(--surface-overlay);
    border-color: var(--border-strong);
  }
  .btn:active {
    background: var(--surface);
  }
  .btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    border-radius: inherit;
  }

  /* 2px is below the 4px scale on purpose: `sm` is the dense-row size, and
     --s1 top and bottom would make every list row taller than it is today.
     Phase 1 moves nothing. */
  .sm {
    font-size: var(--t-caption);
    padding: 2px var(--s2);
  }
  .icon {
    padding: var(--s1);
  }
  .icon.sm {
    padding: 2px;
  }

  .primary {
    background: var(--accent-dim);
    border-color: var(--accent);
    color: var(--accent);
    font-weight: 600;
  }
  .primary:hover {
    border-color: var(--accent);
  }
  .primary:active {
    background: var(--accent-dim);
  }

  /* Transparent, but never invisible — see the note at the top of this file. */
  .ghost {
    background: transparent;
    border-color: transparent;
    color: var(--text-secondary);
  }
  .ghost:hover {
    background: var(--surface-raised);
    border-color: transparent;
    color: var(--text);
  }
  .ghost:active {
    background: var(--surface);
  }

  .danger {
    background: var(--surface-raised);
    border-color: var(--danger);
    color: var(--danger);
  }
  .danger:hover {
    background: var(--danger-dim);
    border-color: var(--danger);
  }
  .danger:active {
    background: var(--surface-raised);
  }

  .pressed {
    border-color: var(--accent);
    color: var(--text);
  }

  .btn:disabled {
    opacity: var(--o-disabled);
    cursor: default;
  }
  .btn:disabled:hover {
    background: var(--surface-raised);
    border-color: var(--border);
  }
  .ghost:disabled:hover {
    background: transparent;
    border-color: transparent;
  }
</style>
