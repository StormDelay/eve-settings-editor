<script lang="ts">
  // A checkbox list over formations, used by BOTH Export (this account's set)
  // and Import (a file's). It knows nothing about ids, files or the clipboard:
  // it is handed items and hands back the indices that were ticked.
  import type { FormationSpec } from "./api";
  import { formatUnit } from "./probes";
  import Button from "./ui/Button.svelte";
  import Sheet from "./ui/Sheet.svelte";

  let { title, items, confirmLabel, onconfirm, oncancel }:
    { title: string; items: FormationSpec[]; confirmLabel: string;
      onconfirm: (indices: number[]) => void; oncancel: () => void } = $props();

  // Everything starts ticked: "all of them" is the common case both ways, and
  // unticking one is easier to discover than hunting for a select-all first.
  // The initial capture is the point — the picker is mounted fresh per use, and
  // the ticks are the user's from then on, not a mirror of the prop.
  // svelte-ignore state_referenced_locally
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

<Sheet {title} onclose={oncancel} data-testid="picker-backdrop">
  <h2>{title}</h2>
  <ul>
    {#each items as f, i}
      <li>
        <!-- The row stays a wrapping <label> rather than becoming a Field: the
             whole row, meta included, is the hit target, and the spec clicks the
             meta text to toggle the tick. A Field would put the control and its
             label side by side and lose that. -->
        <label>
          <!-- Without this, the label's own text (name + meta) becomes the
               accessible name; this pins it to just the formation's name. -->
          <input type="checkbox" checked={picked[i]} aria-label={f.name}
                 onchange={(e) => (picked[i] = e.currentTarget.checked)} />
          <span class="name">{f.name}</span>
          <span class="meta">
            {f.probes.length} {f.probes.length === 1 ? "probe" : "probes"} · {rangeLabel(f)}
          </span>
        </label>
      </li>
    {/each}
  </ul>

  {#snippet footer()}
    <Button onclick={() => (picked = picked.map(() => !allOn))}>
      {allOn ? "None" : "All"}
    </Button>
    <span class="spacer"></span>
    <Button onclick={oncancel}>Cancel</Button>
    <Button
      variant="primary"
      disabled={chosen.length === 0}
      disabledReason="Tick at least one formation"
      onclick={() => onconfirm(chosen)}>
      {confirmLabel} {chosen.length}
    </Button>
  {/snippet}
</Sheet>

<style>
  /* `.spacer` is global (app.css). */
  h2 { margin: 0 0 var(--s2); font-size: var(--t-title); font-weight: 600; }
  ul { list-style: none; margin: 0; padding: 0; max-height: 50vh; overflow-y: auto; }
  li label { display: flex; align-items: baseline; gap: var(--s2); padding: var(--s1); cursor: pointer; }
  input[type="checkbox"] { accent-color: var(--accent); }
  .name { flex: 1; }
  .meta { color: var(--text-muted); font-size: var(--t-caption); white-space: nowrap; }
</style>
