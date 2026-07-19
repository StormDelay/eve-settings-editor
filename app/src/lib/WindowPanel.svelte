<script lang="ts">
  import type { WindowRect, BoolFlag, NodePath, Stack } from "$lib/api";

  let {
    windows,
    stacks,
    selectedId,
    readOnly,
    onSelect,
    onToggleOpen,
    onGeom,
    onFlag,
    onReveal,
    onUnstack,
    onReorder,
    onAddToStack,
    onCreateStack,
  }: {
    windows: WindowRect[];
    stacks: Stack[];
    selectedId: string | null;
    readOnly: boolean;
    onSelect: (id: string) => void;
    onToggleOpen: (w: WindowRect) => void;
    onGeom: (w: WindowRect, field: "x" | "y" | "w" | "h", value: number) => void;
    onFlag: (w: WindowRect, flag: BoolFlag, value: boolean) => void;
    onReveal: (path: NodePath) => void;
    onUnstack: (id: string) => void;
    onReorder: (container: string, members: string[]) => void;
    onAddToStack: (member: string, container: string) => void;
    onCreateStack: (m1: string, m2: string) => void;
  } = $props();

  // Right-click a property to reveal the value's node in the raw tree.
  // TODO(revisit): jump directly for now; a right-click context menu with a
  // "show in tree" item (and room for more actions) is the intended UX.
  const reveal = (path: NodePath) => (e: MouseEvent) => {
    e.preventDefault();
    onReveal(path);
  };
  function geomPath(w: WindowRect, field: "x" | "y" | "w" | "h"): NodePath {
    const g = w.geom!;
    return { x: g.x_path, y: g.y_path, w: g.w_path, h: g.h_path }[field];
  }

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

  // A stack's `members` list can name an id absent from `windows` on a
  // geometry-less file (the projection still reports the stack, but there's
  // no window-rect to show) — every lookup below must tolerate a miss.
  const findWindow = (id: string) => windows.find((w) => w.id === id);

  const freeWindows = $derived(windows.filter((w) => w.stack === null));

  function swapped(members: string[], i: number, j: number): string[] {
    const next = [...members];
    [next[i], next[j]] = [next[j], next[i]];
    return next;
  }
</script>

