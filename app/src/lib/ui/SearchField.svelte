<script lang="ts">
  import Button from "./Button.svelte";
  import Field from "./Field.svelte";

  // Five search boxes existed, using two verbs ("Search", "Filter"), three
  // placeholder conventions (bare, trailing "…", trailing "(Ctrl+F)") and three
  // style blocks. The placeholder is BUILT here rather than passed in, which is
  // what collapses those conventions into one rule — and it is why `nouns` is a
  // prop and `placeholder` is not.
  let {
    value = $bindable(""),
    verb = "filter",
    nouns,
    shortcut,
    count,
    total,
    onclear,
    element = $bindable(),
    class: klass = "",
    ...rest
  }: {
    value?: string;
    verb?: "search" | "filter";
    nouns: string;
    shortcut?: string;
    count?: number;
    total?: number;
    onclear?: () => void;
    /** The input node, for callers that manage focus. */
    element?: HTMLInputElement | HTMLSelectElement;
    class?: string;
    [key: string]: unknown;
  } = $props();

  // No trailing `…` on a filter — R2 reserves the ellipsis for "this will not
  // finish without more input", and narrowing a list you can already see
  // finishes as you type. And the accelerator is NOT baked into the string: it
  // renders as a <kbd> below, so the five boxes stop advertising it three
  // different ways and macOS stops being told to press Ctrl.
  const placeholder = $derived(verb === "search" ? `Search ${nouns}` : `Filter ${nouns}`);

  const meta = $derived(
    count === undefined ? "" : total === undefined ? `${count}` : `${count} of ${total}`,
  );

  function clear() {
    value = "";
    onclear?.();
  }
</script>

<div class="searchbar {klass}">
  <Field
    kind="search"
    bind:value
    bind:element
    {placeholder}
    ariaLabel={placeholder}
    class="search"
    onkeydown={(e: KeyboardEvent) => {
      if (e.key === "Escape") clear();
    }}
    {...rest} />
  {#if meta}<span class="meta">{meta}</span>{/if}
  <!-- Discovery rule 3, at the control rather than in a menu: the shortcut is
       shown where it is used, and it is hidden once the box has content because
       by then the user is already here. -->
  {#if shortcut && value === ""}<kbd>{shortcut}</kbd>{/if}
  <!-- Absent rather than dimmed when empty, matching today's {#if searching} guard. -->
  {#if value !== ""}
    <Button variant="ghost" size="sm" iconOnly title="Clear" onclick={clear}>×</Button>
  {/if}
</div>

<style>
  .searchbar {
    display: flex;
    align-items: center;
    gap: var(--s2);
    min-width: 0;
  }
  .searchbar :global(.search) {
    flex: 1;
    min-width: 0;
  }
  .searchbar :global(.search input) {
    width: 100%;
  }
  .meta {
    color: var(--text-muted);
    font-size: var(--t-caption);
    white-space: nowrap;
  }
  kbd {
    color: var(--text-muted);
    font-family: inherit;
    font-size: var(--t-caption);
    white-space: nowrap;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 0 var(--s1);
  }
</style>
