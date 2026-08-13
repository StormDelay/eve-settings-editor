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

  const placeholder = $derived(
    (verb === "search" ? `Search ${nouns}` : `Filter ${nouns}…`) + (shortcut ? ` (${shortcut})` : ""),
  );

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
  <!-- Absent rather than dimmed when empty, matching today's {#if searching} guard. -->
  {#if value !== ""}
    <Button variant="ghost" size="sm" iconOnly title="Clear (Esc)" onclick={clear}>×</Button>
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
</style>