{#snippet rowHead(w: WindowRect)}
  {@const openFlag = w.flags.find((f) => f.name === "openWindows")}
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
{/snippet}

{#snippet detail(w: WindowRect)}
  {@const g = w.geom!}
  <div class="detail">
    <div class="coords">
      {#each COORDS as field}
        <label title="right-click: show in tree" oncontextmenu={reveal(geomPath(w, field))}>
          {field}
          <input
            type="number"
            value={g[field]}
            disabled={readOnly}
            onchange={numberEdit(w, field)} />
        </label>
      {/each}
    </div>
    <div class="flags">
      {#each detailFlags(w) as f (f.name)}
        <label
          class="flag"
          title={f.set.how === "unavailable"
            ? "Not present in this file"
            : f.set.how === "set"
              ? "right-click: show in tree"
              : ""}
          oncontextmenu={f.set.how === "set" ? reveal(f.set.path) : undefined}>
          <input
            type="checkbox"
            checked={f.value}
            disabled={readOnly || f.set.how === "unavailable"}
            onchange={(e) => onFlag(w, f, (e.target as HTMLInputElement).checked)} />
          {f.name}
        </label>
      {/each}
    </div>
  </div>
{/snippet}

<div class="window-panel">
  {#each stacks as stack (stack.container_id)}
    {@const containerWindow = findWindow(stack.container_id)}
    <div class="stack-group">
      {#if containerWindow}
        <div
          class="row frame"
          class:selected={stack.container_id === selectedId}
          use:scrollOnSelect={stack.container_id === selectedId}>
          <div class="row-head">
            <span class="frame-label" title="Stack frame">frame</span>
            {@render rowHead(containerWindow)}
            <span class="stack-count">{stack.members.length}</span>
          </div>
          {#if stack.container_id === selectedId && containerWindow.geom}
            {@render detail(containerWindow)}
          {/if}
        </div>
      {:else}
        <div class="stack-head">
          <span class="stack-title">{stack.container_label}</span>
          <span class="stack-count">{stack.members.length}</span>
        </div>
      {/if}
      {#each stack.members as memberId, i (memberId)}
        {@const w = findWindow(memberId)}
        {#if w}
          <div class="row member" class:selected={w.id === selectedId} use:scrollOnSelect={w.id === selectedId}>
            <div class="row-head">
              {@render rowHead(w)}
              <button
                class="stack-btn"
                disabled={readOnly || i === 0}
                title="Move up in stack order"
                aria-label="Move up in stack order"
                onclick={() => onReorder(stack.container_id, swapped(stack.members, i, i - 1))}>
                ↑
              </button>
              <button
                class="stack-btn"
                disabled={readOnly || i === stack.members.length - 1}
                title="Move down in stack order"
                aria-label="Move down in stack order"
                onclick={() => onReorder(stack.container_id, swapped(stack.members, i, i + 1))}>
                ↓
              </button>
              <button
                class="stack-btn"
                disabled={readOnly}
                title="Remove from stack"
                aria-label="Remove from stack"
                onclick={() => onUnstack(w.id)}>
                unstack
              </button>
            </div>
            {#if w.id === selectedId && w.geom}
              {@render detail(w)}
            {/if}
          </div>
        {/if}
      {/each}
    </div>
  {/each}

  {#each freeWindows as w (w.id)}
    <div class="row" class:selected={w.id === selectedId} use:scrollOnSelect={w.id === selectedId}>
      <div class="row-head">
        {@render rowHead(w)}
      </div>
      {#if stacks.length > 0 || freeWindows.length > 1}
        <div class="free-controls">
          {#if stacks.length > 0}
            <select
              aria-label="Add to stack"
              disabled={readOnly}
              value=""
              onchange={(e) => {
                const v = (e.currentTarget as HTMLSelectElement).value;
                if (v) onAddToStack(w.id, v);
              }}>
              <option value="" disabled>Add to stack…</option>
              {#each stacks as s (s.container_id)}
                <option value={s.container_id}>{s.container_label}</option>
              {/each}
            </select>
          {/if}
          {#if freeWindows.length > 1}
            <select
              aria-label="Stack with another window"
              disabled={readOnly}
              value=""
              onchange={(e) => {
                const v = (e.currentTarget as HTMLSelectElement).value;
                if (v) onCreateStack(w.id, v);
              }}>
              <option value="" disabled>Stack with…</option>
              {#each freeWindows.filter((o) => o.id !== w.id) as other (other.id)}
                <option value={other.id}>{other.label}</option>
              {/each}
            </select>
          {/if}
        </div>
      {/if}
      {#if w.id === selectedId && w.geom}
        {@render detail(w)}
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
  .stack-group {
    border-bottom: 1px solid var(--border);
  }
  .stack-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.5rem;
    background: rgba(255, 255, 255, 0.04);
    font-weight: 600;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .stack-count {
    font-weight: 400;
  }
  .row.frame .row-head {
    background: rgba(255, 255, 255, 0.04);
    font-weight: 600;
  }
  .frame-label {
    flex: 0 0 auto;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--fg-dim);
  }
  .row.member {
    border-bottom: none;
  }
  .row.member .row-head {
    padding-left: 1.1rem;
  }
  .row.member:last-child {
    border-bottom: 1px solid var(--border);
  }
  .stack-btn {
    flex: 0 0 auto;
    padding: 0 5px;
    font-size: 0.85em;
  }
  .stack-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .free-controls {
    display: flex;
    gap: 0.3rem;
    padding: 0 0.5rem 0.4rem 0.5rem;
    flex-wrap: wrap;
  }
  /* Native <select>/<option> render light-on-white by default even in this
     dark WebView2 shell unless given explicit colors — same reasoning as the
     .detail input styling below. */
  select {
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 4px;
    font: inherit;
    max-width: 9rem;
  }
  select option {
    background: var(--bg);
    color: var(--fg);
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
  .coords label {
    display: grid;
    gap: 0.1rem;
    font-size: 11px;
    color: var(--fg-dim);
  }
  /* Only the number fields get the boxed styling; a blanket `.detail input`
     rule also stretched the flag checkboxes to full width and misaligned them. */
  .detail input[type="number"] {
    width: 100%;
    box-sizing: border-box;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 4px;
    font: inherit;
  }
  .detail input[type="number"]:focus {
    outline: 1px solid var(--accent);
  }
  .flags {
    display: grid;
    gap: 0.15rem;
  }
  .flag {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 0.3rem;
    color: var(--fg);
  }
  .flag input {
    margin: 0;
    flex: 0 0 auto;
  }
</style>
