<script lang="ts">
  import { api, errMessage } from "$lib/api";
  import type { WindowLayout, WindowRect, BoolFlag, Mutation, NewValue } from "$lib/api";
  import { canvasScale, toCanvas, toData, openWindows } from "$lib/layout";
  import WindowPanel from "$lib/WindowPanel.svelte";
  import { message } from "@tauri-apps/plugin-dialog";

  let {
    runMutation,
    readOnly,
    refreshToken,
  }: {
    runMutation: (m: Mutation, rethrow?: boolean) => Promise<void>;
    readOnly: boolean;
    refreshToken: number;
  } = $props();

  let layout = $state<WindowLayout | null>(null);
  let selectedId: string | null = $state(null);
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
  const drawn = $derived(openWindows(layout?.windows ?? []));
  const canvasHeight = $derived(toCanvas(layout?.reference_h ?? 0, scale));

  async function load() {
    try {
      layout = await api.windowLayout();
      if (selectedId && !layout.windows.some((w) => w.id === selectedId)) {
        selectedId = null;
      }
    } catch (e) {
      await message(errMessage(e), { title: "Layout unavailable", kind: "error" });
    }
  }

  // Reload when the parent signals a save/restore.
  let lastToken = -1;
  $effect(() => {
    if (refreshToken !== lastToken) {
      lastToken = refreshToken;
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

  const onStack = (w: WindowRect, text: string) =>
    w.stacks && commit([{ op: "set_scalar", path: w.stacks.path, text }]);

  // --- Canvas drag & resize ------------------------------------------------

  type Drag =
    | { kind: "move"; w: WindowRect; startX: number; startY: number; ox: number; oy: number }
    | { kind: "resize"; w: WindowRect; startX: number; startY: number; ow: number; oh: number };
  let drag: Drag | null = null;

  // Capture on the canvas (not the rectangle) so its onpointermove/up keep
  // firing even as the pointer leaves the rectangle during a drag.
  function startMove(w: WindowRect, e: PointerEvent) {
    if (readOnly) return;
    selectedId = w.id;
    drag = { kind: "move", w, startX: e.clientX, startY: e.clientY, ox: w.geom!.x, oy: w.geom!.y };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  function startResize(w: WindowRect, e: PointerEvent) {
    if (readOnly) return;
    selectedId = w.id;
    drag = { kind: "resize", w, startX: e.clientX, startY: e.clientY, ow: w.geom!.w, oh: w.geom!.h };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
    e.stopPropagation();
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    const dx = toData(e.clientX - drag.startX, scale);
    const dy = toData(e.clientY - drag.startY, scale);
    if (drag.kind === "move") {
      preview = { ...preview, [drag.w.id]: { ...rectOf(drag.w), x: drag.ox + dx, y: drag.oy + dy } };
    } else {
      preview = {
        ...preview,
        [drag.w.id]: { ...rectOf(drag.w), w: Math.max(0, drag.ow + dx), h: Math.max(0, drag.oh + dy) },
      };
    }
  }

  function onPointerUp() {
    if (!drag) return;
    const w = drag.w;
    const p = preview[w.id];
    const d = drag;
    drag = null;
    const rest = { ...preview };
    delete rest[w.id];
    preview = rest;
    if (!p) return;
    commit(geomMutations(w, d.kind === "move" ? { x: p.x, y: p.y } : { w: p.w, h: p.h }));
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
        {#each drawn as w (w.id)}
          {@const r = rectOf(w)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="win"
            class:selected={w.id === selectedId}
            style="left: {toCanvas(r.x, scale)}px; top: {toCanvas(r.y, scale)}px;
                   width: {toCanvas(r.w, scale)}px; height: {toCanvas(r.h, scale)}px;"
            onpointerdown={(e) => startMove(w, e)}>
            <span class="win-label">{w.label}</span>
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span class="resize" onpointerdown={(e) => startResize(w, e)}></span>
          </div>
        {/each}
      </div>
      <p class="ref">reference {layout.reference_w}×{layout.reference_h}</p>
    </div>
    <WindowPanel
      windows={layout.windows}
      {selectedId}
      {readOnly}
      {onSelect}
      {onToggleOpen}
      {onGeom}
      {onFlag}
      {onStack} />
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
  .resize {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 12px;
    height: 12px;
    cursor: se-resize;
    background: currentColor;
    opacity: 0.6;
    touch-action: none;
  }
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
