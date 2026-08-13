<script lang="ts">
  import { api, errMessage, type OverviewColumns } from "./api";
  import { message, confirm, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { documentDir } from "@tauri-apps/api/path";
  import { names } from "./names.svelte";
  import { parseTabName, formatTabName, plainTabName, cssColor, EVE_PALETTE, type TabName } from "./tabName";
  import OverviewColumnsTab from "./OverviewColumnsTab.svelte";
  import OverviewFiltersTab from "./OverviewFiltersTab.svelte";
  import OverviewAppearanceTab from "./OverviewAppearanceTab.svelte";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import Popover from "./ui/Popover.svelte";
  import ScopeBanner from "./ui/ScopeBanner.svelte";
  import Tabs from "./ui/Tabs.svelte";

  let { userOpen, userId, charId, charOpen, characters, refreshToken, onLoadCharacter, onUserDirty, onCharDirty, onWindowAdded, onShowAccounts, sharedLabel = "" }:
    { userOpen: boolean; userId: number | null; charId: number | null; charOpen: boolean; characters: number[]; refreshToken: number;
      onLoadCharacter: (id: number) => void; onUserDirty: () => void; onCharDirty: () => void;
      onWindowAdded: (windowId: string) => void; onShowAccounts: () => void; sharedLabel?: string } = $props();

  let data = $state<OverviewColumns | null>(null);
  let tabIndex = $state<number | null>(null);
  let error = $state<string | null>(null);
  // Sub-tab selector added in the Columns/Filters/Appearance split; each child
  // stays mounted (hidden via the `hidden` attribute, not `{#if}`) so switching
  // sub-tabs doesn't re-run a child's effects or reset its local state.
  let sub = $state("Columns");

  async function reload() {
    if (!userOpen) { data = null; return; }
    error = null;
    try {
      data = await api.overviewColumns();
      // Keep the selected tab if it still exists in the (possibly just-switched)
      // file; otherwise fall back to the first tab.
      if (tabIndex === null || !data.tabs.some((t) => t.index === tabIndex)) {
        tabIndex = data.tabs[0]?.index ?? null;
      }
    } catch (e) { error = errMessage(e); }
  }
  // Reload when the slot's file changes (refreshToken bumps on every open/save),
  // not only when userOpen/charId flip — switching between two account files
  // leaves both unchanged and would otherwise show the previous file's overview.
  $effect(() => { void userOpen; void userId; void charId; void refreshToken; reload(); });

  const tab = $derived(data?.tabs.find((t) => t.index === tabIndex) ?? null);
  // The window strip whose tab_indices contains the selected tab (null for an
  // orphan tab that isn't listed under any window).
  const currentWindow = $derived(data?.windows.find((w) => w.tab_indices.includes(tabIndex ?? -1)) ?? null);
  const currentWindowIndex = $derived(currentWindow?.index ?? null);

  /** The Tab picker's options, grouped by overview window. Tabs that belong to
   * no window fall into "Other" rather than vanishing from the list. */
  const tabOptions = $derived.by(() => {
    const d = data;
    if (!d) return [];
    const label = (i: number) => {
      const t = d.tabs.find((x) => x.index === i);
      return t ? plainTabName(t.name) : `Tab ${i}`;
    };
    if (d.windows.length === 0) {
      return d.tabs.map((t) => ({ value: t.index, label: plainTabName(t.name) }));
    }
    const grouped = new Set(d.windows.flatMap((w) => w.tab_indices));
    return [
      ...d.windows.flatMap((w) =>
        w.tab_indices.map((idx) => ({
          value: idx,
          label: label(idx),
          group: `Overview ${w.index + 1}`,
        })),
      ),
      ...d.tabs
        .filter((t) => !grouped.has(t.index))
        .map((t) => ({ value: t.index, label: plainTabName(t.name), group: "Other" })),
    ];
  });

  // The tab a create just added, which is always the highest index: the backend
  // allocates max+1, and the gap-compaction that follows keeps it last. Diffing
  // the index set against a snapshot taken before the call does NOT survive that
  // compaction — on a table that had gaps, renumbering frees up low indices that
  // were not in the snapshot either, and the first of those wins the diff.
  function newestTab(): number | null {
    const highest = Math.max(...(data?.tabs.map((t) => t.index) ?? []));
    return Number.isFinite(highest) ? highest : null;
  }

  // Name entry is an inline input (see the markup below), NOT window.prompt —
  // which the WebView2 renders as an ugly "localhost:1420 says …" dialog. One
  // pending action drives all three tab/window name-entry flows (preset rename
  // has its own pending state now, local to OverviewFiltersTab).
  let pending = $state<
    | { kind: "createTab"; value: string }
    | { kind: "renameTab"; value: string; tabIdx: number }
    | { kind: "addWindow"; value: string }
    | null
  >(null);
  // Was a `use:` action, which Svelte cannot apply to a component. Field hands
  // back its control node instead, and this focuses it when the name box
  // appears — same moment, same effect.
  let nameInput: HTMLInputElement | HTMLSelectElement | undefined = $state();
  $effect(() => {
    if (!nameInput) return;
    nameInput.focus();
    if (nameInput instanceof HTMLInputElement) nameInput.select();
  });

  /** Cleared as soon as the pick is acted on, so the control returns to its
   * prompt rather than showing the window you just moved the tab to. */
  let movePick = $state("");

  function startCreateTab() {
    if (!data || data.tabs.length === 0) return;
    pending = { kind: "createTab", value: "" };
  }
  function startRenameTab() {
    if (!tab) return;
    // The box edits the readable text; the tab's colour and bold ride along
    // through `submitPending` rather than being retyped as raw markup.
    pending = { kind: "renameTab", value: parseTabName(tab.name).text, tabIdx: tab.index };
  }

  // Tab names carry EVE's markup — see tabName.ts. The swatch and the B button
  // rewrite the same `name` string the Rename box does, so neither needs a
  // backend command of its own.
  const nameParts = $derived(tab ? parseTabName(tab.name) : null);
  let swatchOpen = $state(false);
  let swatchEl: HTMLDivElement | undefined = $state();

  async function setNameFormat(patch: Partial<TabName>) {
    if (!tab || !nameParts) return;
    const next = formatTabName({ ...nameParts, ...patch });
    if (next === tab.name) return;
    try { data = await api.tabRename(tab.index, next); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }

  function chipStyle(name: string): string {
    const n = parseTabName(name);
    return [n.color ? `color:${cssColor(n.color)}` : "", n.bold ? "font-weight:700" : ""]
      .filter(Boolean).join(";");
  }
  function startAddWindow() {
    if (!data || data.windows.length === 0) return;
    pending = { kind: "addWindow", value: "Overview" };
  }
  // A windowless account is normal: EVE's own overview importer deletes the
  // tab-to-window mapping, so anyone who has imported a pack lands here. Writing
  // one REPLACES the client's default distribution and pins every tab into a
  // single window, so it is offered rather than done — and the confirm says so.
  async function setUpWindowMapping() {
    if (!data || data.windows.length > 0) return;
    const n = data.tabs.length;
    const ok = await confirm(
      `Put all ${n} tab${n === 1 ? "" : "s"} in one overview window?\n\n` +
        `This account currently lets EVE decide which of your overview windows each tab ` +
        `appears in. Setting this up replaces that with an explicit list, so every tab ` +
        `starts in one window and you arrange them from there.\n\n` +
        `The editor can't undo this — it can't remove the last overview window. If you ` +
        `save and change your mind, importing an overview pack through the client removes ` +
        `the list again.`,
      { title: "Set up per-window tabs", kind: "warning" },
    );
    if (!ok) return;
    try {
      data = await api.overviewCreateWindowMapping();
      onUserDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function submitPending() {
    if (!pending) return;
    const p = pending;
    const name = p.value.trim();
    pending = null;
    if (!name) return;
    try {
      if (p.kind === "createTab") {
        // `currentWindowIndex` is null in two different situations, and 0 is the
        // right answer to both. A windowless account ignores the argument
        // entirely — the backend refuses to fabricate a mapping and EVE
        // distributes tabs itself. An account that HAS windows but whose
        // selected tab belongs to none of them (the "Other" group) gets the new
        // tab in window 0: arbitrary, but visible and movable, where refusing
        // would leave the New button dead for a selection that looks ordinary.
        data = await api.tabCreate(currentWindowIndex ?? 0, name, tabIndex);
        tabIndex = newestTab() ?? tabIndex;
        onUserDirty();
      } else if (p.kind === "renameTab") {
        const current = data?.tabs.find((t) => t.index === p.tabIdx)?.name ?? "";
        // `p.value`, not the trimmed `name`: padding is how a tab is widened in
        // game ("  main  ", "  3  "), so the typed spacing is kept verbatim and
        // the trim above only answers "did they type anything at all".
        const next = formatTabName({ ...parseTabName(current), text: p.value });
        if (next === current) return;
        data = await api.tabRename(p.tabIdx, next);
        onUserDirty();
      } else if (p.kind === "addWindow") {
        // Add window writes the user grouping AND the char-file geometry, so mark
        // BOTH slots dirty — otherwise saveFile skips the char slot and the new
        // window's position never persists. Then hand the new window's id up so
        // the Layout editor selects it: it defaults offset on top of window 0, so
        // without selecting it it's easy to miss.
        data = await api.overviewWindowAdd(name, tabIndex);
        tabIndex = newestTab() ?? tabIndex;
        onUserDirty();
        onCharDirty();
        const w = data.windows[data.windows.length - 1];
        if (w) onWindowAdded(w.index === 0 ? "overview" : `overview_${w.index}`);
      }
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function deleteTab() {
    if (!tab) return;
    const ok = await confirm(`Delete tab "${plainTabName(tab.name)}"? This can't be undone.`, { title: "Delete tab", kind: "warning" });
    if (!ok) return;
    try {
      const result = await api.tabDelete(tab.index);
      data = result;
      tabIndex = result.tabs[0]?.index ?? null;
      onUserDirty();
      // A delete renumbers the account's tabs, and the backend carries the open
      // character's per-tab column widths and sort setting across with them —
      // so that slot has unsaved work too whenever a character is open.
      if (charOpen) onCharDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function moveTab(toWindow: number) {
    if (!tab || !currentWindow) return;
    const pos = data?.windows.find((w) => w.index === toWindow)?.tab_indices.length ?? 0;
    try {
      data = await api.tabMove(tab.index, currentWindow.index, toWindow, pos);
      tabIndex = keepSelection(toWindow, pos);
      onUserDirty();
    }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function removeWindow() {
    if (!data || data.windows.length <= 1 || !currentWindow) return;
    const ok = await confirm(
      `Remove Overview ${currentWindow.index + 1}? Its tabs move to Overview 1.`,
      { title: "Remove overview window", kind: "warning" },
    );
    if (!ok) return;
    try {
      // Edits both slots (grouping + geometry) — mark both dirty so saveFile
      // doesn't skip the char slot.
      data = await api.overviewWindowRemove(currentWindow.index);
      tabIndex = data.tabs[0]?.index ?? null;
      onUserDirty();
      onCharDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }

  // Reordering or moving a tab RENUMBERS the tab table — EVE draws a window's
  // tabs in ascending tab index, so that is the only way an order reaches the
  // game. The selected tab's index therefore changes under us, and left alone
  // `tabIndex` would silently come to name a different tab. Re-point it by
  // POSITION instead: whatever the backend now lists where the tab landed is
  // that tab, without this file repeating the backend's renumbering arithmetic.
  function keepSelection(windowIdx: number, pos: number): number | null {
    const strip = data?.windows.find((w) => w.index === windowIdx)?.tab_indices ?? [];
    return strip[pos] ?? tabIndex;
  }

  // Drag-reorder of tabs within the current window (same pattern as the column list).
  let tabDragFrom = $state<number | null>(null);
  async function dropTab(to: number) {
    if (tabDragFrom === null || !currentWindow) { tabDragFrom = null; return; }
    const order = [...currentWindow.tab_indices];
    const [moved] = order.splice(tabDragFrom, 1);
    order.splice(to, 0, moved);
    const windowIdx = currentWindow.index;
    const selectedAt = tabIndex === null ? -1 : order.indexOf(tabIndex);
    tabDragFrom = null;
    try {
      data = await api.tabReorder(windowIdx, order);
      if (selectedAt >= 0) tabIndex = keepSelection(windowIdx, selectedAt);
      onUserDirty();
    }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }

  // Pack import/export is account-wide, so it lives in the view header rather
  // than inside one sub-tab. Import marks the slot dirty; the user still saves.
  let packBusy = $state(false);

  // EVE's own export lands in Documents/EVE/Overview, so start the picker there.
  // Best-effort: if the path can't be resolved the dialog just opens wherever it
  // last was.
  async function overviewFolder(): Promise<string | undefined> {
    try {
      return `${await documentDir()}EVE/Overview`;
    } catch {
      return undefined;
    }
  }

  async function importPack() {
    const picked = await openDialog({
      multiple: false,
      defaultPath: await overviewFolder(),
      filters: [{ name: "Overview pack", extensions: ["yaml", "yml"] }],
    });
    if (typeof picked !== "string") return;
    packBusy = true;
    try {
      const summary = await api.packPreview(picked);
      const what = summary.sections
        .map(([name, count]) => (count > 0 ? `${name} (${count})` : name))
        .join(", ");
      const ignored = summary.ignored.length
        ? `\n\nIgnored unknown sections: ${summary.ignored.join(", ")}`
        : "";
      // Per-tab column overrides are only ever stripped inside apply_tabs,
      // which only runs when the pack defines a non-empty tabSetup section —
      // a preset-only pack never touches them, so don't claim it does.
      const dropsColumns = summary.sections.some(([name, count]) => name === "tabSetup" && count > 0);
      const columnsNote = dropsColumns ? " Per-tab column overrides are discarded." : "";
      const ok = await confirm(
        `This pack contains: ${what}.\n\nEach of those replaces your account's current overview settings.${columnsNote}${ignored}`,
        { title: "Import overview pack", kind: "warning" },
      );
      if (!ok) return;
      const result = await api.packImport(picked);
      data = result.columns;
      // A pack can replace the tab set wholesale (or the account may have had
      // none at all); if the previously-selected tab no longer exists, fall
      // back to the first tab rather than leaving tabIndex dangling — same
      // rule deleteTab already follows.
      if (!data.tabs.some((t) => t.index === tabIndex)) tabIndex = data.tabs[0]?.index ?? null;
      onUserDirty();
      const warnings = result.report.warnings.length ? `\n\n${result.report.warnings.join("\n")}` : "";
      await message(`Pack imported. Save to write it to the account file.${warnings}`, { title: "Import overview pack" });
    } catch (e) {
      await message(errMessage(e), { title: "Import failed", kind: "error" });
    } finally {
      packBusy = false;
    }
  }

  async function exportPack() {
    const picked = await saveDialog({
      defaultPath: "overview.yaml",
      filters: [{ name: "Overview pack", extensions: ["yaml"] }],
    });
    if (typeof picked !== "string") return;
    packBusy = true;
    try {
      const report = await api.packExport(picked);
      const warnings = report.warnings.length ? `\n\n${report.warnings.join("\n")}` : "";
      await message(`Exported ${report.applied.length} section(s).${warnings}`, { title: "Export overview pack" });
    } catch (e) {
      await message(errMessage(e), { title: "Export failed", kind: "error" });
    } finally {
      packBusy = false;
    }
  }
</script>

<!-- Dismiss the palette on a click anywhere outside it. Tested by containment
     rather than by stopPropagation inside the popover, which would need a
     handler on a non-interactive element. -->
<svelte:window onpointerdown={(e) => {
  if (swatchOpen && swatchEl && !swatchEl.contains(e.target as Node)) swatchOpen = false;
}} />

{#if !userOpen && charId !== null}
  <!-- Same prompt AutofillView renders, and now with the same button
       treatment — which was §5.7's actual complaint about the pair. -->
  <div class="pair">
    <p>Link this character to an account to edit shared settings — overview columns live in the account file.</p>
    <Button onclick={onShowAccounts}>Pair…</Button>
  </div>
{:else if !userOpen}
  <EmptyState title="Open a character or account file to edit overview columns." />
{:else if error}
  <InlineMessage variant="error">{error}</InlineMessage>
{:else if data}
  <ScopeBanner label={sharedLabel ?? ""} />
  {#if data.tabs.length === 0}
    <EmptyState title="This account file has no overview tabs." />
  {:else}
    <div class="ov-controls">
      <!-- Plain text in the options: an <option> cannot carry the colour, and
           raw `<color=0x...>` in the dropdown is worse than neither. -->
      <Field kind="select" label="Tab" bind:value={tabIndex} options={tabOptions} />
      <div class="tab-actions">
        <Button onclick={startCreateTab} disabled={!data || data.tabs.length === 0}
                disabledReason="This account file has no overview tabs" title="New tab">+ New</Button>
        <Button onclick={startRenameTab} disabled={!tab} disabledReason="Pick a tab first"
                title="Rename selected tab">Rename</Button>
        <Button variant="danger" onclick={deleteTab} disabled={!tab} disabledReason="Pick a tab first"
                title="Delete selected tab">Delete</Button>
        <div class="swatch-wrap" bind:this={swatchEl}>
          <!-- aria-label as well as title: the swatch's only content is a dash
               or nothing at all, and the spec finds it by its label. -->
          <Button class="swatch" disabled={!tab} disabledReason="Pick a tab first" title="Tab name colour"
                  aria-label="Tab name colour"
                  style={nameParts?.color ? `background:${cssColor(nameParts.color)}` : ""}
                  onclick={() => (swatchOpen = !swatchOpen)}>{nameParts?.color ? "" : "—"}</Button>
          <!-- A Popover, so it clamps inside the viewport and closes on Escape.
               As a bare absolutely-positioned div it did neither, and near the
               right edge of the window it rendered partly offscreen. -->
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
                          onclick={() => { setNameFormat({ color: `FF${c.toUpperCase()}` }); swatchOpen = false; }}></button>
                {/each}
              </div>
              <Button variant="ghost" size="sm" class="palette-none"
                      onclick={() => { setNameFormat({ color: null }); swatchOpen = false; }}>No colour</Button>
            </Popover>
          {/if}
        </div>
        <Button class="bold-toggle" pressed={!!nameParts?.bold} disabled={!tab}
                disabledReason="Pick a tab first" title="Bold tab name"
                onclick={() => setNameFormat({ bold: !nameParts?.bold })}>B</Button>
        {#if currentWindow && data.windows.length > 1}
          {@const cw = currentWindow}
          <Field
            kind="select"
            aria-label="Move to window"
            bind:value={movePick}
            onchange={() => {
              const v = movePick;
              movePick = "";
              if (v) moveTab(Number(v));
            }}
            options={[
              { value: "", label: "Move to window…", disabled: true },
              ...data.windows
                .filter((w) => w.index !== cw.index)
                .map((w) => ({ value: String(w.index), label: `Overview ${w.index + 1}` })),
            ]} />
        {/if}
        {#if data.windows.length >= 1}
          <Button onclick={startAddWindow} title="Add a new overview window">+ Window</Button>
        {/if}
        {#if currentWindow && data.windows.length > 1 && currentWindow.index === data.windows.length - 1}
          <Button variant="danger" onclick={removeWindow}
                  title="Remove this (last) overview window">Remove Window</Button>
        {/if}
      </div>
      {#if data.windows.length === 0}
        <InlineMessage class="no-windows">
          Tabs aren't assigned to specific overview windows on this account — EVE spreads them
          across your windows itself. That's normal: importing an overview pack through the
          client removes the assignment.
          <Button size="sm" onclick={setUpWindowMapping}>Set up per-window tabs</Button>
        </InlineMessage>
      {/if}
      {#if pending}
        <div class="name-entry">
          <Field bind:value={pending.value} bind:element={nameInput}
                 ariaLabel={pending.kind === "addWindow" ? "First tab name" : "Tab name"}
                 placeholder={pending.kind === "addWindow" ? "First tab name" : "Tab name"}
                 onkeydown={(e: KeyboardEvent) => {
                   if (e.key === "Enter") { e.preventDefault(); submitPending(); }
                   else if (e.key === "Escape") pending = null;
                 }} />
          <Button variant="primary" onclick={submitPending}>
            {pending.kind === "addWindow" ? "Add window" : pending.kind === "renameTab" ? "Rename" : "Add tab"}
          </Button>
          <Button onclick={() => (pending = null)}>Cancel</Button>
        </div>
      {/if}
      <Field
        kind="select"
        label="Character (for widths)"
        value={charId ?? ""}
        onchange={(e) => onLoadCharacter(Number((e.target as HTMLSelectElement).value))}
        options={[
          { value: "", label: "Select…", disabled: true },
          ...characters.map((c) => ({ value: c, label: names[c]?.name ?? String(c) })),
        ]} />
    </div>
    {#if currentWindow && currentWindow.tab_indices.length > 1}
      {@const cw = currentWindow}
      <ul class="ov-tabs">
        {#each cw.tab_indices as idx, i (idx)}
          {@const t = data.tabs.find((x) => x.index === idx)}
          <li draggable="true" class:selected={idx === tabIndex}
              ondragstart={(e) => { tabDragFrom = i;
                // WebView2/Chromium won't fire `drop` unless dragstart sets data.
                e.dataTransfer?.setData("text/plain", String(i));
                if (e.dataTransfer) e.dataTransfer.effectAllowed = "move"; }}
              ondragover={(e) => { e.preventDefault();
                if (e.dataTransfer) e.dataTransfer.dropEffect = "move"; }}
              ondrop={(e) => { e.preventDefault(); dropTab(i); }}
              ondragend={() => (tabDragFrom = null)}>
            <span class="grip" title="Drag to reorder">⠿</span>
            <!-- The chips are the one place a tab's real colour and weight can
                 be shown, so they render it. -->
            <button type="button" class="tab-chip" style={t ? chipStyle(t.name) : ""}
                    onclick={() => (tabIndex = idx)}>{t ? plainTabName(t.name) : `Tab ${idx}`}</button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if characters.length === 0}
      <EmptyState title="No characters associated with this account yet — pair one in Accounts to edit widths." />
    {/if}
  {/if}

  <!-- The pack buttons sat INSIDE the tablist, carrying no role at all, which
       made it an invalid ARIA tree that svelte-check does not catch. They are
       beside it now; Phase 4 moves them properly. -->
  <div class="sub-row">
    {#if data.tabs.length > 0}
      <Tabs
        variant="underline"
        class="subtabs"
        ariaLabel="Overview section"
        tabs={[
          { id: "Columns", label: "Columns" },
          { id: "Filters", label: "Filters" },
          { id: "Appearance", label: "Appearance" },
        ]}
        bind:value={sub} />
    {/if}
    <span class="pack-actions">
      <Button onclick={importPack} disabled={packBusy} disabledReason="A pack command is in flight"
              title="Replace this account's overview from an EVE overview pack">Import pack…</Button>
      <Button onclick={exportPack} disabled={packBusy} disabledReason="A pack command is in flight"
              title="Write this account's overview out as an EVE overview pack">Export pack…</Button>
    </span>
  </div>
  {#if data.tabs.length > 0}
    <div hidden={sub !== "Columns"}>
      <OverviewColumnsTab {data} {tabIndex} {charOpen} onChanged={(next) => (data = next)} {onUserDirty} {onCharDirty} />
    </div>
    <div hidden={sub !== "Filters"}>
      <OverviewFiltersTab {data} {tabIndex} onChanged={(next) => (data = next)} {onUserDirty} />
    </div>
    <div hidden={sub !== "Appearance"}>
      <OverviewAppearanceTab {data} onChanged={(next) => (data = next)} {onUserDirty} />
    </div>
  {/if}
{/if}

<style>
  /* The dark-native-control block is gone — every select, option, optgroup and
     text input here is a Field now. */
  .pair { display: flex; align-items: center; gap: var(--s2); }
  .ov-controls { display: flex; gap: var(--s4); margin-bottom: var(--s2); align-items: center; flex-wrap: wrap; }
  .tab-actions { display: flex; gap: var(--s1); align-items: center; flex-wrap: wrap; }
  .name-entry { display: flex; gap: var(--s1); align-items: center; margin-bottom: var(--s2); }
  .name-entry :global(.field) { flex: 1; max-width: 16rem; }
  .name-entry :global(input) { width: 100%; }
  :global(.no-windows) { flex-basis: 100%; }
  /* A reorderable list with a colour swatch and a bold toggle per item, not a
     tab strip — Tabs is the wrong shape for it. Tokenised in place; Phase 4
     restructures it as a ListRow set. */
  .ov-tabs { list-style: none; padding: 0; margin: 0 0 var(--s2); display: flex; gap: var(--s1); flex-wrap: wrap; }
  .ov-tabs li {
    display: flex; align-items: center; gap: var(--s1); padding: 0 var(--s2);
    border: 1px solid var(--border); border-radius: var(--r-sm); cursor: pointer;
  }
  .ov-tabs li.selected { border-color: var(--accent); }
  .ov-tabs button.tab-chip { background: none; border: none; padding: 0; margin: 0; color: inherit; font: inherit; cursor: pointer; }
  .grip { cursor: grab; color: var(--text-muted); }
  /* Tab-name markup controls (see tabName.ts). */
  .swatch-wrap { position: relative; display: inline-flex; }
  .tab-actions :global(.swatch) { width: 1.9rem; }
  .tab-actions :global(.bold-toggle) { font-weight: 700; }
  :global(.palette) { display: block; }
  .palette-grid { display: grid; grid-template-columns: repeat(8, 1.1rem); gap: 2px; }
  /* --border-strong, not --border: this outline has to read against an
     arbitrary user colour on either side of it. */
  .palette-grid button {
    width: 1.1rem; height: 1.1rem; border: 1px solid var(--border-strong);
    border-radius: var(--r-sm); padding: 0; cursor: pointer;
  }
  .palette-grid button:hover { outline: 1px solid var(--text); }
  :global(.palette-none) { display: block; width: 100%; margin-top: var(--s1); }
  .sub-row { display: flex; align-items: flex-end; gap: var(--s2); margin: var(--s2) 0; }
  .sub-row :global(.subtabs) { flex: 1; }
  .pack-actions { margin-left: auto; display: flex; gap: var(--s1); }
</style>
