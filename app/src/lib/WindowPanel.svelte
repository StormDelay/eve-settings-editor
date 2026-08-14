<script lang="ts">
  import type { WindowRect, BoolFlag, NodePath, Stack, ChatPanel } from "$lib/api";
  import { describe, groupByFamily, displayName, displayNameOf, nameOf, stackLabel, isClutter, type ClutterOverrides } from "$lib/windowLabels";
  import { windowMatches, isOrphanFrame, NO_FILTER, type WindowFilter } from "$lib/layout";
  import ContextMenu, { type MenuItem } from "$lib/ContextMenu.svelte";
  import ChatSplit from "$lib/ChatSplit.svelte";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import MenuButton from "./ui/MenuButton.svelte";
  import SearchField from "./ui/SearchField.svelte";
  import { revealAndFocus } from "$lib/keymap";

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
    overrides,
    onClutterOverride,
    chats,
    accountReadOnly,
    userOpen,
    sharedNames,
    onSetChatSplits,
    stackError = null,
    chatError = null,
    filter = $bindable({ ...NO_FILTER }),
    focusFilter = $bindable(undefined),
  }: {
    windows: WindowRect[];
    stacks: Stack[];
    /** Refused edits, owned by LayoutView and rendered at the control group each
     *  belongs to: the stack list, and the chat split fields. */
    stackError?: { text: string; detail: string } | null;
    chatError?: { text: string; detail: string } | null;
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
    /** Shared with the canvas — see LayoutView. The panel renders the controls;
     * LayoutView owns the state and applies the same predicate to the rects. */
    filter?: WindowFilter;
    /** Exposed so the global Ctrl+F handler in +page.svelte can focus this
     * input from outside — LayoutView forwards it up. The input lives here,
     * so this is where the bind:this actually is. */
    focusFilter?: () => void;
  } = $props();

  let filterInput: HTMLInputElement | HTMLSelectElement | undefined = $state();
  // Per-row selection for the two "stack with…" pickers, cleared as soon as the
  // pick is acted on so each control returns to its prompt.
  let addPick: Record<string, string> = $state({});
  let withPick: Record<string, string> = $state({});
  // Scrolls the box into view as well as focusing it: this one sits at the top
  // of a tall inspector, so Ctrl+F used to focus something off screen.
  focusFilter = () => revealAndFocus(filterInput);

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
    } else if (isClutter(w.id, overrides)) {
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
  </div>
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
  <div class="filters">
    <!-- aria-label passed as a raw attribute so it stays "Filter windows"
         exactly: SearchField names the box from its built placeholder, which
         carries a trailing ellipsis, and WindowPanel.spec looks it up by the
         bare phrase. -->
    <SearchField
      nouns="windows"
      aria-label="Filter windows"
      bind:element={filterInput}
      bind:value={filter.text} />
    <!-- The one filter whose name overclaims, so it is the one that most needs
         a tooltip: EVE's flag is sticky, and a window it still calls open may
         well not be on screen. See docs/format-notes.md, "openWindows is
         sticky". -->
    <Field
      kind="checkbox"
      class="toggle"
      label="Open only"
      bind:value={filter.openOnly}
      title="Shows only windows EVE's own openWindows flag calls open. That flag is set when a window is opened and is NOT cleared when it is closed, so a window can read as open here while not being on screen in game. Right-click a window and choose “Treat as clutter” to keep one out of the list and the canvas." />
    <Field
      kind="checkbox"
      class="toggle"
      label="Hide clutter"
      bind:value={filter.hideClutter}
      title="Hides windows EVE spawns per conversation, item or dialog — chat invitations, private chats, channel settings, mail messages, info popups, per-container windows. Standing channels and parent windows stay." />
    <div
      class="envs"
      role="radiogroup"
      aria-label="Environment"
      title="Shows only the windows that exist in one environment. Station and player structure are one “Docked” view — EVE stores a single position per window, so this filters the picture, it does not switch layouts. A window the editor does not recognise shows in both.">
      {#each [["all", "All"], ["docked", "Docked"], ["space", "In space"]] as const as [value, label]}
        <Field kind="radio" class="toggle" name="env" radioValue={value} {label} bind:value={filter.env} />
      {/each}
    </div>
  </div>
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
  .filters {
    display: grid;
    gap: var(--s1);
    padding: var(--s1) var(--s2);
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--surface);
    z-index: 1;
  }
  .filters :global(.search) {
    width: 100%;
  }
  .filters :global(.search input) {
    width: 100%;
  }
  .window-panel :global(.toggle) {
    font-size: var(--t-caption);
    color: var(--text-muted);
  }
  .envs {
    display: flex;
    gap: var(--s2);
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
</style>
