<script lang="ts">
  import type { MenuItem } from "./ContextMenu.svelte";
  import type { OverviewColumns, OverviewTab } from "./api";
  import type { TabPiece } from "./api";
  import { pieceStyle, plainTabName, type TabNameEdit } from "./tabName";
  import TabNameEditor from "./TabNameEditor.svelte";
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
    parseName,
    formatName,
    onReorder,
    onMove,
    onSetUpWindowMapping,
    editError = null,
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
    /** The row editor's result: styled pieces, or raw markup. Spacing inside
        a piece goes out verbatim: it is how a tab is widened in game. */
    onRenameTab: (tabIdx: number, edit: TabNameEdit) => void;
    /** The backend's tab-name parser and writer, for the editor's raw mode. */
    parseName: (raw: string) => Promise<{ pieces: TabPiece[]; editable: boolean; warnings?: string[] }>;
    formatName: (pieces: TabPiece[]) => Promise<string>;
    onReorder: (windowIdx: number, order: number[]) => void;
    onMove: (tabIdx: number, from: number, to: number, pos: number) => void;
    onSetUpWindowMapping: () => void;
    /** A refused edit, owned by OverviewView (which runs the commands) and
     *  rendered here (which owns the controls). `where` picks the slot: the
     *  windowless band, the name-entry row, the tab strip, or the row actions. */
    editError?: { where: string; text: string; detail: string } | null;
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

  function rowMenu(t: OverviewTab, g: Group, i: number): MenuItem[] {
    const loose = g.windowIdx === null;
    const looseHint = "This tab isn't assigned to a window — EVE decides where it appears";
    // The keyboard route for the drag's reorder: the same `onReorder`, one step.
    const step = (to: number) => () => {
      const order = g.tabs.map((x) => x.index);
      order.splice(i, 1);
      order.splice(to, 0, t.index);
      onReorder(g.windowIdx as number, order);
    };
    const items: MenuItem[] = [
      // Renaming happens ON the row. A rename started here and finished in a
      // panel below was two places for one gesture, and it left a Name field
      // sitting there permanently for the 99% of the time nobody is renaming.
      { label: "Rename tab…", run: () => startRename(t) },
      { label: "Delete tab", run: () => onDeleteTab(t.index) },
      { label: "Move up", run: step(i - 1), disabled: loose || i === 0,
        hint: loose ? looseHint : i === 0 ? "Already first in this window" : undefined },
      { label: "Move down", run: step(i + 1), disabled: loose || i === g.tabs.length - 1,
        hint: loose ? looseHint : i === g.tabs.length - 1 ? "Already last in this window" : undefined },
    ];
    // Cross-window drag is the fast route; this is the keyboard one, and the
    // one that still works when the two windows are scrolled apart. Present and
    // disabled for a tab in no window, because `move_tab` needs a source.
    for (const w of data.windows) {
      if (w.index === g.windowIdx) continue;
      items.push({
        label: `Move to Overview ${w.index + 1}`,
        run: () => onMove(t.index, g.windowIdx as number, w.index, w.tab_indices.length),
        disabled: loose,
        hint: loose ? looseHint : undefined,
      });
    }
    return items;
  }

  // Present-and-disabled, never absent: the reasons are the backend's own error
  // cases, which is what the vanishing "Remove Window" button was communicating
  // by disappearing.
  function groupMenu(g: Group): MenuItem[] {
    // Other has no window: the view puts its new tab in Overview 1, so say so.
    const items: MenuItem[] = [{
      label: g.windowIdx === null ? "New tab (goes to Overview 1)" : "New tab in this window",
      run: () => startCreate(g),
    }];
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

  // One inline name entry for creating a tab and naming a new window's first
  // tab. Renaming an existing tab opens TabNameEditor on the row instead: a
  // name's colours and styling ARE the name, stored as markup in one string.
  let pending = $state<
    {
      kind: "tab" | "window" | "rename";
      windowIdx: number | null;
      tabIdx?: number;
      value: string;
    } | null
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
    pending = { kind: "rename", windowIdx: null, tabIdx: t.index, value: "" };
  }
  function submit() {
    const p = pending;
    pending = null;
    if (!p) return;
    if (p.kind === "rename") return;
    const name = p.value.trim();
    if (!name) return;
    if (p.kind === "window") onAddWindow(name);
    else onCreateTab(name, p.windowIdx);
  }

  // Drag. Every row is draggable — not only the ones in a window holding two or
  // more tabs, which is the rule that made reorder silently unavailable.
  let drag = $state<{ tabIdx: number; windowIdx: number; pos: number } | null>(null);
  let dropEnd = $state<string | null>(null);

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
      <!-- The sentence the deleted confirm used to carry, moved to where it is
           read BEFORE the click rather than after it. It is the one place in
           this app where "can't undo" is nearly true, and it earns its keep by
           saying why: the editor has no command that removes the last overview
           window. -->
      Assigning them replaces that with an explicit list, and the editor can't
      undo it — it can't remove the last overview window. Importing an overview
      pack through the client removes the list again.
      <Button size="sm" onclick={onSetUpWindowMapping}>Assign tabs to windows</Button>
    </InlineMessage>
  {/if}
  {#if editError?.where === "windows"}
    <InlineMessage variant="error" detail={editError.detail}>{editError.text}</InlineMessage>
  {/if}

  {#if data.tabs.length === 0}
    <EmptyState
      title="No overview tabs"
      description="This account file holds none. Add one with + Tab, or import an overview pack." />
  {/if}
  <!-- The strip's own failures, above the strip. -->
  {#if editError && ["strip", "actions", "move"].includes(editError.where)}
    <InlineMessage variant="error" detail={editError.detail}>{editError.text}</InlineMessage>
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
              <TabNameEditor tab={t} {parseName} {formatName}
                             oncommit={(edit) => { pending = null; onRenameTab(t.index, edit); }}
                             oncancel={() => (pending = null)} />
            {:else}
            <ListRow
              selected={t.index === tabIndex}
              onclick={() => onSelect(t.index)}
              actions={rowMenu(t, g, i)}
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
              ondrop={g.windowIdx === null ? undefined : (e: DragEvent) => { e.preventDefault(); drop(g, i); }}
              ondragend={() => (drag = null)}>
              <!-- The one truthful rendering of a tab in the app: every piece in
                   its real style over the tab's own colour, the way it looks in
                   game. Text EVE won't draw is left out, and flagged. -->
              <span class="name" title={plainTabName(t)}>{#each t.pieces as p, pi (pi)}{#if !p.hidden}<span style={pieceStyle(p, t.color)}>{p.text}</span>{/if}{/each}</span>
              {#if t.warnings?.length}
                <span class="warn" role="img" aria-label="Won't show fully in game" title={t.warnings.join("\n")}>⚠</span>
              {/if}
            </ListRow>
            {/if}
          </li>
        {/each}
        <!-- Rows only take a drop BEFORE themselves, so the end of a window,
             and an empty window, need a target of their own. -->
        {#if drag && g.windowIdx !== null}
          <li class="drop-end" class:over={dropEnd === g.key}
              ondragover={(e: DragEvent) => { e.preventDefault(); dropEnd = g.key;
                if (e.dataTransfer) e.dataTransfer.dropEffect = "move"; }}
              ondragleave={() => (dropEnd = null)}
              ondrop={(e: DragEvent) => { e.preventDefault(); dropEnd = null; drop(g, g.tabs.length); }}>
            Drop here to move to the end
          </li>
        {/if}
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
  <!-- Under the name-entry row, which stays open on a refusal so the name the
       user typed is still there to retry with. -->
  {#if editError?.where === "entry"}
    <InlineMessage variant="error" detail={editError.detail}>{editError.text}</InlineMessage>
  {/if}

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
      <!-- With no selection (a zero-tab account) the entry opens under the
           first group, which is where the view creates it. -->
      <Button size="sm" onclick={() => startCreate(selectedGroup ?? groups[0])}>+ Tab</Button>
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
  .drop-end {
    padding: var(--s1) var(--s2);
    border: 1px dashed var(--border);
    border-radius: var(--r-sm);
    color: var(--text-muted);
    font-size: var(--t-caption);
  }
  .drop-end.over { border-color: var(--accent); color: var(--text); }
  /* The same side padding ListRow gives a row, so the group's "⋯" lands in the
     same column as every row's "⋯" instead of one step further out. */
  .group-head { display: flex; align-items: center; gap: var(--s1); padding: 0 var(--s2); }
  .name { white-space: pre; }
  .warn { color: var(--warn); margin-left: var(--s1); }
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
