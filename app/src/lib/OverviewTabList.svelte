<script lang="ts">
  import type { MenuItem } from "./ContextMenu.svelte";
  import type { OverviewColumns, OverviewTab } from "./api";
  import { parseTabName, plainTabName, cssColor } from "./tabName";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";
  import MenuButton from "./ui/MenuButton.svelte";
  import PanelHeader from "./ui/PanelHeader.svelte";

  // The ONE control that selects an overview tab. It replaces a grouped
  // <select>, a chip row that only appeared when the selected tab's window held
  // two or more tabs, and the four toolbar buttons that acted on the selection.
  //
  // Presentational: no `api` import and no copy of `keepSelection`. Every
  // mutation goes out as a callback and comes back as new `data`, because the
  // backend renumbers the tab table on a reorder and only the view — which
  // holds `tabIndex` — can re-point the selection afterwards.
  let {
    data,
    tabIndex,
    onSelect,
    onCreateTab,
    onAddWindow,
    onRemoveWindow,
    onDeleteTab,
    onRenameTab,
    onReorder,
    onMove,
    onSetUpWindowMapping,
  }: {
    data: OverviewColumns;
    tabIndex: number | null;
    onSelect: (idx: number) => void;
    /** `windowIdx` is null for a windowless account and for the Other group;
        the view resolves both to today's `currentWindowIndex ?? 0`. */
    onCreateTab: (name: string, windowIdx: number | null) => void;
    onAddWindow: (name: string) => void;
    onRemoveWindow: (windowIdx: number) => void;
    onDeleteTab: (tabIdx: number) => void;
    /** The READABLE text, not the stored markup: the view re-composes the
        colour and bold around it. Spacing goes out verbatim — padding is how a
        tab is widened in game. */
    onRenameTab: (tabIdx: number, text: string) => void;
    onReorder: (windowIdx: number, order: number[]) => void;
    onMove: (tabIdx: number, from: number, to: number, pos: number) => void;
    onSetUpWindowMapping: () => void;
  } = $props();

  type Group = {
    key: string;
    /** "" for the one ungrouped list a windowless account gets — there are no
        windows to name, so naming one would be a lie. */
    label: string;
    windowIdx: number | null;
    tabs: OverviewTab[];
  };

  const groups = $derived.by<Group[]>(() => {
    const byIndex = new Map(data.tabs.map((t) => [t.index, t]));
    if (data.windows.length === 0) {
      return [{ key: "all", label: "", windowIdx: null, tabs: data.tabs }];
    }
    // `tab_indices` order as stored: the backend renumbers to strip order, so
    // this IS the in-game order. Do not sort it and do not re-derive it.
    const out: Group[] = data.windows.map((w) => ({
      key: `w${w.index}`,
      label: `Overview ${w.index + 1}`,
      windowIdx: w.index,
      tabs: w.tab_indices.map((i) => byIndex.get(i)).filter((t): t is OverviewTab => !!t),
    }));
    const grouped = new Set(data.windows.flatMap((w) => w.tab_indices));
    const orphans = data.tabs.filter((t) => !grouped.has(t.index));
    if (orphans.length > 0) out.push({ key: "other", label: "Other", windowIdx: null, tabs: orphans });
    return out;
  });

  /** The group the selection is in, so the footer's `+ Tab` creates beside it. */
  const selectedGroup = $derived(groups.find((g) => g.tabs.some((t) => t.index === tabIndex)) ?? null);

  function style(name: string): string {
    const n = parseTabName(name);
    return [n.color ? `color:${cssColor(n.color)}` : "", n.bold ? "font-weight:700" : ""]
      .filter(Boolean).join(";");
  }

  function rowMenu(t: OverviewTab): MenuItem[] {
    return [
      // Renaming happens ON the row. A rename started here and finished in a
      // panel below was two places for one gesture, and it left a Name field
      // sitting there permanently for the 99% of the time nobody is renaming.
      { label: "Rename", run: () => startRename(t) },
      { label: "Delete tab", run: () => onDeleteTab(t.index) },
    ];
  }

  // Present-and-disabled, never absent: the reasons are the backend's own error
  // cases, which is what the vanishing "Remove Window" button was communicating
  // by disappearing.
  function groupMenu(g: Group): MenuItem[] {
    const items: MenuItem[] = [{ label: "New tab in this window", run: () => startCreate(g) }];
    if (g.windowIdx === null) return items;
    const only = data.windows.length <= 1;
    const notLast = g.windowIdx !== data.windows.length - 1;
    items.push({
      label: "Remove this window",
      run: () => onRemoveWindow(g.windowIdx as number),
      disabled: only || notLast,
      hint: only
        ? "This is the only overview window"
        : notLast
          ? "Only the last overview window can be removed — EVE numbers windows by position"
          : undefined,
    });
    return items;
  }

  // One inline editor for all three name gestures: create a tab, name a new
  // window's first tab, and rename an existing tab in place.
  let pending = $state<
    { kind: "tab" | "window" | "rename"; windowIdx: number | null; tabIdx?: number; value: string } | null
  >(null);
  let nameInput: HTMLInputElement | HTMLSelectElement | undefined = $state();
  $effect(() => {
    if (!nameInput) return;
    nameInput.focus();
    if (nameInput instanceof HTMLInputElement) nameInput.select();
  });

  function startCreate(g: Group | null) {
    pending = { kind: "tab", windowIdx: g?.windowIdx ?? null, value: "" };
  }
  function startRename(t: OverviewTab) {
    onSelect(t.index);
    // The READABLE text, padding and all — never the stored markup.
    pending = { kind: "rename", windowIdx: null, tabIdx: t.index, value: parseTabName(t.name).text };
  }
  function submit() {
    const p = pending;
    pending = null;
    if (!p) return;
    if (p.kind === "rename") {
      // `p.value`, not a trimmed copy: padding is how a tab is widened in game
      // ("  main  ", "  3  "). The trim only answers "did they type anything at
      // all", which is also how the pre-Phase-4 rename box read it.
      if (p.value.trim() && p.tabIdx !== undefined) onRenameTab(p.tabIdx, p.value);
      return;
    }
    const name = p.value.trim();
    if (!name) return;
    if (p.kind === "window") onAddWindow(name);
    else onCreateTab(name, p.windowIdx);
  }

  // Drag. Every row is draggable — not only the ones in a window holding two or
  // more tabs, which is the rule that made reorder silently unavailable.
  let drag = $state<{ tabIdx: number; windowIdx: number; pos: number } | null>(null);

  function drop(g: Group, pos: number) {
    const d = drag;
    drag = null;
    if (!d || g.windowIdx === null) return;
    if (d.windowIdx === g.windowIdx) {
      const order = g.tabs.map((t) => t.index);
      const [moved] = order.splice(d.pos, 1);
      order.splice(pos, 0, moved);
      onReorder(g.windowIdx, order);
    } else {
      onMove(d.tabIdx, d.windowIdx, g.windowIdx, pos);
    }
  }
