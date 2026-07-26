<script lang="ts">
  import type { WindowRect, BoolFlag, NodePath, Stack } from "$lib/api";
  import { describe, groupByFamily, displayName } from "$lib/windowLabels";
  import { windowMatches, NO_FILTER, type WindowFilter } from "$lib/layout";
  import ContextMenu, { type MenuItem } from "$lib/ContextMenu.svelte";

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
    filter = $bindable({ ...NO_FILTER }),
    focusFilter = $bindable(undefined),
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
    /** Shared with the canvas — see LayoutView. The panel renders the controls;
     * LayoutView owns the state and applies the same predicate to the rects. */
    filter?: WindowFilter;
    /** Exposed so the global Ctrl+F handler in +page.svelte can focus this
     * input from outside — LayoutView forwards it up. The input lives here,
     * so this is where the bind:this actually is. */
    focusFilter?: () => void;
  } = $props();

  let filterInput: HTMLInputElement | undefined = $state();
  focusFilter = () => {
    filterInput?.focus();
    filterInput?.select();
  };

  // Right-click opens a menu. This replaces the M2-era direct tree jump — the
  // TODO that shipped with the layout canvas.
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  function openMenu(e: MouseEvent, items: MenuItem[]) {
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY, items };
  }

  const copyId = (id: string): MenuItem => ({
    label: "Copy window id",
    // Best-effort: a clipboard refusal must not throw into the click handler.
    run: () => void navigator.clipboard.writeText(id).catch(() => {}),
  });

  const showInTree = (path: NodePath): MenuItem => ({
    label: "Show in tree",
    run: () => onReveal(path),
  });

  // The item lists are built here, not inline in the template: `f.set` is a
  // discriminated union, and TypeScript only narrows `f.set.path` inside a
  // plain function body — a narrowing written into a template ternary does not
  // reach the arrow function it creates.
  function rowMenu(w: WindowRect): MenuItem[] {
    const items: MenuItem[] = [];
    if (w.geom) {
      const path = w.geom.x_path;
      items.push({ label: "Show geometry in tree", run: () => onReveal(path) });
    }
    items.push(copyId(w.id), { label: "Select on canvas", run: () => onSelect(w.id) });
    return items;
  }

  function flagMenu(w: WindowRect, f: BoolFlag): MenuItem[] {
    const items: MenuItem[] = [];
    if (f.set.how === "set") items.push(showInTree(f.set.path));
    items.push(copyId(w.id));
    return items;
  }

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

  const freeWindows = $derived(windows.filter((w) => w.stack === null && windowMatches(w, filter)));
  // Folding is list presentation only: a family with more than one member
  // renders as one collapsible row. It never changes what the canvas draws —
  // that is the filter's job (LayoutView owns it).
  const freeGroups = $derived(groupByFamily(freeWindows));

  // Per-family collapse of the member rows. Families start folded: a real file
  // carries ~47 chat windows and folding is the whole point.
  let famOpen = $state<Record<string, boolean>>({});

  // A canvas click can select a window inside a folded family — unfold it, or
  // the selection is invisible and scrollOnSelect has nothing to scroll to.
  $effect(() => {
    if (selectedId === null) return;
    const fam = describe(selectedId).family;
    if (freeGroups.some((g) => g.family === fam && g.items.length > 1)) {
      famOpen[fam] = true;
    }
  });

  // Per-stack collapse of the member sub-rows (default expanded); the frame
  // row itself always stays visible.
  let collapsed = $state<Record<string, boolean>>({});

  function swapped(members: string[], i: number, j: number): string[] {
    const next = [...members];
    [next[i], next[j]] = [next[j], next[i]];
    return next;
  }

  // Members currently matching the filter, for gating the stack's frame row
  // and its count badge (I2) — a stack whose members are all filtered out
  // must disappear from the list exactly as it disappears from the canvas.
  function matchingMembers(stack: Stack): string[] {
    return stack.members.filter((id) => {
      const w = findWindow(id);
      return !!w && windowMatches(w, filter);
    });
  }
</script>

