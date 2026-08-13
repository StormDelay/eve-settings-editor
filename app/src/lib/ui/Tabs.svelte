<script lang="ts">
  // One tab strip, replacing three visual styles (.viewtabs, .subtabs,
  // .tree-file) and, more importantly, three different levels of ARIA.
  // Today the Overview sub-tabs set role="tablist" on a container that also
  // holds two pack buttons carrying no role at all — an invalid ARIA tree that
  // svelte-check does not catch — and the main view strip is a bare <span> of
  // buttons with no roles whatsoever. None of the three has a roving tabindex
  // or arrow-key movement. Doing it once is the only way it gets done right.
  let {
    tabs,
    value = $bindable(),
    variant = "segmented",
    ariaLabel,
    class: klass = "",
  }: {
    tabs: { id: string; label: string; disabled?: boolean; disabledReason?: string }[];
    value?: string;
    variant?: "segmented" | "underline";
    ariaLabel: string;
    class?: string;
  } = $props();

  let els: (HTMLButtonElement | undefined)[] = $state([]);

  const selectable = $derived(tabs.map((t, i) => (t.disabled ? -1 : i)).filter((i) => i >= 0));

  function pick(i: number) {
    const t = tabs[i];
    if (!t || t.disabled) return;
    value = t.id;
    els[i]?.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    const here = selectable.indexOf(tabs.findIndex((t) => t.id === value));
    const last = selectable.length - 1;
    const to =
      e.key === "ArrowRight" ? (here + 1 > last ? 0 : here + 1)
      : e.key === "ArrowLeft" ? (here - 1 < 0 ? last : here - 1)
      : e.key === "Home" ? 0
      : e.key === "End" ? last
      : -1;
    if (to < 0 || last < 0) return;
    e.preventDefault();
    pick(selectable[to]);
  }
</script>

<div
  class="tabs {klass}"
  class:underline={variant === "underline"}
  role="tablist"
  aria-label={ariaLabel}
  tabindex="-1"
  {onkeydown}>
  {#each tabs as t, i (t.id)}
    <button
      bind:this={els[i]}
      type="button"
      role="tab"
      class="tab"
      class:active={t.id === value}
      aria-selected={t.id === value}
      aria-disabled={t.disabled ? "true" : undefined}
      title={t.disabled ? t.disabledReason : undefined}
      tabindex={t.id === value ? 0 : -1}
      onclick={() => pick(i)}>{t.label}</button>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    gap: var(--s1);
  }
  .tab {
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    padding: 1px var(--s3);
    font: inherit;
    font-size: var(--t-ui);
    cursor: pointer;
  }
  .tab:hover {
    background: var(--surface-raised);
    color: var(--text);
  }
  .tab:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .tab.active {
    background: var(--accent-dim);
    border-color: var(--accent);
    color: var(--accent);
  }
  /* Disabled rather than hidden, so the strip stops rearranging under the
     cursor as files load. Phase 2 is what actually passes `disabled`. */
  .tab[aria-disabled="true"] {
    opacity: var(--o-disabled);
    cursor: default;
  }
  .tab[aria-disabled="true"]:hover {
    background: transparent;
    color: var(--text-secondary);
  }

  .underline {
    gap: 0;
    border-bottom: 1px solid var(--border);
  }
  .underline .tab {
    border-radius: 0;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
  }
  .underline .tab.active {
    background: transparent;
    border-color: transparent;
    border-bottom-color: var(--accent);
    color: var(--text);
  }
</style>
