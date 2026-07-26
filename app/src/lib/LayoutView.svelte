<script lang="ts">
  import { api, errMessage } from "$lib/api";
  import type { WindowLayout, WindowRect, BoolFlag, Mutation, NewValue, NodePath, Slot, Hud } from "$lib/api";
  import {
    canvasScale, toCanvas, toData, resizeRect, stackUnits, hudRects, shipOffsetFromX,
    hudPointFromRect, NO_FILTER, filterIsActive, visibleIds, drawnWindowCount,
    snapLines, movingEdges, snapDelta,
    type Corner, type DrawUnit, type FurnitureRect, type WindowFilter, type SnapLines,
  } from "$lib/layout";
  import { nameOf } from "$lib/windowLabels";
  import WindowPanel from "$lib/WindowPanel.svelte";
  import HudPanel from "$lib/HudPanel.svelte";
  import { message } from "@tauri-apps/plugin-dialog";

  let {
    slot,
    runMutations,
    readOnly,
    refreshToken,
    userOpen,
    selectedId = $bindable(null),
    onReveal,
    onDirty,
    sharedNames = [],
    focusFilter = $bindable(undefined),
  }: {
    slot: Slot;
    runMutations: (ms: Mutation[], rethrow?: boolean) => Promise<void>;
    readOnly: boolean;
    refreshToken: number;
    userOpen: boolean;
    selectedId?: string | null;
    onReveal: (path: NodePath) => void;
    onDirty: (slot: Slot) => void;
    /** Other characters on this account — named in HudPanel's account-row
     * legend. Unlike the Overview/Autofill views, only four of this view's
     * fields are account-wide, so the warning belongs on those rows, not
     * above the whole view. */
    sharedNames?: string[];
    /** Exposed so +page.svelte's global Ctrl+F handler can focus the window
     * filter input when this view is active. Forwarded from WindowPanel,
     * where the input actually lives — see its own focusFilter doc. */
    focusFilter?: () => void;
  } = $props();

  let layout = $state<WindowLayout | null>(null);
  let hud = $state<Hud | null>(null);
  let containerWidth = $state(0);
  let canvasEl: HTMLDivElement | undefined = $state();
  // Live drag/resize preview by window id (data px); absent when not dragging.
  let preview: Record<string, { x: number; y: number; w: number; h: number }> = $state({});
  // Live drag preview for furniture, keyed by kind (data px).
  let fPreview: Record<string, { x: number; y: number }> = $state({});
  // The selected furniture element, if any. Furniture isn't a window — it has
  // no id in `layout.windows` — so it needs its own selection alongside
  // `selectedId`; the two are kept mutually exclusive (see selectWindow).
  let selectedFurniture = $state<FurnitureRect["kind"] | null>(null);

  // The filter is shared: it narrows the window list AND what the canvas draws.
  // Folding families in the panel is separate and list-only.
  // Deliberately persists across file switches (slot/refreshToken/userOpen
  // changes below do NOT clear it) — that lets the same subset stay applied
  // while flipping between characters to compare them. The "showing N of M
  // · reset" counter is what keeps a carried-over filter visible instead of
  // silently misleading.
  let filter = $state<WindowFilter>({ ...NO_FILTER });

  // ?./?? sidestep a TS limitation: narrowing `layout` doesn't carry across
  // separate reads inside a $derived expression (each read goes through its
  // reactive getter), so a `layout ? layout.x : ...` ternary won't type-check
  // here even though it's safe. canvasScale/toCanvas already treat a missing
  // reference dimension as "no-op", so reading through with `?? 0` is exact,
  // not an approximation.
  const scale = $derived(canvasScale(layout?.reference_w ?? 0, containerWidth));
  const visible = $derived(
    layout && filterIsActive(filter) ? visibleIds(layout.windows, filter) : null,
  );
  const units = $derived(
    stackUnits(layout ?? { reference_w: 0, reference_h: 0, windows: [], stacks: [] }, visible),
  );
  // Both ends counted from what stackUnits actually draws, not the raw window
  // list (M5): a stack draws one rectangle but represents each of its visible
  // tabs, and a container that matches the filter while none of its members do
  // draws nothing at all, so counting the raw list over-reports that case.
  // The denominator re-runs stackUnits unfiltered rather than reusing
  // `drawable`, or the unfiltered case would read "67 of 68".
  const allUnits = $derived(
    stackUnits(layout ?? { reference_w: 0, reference_h: 0, windows: [], stacks: [] }, null),
  );
  const shownCount = $derived(drawnWindowCount(units));
  const totalCount = $derived(drawnWindowCount(allUnits));
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
    // Furniture is a bonus view: an account file open on its own, or a document
    // with no HUD keys, must not take the canvas down with it.
    hud = await api.hud().catch(() => null);
  }

  // Reload when the parent signals a save/restore, when the slot switches, or
  // when an account gets paired while this view is open (the four account-side
  // HUD rows and the neocom bar depend on it, and pairing doesn't otherwise
  // bump refreshToken or change slot). Still only reloads when something
  // actually changed, not on every tick.
  let lastToken = -1;
  let lastSlot: Slot | null = null;
  let lastUserOpen = false;
  $effect(() => {
    if (refreshToken !== lastToken || slot !== lastSlot || userOpen !== lastUserOpen) {
      lastToken = refreshToken;
      lastSlot = slot;
      lastUserOpen = userOpen;
      // A preview belongs to the document that was open when it was made.
      // EVE window ids are per-character dict keys and common ones (overview,
      // market, ...) repeat across characters, so a preview left over from
      // the previous document could otherwise be committed onto a same-named
      // window belonging to whatever just loaded. fPreview has the same
      // hazard and worse: it's keyed by furniture `kind`, which collides
      // across every document by construction (there's only one "shipui").
      preview = {};
      fPreview = {};
      nudging = null;
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
      await runMutations(ms, true);
    } catch (e) {
      await message(errMessage(e), { title: "Edit failed", kind: "error" });
    }
    await load(); // refresh paths/values from the authoritative document
  }

  // --- Panel callbacks -----------------------------------------------------

  /** Selecting a window clears the furniture selection, and vice versa (see
   * startFurniture) — the canvas shows one selection, not two. */
  function selectWindow(id: string) {
    selectedId = id;
    selectedFurniture = null;
  }

  /** The mirror of clicking a rectangle: selecting a group in HudPanel
   * highlights the furniture it edits on the canvas. */
  function selectFurniture(kind: FurnitureRect["kind"]) {
    selectedFurniture = kind;
    selectedId = null;
  }

  const onSelect = (id: string) => selectWindow(id);

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
      onDirty("char"); // stack ops edit the character document in the backend
      if (selectedId && !layout.windows.some((w) => w.id === selectedId)) selectedId = null;
    } catch (e) {
      await message(errMessage(e), { title: "Stack edit failed", kind: "error" });
    }
  }
  const onUnstack = (id: string) => runStack(api.stackUnstack(id));
  const onReorder = (container: string, members: string[]) => runStack(api.stackReorder(container, members));
  const onAddToStack = (member: string, container: string) => runStack(api.stackAdd(member, container));
  const onCreateStack = (m1: string, m2: string) => runStack(api.stackCreate(m1, m2));

  /** Write one HUD field and refresh the projection. */
  async function setHud(name: string, text: string) {
    try {
      hud = await api.setHudValue(name, text);
      const e = hud.entries.find((x) => x.name === name);
      onDirty(e?.scope === "account" ? "user" : "char");
    } catch (e) {
      await message(errMessage(e), { title: "HUD edit failed", kind: "error" });
    }
  }

  // --- Canvas drag & resize ------------------------------------------------

  type Drag =
    | { kind: "move"; unit: DrawUnit; startX: number; startY: number; ox: number; oy: number; lines: SnapLines }
    | { kind: "resize"; unit: DrawUnit; corner: Corner; startX: number; startY: number; ox: number; oy: number; ow: number; oh: number; lines: SnapLines }
    | { kind: "furniture"; f: FurnitureRect; startX: number; startY: number; ox: number; oy: number };
  let drag: Drag | null = null;

  // The lines the current drag has locked onto, in data px; null when this axis
  // isn't snapped. Drawn as guides, cleared on drop.
  let guides = $state<{ x: number | null; y: number | null }>({ x: null, y: null });

  // The window id a key-repeat nudge is currently in flight for (Task 3), so a
  // commit landing mid-nudge doesn't clear the preview under it.
  let nudging: string | null = null;

  const furniture = $derived(hud && layout ? hudRects(hud, layout) : []);
  const fRectOf = (f: FurnitureRect) => fPreview[f.kind] ?? { x: f.x, y: f.y };

  /** Candidate edges for a drag of `unit`: every rect the canvas currently
   * draws except the dragged unit's own windows, plus the furniture, plus the
   * screen. Displayed rects (rectOf), so a neighbour still showing a preview
   * offers the edge the player can see. Collected once per drag — the set stays
   * fixed while the dragged rect moves through it. */
  function linesFor(unit: DrawUnit): SnapLines {
    const mine = new Set(unit.fanTargets.map((w) => w.id));
    const rects = units
      .filter((u) => !mine.has(u.anchor.id))
      .map((u) => rectOf(u.anchor));
    for (const f of furniture) rects.push({ ...fRectOf(f), w: f.w, h: f.h });
    return snapLines(rects, layout?.reference_w ?? 0, layout?.reference_h ?? 0);
  }

  // Selecting furniture and dragging it are separate: the neocom can't be
  // dragged (its width is a field, not a rect) but must still be selectable, or
  // clicking it looks broken. Selection is exclusive with the window selection,
  // so exactly one thing on the canvas ever reads as selected.
  function startFurniture(f: FurnitureRect, e: PointerEvent) {
    selectFurniture(f.kind);
    e.stopPropagation();
    if (readOnly || f.drag === "none") return;
    const r = fRectOf(f);
    drag = { kind: "furniture", f, startX: e.clientX, startY: e.clientY, ox: r.x, oy: r.y };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  // Capture on the canvas (not the rectangle) so its onpointermove/up keep
  // firing even as the pointer leaves the rectangle during a drag.
  function startMove(unit: DrawUnit, e: PointerEvent) {
    if (readOnly) return;
    selectWindow(unit.anchor.id);
    // Origin from the DISPLAYED rect (preview if a prior drop is still
    // committing), not the committed geom — otherwise a re-drag before the
    // async commit lands would start from stale coordinates and jump.
    const r = rectOf(unit.anchor);
    drag = { kind: "move", unit, startX: e.clientX, startY: e.clientY, ox: r.x, oy: r.y, lines: linesFor(unit) };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
  }

  function startResize(unit: DrawUnit, corner: Corner, e: PointerEvent) {
    if (readOnly) return;
    selectWindow(unit.anchor.id);
    // Origin from the displayed rect (see startMove), so a resize started
    // before a prior drop finishes committing doesn't jump.
    const r = rectOf(unit.anchor);
    drag = {
      kind: "resize", unit, corner, startX: e.clientX, startY: e.clientY,
      ox: r.x, oy: r.y, ow: r.w, oh: r.h, lines: linesFor(unit),
    };
    canvasEl?.setPointerCapture(e.pointerId);
    e.preventDefault();
    e.stopPropagation();
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    const dx = toData(e.clientX - drag.startX, scale);
    const dy = toData(e.clientY - drag.startY, scale);
    if (drag.kind === "furniture") {
      const f = drag.f;
      fPreview = {
        ...fPreview,
        [f.kind]: { x: drag.ox + dx, y: f.drag === "xy" ? drag.oy + dy : drag.oy },
      };
      return;
    }
    // Six CANVAS px, so the grab feels identical however far the canvas is
    // scaled down. Alt held passes the drag straight through — read off the
    // event, so pressing or releasing it mid-drag takes effect on the next
    // pointer move (there's no key listener here to catch it sooner; holding
    // Alt without moving the mouse leaves the last snap applied).
    const tol = toData(6, scale);
    const raw = drag.kind === "move"
      ? { ...rectOf(drag.unit.anchor), x: drag.ox + dx, y: drag.oy + dy }
      : resizeRect({ x: drag.ox, y: drag.oy, w: drag.ow, h: drag.oh }, drag.corner, dx, dy);
    const corner = drag.kind === "resize" ? drag.corner : null;
    const snap = e.altKey
      ? { dx: 0, dy: 0, gx: null, gy: null }
      : snapDelta(movingEdges(raw, corner), drag.lines, tol);
    guides = { x: snap.gx, y: snap.gy };
    preview = {
      ...preview,
      [drag.unit.anchor.id]: drag.kind === "move"
        ? { ...raw, x: raw.x + snap.dx, y: raw.y + snap.dy }
        // The correction goes into the DELTA, so resizeRect's anchor-crossing
        // guards still run on the final numbers.
        : resizeRect({ x: drag.ox, y: drag.oy, w: drag.ow, h: drag.oh }, drag.corner, dx + snap.dx, dy + snap.dy),
    };
  }

  function clearPreview(id: string) {
    const rest = { ...preview };
    delete rest[id];
    preview = rest;
  }

  async function onPointerUp() {
    if (!drag) return;
    const d = drag;
    drag = null;
    guides = { x: null, y: null };

    if (d.kind === "furniture") {
      const p = fPreview[d.f.kind];
      if (!p) return;
      if (d.f.kind === "shipui" && layout) {
        // Compare the derived offsets, not the raw preview x against the
        // drag-start rect x — those are different quantities (a rect
        // coordinate vs. a stored offset) and comparing them directly would
        // either miss a real change or flag a no-op drag as one, depending on
        // how hudRects' ship-HUD placement is defined. shipOffsetFromX(d.f.x, …)
        // recovers the offset hudRects placed this rect at, since d.f is the
        // rect captured at drag start (undragged, so still the committed one).
        const next = shipOffsetFromX(p.x, layout.reference_w);
        if (next !== shipOffsetFromX(d.f.x, layout.reference_w)) {
          await setHud("ship_offset", String(next));
        }
      } else if (d.f.kind === "fighter" || d.f.kind === "badge") {
        const prefix = d.f.kind === "fighter" ? "fighter" : "badge";
        // Route through hudPointFromRect (see layout.ts) rather than writing
        // the raw preview rect coordinates: it's the point-convention inverse
        // that must stay matched with hudRects' placement, and comparing its
        // output (not the raw preview x/y) is what makes a sub-pixel drag that
        // rounds back to the same stored value a no-op instead of a dirtying write.
        const stored = hudPointFromRect(d.f.kind, p.x, p.y);
        if (stored.x !== d.f.x) await setHud(`${prefix}_x`, String(stored.x));
        if (stored.y !== d.f.y) await setHud(`${prefix}_y`, String(stored.y));
      }
      // A re-grab on the same furniture piece may have started during the
      // async write and now owns fPreview — don't wipe it out from under the
      // new drag. (The cast: TS narrowed `drag` to null above and can't see
      // the reassignment a concurrent startFurniture may have made across
      // the await.)
      const activeF = drag as Drag | null;
      if (!activeF || activeF.kind !== "furniture" || activeF.f.kind !== d.f.kind) {
        const rest = { ...fPreview };
        delete rest[d.f.kind];
        fPreview = rest;
      }
      return;
    }

    await commitUnit(d.unit);
  }

  /** Commit a unit's previewed rect: fan it out to every renderable window in
   * the unit so a stack moves/resizes coherently and stale members are
   * repaired, then drop the preview unless something has re-claimed it. The
   * full rect (not just x/y) is sent even for a move, so members also snap to
   * the anchor's w/h — geomMutations diffs per field, so an unchanged w/h emits
   * nothing and plain single-window units are unaffected. */
  async function commitUnit(unit: DrawUnit) {
    const p = preview[unit.anchor.id];
    if (!p) return;
    const next = { x: p.x, y: p.y, w: p.w, h: p.h };
    await commit(unit.fanTargets.flatMap((w) => geomMutations(w, next)));
    // A re-drag or a fresh nudge on the same window may have started during the
    // async commit and now owns the preview — don't wipe it out from under it.
    // (The cast: TS narrowed `drag` to null in onPointerUp and can't see a
    // reassignment a concurrent startMove may have made across the await.)
    const active = drag as Drag | null;
    const dragging = active && active.kind !== "furniture" && active.unit.anchor.id === unit.anchor.id;
    if (!dragging && nudging !== unit.anchor.id) clearPreview(unit.anchor.id);
  }

  // --- Arrow-key nudge -------------------------------------------------------
  // Bound on the window rather than a focusable canvas: the selection can just
  // as well have been made in the window panel, and a focus-scoped handler
  // would silently do nothing in that case.

  const NUDGE = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] } as const;

  /** The unit the nudge moves: the one whose anchor or tabs carry the
   * selection, so nudging a stacked window moves its whole stack — the same
   * unit a canvas drag would have grabbed. */
  const selectedUnit = () =>
    units.find((u) => u.anchor.id === selectedId || u.tabs.some((t) => t.id === selectedId)) ?? null;

  function onKeyDown(e: KeyboardEvent) {
    // Alt is the snap-disable modifier for drags; a nudge never snaps, so
    // Alt+Arrow has nothing to disable and is left to do nothing.
    if (readOnly || drag || e.ctrlKey || e.metaKey || e.altKey) return;
    const step = NUDGE[e.key as keyof typeof NUDGE];
    if (!step) return;
    // Never steal the arrows from a field that uses them — a text box moves
    // its caret, a number input steps its value. A checkbox or radio does
    // NOT: the panel's own filter toggles are checkboxes, and treating every
    // INPUT alike left the nudge dead for as long as one kept focus, with the
    // arrows scrolling the window list instead.
    const t = e.target as HTMLElement | null;
    const kind = (t as HTMLInputElement | null)?.type;
    if (t && (t.isContentEditable || t.tagName === "SELECT" || t.tagName === "TEXTAREA"
      || (t.tagName === "INPUT" && kind !== "checkbox" && kind !== "radio"))) return;
    // A held nudge owns its unit until keyup: re-resolving from the live
    // selection on every auto-repeat keydown would let a mid-hold selection
    // change (e.g. a click in the window panel) retarget onto a different
    // unit, stranding the first unit's accumulated preview uncommitted.
    const unit = nudging ? (units.find((u) => u.anchor.id === nudging) ?? null) : selectedUnit();
    if (!unit) return;
    e.preventDefault(); // or the canvas pane scrolls out from under the nudge
    const n = e.shiftKey ? 10 : 1;
    const r = rectOf(unit.anchor);
    // Preview only — no backend traffic per keypress. Key auto-repeat fires
    // dozens of keydowns and exactly ONE keyup, so a glide costs one commit.
    // No snapping: nudging is the tool you reach for when a snap put the window
    // one pixel off, and snapping it back would make the two fight.
    nudging = unit.anchor.id;
    preview = { ...preview, [unit.anchor.id]: { ...r, x: r.x + step[0] * n, y: r.y + step[1] * n } };
  }

  /** Commit whatever nudge is in flight, if any. Shared by keyup and window
   * blur: if the webview loses focus mid-hold (Alt+Tab, a taskbar click, an OS
   * notification), the keyup fires elsewhere and `nudging` would otherwise
   * stay stuck on the old unit — the next arrow press on a freshly selected
   * window would then resolve `nudging`'s stale unit instead, committing the
   * wrong window's stranded glide to the file on the eventual keyup. */
  async function endNudge() {
    if (!nudging) return;
    const id = nudging;
    nudging = null;
    const unit = units.find((u) => u.anchor.id === id);
    if (unit) await commitUnit(unit);
  }

  const onKeyUp = (e: KeyboardEvent) => {
    if (e.key in NUDGE) return endNudge();
  };
