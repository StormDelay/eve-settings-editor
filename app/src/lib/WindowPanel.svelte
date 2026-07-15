<script lang="ts">
  import type { WindowRect, BoolFlag } from "$lib/api";

  let {
    windows,
    selectedId,
    readOnly,
    onSelect,
    onToggleOpen,
    onGeom,
    onFlag,
    onStack,
  }: {
    windows: WindowRect[];
    selectedId: string | null;
    readOnly: boolean;
    onSelect: (id: string) => void;
    onToggleOpen: (w: WindowRect) => void;
    onGeom: (w: WindowRect, field: "x" | "y" | "w" | "h", value: number) => void;
    onFlag: (w: WindowRect, flag: BoolFlag, value: boolean) => void;
    onStack: (w: WindowRect, text: string) => void;
  } = $props();

  // Flags shown in the detail; openWindows lives on the row header instead.
  const detailFlags = (w: WindowRect) => w.flags.filter((f) => f.name !== "openWindows");

  const COORDS = ["x", "y", "w", "h"] as const;

  const numberEdit = (w: WindowRect, field: "x" | "y" | "w" | "h") => (e: Event) => {
    const v = parseInt((e.target as HTMLInputElement).value, 10);
    if (!Number.isNaN(v)) onGeom(w, field, v);
  };

  // Bring a row into view when it becomes selected — a canvas click can select
  // a window whose row is scrolled far out of a long list.
  function scrollOnSelect(node: HTMLElement, selected: boolean) {
    const run = (sel: boolean) => {
      if (sel) node.scrollIntoView({ block: "nearest" });
    };
    run(selected);
    return { update: run };
  }
</script>

<div class="window-panel">
  {#each windows as w (w.id)}
    {@const openFlag = w.flags.find((f) => f.name === "openWindows")}
    <div class="row" class:selected={w.id === selectedId} use:scrollOnSelect={w.id === selectedId}>
      <div class="row-head">
        <input
          type="checkbox"
          checked={w.open}
          disabled={readOnly || openFlag?.set.how === "unavailable"}
          title="Open (shown on the canvas)"
          aria-label="Open (shown on the canvas)"
          onchange={() => onToggleOpen(w)} />
        <button class="name" onclick={() => onSelect(w.id)}>
          {w.label}
        </button>
        {#if !w.renderable}
          <span class="badge warn" title="Geometry is not a 6-tuple — edit in the raw tree">
            unrenderable
          </span>
        {:else if !w.resolution_matches}
          <span class="badge warn" title="Saved at a different resolution than the canvas">
            {w.geom?.screen_w}×{w.geom?.screen_h}
          </span>
        {/if}
      </div>

      {#if w.id === selectedId && w.geom}
        <div class="detail">
          <div class="coords">
            {#each COORDS as field}
              <label>
                {field}
                <input
                  type="number"
                  value={w.geom[field]}
                  disabled={readOnly}
                  onchange={numberEdit(w, field)} />
              </label>
            {/each}
          </div>
          <div class="flags">
            {#each detailFlags(w) as f (f.name)}
              <label class="flag" title={f.set.how === "unavailable" ? "Not present in this file" : ""}>
                <input
                  type="checkbox"
                  checked={f.value}
                  disabled={readOnly || f.set.how === "unavailable"}
                  onchange={(e) => onFlag(w, f, (e.target as HTMLInputElement).checked)} />
                {f.name}
              </label>
            {/each}
          </div>
          {#if w.stacks}
            <label class="stack">
              stack id
              <input
                type="number"
                value={w.stacks.text}
                disabled={readOnly}
                onchange={(e) => onStack(w, (e.target as HTMLInputElement).value)} />
            </label>
          {/if}
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .window-panel {
    overflow-y: auto;
    font-size: 13px;
    border-left: 1px solid var(--border);
    background: var(--bg-panel);
    color: var(--fg);
  }
  .row {
    border-bottom: 1px solid var(--border);
  }
  .row.selected {
    background: rgba(79, 156, 240, 0.18);
  }
  .row-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.5rem;
  }
  .name {
    flex: 1;
    min-width: 0; /* allow truncation instead of forcing the row wider */
    text-align: left;
    background: none;
    border: none;
    color: var(--fg);
    cursor: pointer;
    font: inherit;
    padding: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge.warn {
    background: var(--warn);
    color: #33260a;
    border-radius: 3px;
    padding: 0 0.3rem;
    font-size: 11px;
    white-space: nowrap;
  }
  .detail {
    padding: 0.4rem 0.6rem 0.6rem;
    display: grid;
    gap: 0.5rem;
  }
  .coords {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0.3rem;
  }
  .coords label,
  .stack {
    display: grid;
    gap: 0.1rem;
    font-size: 11px;
    color: var(--fg-dim);
  }
  .detail input {
    width: 100%;
    box-sizing: border-box;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 4px;
    font: inherit;
  }
  .detail input:focus {
    outline: 1px solid var(--accent);
  }
  .flags {
    display: grid;
    gap: 0.15rem;
  }
  .flag {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    color: var(--fg);
  }
</style>