{#snippet rowHead(w: WindowRect)}
  {@const n = describe(w.id)}
  {@const openFlag = w.flags.find((f) => f.name === "openWindows")}
  <input
    type="checkbox"
    checked={w.open}
    disabled={readOnly || openFlag?.set.how === "unavailable"}
    title="Open (shown on the canvas)"
    aria-label="Open (shown on the canvas)"
    onchange={() => onToggleOpen(w)} />
  <button
    class="name"
    title={w.id}
    onclick={() => onSelect(w.id)}
    oncontextmenu={(e) => openMenu(e, rowMenu(w))}>
    {n.label}{#if n.detail}<span class="detail">{n.detail}</span>{/if}
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
        <label title="right-click for actions" oncontextmenu={(e) => openMenu(e, [showInTree(geomPath(w, field)), copyId(w.id)])}>
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
          title={f.set.how === "unavailable" ? "Not present in this file" : "right-click for actions"}
          oncontextmenu={(e) => openMenu(e, flagMenu(w, f))}>
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

{#snippet freeRow(w: WindowRect)}
  <!-- stackTargets is deliberately filtered too: it derives from freeWindows,
       so "Hide chat & session windows" also hides those windows from "Stack
       with…" (M7). Falls out of freeWindows being the shared source, but it's
       defensible on its own — you can only stack with what you can see — so
       it stays, recorded rather than silently inherited. -->
  {@const stackTargets = freeWindows.filter((o) => o.id !== w.id && o.renderable)}
  <div class="row" class:selected={w.id === selectedId} use:scrollOnSelect={w.id === selectedId}>
    <div class="row-head">
      {@render rowHead(w)}
    </div>
    {#if w.renderable && (stacks.length > 0 || stackTargets.length > 0)}
      <div class="free-controls">
        {#if stacks.length > 0}
          <select
            aria-label="Add to stack"
            disabled={readOnly}
            value=""
            onchange={(e) => {
              const el = e.currentTarget as HTMLSelectElement;
              const v = el.value;
              el.value = "";
              if (v) onAddToStack(w.id, v);
            }}>
            <option value="" disabled>Add to stack…</option>
            {#each stacks as s (s.container_id)}
              <option value={s.container_id}>{displayName(s.container_id)}</option>
            {/each}
          </select>
        {/if}
        {#if stackTargets.length > 0}
          <select
            aria-label="Stack with another window"
            disabled={readOnly}
            value=""
            onchange={(e) => {
              const el = e.currentTarget as HTMLSelectElement;
              const v = el.value;
              el.value = "";
              if (v) onCreateStack(w.id, v);
            }}>
            <option value="" disabled>Stack with…</option>
            {#each stackTargets as other (other.id)}
              <option value={other.id}>{displayName(other.id)}</option>
            {/each}
          </select>
        {/if}
      </div>
    {/if}
    {#if w.id === selectedId && w.geom}
      {@render detail(w)}
    {/if}
  </div>
{/snippet}

<div class="window-panel">
  <div class="filters">
    <input
      type="search"
      placeholder="Filter windows…"
      aria-label="Filter windows"
      bind:this={filterInput}
      bind:value={filter.text} />
    <label class="toggle">
      <input type="checkbox" bind:checked={filter.openOnly} />
      Open only
    </label>
    <label class="toggle">
      <input type="checkbox" bind:checked={filter.hideNoise} />
      Hide closed chat &amp; session windows
    </label>
  </div>
  {#each stacks as stack (stack.container_id)}
    {@const containerWindow = findWindow(stack.container_id)}
    {@const matched = matchingMembers(stack)}
    {@const containerMatches = !!containerWindow && windowMatches(containerWindow, filter)}
    {#if matched.length > 0 || containerMatches}
    <div class="stack-group">
      {#if containerWindow}
        <div
          class="row frame"
          class:selected={stack.container_id === selectedId}
          use:scrollOnSelect={stack.container_id === selectedId}>
          <div class="row-head">
            <button
              class="caret"
              aria-label="Collapse stack"
              onclick={(e) => { e.stopPropagation(); collapsed[stack.container_id] = !collapsed[stack.container_id]; }}>
              {collapsed[stack.container_id] ? "▸" : "▾"}
            </button>
            <span class="frame-label" title="Stack frame">frame</span>
            {@render rowHead(containerWindow)}
            <span class="stack-count">{matched.length}</span>
          </div>
          {#if stack.container_id === selectedId && containerWindow.geom}
            {@render detail(containerWindow)}
          {/if}
        </div>
      {:else}
        <div class="stack-head">
          <button
            class="caret"
            aria-label="Collapse stack"
            onclick={(e) => { e.stopPropagation(); collapsed[stack.container_id] = !collapsed[stack.container_id]; }}>
            {collapsed[stack.container_id] ? "▸" : "▾"}
          </button>
          <span class="stack-title" title={stack.container_id}>{describe(stack.container_id).label}</span>
          <span class="stack-count">{matched.length}</span>
        </div>
      {/if}
      {#if !collapsed[stack.container_id]}
        {#each stack.members as memberId, i (memberId)}
          {@const w = findWindow(memberId)}
          {#if w && windowMatches(w, filter)}
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
      {/if}
    </div>
    {/if}
  {/each}

  {#each freeGroups as group (group.family)}
    {#if group.items.length === 1}
      {@render freeRow(group.items[0])}
    {:else}
      <div class="fam-group">
        <div class="fam-head">
          <button
            class="caret"
            aria-label="Expand family"
            aria-expanded={!!famOpen[group.family]}
            onclick={() => (famOpen[group.family] = !famOpen[group.family])}>
            {famOpen[group.family] ? "▾" : "▸"}
          </button>
          <span class="fam-title">{group.label}</span>
          <span class="stack-count">{group.items.length}</span>
        </div>
        {#if famOpen[group.family]}
          {#each group.items as w (w.id)}
            <div class="fam-member">{@render freeRow(w)}</div>
          {/each}
        {/if}
      </div>
    {/if}
  {/each}

  {#if menu}
    <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
  {/if}
</div>

<style>
  .window-panel {
    overflow-y: auto;
    font-size: 13px;
    border-left: 1px solid var(--border);
    background: var(--bg-panel);
    color: var(--fg);
  }
  .filters {
    display: grid;
    gap: 0.25rem;
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--bg-panel);
    z-index: 1;
  }
  .filters input[type="search"] {
    width: 100%;
    box-sizing: border-box;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 2px 4px;
    font: inherit;
  }
  .filters input[type="search"]:focus {
    outline: 1px solid var(--accent);
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .toggle input {
    margin: 0;
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
  span.detail {
    color: var(--fg-dim);
    margin-left: 0.35rem;
    font-size: 0.9em;
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
  .caret {
    flex: 0 0 auto;
    background: none;
    border: none;
    color: var(--fg-dim);
    cursor: pointer;
    padding: 0 2px;
    font: inherit;
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
  div.detail {
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
  .fam-group {
    border-bottom: 1px solid var(--border);
  }
  .fam-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.5rem;
    background: rgba(255, 255, 255, 0.04);
    font-weight: 600;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .fam-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fam-member .row {
    border-bottom: none;
  }
  .fam-member .row-head {
    padding-left: 1.1rem;
  }
</style>
