<script lang="ts">
  // A checkbox list over formations, used by BOTH Export (this account's set)
  // and Import (a file's). It knows nothing about ids, files or the clipboard:
  // it is handed items and hands back the indices that were ticked.
  import type { FormationSpec } from "./api";
  import { formatUnit } from "./probes";

  let { title, items, confirmLabel, onconfirm, oncancel }:
    { title: string; items: FormationSpec[]; confirmLabel: string;
      onconfirm: (indices: number[]) => void; oncancel: () => void } = $props();

  // Everything starts ticked: "all of them" is the common case both ways, and
  // unticking one is easier to discover than hunting for a select-all first.
  let picked = $state(items.map(() => true));
  const chosen = $derived(picked.flatMap((on, i) => (on ? [i] : [])));
  const allOn = $derived(picked.every(Boolean));

  /** A formation's range in AU, or "mixed" when its probes disagree — the one
   * case a single number would misreport (spec §2.3). */
  function rangeLabel(f: FormationSpec): string {
    const first = f.ranges[0] ?? 0;
    return f.ranges.every((r) => r === first) ? `${formatUnit(first, "au")} AU` : "mixed";
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay" role="none" data-testid="picker-backdrop" onclick={oncancel}>
  <div class="modal" role="none" onclick={(e) => e.stopPropagation()}>
    <h2>{title}</h2>
    <ul>
      {#each items as f, i}
        <li>
          <label>
            <input type="checkbox" checked={picked[i]}
                   onchange={(e) => (picked[i] = e.currentTarget.checked)} />
            <span class="name">{f.name}</span>
          </label>
          <span class="meta">
            {f.probes.length} {f.probes.length === 1 ? "probe" : "probes"} · {rangeLabel(f)}
          </span>
        </li>
      {/each}
    </ul>
    <div class="form-actions">
      <button onclick={() => (picked = picked.map(() => !allOn))}>
        {allOn ? "Select none" : "Select all"}
      </button>
      <span class="spacer"></span>
      <button onclick={oncancel}>Cancel</button>
      <button disabled={chosen.length === 0} onclick={() => onconfirm(chosen)}>
        {confirmLabel} {chosen.length}
      </button>
    </div>
  </div>
</div>

<style>
  /* `.overlay`, `.modal`, `.form-actions` and `.spacer` are global (app.css) —
     this is the same modal the tree's insert form uses, not a second one. */
  h2 { margin: 0 0 0.6rem; font-size: 1em; font-weight: 600; }
  ul { list-style: none; margin: 0; padding: 0; max-height: 50vh; overflow-y: auto; }
  li { display: flex; align-items: baseline; gap: 0.5rem; padding: 3px 2px; }
  li label { display: flex; align-items: baseline; gap: 0.5rem; cursor: pointer; flex: 1; }
  .name { flex: 1; }
  .meta { opacity: 0.7; font-size: 0.85em; white-space: nowrap; }
</style>
