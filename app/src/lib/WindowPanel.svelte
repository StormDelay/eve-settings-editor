<script lang="ts">
  import type { WindowRect, BoolFlag, NodePath, Stack, ChatPanel, OverviewColumns, OverviewTab } from "$lib/api";
  import { overviewIndex } from "$lib/detail";
  import { plainTabName } from "$lib/tabName";
  import { describe, groupByFamily, displayName, displayNameOf, nameOf, stackLabel, isClutter, type ClutterOverrides } from "$lib/windowLabels";
  import { windowMatches, isOrphanFrame, isLeavableChat, NO_FILTER, type WindowFilter } from "$lib/layout";
  import ContextMenu, { type MenuItem } from "$lib/ContextMenu.svelte";
  import ChatSplit from "$lib/ChatSplit.svelte";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import MenuButton from "./ui/MenuButton.svelte";

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
    onDeleteOrphans,
    onLeaveChat,
    overrides,
    onClutterOverride,
    chats,
    accountReadOnly,
    userOpen,
    sharedNames,
    onSetChatSplits,
    columns = null,
    onSetOverviewWidth = () => {},
    stackError = null,
    chatError = null,
    widthError = null,
    filter = { ...NO_FILTER },
  }: {
    windows: WindowRect[];
    stacks: Stack[];
    /** Refused edits, owned by LayoutView and rendered at the control group each
     *  belongs to: the stack list, and the chat split fields. */
    stackError?: { text: string; detail: string } | null;
    chatError?: { text: string; detail: string } | null;
    widthError?: { text: string; detail: string } | null;
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
    onDeleteOrphans: () => void;
    onLeaveChat: (w: WindowRect) => void;
    /** The user's per-window clutter overrides — owned by prefs.svelte, passed
     * down so this stays a presentational component like every other prop
     * it takes. */
    overrides: ClutterOverrides;
    onClutterOverride: (id: string, mode: "clutter" | "visible" | "default") => void;
    /** Per-channel chat splits, from the ACCOUNT document. Empty both when no
     * account file is open AND when one is open but no channel has ever had a
     * split stored — `userOpen` is what actually distinguishes the two. */
    chats: ChatPanel[];
    /** The account document's read-only flag. The chat splits are the only
     * thing this panel writes to that file, so it is theirs alone to honour. */
    accountReadOnly: boolean;
    /** Whether an account file is open at all. `chats.length === 0` cannot
     * stand in for this: it is equally true for an open account that simply
     * has no chat split stored yet, and disabling on that would permanently
     * block the mint path the chat split fields exist to offer. */
    userOpen: boolean;
    /** Other characters on this account — named in the chat block's legend,
     * because these two fields are account-wide. */
    sharedNames: string[];
    onSetChatSplits: (ids: string[], userlistWidth: number | null, inputHeight: number | null) => void;
    /** The overview projection, for an overview window's column widths. Null
     * when no account file is open: the tabs live there, the widths here. */
    columns?: OverviewColumns | null;
    onSetOverviewWidth?: (tabIndex: number, column: string, width: number) => void;
    /** Owned by LayoutView, whose toolbar renders the controls; the list and
     * the canvas apply the same predicate. */
    filter?: WindowFilter;
  } = $props();

  /** An overview window's first tab, in the window's own order. */
  function firstTabOf(id: string): OverviewTab | undefined {
    const ov = overviewIndex(id);
    const first = columns?.windows.find((x) => x.index === ov)?.tab_indices[0];
    return ov === null ? undefined : columns?.tabs.find((t) => t.index === first);
  }

  // Per-row selection for the two "stack with…" pickers, cleared as soon as the
  // pick is acted on so each control returns to its prompt.
  let addPick: Record<string, string> = $state({});
  let withPick: Record<string, string> = $state({});

  // Counted from the same predicate the filter uses, so the offer can never
  // name a number the `Hide clutter` toggle disagrees with.
  const orphanCount = $derived(windows.filter(isOrphanFrame).length);

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
    // One item, never both, labelled for what the click will do. The built-in
    // tables can never be complete, so this is the per-window escape hatch.
    const overridden = overrides.clutter.has(w.id) || overrides.visible.has(w.id);
    if (overridden) {
      items.push({ label: "Use the default clutter rule", run: () => onClutterOverride(w.id, "default") });
    } else if (isClutter(w.id, overrides, w.name)) {
      items.push({ label: "Stop treating as clutter", run: () => onClutterOverride(w.id, "visible") });
    } else {
      items.push({ label: "Treat as clutter", run: () => onClutterOverride(w.id, "clutter") });
    }
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

  // Resync the field with the model on every commit — see HudPanel's copy of
  // this note. Svelte patches `value` only when the expression changes, so a
  // rejected edit (blank, "abc", or one the backend refuses) used to leave the
  // typed text on screen next to geometry that never moved.
  const numberEdit = (w: WindowRect, field: "x" | "y" | "w" | "h") => (e: Event) => {
    const el = e.target as HTMLInputElement;
    const v = parseInt(el.value, 10);
    if (!Number.isNaN(v)) onGeom(w, field, v);
    el.value = String(w.geom![field]);
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

  const freeWindows = $derived(windows.filter((w) => w.stack === null && windowMatches(w, filter, overrides)));
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

  // Whether a stack member currently passes the filter (and still exists).
  // Shared by matchingMembers (below) and the ↑/↓ reorder buttons, which must
  // disable rather than swap with a neighbour the filter is hiding.
  function memberVisible(id: string): boolean {
    const w = findWindow(id);
    return !!w && windowMatches(w, filter, overrides);
  }

  // Members currently matching the filter, for gating the stack's frame row
  // and its count badge (I2) — a stack whose members are all filtered out
  // must disappear from the list exactly as it disappears from the canvas.
  function matchingMembers(stack: Stack): string[] {
    return stack.members.filter(memberVisible);
  }
</script>

{#snippet rowHead(w: WindowRect)}
  {@const n = nameOf(w)}
  {@const openFlag = w.flags.find((f) => f.name === "openWindows")}
  <Field
    kind="checkbox"
    value={w.open}
    disabled={readOnly || openFlag?.set.how === "unavailable"}
    disabledReason="Not present in this file"
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
    <Chip tone="warn" size="sm" title="Geometry is not a 6-tuple — edit in the raw tree">
      unrenderable
    </Chip>
  {:else if !w.resolution_matches}
    <Chip tone="warn" size="sm" title="Saved at a different resolution than the canvas">
      {w.geom?.screen_w}×{w.geom?.screen_h}
    </Chip>
  {/if}
  <!-- The same menu the right-click opens, and the right-click keeps working:
       this ADDS a route, it does not replace one. Four commands — including the
       per-window clutter escape hatch — were reachable only by right-clicking a
       row that advertised nothing. It lives inside `rowHead` rather than at the
       end of each of the three `.row-head` containers so that it always sits in
       the same place relative to the name it acts on. -->
  <MenuButton items={() => rowMenu(w)} title="Window actions" />
{/snippet}

{#snippet detail(w: WindowRect)}
  {@const g = w.geom!}
  <div class="detail">
    <div class="coords">
      {#each COORDS as field}
        <label title="Right-click for actions" oncontextmenu={(e) => openMenu(e, [showInTree(geomPath(w, field)), copyId(w.id)])}>
          {field}
          <Field
            kind="number"
            value={g[field]}
            disabled={readOnly}
            disabledReason="This file is read-only"
            onchange={numberEdit(w, field)} />
        </label>
      {/each}
    </div>
    <div class="flags">
      {#each detailFlags(w) as f (f.name)}
        <label
          class="flag"
          title={f.set.how === "unavailable" ? "Not present in this file" : "Right-click for actions"}
          oncontextmenu={(e) => openMenu(e, flagMenu(w, f))}>
          <Field
            kind="checkbox"
            value={f.value}
            disabled={readOnly || f.set.how === "unavailable"}
            disabledReason={f.set.how === "unavailable" ? "Not present in this file" : "This file is read-only"}
            onchange={(e) => onFlag(w, f, (e.target as HTMLInputElement).checked)} />
          {f.name}
        </label>
      {/each}
    </div>
    {#if w.id.startsWith("chatchannel_")}
      {#if isLeavableChat(w.id)}
        <Button
          size="sm"
          type="button"
          disabled={readOnly}
          disabledReason="This file is read-only"
          title="Remove this channel from the character, so EVE does not bring it back"
          onclick={() => onLeaveChat(w)}>
          {w.id.startsWith("chatchannel_private_") ? "Leave conversation" : "Leave channel"}
        </Button>
      {/if}
      {@const chatStack = w.stack ? (stacks.find((s) => s.container_id === w.stack!.container_id) ?? null) : null}
      <!-- A stacked chat window is DISPLAYED at its stack anchor's size (the
           canvas draws every split against `rectOf(unit.anchor)` — see
           LayoutView), not its own stored geometry — those two can differ for
           a stacked member. Falls back to the window's own geom when it is
           not stacked, or when the anchor can't be found. -->
      {@const chatGeom = (chatStack ? findWindow(chatStack.anchor_id)?.geom : null) ?? w.geom}
      <ChatSplit
        windowId={w.id}
        geom={chatGeom}
        panel={chats.find((c) => c.window_id === w.id)}
        stack={chatStack}
        readOnly={accountReadOnly || !userOpen}
        {sharedNames}
        onSet={onSetChatSplits} />
      {#if chatError}
        <InlineMessage variant="error" detail={chatError.detail}>{chatError.text}</InlineMessage>
      {/if}
    {/if}
    {@render overviewWidths(firstTabOf(w.id))}
  </div>
{/snippet}

{#snippet overviewWidths(firstTab: OverviewTab | undefined)}
  {#if firstTab}
    <!-- The FIRST tab, because it is the one the canvas draws: nothing in the
         files records which tab is selected (detail.ts, overviewParts). The
         other tabs' widths are in Overview → Columns. -->
    <div class="ov-widths">
      <div class="ov-head">Column widths · {plainTabName(firstTab).trim()}</div>
      <div class="fields">
        {#each firstTab.columns.filter((c) => c.visible) as c (c.name)}
          <Field
            kind="number"
            label={c.label}
            layout="column"
            width="5rem"
            min={0}
            value={c.width ?? ""}
            disabled={readOnly}
            disabledReason="This file is read-only"
            onchange={(e) => {
              const el = e.currentTarget as HTMLInputElement;
              const v = Number(el.value);
              // Blank or non-numeric writes nothing and snaps back, as ChatSplit does.
              if (el.value.trim() !== "" && Number.isFinite(v)) onSetOverviewWidth(firstTab.index, c.name, Math.round(v));
              else el.value = String(c.width ?? "");
            }} />
        {/each}
      </div>
      {#if widthError}
        <InlineMessage variant="error" detail={widthError.detail}>{widthError.text}</InlineMessage>
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet freeRow(w: WindowRect)}
  <!-- stackTargets is deliberately filtered too: it derives from freeWindows,
       so "Hide clutter" also hides those windows from "Stack with…" (M7).
       Falls out of freeWindows being the shared source, but it's defensible
       on its own — you can only stack with what you can see — so it stays,
       recorded rather than silently inherited. -->
  {@const stackTargets = freeWindows.filter((o) => o.id !== w.id && o.renderable)}
  <div class="row" class:selected={w.id === selectedId} use:scrollOnSelect={w.id === selectedId}>
    <div class="row-head">
      {@render rowHead(w)}
    </div>
    {#if w.renderable && (stacks.length > 0 || stackTargets.length > 0)}
      <div class="free-controls">
        {#if stacks.length > 0}
          <Field
            kind="select"
            aria-label="Add to stack"
            disabled={readOnly}
            disabledReason="This file is read-only"
            bind:value={addPick[w.id]}
            onchange={() => {
              const v = addPick[w.id];
              addPick[w.id] = "";
              if (v) onAddToStack(w.id, v);
            }}
            options={[
              { value: "", label: "Add to stack…", disabled: true },
              ...stacks.map((s) => ({
                value: s.container_id,
                label: stackLabel(s) ?? displayName(s.container_id),
              })),
            ]} />
        {/if}
        {#if stackTargets.length > 0}
          <!-- An <option> has no hover title, so unlike rowHead's two separate
               spans this keeps the detail inline — dropping it would make two
               same-family unnamed windows (e.g. two chat channels)
               indistinguishable in the dropdown again (the bug 854b0d7
               "Disambiguate stack dropdowns" fixed). -->
          <Field
            kind="select"
            aria-label="Stack with another window"
            disabled={readOnly}
            disabledReason="This file is read-only"
            bind:value={withPick[w.id]}
            onchange={() => {
              const v = withPick[w.id];
              withPick[w.id] = "";
              if (v) onCreateStack(w.id, v);
            }}
            options={[
              { value: "", label: "Stack with…", disabled: true },
              ...stackTargets.map((other) => ({ value: other.id, label: displayNameOf(other) })),
            ]} />
        {/if}
      </div>
    {/if}
    {#if w.id === selectedId && w.geom}
      {@render detail(w)}
    {/if}
  </div>
{/snippet}

<div class="window-panel">
  {#if orphanCount > 0 && !readOnly}
    <InlineMessage variant="warn" class="orphans">
      {orphanCount} empty stack frame{orphanCount === 1 ? "" : "s"} — leftovers that draw a
      rectangle with nothing in it.
      <!-- "Delete them" needed the sentence above it to parse, which is what
           makes it a caption rather than a label. -->
      <Button size="sm" type="button" onclick={onDeleteOrphans}>Delete empty frames</Button>
    </InlineMessage>
  {/if}
  <!-- Above the stack list, which is what every one of these failures is about. -->
  {#if stackError}
    <InlineMessage variant="error" detail={stackError.detail}>{stackError.text}</InlineMessage>
  {/if}
  {#each stacks as stack (stack.container_id)}
    {@const containerWindow = findWindow(stack.container_id)}
    {@const matched = matchingMembers(stack)}
    {@const containerMatches = !!containerWindow && windowMatches(containerWindow, filter, overrides)}
    {@const label = stackLabel(stack)}
    {#if matched.length > 0 || containerMatches}
    <div class="stack-group">
      {#if containerWindow}
        <div
          class="row frame"
          class:selected={stack.container_id === selectedId}
          use:scrollOnSelect={stack.container_id === selectedId}>
          <div class="row-head">
            <Button
              variant="ghost"
              size="sm"
              iconOnly
              title="Collapse stack"
              onclick={(e) => { e.stopPropagation(); collapsed[stack.container_id] = !collapsed[stack.container_id]; }}>
              {collapsed[stack.container_id] ? "▸" : "▾"}
            </Button>
            <span class="frame-label" title="Stack frame">frame</span>
            <!-- "frame" is the type marker (always present, even for an
                 unpaired character with no tabgroups entry); the real label,
                 when EVE has one, shows alongside it — the row then names
                 both what it is and which stack it is. -->
            {#if label}<span class="detail">{label}</span>{/if}
            {@render rowHead(containerWindow)}
            <Chip tone="neutral" size="sm">{matched.length}</Chip>
          </div>
          {#if stack.container_id === selectedId && containerWindow.geom}
            {@render detail(containerWindow)}
          {/if}
        </div>
      {:else}
        <div class="stack-head">
          <Button
            variant="ghost"
            size="sm"
            iconOnly
            title="Collapse stack"
            onclick={(e) => { e.stopPropagation(); collapsed[stack.container_id] = !collapsed[stack.container_id]; }}>
            {collapsed[stack.container_id] ? "▸" : "▾"}
          </Button>
          <span class="stack-title" title={stack.container_id}>{label ?? describe(stack.container_id).label}</span>
          <Chip tone="neutral" size="sm">{matched.length}</Chip>
        </div>
      {/if}
      {#if !collapsed[stack.container_id]}
        {#each stack.members as memberId, i (memberId)}
          {@const w = findWindow(memberId)}
          {#if w && windowMatches(w, filter, overrides)}
            <div class="row member" class:selected={w.id === selectedId} use:scrollOnSelect={w.id === selectedId}>
              <div class="row-head">
                {@render rowHead(w)}
                <Button
                  size="sm"
                  class="stack-btn"
                  disabled={readOnly || i === 0 || !memberVisible(stack.members[i - 1])}
                  disabledReason={i === 0 ? "Already first in the stack" : "The window above is filtered out"}
                  title="Move up in stack order"
                  aria-label="Move up in stack order"
                  onclick={() => onReorder(stack.container_id, swapped(stack.members, i, i - 1))}>
                  ↑
                </Button>
                <Button
                  size="sm"
                  class="stack-btn"
                  disabled={readOnly || i === stack.members.length - 1 || !memberVisible(stack.members[i + 1])}
                  disabledReason={i === stack.members.length - 1
                    ? "Already last in the stack"
                    : "The window below is filtered out"}
                  title="Move down in stack order"
                  aria-label="Move down in stack order"
                  onclick={() => onReorder(stack.container_id, swapped(stack.members, i, i + 1))}>
                  ↓
                </Button>
                <Button
                  size="sm"
                  class="stack-btn"
                  disabled={readOnly}
                  disabledReason="This file is read-only"
                  title="Remove this window from the stack"
                  aria-label="Remove this window from the stack"
                  onclick={() => onUnstack(w.id)}>
                  Unstack
                </Button>
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
          <Button
            variant="ghost"
            size="sm"
            iconOnly
            title="Expand family"
            aria-expanded={!!famOpen[group.family]}
            onclick={() => (famOpen[group.family] = !famOpen[group.family])}>
            {famOpen[group.family] ? "▾" : "▸"}
          </Button>
          <span class="fam-title">{group.label}</span>
          <Chip tone="neutral" size="sm">{group.items.length}</Chip>
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
  /* Every "give the native control explicit dark colours" rule in this file is
     gone — the search box, the two selects and the number inputs are Fields
     now, and Field is the only place in the app that styles one. */
  /* NO `overflow-y` here, and that is the fix rather than a tidy-up.

     This panel used to BE the right-hand column and owned its own scrolling.
     It is now one of several stacked inside `.inspector`, which is a flex
     column that scrolls — so a second scroll container nested in the first made
     this a flex item that shrinks to whatever space HudPanel left and hides the
     remainder inside itself. HudPanel does not scroll, so it took the room, and
     this panel collapsed to a sliver at the bottom of the column.

     The visible result was that the window filter did not exist as far as
     anyone could tell: present in the DOM, focusable by Ctrl+F, and never on
     screen. One scroll container per column. */
  .window-panel {
    font-size: var(--t-body);
    color: var(--text);
  }
  .window-panel :global(.orphans) {
    margin-bottom: var(--s1);
  }
  .row {
    border-bottom: 1px solid var(--border);
  }
  .row.selected {
    background: var(--accent-dim);
  }
  .row-head {
    display: flex;
    align-items: center;
    gap: var(--s1);
    padding: var(--s1) var(--s2);
  }
  .name {
    flex: 1;
    min-width: 0; /* allow truncation instead of forcing the row wider */
    text-align: left;
    background: none;
    border: none;
    color: var(--text);
    cursor: pointer;
    font: inherit;
    padding: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  span.detail {
    color: var(--text-muted);
    margin-left: var(--s1);
    font-size: var(--t-caption);
  }
  .stack-group {
    border-bottom: 1px solid var(--border);
  }
  .stack-head {
    display: flex;
    align-items: center;
    gap: var(--s1);
    padding: var(--s1) var(--s2);
    background: var(--surface-raised);
    font-weight: 600;
    font-size: var(--t-caption);
    color: var(--text-secondary);
  }
  .row.frame .row-head {
    background: var(--surface-raised);
    font-weight: 600;
  }
  .frame-label {
    flex: 0 0 auto;
    font-size: var(--t-caption);
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-muted);
  }
  .row.member {
    border-bottom: none;
  }
  .row.member .row-head {
    padding-left: var(--s5);
  }
  .row.member:last-child {
    border-bottom: 1px solid var(--border);
  }
  .window-panel :global(.stack-btn) {
    flex: 0 0 auto;
  }
  .free-controls {
    display: flex;
    gap: var(--s1);
    padding: 0 var(--s2) var(--s1);
    flex-wrap: wrap;
  }
  .free-controls :global(select) {
    max-width: 9rem;
  }
  div.detail {
    padding: var(--s1) var(--s2) var(--s2);
    display: grid;
    gap: var(--s2);
  }
  .coords {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--s1);
  }
  .coords label {
    display: grid;
    gap: 0;
    font-size: var(--t-caption);
    color: var(--text-muted);
  }
  .coords :global(input) {
    width: 100%;
  }
  .flags {
    display: grid;
    gap: 0;
  }
  .flag {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: var(--s1);
    color: var(--text);
  }
  .fam-group {
    border-bottom: 1px solid var(--border);
  }
  .fam-head {
    display: flex;
    align-items: center;
    gap: var(--s1);
    padding: var(--s1) var(--s2);
    background: var(--surface-raised);
    font-weight: 600;
    font-size: var(--t-caption);
    color: var(--text-secondary);
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
    padding-left: var(--s4);
  }
  /* ChatSplit's frame, for the block that sits where it would. Wraps: an
     overview shows eight or more columns, a chat split two fields. */
  .ov-widths {
    border-top: 1px solid var(--border);
    margin-top: var(--s1);
    padding-top: var(--s1);
  }
  .ov-head {
    color: var(--text-muted);
    font-size: var(--t-caption);
    margin-bottom: var(--s1);
  }
  .ov-widths .fields {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s2);
  }
</style>
