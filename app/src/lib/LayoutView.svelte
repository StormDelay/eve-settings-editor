<script lang="ts">
  import { api, errMessage, errText } from "$lib/api";
  import type { WindowLayout, WindowRect, BoolFlag, Mutation, NewValue, NodePath, Slot, Hud, NeocomBar, OverviewColumns, ChatPanel } from "$lib/api";
  import {
    canvasScale, toCanvas, toData, resizeRect, stackUnits, hudRects,
    hudNum, hudFlag, targetAnchor, targetRect, targetCorner, furnitureWrites,
    DEFAULT_FILTER, filterIsActive, isOrphanFrame, visibleIds, drawnWindowCount,
    snapLines, movingEdges, snapDelta, unitAt, rectsAt, dropAction, linkInventory,
    tabTargetAt, isNudgeKey, nudgeStep, swallowsArrowKeys,
    type Corner, type DrawUnit, type FurnitureRect, type WindowFilter, type SnapLines, type DropAction, type Rect,
  } from "$lib/layout";
  import { displayName, displayNameOf, stackLabel } from "$lib/windowLabels";
  import ContextMenu, { type MenuItem } from "$lib/ContextMenu.svelte";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import { clutterOverrides, overrideCount, clearClutterOverrides, setClutterOverride, detailOn, setDetail, targetCount, setTargetCount, effectCount, setEffectCount } from "$lib/prefs.svelte";
  import WindowPanel from "$lib/WindowPanel.svelte";
  import HudPanel from "$lib/HudPanel.svelte";
  import DetailParts from "$lib/DetailParts.svelte";
  import { shipHudParts, fighterParts, neocomParts, targetParts, windowDetail } from "$lib/detail";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import { toast } from "./ui/toasts.svelte";
  import { undoAction } from "./undo.svelte";

  let {
    slot,
    runMutations,
    readOnly,
    accountReadOnly = false,
    refreshToken,
    onCollapseInspector,
    userOpen,
    selectedId = $bindable(null),
    onReveal,
    onDirty,
    sharedNames = [],
    focusSearch = $bindable(undefined),
  }: {
    slot: Slot;
    runMutations: (ms: Mutation[], rethrow?: boolean) => Promise<void>;
    readOnly: boolean;
    /** The account document's read-only flag. Only the four account-scoped HUD
     * rows write that file, so it is theirs alone to honour. */
    accountReadOnly?: boolean;
    refreshToken: number;
    /** A view that supplies its own inspector supplies its own hide control
        too — otherwise the column can be reopened from here but only closed
        from a view that has the shell's aside. */
    onCollapseInspector?: () => void;
    userOpen: boolean;
    selectedId?: string | null;
    onReveal: (path: NodePath) => void;
    onDirty: (slot: Slot) => void;
    /** Other characters on this account — named in HudPanel's account-row
     * legend. Unlike the Overview/Autofill views, only four of this view's
     * fields are account-wide, so the warning belongs on those rows, not
     * above the whole view. */
    sharedNames?: string[];
    /** Exposed so the shell's global Ctrl+F handler can focus the window filter
     * input when this view is active. Forwarded from WindowPanel, where the
     * input actually lives — see its own focusFilter doc.
     *
     * Renamed from `focusFilter`: it is now ONE bindable that whichever view is
     * active sets, so Ctrl+F stops being a suppressed no-op on the tabs that
     * have their own search box. */
    focusSearch?: () => void;
  } = $props();

  let layout = $state<WindowLayout | null>(null);
  let hud = $state<Hud | null>(null);
  let neocom = $state<NeocomBar | null>(null);
  let columns = $state<OverviewColumns | null>(null);
  let chats = $state<ChatPanel[]>([]);
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
  let filter = $state<WindowFilter>({ ...DEFAULT_FILTER });

  // ?./?? sidestep a TS limitation: narrowing `layout` doesn't carry across
  // separate reads inside a $derived expression (each read goes through its
  // reactive getter), so a `layout ? layout.x : ...` ternary won't type-check
  // here even though it's safe. canvasScale/toCanvas already treat a missing
  // reference dimension as "no-op", so reading through with `?? 0` is exact,
  // not an approximation.
  const scale = $derived(canvasScale(layout?.reference_w ?? 0, containerWidth));
  const visible = $derived(
    layout && filterIsActive(filter) ? visibleIds(layout.windows, filter, clutterOverrides()) : null,
  );
  const units = $derived(
    linkInventory(
      stackUnits(layout ?? { reference_w: 0, reference_h: 0, windows: [], stacks: [] }, visible),
      filter.env,
      // The raw window list, NOT the drawn units: the Inventory fan has to
      // reach a closed copy, and stackUnits only makes units from open ones.
      layout?.windows ?? [],
    ),
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
  // One live message per owning control (§3.1). Five slots because this view
  // drives five separate control groups, and "Edit failed" in a modal named none
  // of them — which is the whole complaint.
  type Msg = { text: string; detail: string };
  let loadError = $state<string | null>(null);
  let barError = $state<Msg | null>(null);
  let stackError = $state<Msg | null>(null);
  let hudError = $state<(Msg & { name: string }) | null>(null);
  let chatError = $state<Msg | null>(null);
  let neocomError = $state<Msg | null>(null);

  const totalCount = $derived(drawnWindowCount(allUnits));
  const canvasHeight = $derived(toCanvas(layout?.reference_h ?? 0, scale));
  // Every window this document has — NOT the filtered set. See overrideCount.
  const documentWindowIds = $derived(new Set((layout?.windows ?? []).map((w) => w.id)));

  async function load() {
    loadError = null;
    try {
      layout = await api.windowLayout(slot);
      if (selectedId && !layout.windows.some((w) => w.id === selectedId)) {
        selectedId = null;
      }
    } catch (e) {
      // Replaces the canvas rather than covering it. A dismissed modal used to
      // leave an empty canvas with no explanation, which is the worst of both:
      // the error was blocking AND unrecoverable.
      loadError = `The window layout couldn't be read — ${errText(e)}`;
    }
    // Furniture is a bonus view: an account file open on its own, or a document
    // with no HUD keys, must not take the canvas down with it.
    hud = await api.hud().catch(() => null);
    // Same tolerance as the HUD: an account file opened on its own, or a document
    // with no neocom key, must not take the canvas down with it.
    neocom = await api.neocomBar().catch(() => null);
    // Same tolerance as the HUD and the neocom: a character with no overview
    // container, or an account file opened on its own, must not take the canvas
    // down with it. Detail is a bonus layer.
    columns = await api.overviewColumns().catch(() => null);
    chats = await api.chatPanels().catch(() => []);
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
      dropTarget = null;
      draggingTab = null;
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
    barError = null;
    try {
      await runMutations(ms, true);
    } catch (e) {
      barError = { text: `That window wasn't moved — ${errText(e)}`, detail: errMessage(e) };
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

  // --- picking a rectangle out from under another --------------------------
  // A click can only ever reach the top rectangle, and a real file overlaps
  // heavily (one character: 381 windows, ~9 on screen), so anything underneath
  // is findable only if you already know its name. Right-click lists what is
  // actually at the point instead. Selecting is enough on its own: the pick
  // gets `.win.selected`'s z-index and paints above its neighbours, and the
  // panel scrolls its row into view.
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  /** A unit's name, the same way the canvas and the panel spell it, so the
   * three cannot disagree. */
  const unitLabel = (u: DrawUnit) =>
    u.stack ? (stackLabel(u.stack) ?? displayName(u.stack.container_id)) : displayNameOf(u.anchor);

  function onCanvasContextMenu(e: MouseEvent) {
    e.preventDefault();
    const p = pointerData(e as unknown as PointerEvent);
    // Windows first, then furniture: furniture always paints beneath the
    // windows, so listing it last matches what is actually stacked there.
    const items: MenuItem[] = [
      ...rectsAt(units, (u) => rectOf(u.anchor), p.x, p.y)
        .map((u) => ({ label: unitLabel(u), run: () => selectWindow(u.anchor.id) })),
      ...rectsAt(furniture, (f) => f, p.x, p.y)
        .map((f) => ({ label: f.label, run: () => selectFurniture(f.kind) })),
    ];
    // Empty canvas: no menu rather than an empty box.
    if (items.length === 0) return;
    // ContextMenu positions in client px, not the canvas's data px.
    menu = { x: e.clientX, y: e.clientY, items };
  }

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

  async function runStack(p: Promise<WindowLayout>): Promise<boolean> {
    stackError = null;
    try {
      layout = await p;
      onDirty("char"); // stack ops edit the character document in the backend
      if (selectedId && !layout.windows.some((w) => w.id === selectedId)) selectedId = null;
      return true;
    } catch (e) {
      stackError = { text: `The stack wasn't changed — ${errText(e)}`, detail: errMessage(e) };
      return false;
    }
  }
  const onUnstack = (id: string) => runStack(api.stackUnstack(id));
  const onReorder = (container: string, members: string[]) => runStack(api.stackReorder(container, members));
  const onAddToStack = (member: string, container: string) => runStack(api.stackAdd(member, container));
  const onCreateStack = (m1: string, m2: string) => runStack(api.stackCreate(m1, m2));

  // It no longer asks. The mutation is in-memory and Discard reverses it
  // exactly, and the WindowPanel band above the button already explains what an
  // empty frame is BEFORE the click — so the dialog was a second telling of
  // something already on screen, charging a modal for it.
  //
  // Safe to offer at all only because the client was verified not to re-create
  // these (2026-07-28) — see docs/format-notes.md.
  async function onDeleteOrphans() {
    const n = layout?.windows.filter(isOrphanFrame).length ?? 0;
    if (n === 0) return;
    if (await runStack(api.stackDeleteOrphans())) {
      toast(`Deleted ${n} empty stack frame${n === 1 ? "" : "s"}. Save to write it to disk.`, {
        action: undoAction(),
      });
    }
  }

  /** Write one HUD field and refresh the projection. */
  async function setHud(name: string, text: string) {
    hudError = null;
    try {
      hud = await api.setHudValue(name, text);
      const e = hud.entries.find((x) => x.name === name);
      onDirty(e?.scope === "account" ? "user" : "char");
    } catch (e) {
      hudError = { name, text: `That value wasn't changed — ${errText(e)}`, detail: errMessage(e) };
    }
  }

  /** Write one or more channels' splits and take the refreshed projection. The
   * splits live in the account document, so that is the slot that goes dirty. */
  async function setChatSplits(ids: string[], userlistWidth: number | null, inputHeight: number | null) {
    chatError = null;
    try {
      chats = await api.setChatSplits(ids, userlistWidth, inputHeight);
      onDirty("user");
    } catch (e) {
      chatError = { text: `That chat panel wasn't changed — ${errText(e)}`, detail: errMessage(e) };
      // A refused value (a typed negative, the reachable case) must not stay
      // on screen as if it had been stored — re-read what is actually there.
      chats = await api.chatPanels().catch(() => chats);
    }
  }

  /** Run a neocom command and take its refreshed projection. The bar lives in
   * the character document, so the char slot is what goes dirty.
   *
   * Commands key by index, not id (see neocom.rs), so two fast clicks on the
   * same row before the first re-projection lands would send the same index
   * twice — the second hitting whatever slid into that slot. `neocomBusy`
   * disables the whole panel for the round trip to rule that out. */
  let neocomBusy = $state(false);
  async function runNeocom(p: Promise<NeocomBar>) {
    neocomBusy = true;
    neocomError = null;
    try {
      neocom = await p;
      onDirty("char");
    } catch (e) {
      neocomError = { text: `The neocom wasn't changed — ${errText(e)}`, detail: errMessage(e) };
    } finally {
      neocomBusy = false;
    }
  }

  // --- Canvas drag & resize ------------------------------------------------

  type Drag =
    | { kind: "move"; unit: DrawUnit; startX: number; startY: number; ox: number; oy: number; lines: SnapLines }
    | { kind: "resize"; unit: DrawUnit; corner: Corner; startX: number; startY: number; ox: number; oy: number; ow: number; oh: number; lines: SnapLines }
    | { kind: "furniture"; f: FurnitureRect; startX: number; startY: number; ox: number; oy: number;
        /** The target list only: its ANCHOR at drag start. The rect is placed
         * relative to that anchor and flips to its other side at the middle of
         * the screen, so a preview that moved the rect directly could not
         * follow the flip. Absent for every other piece of furniture. */
        ax?: number; ay?: number }
    | { kind: "tab"; unit: DrawUnit; tabId: string; startX: number; startY: number; gx: number; gy: number };
  let drag: Drag | null = null;
  // Where the target list's anchor is RIGHT NOW during a drag of it, and null
  // when no such drag is in flight. $state because the anchor marker renders
  // from it — a marker left on the committed corner while the preview flipped
  // would point at the wrong one. The drop reads it too: it needs the anchor
  // the drag ended at, which a rect cannot be inverted back to unambiguously
  // once it has crossed the middle.
  let targetDragAnchor = $state<{ x: number; y: number } | null>(null);

  // The lines the current drag has locked onto, in data px; null when this axis
  // isn't snapped. Drawn as guides, cleared on drop.
  let guides = $state<{ x: number | null; y: number | null }>({ x: null, y: null });

  // The DrawUnit.key of the unit a Shift-drag (or a tab drag) is hovering as a
  // stack target; null when the drop would not stack anything. Drives the
  // highlight only — the drop re-resolves the target from the up event.
  let dropTarget = $state<string | null>(null);

  // The tab id of a tab drag that has passed the travel threshold; null while a
  // press is still just a click. Without the threshold, selecting a tab with a
  // twitchy mouse would unstack it. $state because the template reads it (the
  // `drag` variable itself is deliberately not reactive and must not be read
  // from markup).
  let draggingTab = $state<string | null>(null);

  // The window id a key-repeat nudge is currently in flight for (Task 3), so a
  // commit landing mid-nudge doesn't clear the preview under it.
  let nudging: string | null = null;

  const furniture = $derived(hud && layout ? hudRects(hud, layout, targetCount()) : []);
  const fRectOf = (f: FurnitureRect) => fPreview[f.kind] ?? { x: f.x, y: f.y };

  /** The internals of a furniture element. The ship HUD, the fighter panel and
   * the target list's slots are constant (measured, not stored); the neocom is
   * drawn from its real button list. The badge has neither and stays a plain
   * box. */
  const furnitureDetail = (f: FurnitureRect) =>
    f.kind === "shipui" && hud ? shipHudParts(effectCount(), hudFlag(hud, "ship_top"))
    : f.kind === "fighter" ? fighterParts()
    : f.kind === "neocom" && neocom ? neocomParts(neocom, f.w, f.h)
    : f.kind === "target" && hud ? targetParts(targetCount(), hudFlag(hud, "target_horizontal"))
    : [];

  /** Which corner of the target list's box its anchor is — what the marker is
   * drawn on. Follows the drag while one is in flight, so the marker moves to
   * the other corner exactly when the box flips sides. */
  const targetMarkerCorner = $derived.by(() => {
    if (!layout) return null;
    const a = targetDragAnchor ?? committedTargetAnchor();
    return a ? targetCorner(a.x, a.y, layout.reference_w, layout.reference_h) : null;
  });

  /** The target list's committed anchor, from the stored fractions. Null when
   * the file has no anchor to move (the canvas draws nothing then either). */
  function committedTargetAnchor(): { x: number; y: number } | null {
    if (!hud || !layout) return null;
    const fx = hudNum(hud, "target_x");
    const fy = hudNum(hud, "target_y");
    if (fx === null || fy === null) return null;
    return targetAnchor(fx, fy, layout.reference_w, layout.reference_h);
  }

  /** Pointer position in data px, relative to the canvas origin. */
  function pointerData(e: PointerEvent) {
    const box = canvasEl!.getBoundingClientRect();
    return { x: toData(e.clientX - box.left, scale), y: toData(e.clientY - box.top, scale) };
  }

  /** The index of the tab element under the pointer, or null. Read off the
   * elements' own data attribute rather than computed: tab widths come from
   * their text, so there is nothing in the data to compute from.
   * elementsFromPoint still sees them while the canvas holds pointer capture. */
  function tabIndexAt(clientX: number, clientY: number): number | null {
    for (const el of document.elementsFromPoint(clientX, clientY)) {
      const i = (el as HTMLElement).dataset?.tabIndex;
      if (i !== undefined) return Number(i);
    }
    return null;
  }

  /** The unit under the pointer, excluding the one being dragged. */
  function targetAt(e: PointerEvent, dragged: DrawUnit): DrawUnit | null {
    const p = pointerData(e);
    const u = unitAt(units, (x) => rectOf(x.anchor), p.x, p.y);
    return u && u.key !== dragged.key ? u : null;
  }

  /** The unit under the pointer for a tab drag — see layout.ts's `tabTargetAt`
   * for why the dragged unit gets to win. */
  const tabTargetOf = (p: { x: number; y: number }, unit: DrawUnit): DrawUnit | null =>
    tabTargetAt(units, (x) => rectOf(x.anchor), p.x, p.y, unit);

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

  /** Furniture whose handle is its anchor MARKER rather than its whole box —
   * today, only the target list. In game the anchor is the only thing you can
   * grab; grabbing the box instead leaves the cursor at an arbitrary offset
   * from the anchor, and that offset changes SIGN the moment the box flips to
   * the other side of the anchor at the middle of the screen. The box stays
   * selectable, it just doesn't start a drag. */
  const dragsByMarker = (f: FurnitureRect) => f.kind === "target";

  // Selecting furniture and dragging it are separate: the neocom can't be
  // dragged (its width is a field, not a rect) but must still be selectable, or
  // clicking it looks broken. Selection is exclusive with the window selection,
  // so exactly one thing on the canvas ever reads as selected.
  //
  // `onMarker` is the anchor-dot press; the dot stops propagation, so the box's
  // own handler never doubles it.
  function startFurniture(f: FurnitureRect, e: PointerEvent, onMarker = false) {
    selectFurniture(f.kind);
    e.stopPropagation();
    if (readOnly || f.drag === "none") return;
    if (dragsByMarker(f) && !onMarker) return;
    const r = fRectOf(f);
    // The anchor comes from the stored value, not from the rect: inverting a
    // rect back to an anchor is ambiguous in the band around the middle where
    // both sides would place it there.
    const a = f.kind === "target" && hud && layout ? committedTargetAnchor() : null;
    targetDragAnchor = a;
    drag = { kind: "furniture", f, startX: e.clientX, startY: e.clientY, ox: r.x, oy: r.y, ax: a?.x, ay: a?.y };
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

  /** A tab press selects (as it always has) and arms a drag. Whether that drag
   * reorders, moves the window to another stack, or pulls it out is decided
   * entirely by where it is released — see dropAction. */
  function startTab(unit: DrawUnit, tabId: string, e: PointerEvent) {
    e.stopPropagation(); // or the stack's own move drag starts underneath
    selectWindow(tabId);
    if (readOnly) return;
    const r = rectOf(unit.anchor);
    const p = pointerData(e);
    drag = { kind: "tab", unit, tabId, startX: e.clientX, startY: e.clientY, gx: p.x - r.x, gy: p.y - r.y };
    draggingTab = null;
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
      // The target list is placed from its anchor, so the preview moves the
      // ANCHOR and re-places the box — which is what makes it flip sides the
      // moment the anchor crosses the middle, instead of jumping on drop.
      if (drag.ax !== undefined && drag.ay !== undefined && layout) {
        targetDragAnchor = { x: drag.ax + dx, y: drag.ay + dy };
        fPreview = {
          ...fPreview,
          [f.kind]: targetRect(
            targetDragAnchor.x, targetDragAnchor.y, f.w, f.h,
            layout.reference_w, layout.reference_h,
          ),
        };
        return;
      }
      fPreview = {
        ...fPreview,
        [f.kind]: { x: drag.ox + dx, y: f.drag === "xy" ? drag.oy + dy : drag.oy },
      };
      return;
    }
    if (drag.kind === "tab") {
      // 4 canvas px of travel turns the press into a drag. Compared in client
      // px because it is a hand-tremor threshold, not a data-space distance.
      if (Math.abs(e.clientX - drag.startX) > 4 || Math.abs(e.clientY - drag.startY) > 4) {
        draggingTab = drag.tabId;
      }
      if (draggingTab === null) return;
      const p = pointerData(e);
      const over = tabTargetOf(p, drag.unit);
      const own = over?.key === drag.unit.key;
      // Highlight only a drop that goes somewhere else; hovering the tab's own
      // stack is a reorder, which the strip itself shows.
      dropTarget = own ? null : (over?.key ?? null);
      return;
    }
    // Shift over another unit marks it as a stack target. Read off the event
    // like Alt is, so pressing or releasing Shift mid-drag takes effect on the
    // next pointer move. A stack can't be merged into another (spec §2), so a
    // stack drag never highlights anything.
    dropTarget = e.shiftKey && drag.kind === "move" && !drag.unit.stack
      ? (targetAt(e, drag.unit)?.key ?? null)
      : null;
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

  async function onPointerUp(e: PointerEvent) {
    if (!drag) return;
    const d = drag;
    drag = null;
    guides = { x: null, y: null };

    if (d.kind === "furniture") {
      const p = fPreview[d.f.kind];
      if (!p) return;
      // `d.f` is the rect captured at pointerdown — undragged, so still the
      // committed one, which is what lets furnitureWrites tell a real move from
      // a drag that rounded back to where it started. It returns nothing for
      // the latter, and nothing is what a no-op drag must write.
      if (layout && hud) {
        for (const [name, value] of furnitureWrites(d.f.kind, d.f, p, targetDragAnchor, hud, layout)) {
          await setHud(name, value);
        }
      }
      // Only while a drag is in flight: the marker falls back to the stored
      // anchor, which the write above has just refreshed.
      if (!drag) targetDragAnchor = null;
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

    if (d.kind === "tab") {
      const wasDrag = draggingTab !== null;
      draggingTab = null;
      dropTarget = null;
      if (!wasDrag) return; // a press that never travelled is just a select
      const p = pointerData(e);
      const r = rectOf(d.unit.anchor);
      const target = tabTargetOf(p, d.unit);
      // Only measured when the drop resolves to the tab's own strip (the
      // reorder case) — see tabTargetAt for why that's the pointer-inside-own-
      // rect case, matching onPointerMove's `own` derivation.
      const own = target?.key === d.unit.key;
      const index = own ? tabIndexAt(e.clientX, e.clientY) : null;
      await applyDrop(
        dropAction(
          { unit: d.unit, tabId: d.tabId, rect: { x: p.x - d.gx, y: p.y - d.gy, w: r.w, h: r.h } },
          target,
          e.shiftKey,
          index,
        ),
        d.unit,
      );
      return;
    }

    // Gated on `preview` having an entry for this unit, not just Shift+move:
    // selection alone (no drag) can raise an already-selected window's
    // z-index above a neighbour it overlaps, and targetAt excludes the
    // dragged unit — so a Shift-click with zero travel would otherwise
    // resolve to that neighbour and silently stack onto it. preview is only
    // ever set from onPointerMove, so its presence means the pointer actually
    // moved. Also skips a whole-stack drag (dropAction ignores the target for
    // one anyway, per spec §2).
    const target = d.kind === "move" && e.shiftKey && !d.unit.stack && preview[d.unit.anchor.id]
      ? targetAt(e, d.unit) : null;
    dropTarget = null;
    await applyDrop(
      dropAction({ unit: d.unit, tabId: null, rect: rectOf(d.unit.anchor) }, target, e.shiftKey, null),
      d.unit,
    );
  }

  /** Commit a unit's previewed rect: fan it out to every renderable window in
   * the unit so a stack moves/resizes coherently and stale members are
   * repaired, then drop the preview unless something has re-claimed it. The
   * full rect (not just x/y) is sent even for a move, so members also snap to
   * the anchor's w/h — geomMutations diffs per field, so an unchanged w/h emits
   * nothing and plain single-window units are unaffected. */
  async function commitUnit(captured: DrawUnit) {
    // Re-resolve the unit from the live list, falling back to the captured one.
    // A drag captures its unit at pointerdown; a commit landing MID-drag — which
    // a nudge keyup can now cause — would otherwise fan out against pre-reload
    // `geom`, and `geomMutations`' per-field diff would skip a write that is
    // actually needed. Re-resolving alone was not enough on its own: it silently
    // skips the commit when the unit has since been filtered out of the list,
    // which is the worse failure of the two. Hence both.
    const unit = units.find((u) => u.anchor.id === captured.anchor.id) ?? captured;
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

  /** Carry out a decided drop. A stacking drop deliberately does NOT commit the
   * drag's geometry: the joining window adopts the stack's rect in the backend
   * (stacks.rs), so writing the drag coordinates first would be a write the
   * next projection immediately overwrites. */
  async function applyDrop(a: DropAction, unit: DrawUnit) {
    switch (a.op) {
      case "move":
        await commitUnit(unit);
        return;
      case "none":
        clearPreview(unit.anchor.id);
        return;
      case "create":
        clearPreview(unit.anchor.id);
        await runStack(api.stackCreate(a.first, a.second));
        return;
      case "add":
        clearPreview(unit.anchor.id);
        await runStack(api.stackAdd(a.member, a.container));
        return;
      case "reorder":
        await runStack(api.stackReorder(a.container, a.order));
        return;
      case "unstack":
        await unstackTo(a.member, a.rect);
        return;
      case "unstackInto":
        if (await runStack(api.stackUnstack(a.member))) await runStack(api.stackAdd(a.member, a.container));
        return;
      case "unstackCreate":
        if (await runStack(api.stackUnstack(a.member))) await runStack(api.stackCreate(a.target, a.member));
        return;
      default: {
        // Exhaustiveness guard: a DropAction variant added without a case here
        // fails the build instead of silently no-opping the drop.
        const _exhaustive: never = a;
        return _exhaustive;
      }
    }
  }

  /** Free a window from its stack and put it where it was dropped. The geometry
   * paths MUST come from the layout the unstack returned: the projection
   * captured before it describes a document that no longer exists in that
   * shape. Without the placement half the freed window would take the stack's
   * exact rect and sit invisibly behind it. */
  async function unstackTo(member: string, rect: Rect) {
    if (!(await runStack(api.stackUnstack(member)))) return;
    const w = layout?.windows.find((x) => x.id === member);
    if (!w?.geom) return;
    await commit(geomMutations(w, rect));
  }

  // --- Arrow-key nudge -------------------------------------------------------
  // Bound on the window rather than a focusable canvas: the selection can just
  // as well have been made in the window panel, and a focus-scoped handler
  // would silently do nothing in that case.

  /** The unit the nudge moves: the one whose anchor or tabs carry the
   * selection, so nudging a stacked window moves its whole stack — the same
   * unit a canvas drag would have grabbed. */
  const selectedUnit = () =>
    units.find((u) => u.anchor.id === selectedId || u.tabs.some((t) => t.id === selectedId)) ?? null;

  function onKeyDown(e: KeyboardEvent) {
    if (readOnly || drag) return;
    const step = nudgeStep(e.key, {
      shift: e.shiftKey, ctrl: e.ctrlKey, meta: e.metaKey, alt: e.altKey,
    });
    if (!step) return;
    // Never steal the arrows from a field that uses them — see swallowsArrowKeys.
    if (swallowsArrowKeys(e.target as HTMLElement | null)) return;
    // A held nudge owns its unit until keyup: re-resolving from the live
    // selection on every auto-repeat keydown would let a mid-hold selection
    // change (e.g. a click in the window panel) retarget onto a different
    // unit, stranding the first unit's accumulated preview uncommitted.
    const unit = nudging ? (units.find((u) => u.anchor.id === nudging) ?? null) : selectedUnit();
    if (!unit) return;
    e.preventDefault(); // or the canvas pane scrolls out from under the nudge
    const r = rectOf(unit.anchor);
    // Preview only — no backend traffic per keypress. Key auto-repeat fires
    // dozens of keydowns and exactly ONE keyup, so a glide costs one commit.
    nudging = unit.anchor.id;
    preview = { ...preview, [unit.anchor.id]: { ...r, x: r.x + step.dx, y: r.y + step.dy } };
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
    if (isNudgeKey(e.key)) return endNudge();
  };
</script>

<!-- blur doesn't bubble, so this only fires for the window itself (an inner
     input losing focus to another inner element won't trip it) — see endNudge. -->
<svelte:window onkeydown={onKeyDown} onkeyup={onKeyUp} onblur={endNudge} />

{#if loadError !== null}
  <div class="work">
    <EmptyState variant="error" title="This layout can't be shown" description={loadError} />
  </div>
{:else if layout === null}
  <div class="work"><EmptyState title="Loading layout…" /></div>
{:else}
  <div class="layout-view">
    <div class="canvas-wrap work" bind:clientWidth={containerWidth}>
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
        onpointerup={onPointerUp}
        oncontextmenu={onCanvasContextMenu}>
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
            class:draggable={f.drag !== "none" && !readOnly && !dragsByMarker(f)}
            class:selected={selectedFurniture === f.kind}
            class:spills={f.kind === "shipui"}
            style="left: {toCanvas(r.x, scale)}px; top: {toCanvas(r.y, scale)}px;
                   width: {toCanvas(f.w, scale)}px; height: {toCanvas(f.h, scale)}px;"
            onpointerdown={(e) => startFurniture(f, e)}>
            {#if detailOn()}
              <DetailParts parts={furnitureDetail(f)} {scale} />
            {/if}
            <!-- The anchor is what the file stores and what a drag writes; the
                 box is just what the list covers from there. In game the anchor
                 is also the only thing you can grab, so it is the handle here
                 too: the marker drags, the rest of the box only selects. -->
            {#if f.kind === "target" && targetMarkerCorner}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span class="anchor-dot {targetMarkerCorner}"
                title="The stored anchor — drag the list by this. The list grows from here toward the middle of the screen."
                onpointerdown={(e) => startFurniture(f, e, true)}></span>
            {/if}
            <span class="furniture-label">{f.label}</span>
          </div>
        {/each}
        {#each units as unit (unit.key)}
          {@const r = rectOf(unit.anchor)}
          <!-- A folded unit (today, only linkInventory's docked Inventory pair)
               draws one rectangle for more than one window id, and a drag or
               nudge writes the full rect to all of them — resize included, not
               just position. Nothing else on the rectangle says that, so name
               the fanned ids in a tooltip. Stacks already show this via their
               own tab strip, hence `!unit.stack`. -->
          {@const foldTitle = !unit.stack && unit.fanTargets.length > 1
            ? `Moves and resizes together with: ${unit.fanTargets.map((w) => w.id).join(", ")}`
            : undefined}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="win"
            class:selected={unit.tabs.some((t) => t.id === selectedId) || unit.anchor.id === selectedId}
            class:stacked={!!unit.stack}
            class:droptarget={dropTarget === unit.key}
            style="left: {toCanvas(r.x, scale)}px; top: {toCanvas(r.y, scale)}px;
                   width: {toCanvas(r.w, scale)}px; height: {toCanvas(r.h, scale)}px;"
            title={foldTitle}
            onpointerdown={(e) => startMove(unit, e)}>
            {#if detailOn()}
              <DetailParts parts={windowDetail(unit, selectedId, columns, chats, r)} {scale} />
            {/if}
            {#if unit.stack}
              <div class="tabs">
                {#each unit.tabs as tab, i (tab.id)}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <span class="tab" class:active={tab.id === selectedId}
                    class:dragging={draggingTab === tab.id}
                    data-tab-index={i} title={tab.id}
                    onpointerdown={(e) => startTab(unit, tab.id, e)}>{displayNameOf(tab)}</span>
                {/each}
              </div>
            {:else}
              <span class="win-label" title={unit.anchor.id}>{displayNameOf(unit.anchor)}</span>
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
      <!-- A status BAR, not a sentence: one line used to carry a fact, a view
           setting, an instruction and two counters with links, in five tones.
           Left is what is true and what you are looking at, and neither half
           ever appears or disappears. Right is what is currently NARROWING the
           view — chips, because both are exceptional states with an escape and
           both vanish when there is nothing to say. The drag hint was
           instruction rather than status and moved to the inspector. -->
      <!-- The canvas's own failures land here, under the canvas they are about
           and above nothing that could be pushed out from under the cursor. -->
      {#if barError}
        <InlineMessage variant="error" detail={barError.detail}>{barError.text}</InlineMessage>
      {/if}
      <div class="statusbar">
        <span class="facts">
          <span class="ref">reference {layout.reference_w}×{layout.reference_h}</span>
          <Field
            kind="checkbox"
            class="det"
            label="Detail"
            value={detailOn()}
            onchange={(e) => setDetail((e.currentTarget as HTMLInputElement).checked)} />
        </span>
        <span class="narrowing">
          {#if filterIsActive(filter)}
            <Chip tone="warn" size="sm">
              {shownCount} of {totalCount} windows
              <!-- Back to the DEFAULT, not to nothing: dismissing undoes what
                   the user narrowed, and hiding clutter is the view they
                   started from rather than something they chose. Showing every
                   window is one click away on the toggle itself. -->
              {#snippet actions()}
                <Button variant="ghost" size="sm" iconOnly title="Show every window again"
                  onclick={() => (filter = { ...DEFAULT_FILTER })}>✕</Button>
              {/snippet}
            </Chip>
          {/if}
          {#if overrideCount(documentWindowIds) > 0}
            <Chip tone="warn" size="sm">
              {overrideCount(documentWindowIds)} overridden
              {#snippet actions()}
                <Button variant="ghost" size="sm" iconOnly title="Clear the clutter overrides"
                  onclick={() => clearClutterOverrides(documentWindowIds)}>✕</Button>
              {/snippet}
            </Chip>
          {/if}
        </span>
      </div>
    </div>
    <aside class="inspector">
      <div class="inspector-head">
        <Button variant="ghost" size="sm" iconOnly title="Hide properties"
          onclick={() => onCollapseInspector?.()}>&raquo;</Button>
      </div>
      <!-- "What can I do here", where the pane has to say something anyway.
           The canvas's gestures were explained in a status line, in an
           instruction wedged between two facts; none of it is true on a
           read-only file, which is the gate the old line already carried. -->
      {#if selectedId === null && !readOnly}
        <EmptyState
          title="Nothing selected"
          description="Click a window on the canvas to edit it. Shift-drag onto another window to stack · drag a tab to reorder or pull out." />
      {/if}
      {#if hud}
        <HudPanel
          {hud}
          {readOnly}
          {accountReadOnly}
          onSet={setHud}
          {sharedNames}
          selectedKind={selectedFurniture}
          onSelectKind={selectFurniture}
          targets={targetCount()}
          onTargets={setTargetCount}
          effects={effectCount()}
          onEffects={setEffectCount}
          referenceW={layout.reference_w}
          referenceH={layout.reference_h}
          {neocom}
          {neocomBusy}
          onNeocomReorder={(order) => runNeocom(api.neocomReorder(order))}
          onNeocomRemove={(i) => runNeocom(api.neocomRemove(i))}
          onNeocomAdd={(id, t, icon) => runNeocom(api.neocomAdd(id, t, icon))}
          onNeocomReset={() => runNeocom(api.neocomReset())}
          {hudError}
          {neocomError} />
      {/if}
      <WindowPanel
        windows={layout.windows}
        stacks={layout.stacks}
        {stackError}
        {chatError}
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
        {onDeleteOrphans}
        overrides={clutterOverrides()}
        onClutterOverride={setClutterOverride}
        {chats}
        {accountReadOnly}
        {userOpen}
        {sharedNames}
        onSetChatSplits={setChatSplits}
        bind:filter
        bind:focusFilter={focusSearch} />
    </aside>
  </div>
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  /* This view no longer runs its own two-column grid. It was the ONE view with
     a right-hand region, which is why the right edge of the app meant "backups"
     on five tabs and "window properties" on one. The inspector is a column of
     the SHELL now, and `display: contents` is how a view reaches it without a
     portal: this root stops participating in layout, and its two children become
     grid items of `.shell` in columns 2 and 3.

     The old `height: 100%; overflow: hidden` here is exactly what
     `display: contents` makes redundant — `.canvas-wrap` and `.inspector`
     already owned their own scrolling. */
  .layout-view {
    display: contents;
  }
  .canvas-wrap {
    overflow: auto;
    padding: var(--s2);
  }
  .canvas {
    position: relative;
    /* --surface, so the canvas stays a step lighter than the app ground the way
       it always was — the relationship survives, the second palette does not. */
    background: var(--surface);
    background-image: linear-gradient(var(--border) 1px, transparent 1px),
      linear-gradient(90deg, var(--border) 1px, transparent 1px);
    background-size: 40px 40px;
    border: 1px solid var(--border-strong);
  }
  .furniture {
    position: absolute;
    box-sizing: border-box;
    background: var(--muted-veil);
    border: 1px dashed var(--border-strong);
    /* --text-secondary, not --text-muted: composited over the furniture veil
       this is Lc 80 where #94a3b8 was Lc 47. It stays quieter than a window
       label, which is the distinction that was worth keeping. */
    color: var(--text-secondary);
    /* Canvas-scale type. */
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
  /* The ship HUD is the one element with a part drawn OUTSIDE its box: the
     effects row hangs below it (or above it, bottom-aligned), because EVE does
     not count that row as part of the element and neither does
     HUD_NOMINAL.shipui. Without this the detail layer's overflow clips it away
     entirely. The canvas still clips at the screen edge, which is right — a row
     that would fall off-screen should look like it does. */
  .furniture.spills {
    overflow: visible;
  }
  /* The same amber as .win.selected, so a selection reads identically whether
     it's a window or furniture; the dashed border still says "not a window". */
  .furniture.selected {
    border-color: var(--warn);
    background: var(--warn-veil);
    color: var(--text);
    z-index: 1;
  }
  .anchor-dot {
    position: absolute;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    /* Straddles the corner, so it reads as ON the point rather than inside the
       box — the point is what moves, and it is often outside the drawn list. */
    margin: -5px;
    background: var(--warn);
    border: 1px solid var(--bg);
    /* The handle, not decoration — see startFurniture. It is the ONE child of a
       furniture box that takes a pointer; the detail layer stays inert. */
    pointer-events: auto;
    cursor: move;
    touch-action: none;
  }
  /* The dot straddles the corner and `.furniture`'s overflow clips the outer
     half away, leaving ~6px of grab. This transparent skirt puts the hit area
     back up near the 12px resize handles without moving the mark; its own outer
     half is clipped by the same rule. */
  .anchor-dot::after {
    content: "";
    position: absolute;
    inset: -4px;
  }
  .anchor-dot.tl { top: 0; left: 0; }
  .anchor-dot.tr { top: 0; right: 0; }
  .anchor-dot.bl { bottom: 0; left: 0; }
  .anchor-dot.br { bottom: 0; right: 0; }
  .furniture-label {
    padding: 1px 3px;
    pointer-events: none;
    /* Above the detail layer, which is an absolutely-positioned sibling. */
    position: relative;
    z-index: 1;
  }
  .win {
    position: absolute;
    box-sizing: border-box;
    background: var(--accent-veil);
    border: 1px solid var(--accent);
    color: var(--text);
    /* Canvas-scale type. */
    font-size: 11px;
    overflow: hidden;
    cursor: move;
    touch-action: none;
  }
  .win.selected {
    border-color: var(--warn);
    background: var(--warn-veil);
    z-index: 1;
  }
  /* A stack rectangle gets a heavier border so it reads as a group of windows,
     not a single one — color still follows .win/.win.selected above. */
  .win.stacked {
    border-width: 2px;
  }
  /* The unit a Shift-drag would stack onto. Deliberately NOT the amber of a
     selection — this is a transient "drop here", not a state. */
  .win.droptarget {
    border-color: var(--ok);
    background: var(--ok-veil);
    box-shadow: 0 0 0 2px var(--ok);
    z-index: 1;
  }
  /* Snap feedback: the edge the dragged rect locked onto. Same amber as a
     selection, above every rect, never in the way of the pointer. */
  .guide {
    position: absolute;
    background: var(--warn);
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
    position: relative;
    z-index: 1;
  }
  .tabs {
    display: flex;
    gap: 1px;
    background: var(--bg);
    overflow: hidden;
    /* Above the detail layer, which is an absolutely-positioned sibling. */
    position: relative;
    z-index: 1;
  }
  .tab {
    padding: 1px var(--s1);
    background: var(--surface-raised);
    color: var(--text-secondary);
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* A light tone on its own dim ground, not dark text on a saturated fill:
     that pattern measured Lc 59.6 here, and this measures 69.2. */
  .tab.active {
    background: var(--warn-dim);
    color: var(--warn);
  }
  /* The tab being dragged. No floating ghost rect: the target highlight and
     this are enough to read the gesture, and a ghost would need its own
     hit-test exclusions. */
  .tab.dragging {
    opacity: 0.45;
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
  /* Not canvas-scale: this is a status bar BELOW the canvas, so it takes the
     type scale like the rest of the chrome. */
  .statusbar {
    display: flex;
    align-items: center;
    gap: var(--s3);
    flex-wrap: wrap;
    font-size: var(--t-caption);
    margin-top: var(--s1);
  }
  .facts {
    display: flex;
    align-items: center;
    gap: var(--s3);
  }
  .ref { color: var(--text-muted); }
  /* Right-aligned: the two halves say different kinds of thing, and the gap is
     what separates "this is true" from "this is hiding something from you". */
  .narrowing {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: var(--s2);
  }
  /* The dark-native-control rule is gone — Field owns it. */
  .statusbar :global(.det) {
    color: var(--text-secondary);
    cursor: pointer;
  }
</style>
