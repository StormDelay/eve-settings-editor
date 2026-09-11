<script lang="ts">
  import type { MenuItem } from "./ContextMenu.svelte";
  import type { OverviewColumns, OverviewTab } from "./api";
  import { parseTabName, plainTabName, cssColor, EVE_PALETTE, type TabName } from "./tabName";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";
  import MenuButton from "./ui/MenuButton.svelte";
  import PanelHeader from "./ui/PanelHeader.svelte";
  import Popover from "./ui/Popover.svelte";

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
    /** The whole decomposed name — text, colour and bold — because the row
        editor edits all three and commits them as ONE rename. Spacing in `text`
        goes out verbatim: padding is how a tab is widened in game. */
    onRenameTab: (tabIdx: number, name: TabName) => void;
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

  function style(name: string): string {
    const n = parseTabName(name);
    return [n.color ? `color:${cssColor(n.color)}` : "", n.bold ? "font-weight:700" : ""]
      .filter(Boolean).join(";");
  }

  function rowMenu(t: OverviewTab, g: Group): MenuItem[] {
    const items: MenuItem[] = [
      // Renaming happens ON the row. A rename started here and finished in a
      // panel below was two places for one gesture, and it left a Name field
      // sitting there permanently for the 99% of the time nobody is renaming.
      { label: "Rename tab…", run: () => startRename(t) },
      { label: "Delete tab", run: () => onDeleteTab(t.index) },
    ];
    // Cross-window drag is the fast route; this is the keyboard one, and the
    // one that still works when the two windows are scrolled apart. Present and
    // disabled for a tab in no window, because `move_tab` needs a source.
    for (const w of data.windows) {
      if (w.index === g.windowIdx) continue;
      items.push({
        label: `Move to Overview ${w.index + 1}`,
        run: () => onMove(t.index, g.windowIdx as number, w.index, w.tab_indices.length),
        disabled: g.windowIdx === null,
        hint: g.windowIdx === null
          ? "This tab isn't assigned to a window — EVE decides where it appears"
          : undefined,
      });
    }
    return items;
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
  // window's first tab, and edit an existing tab's name in place. A rename
  // carries the colour and bold as well — they ARE the name, stored as markup
  // inside the same string (see tabName.ts), so editing them anywhere else was
  // splitting one property across two panes.
  let pending = $state<
    {
      kind: "tab" | "window" | "rename";
      windowIdx: number | null;
      tabIdx?: number;
      value: string;
      color?: string | null;
      bold?: boolean;
    } | null
  >(null);
  let swatchOpen = $state(false);
  let swatchEl: HTMLDivElement | undefined = $state();
  let editorEl: HTMLDivElement | undefined = $state();

  /** Live preview: the box shows the name the way the tab will look. */
  const draftStyle = $derived(
    [pending?.color ? `color:${cssColor(pending.color)}` : "", pending?.bold ? "font-weight:700" : ""]
      .filter(Boolean).join(";"),
  );

  // Leaving the editor commits, but the swatch, the palette and the B toggle are
  // PART of the editor — moving focus onto one of them must not close it.
  function editorFocusOut(e: FocusEvent) {
    if (swatchOpen) return;
    const next = e.relatedTarget as Node | null;
    if (next && editorEl?.contains(next)) return;
    submit();
  }
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
    const n = parseTabName(t.name);
    pending = { kind: "rename", windowIdx: null, tabIdx: t.index, value: n.text, color: n.color, bold: n.bold };
  }
  function submit() {
    const p = pending;
    pending = null;
    if (!p) return;
    if (p.kind === "rename") {
      // `p.value`, not a trimmed copy: padding is how a tab is widened in game
      // ("  main  ", "  3  "). The trim only answers "did they type anything at
      // all", which is also how the pre-Phase-4 rename box read it.
      if (p.value.trim() && p.tabIdx !== undefined) {
        onRenameTab(p.tabIdx, { text: p.value, color: p.color ?? null, bold: !!p.bold });
      }
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
      description="This account file holds none. Importing an overview pack adds some." />
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
              <!-- The row BECOMES the editor, and it carries every part of the
                   name: the text, the colour and the weight. All three are one
                   markup-bearing string in the file, so editing them in two
                   places was splitting one property in half. Enter commits,
                   Escape cancels, leaving commits. -->
              <div class="rename" bind:this={editorEl} onfocusout={editorFocusOut}>
                <Field bind:value={pending.value} bind:element={nameInput}
                       ariaLabel="Tab name" placeholder="Tab name"
                       style={draftStyle}
                       onkeydown={(e: KeyboardEvent) => {
                         if (e.key === "Enter") { e.preventDefault(); submit(); }
                         else if (e.key === "Escape") pending = null;
                       }} />
                <div class="swatch-wrap" bind:this={swatchEl}>
                  <!-- aria-label as well as title: the swatch's only content is
                       a dash or nothing at all. -->
                  <Button class="swatch" title="Tab name colour" aria-label="Tab name colour"
                          style={pending.color ? `background:${cssColor(pending.color)}` : ""}
                          onclick={() => (swatchOpen = !swatchOpen)}>{pending.color ? "" : "—"}</Button>
                  {#if swatchOpen && swatchEl}
                    <Popover
                      anchor={swatchEl}
                      placement="bottom-start"
                      ariaLabel="Tab name colour"
                      class="palette"
                      onclose={() => (swatchOpen = false)}>
                      <div class="palette-grid">
                        {#each EVE_PALETTE as c (c)}
                          <button style="background:#{c}" title="#{c}" aria-label="#{c}"
                                  onclick={() => { if (pending) pending.color = `FF${c.toUpperCase()}`; swatchOpen = false; }}></button>
                        {/each}
                      </div>
                      <Button variant="ghost" size="sm" class="palette-none"
                              onclick={() => { if (pending) pending.color = null; swatchOpen = false; }}>No colour</Button>
                    </Popover>
                  {/if}
                </div>
                <Button class="bold-toggle" pressed={!!pending.bold} title="Bold tab name"
                        onclick={() => { if (pending) pending.bold = !pending.bold; }}>B</Button>
              </div>
            {:else}
            <ListRow
              selected={t.index === tabIndex}
              onclick={() => onSelect(t.index)}
              actions={rowMenu(t, g)}
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
  /* The same side padding ListRow gives a row, so the group's "⋯" lands in the
     same column as every row's "⋯" instead of one step further out. */
  .group-head { display: flex; align-items: center; gap: var(--s1); padding: 0 var(--s2); }
  .rename { display: flex; align-items: center; gap: var(--s1); padding: 0 var(--s2); }
  .rename :global(.field) { flex: 1; min-width: 0; }
  .rename :global(input) { width: 100%; }
  .swatch-wrap { position: relative; display: inline-flex; }
  .rename :global(.swatch) { width: 1.9rem; }
  .rename :global(.bold-toggle) { font-weight: 700; }
  :global(.palette) { display: block; }
  .palette-grid { display: grid; grid-template-columns: repeat(8, 1.1rem); gap: var(--s1); }
  /* --border-strong, not --border: this outline has to read against an
     arbitrary user colour on either side of it. */
  .palette-grid button {
    width: 1.1rem; height: 1.1rem; border: 1px solid var(--border-strong);
    border-radius: var(--r-sm); padding: 0; cursor: pointer;
  }
  .palette-grid button:hover { outline: 1px solid var(--text); }
  :global(.palette-none) { display: block; width: 100%; margin-top: var(--s1); }
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