</script>

<!-- blur doesn't bubble, so this only fires for the window itself (an inner
     input losing focus to another inner element won't trip it) — see endNudge. -->
<svelte:window onkeydown={onKeyDown} onkeyup={onKeyUp} onblur={endNudge} />

{#if layout === null}
  <p class="hint">Loading layout…</p>
{:else}
  <div class="layout-view">
    <div class="canvas-wrap" bind:clientWidth={containerWidth}>
      <!-- The capture-phase blur is what gives the canvas the keyboard:
           startMove/startResize/startFurniture all preventDefault their
           pointerdown, which suppresses the browser's focus transfer, so a
           click on a rectangle would otherwise leave focus on whatever was
           last touched — typically a panel input, which then swallows the
           arrow keys the nudge needs. Capture, so a child's stopPropagation
           can't skip it. -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="canvas"
        bind:this={canvasEl}
        style="width: {toCanvas(layout.reference_w, scale)}px; height: {canvasHeight}px;"
        onpointerdowncapture={() => (document.activeElement as HTMLElement | null)?.blur()}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}>
        {#if guides.x !== null}
          <div class="guide vertical" style="left: {toCanvas(guides.x, scale)}px;"></div>
        {/if}
        {#if guides.y !== null}
          <div class="guide horizontal" style="top: {toCanvas(guides.y, scale)}px;"></div>
        {/if}
        {#each furniture as f (f.kind)}
          {@const r = fRectOf(f)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="furniture"
            class:draggable={f.drag !== "none" && !readOnly}
            class:selected={selectedFurniture === f.kind}
            style="left: {toCanvas(r.x, scale)}px; top: {toCanvas(r.y, scale)}px;
                   width: {toCanvas(f.w, scale)}px; height: {toCanvas(f.h, scale)}px;"
            onpointerdown={(e) => startFurniture(f, e)}>
            <span class="furniture-label">{f.label}</span>
          </div>
        {/each}
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
                  <span class="tab" class:active={tab.id === selectedId} title={tab.id}
                    onpointerdown={(e) => { e.stopPropagation(); selectWindow(tab.id); }}>{nameOf(tab).label}</span>
                {/each}
              </div>
            {:else}
              <span class="win-label" title={unit.anchor.id}>{nameOf(unit.anchor).label}</span>
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
      <p class="ref">
        reference {layout.reference_w}×{layout.reference_h}
        {#if filterIsActive(filter)}
          <span class="showing">
            · showing {shownCount} of {totalCount} windows
            <button class="linkish" onclick={() => (filter = { ...NO_FILTER })}>reset</button>
          </span>
        {/if}
      </p>
    </div>
    <div class="side">
      {#if hud}
        <HudPanel
          {hud}
          {readOnly}
          onSet={setHud}
          {sharedNames}
          selectedKind={selectedFurniture}
          onSelectKind={selectFurniture} />
      {/if}
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
        {onCreateStack}
        bind:filter
        bind:focusFilter />
    </div>
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
  .side {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: auto;
  }
  .canvas {
    position: relative;
    background: #1b1f27;
    background-image: linear-gradient(#2a2f3a 1px, transparent 1px),
      linear-gradient(90deg, #2a2f3a 1px, transparent 1px);
    background-size: 40px 40px;
    border: 1px solid #444;
  }
  .furniture {
    position: absolute;
    box-sizing: border-box;
    background: rgba(148, 163, 184, 0.12);
    border: 1px dashed #64748b;
    color: #94a3b8;
    font-size: 11px;
    overflow: hidden;
    /* Clickable so it can be selected, but furniture is drawn BEFORE the window
       rects, so an overlapping window is the later sibling and still wins the
       click — it can't swallow a window drag. */
    pointer-events: auto;
    cursor: pointer;
    touch-action: none;
  }
  .furniture.draggable {
    cursor: move;
  }
  /* The same amber as .win.selected, so a selection reads identically whether
     it's a window or furniture; the dashed border still says "not a window". */
  .furniture.selected {
    border-color: #f59e0b;
    background: rgba(245, 158, 11, 0.25);
    color: #fde68a;
    z-index: 1;
  }
  .furniture-label {
    padding: 1px 3px;
    pointer-events: none;
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
  /* A stack rectangle gets a heavier border so it reads as a group of windows,
     not a single one — color still follows .win/.win.selected above. */
  .win.stacked {
    border-width: 2px;
  }
  /* Snap feedback: the edge the dragged rect locked onto. Same amber as a
     selection, above every rect, never in the way of the pointer. */
  .guide {
    position: absolute;
    background: #f59e0b;
    pointer-events: none;
    z-index: 2;
  }
  .guide.vertical {
    top: 0;
    bottom: 0;
    width: 1px;
  }
  .guide.horizontal {
    left: 0;
    right: 0;
    height: 1px;
  }
  .win-label {
    padding: 1px 3px;
    display: block;
    box-sizing: border-box;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .showing {
    color: var(--warn);
  }
  .linkish {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font: inherit;
    padding: 0;
    text-decoration: underline;
  }
  .hint {
    color: #888;
    padding: 1rem;
  }
</style>
