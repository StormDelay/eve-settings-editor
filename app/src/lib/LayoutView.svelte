<script lang="ts">
  import { api, errMessage } from "$lib/api";
  import type { WindowLayout, WindowRect, BoolFlag, Mutation, NewValue, NodePath, Slot } from "$lib/api";
  import { canvasScale, toCanvas, toData, resizeRect, stackUnits, type Corner, type DrawUnit } from "$lib/layout";
  import WindowPanel from "$lib/WindowPanel.svelte";
  import { message } from "@tauri-apps/plugin-dialog";

  let {
    slot,
    runMutation,
    readOnly,
    refreshToken,
    selectedId = $bindable(null),
    onReveal,
  }: {
    slot: Slot;
    runMutation: (m: Mutation, rethrow?: boolean) => Promise<void>;
    readOnly: boolean;
    refreshToken: number;
    selectedId?: string | null;
    onReveal: (path: NodePath) => void;
  } = $props();

  let layout = $state<WindowLayout | null>(null);
  let containerWidth = $state(0);
  let canvasEl: HTMLDivElement | undefined = $state();
  // Live drag/resize preview by window id (data px); absent when not dragging.
  let preview: Record<string, { x: number; y: number; w: number; h: number }> = $state({});

  // ?./?? sidestep a TS limitation: narrowing `layout` doesn't carry across
  // separate reads inside a $derived expression (each read goes through its
  // reactive getter), so a `layout ? layout.x : ...` ternary won't type-check
  // here even though it's safe. canvasScale/toCanvas already treat a missing
  // reference dimension as "no-op", so reading through with `?? 0` is exact,
  // not an approximation.
  const scale = $derived(canvasScale(layout?.reference_w ?? 0, containerWidth));
  const units = $derived(stackUnits(layout ?? { reference_w: 0, reference_h: 0, windows: [], stacks: [] }));
  const canvasHeight = $derived(toCanvas(layout?.reference_h ?? 0, scale));

  async function load() {
    try {
      layout = await api.windowLayout(slot);
      if (selectedId && !layout.windows.some((w) => w.id === selectedId)) {
        selectedId = null;
      }
    } catch (e) {
      await message(errMessage(e), { title: "Layout unavailable", kind: "error" });
    }
  }

  // Reload when the parent signals a save/restore, or when the slot switches.
  let lastToken = -1;
  let lastSlot: Slot | null = null;
  $effect(() => {
    if (refreshToken !== lastToken || slot !== lastSlot) {
      lastToken = refreshToken;
      lastSlot = slot;
      load();
    }
  });

  // Rect position/size in data px: the live preview if dragging, else committed.
  const rectOf = (w: WindowRect) => preview[w.id] ?? {
    x: w.geom!.x, y: w.geom!.y, w: w.geom!.w, h: w.geom!.h,
  };

  // --- Mutations -----------------------------------------------------------

  function flagMutation(flag: BoolFlag, next: boolean): Mutation | null {
    if (flag.set.how === "set") {
      return { op: "set_scalar", path: flag.set.path, text: next ? "true" : "false" };
    }
    if (flag.set.how === "insert") {
      const value: NewValue = { kind: "bool", v: next };
      return { op: "insert_dict_entry", parent: flag.set.parent, key: flag.set.key, value };
    }
    return null; // unavailable
  }

  function geomMutations(w: WindowRect, next: { x?: number; y?: number; w?: number; h?: number }): Mutation[] {
    const g = w.geom!;
    const ms: Mutation[] = [];
    const setInt = (path: typeof g.x_path, v: number) =>
      ms.push({ op: "set_scalar", path, text: String(v) });
    if (next.x !== undefined && next.x !== g.x) setInt(g.x_path, next.x);
    if (next.y !== undefined && next.y !== g.y) setInt(g.y_path, next.y);
    if (next.w !== undefined && next.w !== g.w) setInt(g.w_path, next.w);
    if (next.h !== undefined && next.h !== g.h) setInt(g.h_path, next.h);
    // New coords are in the reference resolution; align this window's saved
    // resolution to it so the numbers stay meaningful.
    if (ms.length > 0 && !w.resolution_matches && layout) {
      setInt(g.screen_w_path, layout.reference_w);
      setInt(g.screen_h_path, layout.reference_h);
    }
    return ms;
  }

  async function commit(ms: Mutation[]) {
    if (ms.length === 0) return;
    try {
      for (const m of ms) await runMutation(m, true);
    } catch (e) {
      await message(errMessage(e), { title: "Edit failed", kind: "error" });
    }
    await load(); // refresh paths/values from the authoritative document
  }

  // --- Panel callbacks -----------------------------------------------------

  const onSelect = (id: string) => (selectedId = id);

  function onToggleOpen(w: WindowRect) {
    const open = w.flags.find((f) => f.name === "openWindows");
    if (!open) return;
    const m = flagMutation(open, !open.value);
    if (m) commit([m]);
  }

  const onGeom = (w: WindowRect, field: "x" | "y" | "w" | "h", value: number) =>
    commit(geomMutations(w, { [field]: value }));

  function onFlag(w: WindowRect, flag: BoolFlag, value: boolean) {
    const m = flagMutation(flag, value);
    if (m) commit([m]);
  }

  // --- Stack membership ------------------------------------------------------

  async function runStack(p: Promise<WindowLayout>) {
    try {
      layout = await p;
    } catch (e) {
      await message(errMessage(e), { title: "Stack edit failed", kind: "error" });
    }
  }
  const onUnstack = (id: string) => runStack(api.stackUnstack(id));
  const onReorder = (container: string, members: string[]) => runStack(api.stackReorder(container, members));
  const onAddToStack = (member: string, container: string) => runStack(api.stackAdd(member, container));
  const onCreateStack = (m1: string, m2: string) => runStack(api.stackCreate(m1, m2));

  // --- Canvas drag & resize ------------------------------------------------

  type Drag =
    | { kind: "move"; unit: DrawUnit; startX: number; startY: number; ox: number; oy: number }
    | { kind: "resize"; unit: DrawUnit; corner: Corner; startX: number; startY: number; ox: number; oy: number; ow: number; oh: number };
  let drag: Drag | null = null;

  // Every renderable window a move/resize of this unit must repeat the same
  // delta/rect onto: the anchor, its open stack members (tabs), and — for a
  // stack — the container's own window entry if it carries geometry (the
  // container's geom is the stack's true position on screen). De-duplicated
  // by id since the anchor is often also one of the tabs or the container.
  function unitWindows(unit: DrawUnit): WindowRect[] {
    const result: WindowRect[] = [unit.anchor];
    const ids = new Set([unit.anchor.id]);
    for (const t of unit.tabs) {
      if (ids.has(t.id)) continue;
      ids.add(t.id);
      result.push(t);
    }
    if (unit.stack && layout) {
      const container = layout.windows.find((w) => w.id === unit.stack!.container_id);
      if (container && container.renderable && !ids.has(container.id)) {
        result.push(container);
      }
    }
    return result;
  }

  // Capture on the canvas (not the rectangle) so its onpointermove/up keep
  // firing even as the pointer leaves the rectangle during a drag.
  function startMove(unit: DrawUnit, e: PointerEvent) {
    if (readOnly) return;
    selectedId = unit.anchor.id;
    drag = { kind: "move", unit, startX: e.clientX, startY: e.clientY, ox: unit.anchor.geom!.x, oy: unit.anchor.geom!.y };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  function startResize(unit: DrawUnit, corner: Corner, e: PointerEvent) {
    if (readOnly) return;
    selectedId = unit.anchor.id;
    drag = {
      kind: "resize", unit, corner, startX: e.clientX, startY: e.clientY,
      ox: unit.anchor.geom!.x, oy: unit.anchor.geom!.y, ow: unit.anchor.geom!.w, oh: unit.anchor.geom!.h,
    };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
    e.stopPropagation();
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    const dx = toData(e.clientX - drag.startX, scale);
    const dy = toData(e.clientY - drag.startY, scale);
    if (drag.kind === "move") {
      preview = { ...preview, [drag.unit.anchor.id]: { ...rectOf(drag.unit.anchor), x: drag.ox + dx, y: drag.oy + dy } };
    } else {
      preview = {
        ...preview,
        [drag.unit.anchor.id]: resizeRect({ x: drag.ox, y: drag.oy, w: drag.ow, h: drag.oh }, drag.corner, dx, dy),
      };
    }
  }

  function clearPreview(id: string) {
    const rest = { ...preview };
    delete rest[id];
    preview = rest;
  }

  async function onPointerUp() {
    if (!drag) return;
    const p = preview[drag.unit.anchor.id];
    const d = drag;
    drag = null;
    if (!p) return;
    // Fan the new anchor rect out to every renderable window in the unit so a
    // stack moves/resizes coherently and stale members are repaired.
    const targets = unitWindows(d.unit);
    const next = d.kind === "move" ? { x: p.x, y: p.y } : { x: p.x, y: p.y, w: p.w, h: p.h };
    const ms = targets.flatMap((w) => geomMutations(w, next));
    await commit(ms);
    clearPreview(d.unit.anchor.id);
  }
</script>

{#if layout === null}
  <p class="hint">Loading layout…</p>
{:else}
  <div class="layout-view">
    <div class="canvas-wrap" bind:clientWidth={containerWidth}>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="canvas"
        bind:this={canvasEl}
        style="width: {toCanvas(layout.reference_w, scale)}px; height: {canvasHeight}px;"
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}>
        {#each units as unit (unit.key)}
          {@const r = rectOf(unit.anchor)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="win"
            class:selected={unit.tabs.some((t) => t.id === selectedId) || unit.anchor.id === selectedId}
            class:stacked={!!unit.stack}
            style="left: {toCanvas(r.x, scale)}px; top: {toCanvas(r.y, scale)}px;
                   width: {toCanvas(r.w, scale)}px; height: {toCanvas(r.h, scale)}px;"
            onpointerdown={(e) => startMove(unit, e)}>
            {#if unit.stack}
              <div class="tabs">
                {#each unit.tabs as tab (tab.id)}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <span class="tab" class:active={tab.id === selectedId}
                    onpointerdown={(e) => { e.stopPropagation(); selectedId = tab.id; }}>{tab.label}</span>
                {/each}
              </div>
            {:else}
              <span class="win-label">{unit.anchor.label}</span>
            {/if}
            {#if unit.anchor.id === selectedId || unit.tabs.some((t) => t.id === selectedId)}
              {#each (["tl", "tr", "bl", "br"] as const) as c}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <span class="resize {c}" onpointerdown={(e) => startResize(unit, c, e)}></span>
              {/each}
            {/if}
          </div>
        {/each}
      </div>
      <p class="ref">reference {layout.reference_w}×{layout.reference_h}</p>
    </div>
    <WindowPanel
      windows={layout.windows}
      stacks={layout.stacks}
      {selectedId}
      {readOnly}
      {onSelect}
      {onToggleOpen}
      {onGeom}
      {onFlag}
      {onReveal}
      {onUnstack}
      {onReorder}
      {onAddToStack}
      {onCreateStack} />
  </div>
{/if}

<style>
  .layout-view {
    display: grid;
    /* minmax(0,1fr) lets the canvas take the remaining space without being
       pushed to zero by a wide window list; the panel is bounded. */
    grid-template-columns: minmax(0, 1fr) minmax(14rem, 20rem);
    height: 100%;
    overflow: hidden;
  }
  .canvas-wrap {
    overflow: auto;
    padding: 0.5rem;
  }
  .canvas {
    position: relative;
    background: #1b1f27;
    background-image: linear-gradient(#2a2f3a 1px, transparent 1px),
      linear-gradient(90deg, #2a2f3a 1px, transparent 1px);
    background-size: 40px 40px;
    border: 1px solid #444;
  }
  .win {
    position: absolute;
    box-sizing: border-box;
    background: rgba(96, 165, 250, 0.25);
    border: 1px solid #60a5fa;
    color: #dbeafe;
    font-size: 11px;
    overflow: hidden;
    cursor: move;
    touch-action: none;
  }
  .win.selected {
    border-color: #f59e0b;
    background: rgba(245, 158, 11, 0.25);
    z-index: 1;
  }
  .win-label {
    padding: 1px 3px;
    display: inline-block;
    pointer-events: none;
  }
  .tabs {
    display: flex;
    gap: 1px;
    background: #11141a;
    overflow: hidden;
  }
  .tab {
    padding: 1px 4px;
    background: #2a2f3a;
    color: #dbeafe;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tab.active {
    background: #f59e0b;
    color: #1b1f27;
  }
  .resize {
    position: absolute;
    width: 12px;
    height: 12px;
    background: currentColor;
    opacity: 0.6;
    touch-action: none;
  }
  .resize.tl { left: 0; top: 0; cursor: nwse-resize; }
  .resize.tr { right: 0; top: 0; cursor: nesw-resize; }
  .resize.bl { left: 0; bottom: 0; cursor: nesw-resize; }
  .resize.br { right: 0; bottom: 0; cursor: nwse-resize; }
  .ref {
    color: #888;
    font-size: 11px;
    margin: 0.3rem 0 0;
  }
  .hint {
    color: #888;
    padding: 1rem;
  }
</style>