</script>

<div class="tablist">
  <PanelHeader title="Tabs" level={4} />

  {#if data.windows.length === 0}
    <!-- The message explains the SHAPE of this list, so it belongs against the
         list rather than in a toolbar three controls away. -->
    <InlineMessage>
      Tabs aren't assigned to specific overview windows on this account — EVE spreads them
      across your windows itself. That's normal: importing an overview pack through the
      client removes the assignment.
      <Button size="sm" onclick={onSetUpWindowMapping}>Set up per-window tabs</Button>
    </InlineMessage>
  {/if}

  {#if data.tabs.length === 0}
    <EmptyState title="This account file has no overview tabs." />
  {/if}

  <div class="groups">
    {#each groups as g (g.key)}
      {#if g.label}
        <div class="group-head">
          <span class="group-label">{g.label}</span>
          <MenuButton items={() => groupMenu(g)} title="{g.label} actions" />
        </div>
      {/if}
      <ul>
        {#each g.tabs as t, i (t.index)}
          <li>
            {#if pending?.kind === "rename" && pending.tabIdx === t.index}
              <!-- The row BECOMES the editor. Enter commits, Escape cancels,
                   and leaving it commits — the same three keys the name box it
                   replaces used. -->
              <Field bind:value={pending.value} bind:element={nameInput}
                     ariaLabel="Tab name" placeholder="Tab name"
                     onblur={submit}
                     onkeydown={(e: KeyboardEvent) => {
                       if (e.key === "Enter") { e.preventDefault(); submit(); }
                       else if (e.key === "Escape") pending = null;
                     }} />
            {:else}
            <ListRow
              selected={t.index === tabIndex}
              onclick={() => onSelect(t.index)}
              actions={rowMenu(t)}
              oncontextmenu={(e: MouseEvent) => e.preventDefault()}
              draggable={g.windowIdx !== null}
              ondragstart={(e: DragEvent) => {
                drag = { tabIdx: t.index, windowIdx: g.windowIdx as number, pos: i };
                // WebView2/Chromium won't fire `drop` unless dragstart sets data.
                e.dataTransfer?.setData("text/plain", String(t.index));
                if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
              }}
              ondragover={(e: DragEvent) => { e.preventDefault();
                if (e.dataTransfer) e.dataTransfer.dropEffect = "move"; }}
              ondrop={(e: DragEvent) => { e.preventDefault(); drop(g, i); }}
              ondragend={() => (drag = null)}>
              <!-- The one truthful rendering of a tab in the app: its real
                   colour and weight, the way it looks in game. -->
              <span style={style(t.name)}>{plainTabName(t.name)}</span>
            </ListRow>
            {/if}
          </li>
        {/each}
        {#if pending?.kind === "tab" && pending.windowIdx === g.windowIdx}
          <li>
            <Field bind:value={pending.value} bind:element={nameInput}
                   ariaLabel="Tab name" placeholder="Tab name"
                   onkeydown={(e: KeyboardEvent) => {
                     if (e.key === "Enter") { e.preventDefault(); submit(); }
                     else if (e.key === "Escape") pending = null;
                   }} />
          </li>
        {/if}
      </ul>
    {/each}

    {#if pending?.kind === "window"}
      <Field bind:value={pending.value} bind:element={nameInput}
             ariaLabel="First tab name" placeholder="First tab name"
             onkeydown={(e: KeyboardEvent) => {
               if (e.key === "Enter") { e.preventDefault(); submit(); }
               else if (e.key === "Escape") pending = null;
             }} />
    {/if}
  </div>

  <div class="foot">
    <!-- No footer buttons while renaming: that editor commits on blur, so a
         Cancel button would commit on the way to being clicked. Enter, Escape
         and clicking away are its three exits. -->
    {#if pending && pending.kind !== "rename"}
      <Button variant="primary" onclick={submit}>
        {pending.kind === "window" ? "Add window" : "Add tab"}
      </Button>
      <Button onclick={() => (pending = null)}>Cancel</Button>
    {:else if !pending}
      <Button size="sm" onclick={() => startCreate(selectedGroup)}
              disabled={data.tabs.length === 0}
              disabledReason="This account file has no overview tabs">+ Tab</Button>
      <Button size="sm" onclick={() => (pending = { kind: "window", windowIdx: null, value: "Overview" })}
              disabled={data.windows.length === 0}
              disabledReason="This account doesn't assign tabs to windows — set that up first"
              title="Add a new overview window">+ Window</Button>
    {/if}
  </div>
</div>

<style>
  .tablist {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
    min-height: 0;
    padding: var(--s2);
  }
  .groups { flex: 1; min-height: 0; overflow: auto; }
  .group-head { display: flex; align-items: center; gap: var(--s1); }
  .group-label {
    flex: 1;
    color: var(--text-muted);
    font-size: var(--t-caption);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  ul { list-style: none; padding: 0; margin: 0 0 var(--s2); }
  .foot { display: flex; gap: var(--s1); }
</style>
