<script lang="ts">
  import { api, errMessage, type OverviewColumns } from "./api";
  import defaultPresetNames from "./data/default-preset-names.json";
  import overviewGroups from "./data/overview-groups.json";
  import { mergeCatalog, filterCatalog, toggleGroup, unknownGroups, type Category, type CatalogBundle } from "./groups";
  import { message, confirm } from "@tauri-apps/plugin-dialog";
  import { names } from "./names.svelte";

  let { userOpen, userId, charId, characters, refreshToken, onLoadCharacter, onUserDirty, onCharDirty, onWindowAdded, onShowAccounts, sharedLabel = "" }:
    { userOpen: boolean; userId: number | null; charId: number | null; characters: number[]; refreshToken: number;
      onLoadCharacter: (id: number) => void; onUserDirty: () => void; onCharDirty: () => void;
      onWindowAdded: (windowId: string) => void; onShowAccounts: () => void; sharedLabel?: string } = $props();

  let data = $state<OverviewColumns | null>(null);
  let tabIndex = $state<number | null>(null);
  let error = $state<string | null>(null);

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
  // The preset dropdown options: the sorted account presets, plus the tab's
  // current value if (defensively) it isn't among them. Empty "" shows as (default).
  const presetOptions = $derived.by(() => {
    const list = (data?.presets ?? []).map((p) => p.name);
    const cur = tab?.preset ?? "";
    return list.includes(cur) ? list : [cur, ...list];
  });
  // Preset-management actions operate on the selected tab's current preset; they
  // are meaningful only when that preset is a real (listed) account preset.
  const presetIsReal = $derived(!!tab && (data?.presets.some((p) => p.name === tab.preset) ?? false));

  // Preset-contents catalog: load once on mount (the backend server_version-gates
  // the ESI sync, so a repeat call is cheap), merging the bundled tree with any
  // synced additions; fall back to the bundle alone if the sync fails.
  let catalog = $state<Category[]>([]);
  $effect(() => {
    const b = overviewGroups as CatalogBundle;
    api
      .syncGroupCatalog(b.all_group_ids, b.categories.map((c) => c.id))
      .then((additions) => (catalog = mergeCatalog(b, additions)))
      .catch(() => (catalog = mergeCatalog(b, [])));
  });

  let groupFilter = $state("");
  const presetGroups = $derived(data?.presets.find((p) => p.name === tab?.preset)?.groups ?? []);
  const presetGroupSet = $derived(new Set(presetGroups));
  const visibleCategories = $derived(filterCatalog(catalog, groupFilter));
  const unknownIds = $derived(unknownGroups(catalog, presetGroups));

  async function setPresetGroup(id: number, on: boolean) {
    if (!tab) return;
    try { data = await api.presetSetGroups(tab.preset, toggleGroup(presetGroups, id, on)); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }

  // Display label for a preset. EVE's built-in presets are keyed
  // `DefaultPreset_<localizationId>` with no readable name in the file; map the id
  // to its en-US label from the bundled snapshot (see tools/gen-default-preset-names.py).
  // The raw key is still what every edit/API call uses — this only changes shown text.
  function labelFor(name: string): string {
    if (!name) return "(default)";
    const m = /^DefaultPreset_(\d+)$/.exec(name);
    const friendly = m ? (defaultPresetNames as Record<string, string>)[m[1]] : undefined;
    return friendly ?? name;
  }

  // Name entry is an inline input (see the markup below), NOT window.prompt —
  // which the WebView2 renders as an ugly "localhost:1420 says …" dialog. One
  // pending action drives all three name-entry flows.
  let pending = $state<
    | { kind: "createTab"; value: string }
    | { kind: "renameTab"; value: string; tabIdx: number }
    | { kind: "addWindow"; value: string }
    | { kind: "duplicatePreset"; value: string; from: string }
    | { kind: "renamePreset"; value: string; old: string }
    | null
  >(null);
  function focusInput(node: HTMLInputElement) { node.focus(); node.select(); }

  function startCreateTab() {
    if (!data || data.tabs.length === 0) return;
    pending = { kind: "createTab", value: "" };
  }
  function startRenameTab() {
    if (!tab) return;
    pending = { kind: "renameTab", value: tab.name, tabIdx: tab.index };
  }
  function startAddWindow() {
    if (!data || data.windows.length === 0) return;
    pending = { kind: "addWindow", value: "Overview" };
  }
  async function submitPending() {
    if (!pending) return;
    const p = pending;
    const name = p.value.trim();
    pending = null;
    if (!name) return;
    try {
      if (p.kind === "createTab") {
        // No overview windows: currentWindowIndex is null; window 0 is a sentinel
        // the backend ignores for a windowless account (it distributes by default).
        data = await api.tabCreate(currentWindowIndex ?? 0, name, tabIndex);
        tabIndex = Math.max(...data.tabs.map((t) => t.index));
        onUserDirty();
      } else if (p.kind === "renameTab") {
        if (name === data?.tabs.find((t) => t.index === p.tabIdx)?.name) return;
        data = await api.tabRename(p.tabIdx, name);
        onUserDirty();
      } else if (p.kind === "addWindow") {
        // Add window writes the user grouping AND the char-file geometry, so mark
        // BOTH slots dirty — otherwise saveFile skips the char slot and the new
        // window's position never persists. Then hand the new window's id up so
        // the Layout editor selects it: it defaults offset on top of window 0, so
        // without selecting it it's easy to miss.
        data = await api.overviewWindowAdd(name, tabIndex);
        tabIndex = Math.max(...data.tabs.map((t) => t.index));
        onUserDirty();
        onCharDirty();
        const w = data.windows[data.windows.length - 1];
        if (w) onWindowAdded(w.index === 0 ? "overview" : `overview_${w.index}`);
      } else if (p.kind === "duplicatePreset") {
        data = await api.presetCreate(p.from, name);
        onUserDirty();
      } else if (p.kind === "renamePreset") {
        // Compare against the shown label: the rename box is prefilled with
        // labelFor(old), so an unedited submit on a DefaultPreset_<id> (label
        // "Carriers") must be a no-op, not a rename of the raw key to "Carriers".
        if (name === labelFor(p.old)) return;
        data = await api.presetRename(p.old, name);
        onUserDirty();
      }
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function deleteTab() {
    if (!tab) return;
    const ok = await confirm(`Delete tab "${tab.name}"? This can't be undone.`, { title: "Delete tab", kind: "warning" });
    if (!ok) return;
    try {
      const result = await api.tabDelete(tab.index);
      data = result;
      tabIndex = result.tabs[0]?.index ?? null;
      onUserDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function setTabPreset(preset: string) {
    if (!tab || preset === tab.preset) return;
    try { data = await api.tabSetPreset(tab.index, preset); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  function startDuplicatePreset() {
    if (!tab) return;
    pending = { kind: "duplicatePreset", value: `${labelFor(tab.preset)} copy`, from: tab.preset };
  }
  function startRenamePreset() {
    if (!tab) return;
    pending = { kind: "renamePreset", value: labelFor(tab.preset), old: tab.preset };
  }
  async function deletePreset() {
    if (!tab || !data) return;
    const name = tab.preset;
    const list = data.presets.map((p) => p.name);
    const pos = list.indexOf(name);
    if (pos < 0 || list.length <= 1) return;
    const neighbour = pos > 0 ? list[pos - 1] : list[pos + 1];
    const ok = await confirm(
      `Delete preset "${labelFor(name)}"? Tabs using it will move to "${labelFor(neighbour)}".`,
      { title: "Delete preset", kind: "warning" },
    );
    if (!ok) return;
    try { data = await api.presetDelete(name); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function moveTab(toWindow: number) {
    if (!tab || !currentWindow) return;
    const pos = data?.windows.find((w) => w.index === toWindow)?.tab_indices.length ?? 0;
    try { data = await api.tabMove(tab.index, currentWindow.index, toWindow, pos); onUserDirty(); }
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

  // Drag-reorder of tabs within the current window (same pattern as the column list below).
  let tabDragFrom = $state<number | null>(null);
  async function dropTab(to: number) {
    if (tabDragFrom === null || !currentWindow) { tabDragFrom = null; return; }
    const order = [...currentWindow.tab_indices];
    const [moved] = order.splice(tabDragFrom, 1);
    order.splice(to, 0, moved);
    const windowIdx = currentWindow.index;
    tabDragFrom = null;
    try { data = await api.tabReorder(windowIdx, order); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }

  async function toggle(column: string, visible: boolean) {
    try { data = await api.setOverviewVisible(tabIndex!, column, visible); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function setWidth(column: string, raw: string) {
    const width = Number(raw);
    if (charId === null || raw.trim() === "" || Number.isNaN(width)) return;
    try { data = await api.setOverviewWidth(tabIndex!, column, width); onCharDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }

  // Drag-reorder: track the dragged row index, drop reorders the token list.
  let dragFrom = $state<number | null>(null);
  async function drop(to: number) {
    if (dragFrom === null || !tab) return;
    const order = tab.columns.map((c) => c.name);
    const [moved] = order.splice(dragFrom, 1);
    order.splice(to, 0, moved);
    dragFrom = null;
    try { data = await api.setOverviewOrder(tabIndex!, order); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
</script>

{#if !userOpen && charId !== null}
  <div class="hint pair">
    <p>Link this character to an account to edit shared settings — overview columns live in the account file.</p>
    <button onclick={onShowAccounts}>Pair…</button>
  </div>
{:else if !userOpen}
  <p class="hint">Open a character or account file to edit overview columns.</p>
{:else if error}
  <p class="error">{error}</p>
{:else if data && data.tabs.length === 0}
  <p class="hint">This account file has no overview tabs.</p>
{:else if data}
  {#if sharedLabel}<p class="shared-banner">{sharedLabel}</p>{/if}
  <div class="ov-controls">
    <label>Tab
      <select bind:value={tabIndex}>
        {#if data.windows.length > 0}
          {@const grouped = new Set(data.windows.flatMap((w) => w.tab_indices))}
          {@const orphans = data.tabs.filter((t) => !grouped.has(t.index))}
          {#each data.windows as w (w.index)}
            <optgroup label="Overview {w.index + 1}">
              {#each w.tab_indices as idx (idx)}
                <option value={idx}>{data.tabs.find((t) => t.index === idx)?.name ?? `Tab ${idx}`}</option>
              {/each}
            </optgroup>
          {/each}
          {#if orphans.length > 0}
            <optgroup label="Other">
              {#each orphans as t (t.index)}<option value={t.index}>{t.name}</option>{/each}
            </optgroup>
          {/if}
        {:else}
          {#each data.tabs as t (t.index)}<option value={t.index}>{t.name}</option>{/each}
        {/if}
      </select>
    </label>
    <div class="tab-actions">
      <button onclick={startCreateTab} disabled={!data || data.tabs.length === 0} title="New tab">+ New</button>
      <button onclick={startRenameTab} disabled={!tab} title="Rename selected tab">Rename</button>
      <button class="danger" onclick={deleteTab} disabled={!tab} title="Delete selected tab">Delete</button>
      {#if currentWindow && data.windows.length > 1}
        {@const cw = currentWindow}
        <select aria-label="Move to window" value=""
                onchange={(e) => {
                  const el = e.currentTarget as HTMLSelectElement;
                  const v = el.value;
                  el.value = "";
                  if (v) moveTab(Number(v));
                }}>
          <option value="" disabled>Move to window…</option>
          {#each data.windows as w (w.index)}
            {#if w.index !== cw.index}
              <option value={w.index}>Overview {w.index + 1}</option>
            {/if}
          {/each}
        </select>
      {/if}
      {#if data.windows.length >= 1}
        <button onclick={startAddWindow} title="Add a new overview window">+ Window</button>
      {/if}
      {#if currentWindow && data.windows.length > 1 && currentWindow.index === data.windows.length - 1}
        <button class="danger" onclick={removeWindow} title="Remove this (last) overview window">Remove Window</button>
      {/if}
    </div>
    {#if tab}
      <label>Preset
        <select value={tab.preset} onchange={(e) => setTabPreset((e.currentTarget as HTMLSelectElement).value)}>
          {#each presetOptions as p (p)}<option value={p}>{labelFor(p)}</option>{/each}
        </select>
      </label>
      <div class="preset-actions">
        <button onclick={startDuplicatePreset} disabled={!presetIsReal} title="Duplicate this preset">Duplicate preset</button>
        <button onclick={startRenamePreset} disabled={!presetIsReal} title="Rename this preset">Rename preset</button>
        <button class="danger" onclick={deletePreset}
                disabled={!presetIsReal || (data?.presets.length ?? 0) <= 1}
                title="Delete this preset">Delete preset</button>
      </div>
    {/if}
    {#if presetIsReal && tab}
      <div class="preset-contents">
        <div class="contents-head">
          <span class="contents-title">Shows: {labelFor(tab.preset)}</span>
          <input class="group-filter" type="text" placeholder="Filter groups…" bind:value={groupFilter} />
        </div>

        {#if unknownIds.length}
          <div class="unknown-groups">
            Unrecognized groups (not in the catalog):
            {#each unknownIds as id}
              <label><input type="checkbox" checked onchange={() => setPresetGroup(id, false)} /> #{id}</label>
            {/each}
          </div>
        {/if}

        {#each visibleCategories as cat (cat.id)}
          <details class="group-cat" open={!!groupFilter.trim()}>
            <summary>{cat.name}</summary>
            <div class="group-grid">
              {#each cat.groups as g (g.id)}
                <label class="group-item">
                  <input type="checkbox" checked={presetGroupSet.has(g.id)}
                         onchange={(e) => setPresetGroup(g.id, (e.currentTarget as HTMLInputElement).checked)} />
                  {g.name}
                </label>
              {/each}
            </div>
          </details>
        {/each}
      </div>
    {/if}
    {#if pending}
      <div class="name-entry">
        <input type="text" bind:value={pending.value} use:focusInput
               placeholder={pending.kind === "addWindow" ? "First tab name"
                 : pending.kind === "duplicatePreset" || pending.kind === "renamePreset" ? "Preset name"
                 : "Tab name"}
               onkeydown={(e) => {
                 if (e.key === "Enter") { e.preventDefault(); submitPending(); }
                 else if (e.key === "Escape") pending = null;
               }} />
        <button onclick={submitPending}>
          {pending.kind === "addWindow" ? "Add window"
            : pending.kind === "renameTab" ? "Rename"
            : pending.kind === "duplicatePreset" ? "Duplicate"
            : pending.kind === "renamePreset" ? "Rename preset"
            : "Add tab"}
        </button>
        <button onclick={() => (pending = null)}>Cancel</button>
      </div>
    {/if}
    <label>Character (for widths)
      <select value={charId ?? ""} onchange={(e) => onLoadCharacter(Number((e.target as HTMLSelectElement).value))}>
        <option value="" disabled>Select…</option>
        {#each characters as c (c)}<option value={c}>{names[c]?.name ?? c}</option>{/each}
      </select>
    </label>
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
          <button type="button" class="tab-chip" onclick={() => (tabIndex = idx)}>{t?.name ?? `Tab ${idx}`}</button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if characters.length === 0}
    <p class="hint">No characters associated with this account yet — pair one in Accounts to edit widths.</p>
  {/if}
  {#if tab}
    <ul class="ov-cols">
      {#each tab.columns as col, i (col.name)}
        <li draggable="true"
            ondragstart={(e) => { dragFrom = i;
              // WebView2/Chromium won't fire `drop` unless dragstart sets data.
              e.dataTransfer?.setData("text/plain", String(i));
              if (e.dataTransfer) e.dataTransfer.effectAllowed = "move"; }}
            ondragover={(e) => { e.preventDefault();
              if (e.dataTransfer) e.dataTransfer.dropEffect = "move"; }}
            ondrop={(e) => { e.preventDefault(); drop(i); }}
            ondragend={() => (dragFrom = null)}>
          <span class="grip" title="Drag to reorder">⠿</span>
          <label title={col.name}>
            <input type="checkbox" checked={col.visible} onchange={(e) => toggle(col.name, (e.target as HTMLInputElement).checked)} />
            {col.label}
          </label>
          <input class="w" type="number" min="0" disabled={charId === null}
                 value={col.width ?? ""} placeholder="—"
                 onchange={(e) => setWidth(col.name, (e.target as HTMLInputElement).value)} />
        </li>
      {/each}
    </ul>
    {#if tab.inherits}<p class="meta">This tab uses the account-default columns. EVE doesn't save an
      inheriting tab's exact column order, so the order shown here is the account default — editing
      gives the tab its own copy.</p>{/if}
  {/if}
{/if}

<style>
  .shared-banner {
    margin: 0 0 0.6rem; padding: 0.3rem 0.5rem; font-size: 0.85em;
    color: var(--fg-dim); border-left: 2px solid var(--accent); background: var(--bg-panel);
  }
  .pair { display: flex; align-items: center; gap: 0.6rem; }
  .pair button {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 10px; font: inherit; cursor: pointer;
  }
  .ov-controls { display: flex; gap: 1rem; margin-bottom: 0.5rem; align-items: center; flex-wrap: wrap; }
  .ov-controls label { display: flex; gap: 0.4rem; align-items: center; }
  .tab-actions { display: flex; gap: 0.4rem; align-items: center; flex-wrap: wrap; }
  .preset-actions { display: flex; gap: 0.4rem; align-items: center; flex-wrap: wrap; }
  .name-entry { display: flex; gap: 0.4rem; align-items: center; margin-bottom: 0.5rem; }
  .name-entry input { flex: 1; max-width: 16rem; }
  button.danger { border-color: #a33; }
  /* Dark native controls: the app runs in a dark WebView2; give selects, their
     options, and inputs explicit dark colors (see the dark-native-controls memo). */
  select, option, optgroup, input.w, .name-entry input, .group-filter {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 4px; font: inherit;
  }
  /* Full-width so the box below can size a real column grid — it's a flex item
     inside the wrapping .ov-controls row otherwise. */
  .preset-contents { flex-basis: 100%; margin-top: 0.6rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .contents-head { display: flex; gap: 0.6rem; align-items: center; flex-wrap: wrap; }
  .contents-title { font-weight: 600; }
  .group-cat > summary { cursor: pointer; padding: 0.2rem 0; }
  .group-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr)); gap: 0.15rem 0.8rem; padding: 0.2rem 0 0.4rem 1rem; }
  .group-item { display: flex; gap: 0.35rem; align-items: center; }
  .preset-contents input[type="checkbox"] { accent-color: var(--accent); }
  .unknown-groups { display: flex; gap: 0.6rem; flex-wrap: wrap; align-items: center; color: var(--warn); }
  .ov-cols { list-style: none; padding: 0; }
  .ov-cols li { display: flex; align-items: center; gap: 0.5rem; padding: 0.15rem 0; }
  .ov-tabs { list-style: none; padding: 0; margin: 0 0 0.6rem; display: flex; gap: 0.3rem; flex-wrap: wrap; }
  .ov-tabs li {
    display: flex; align-items: center; gap: 0.3rem; padding: 0.15rem 0.5rem;
    border: 1px solid var(--border); border-radius: 3px; cursor: pointer;
  }
  .ov-tabs li.selected { border-color: var(--accent); }
  .ov-tabs button.tab-chip { background: none; border: none; padding: 0; margin: 0; color: inherit; font: inherit; cursor: pointer; }
  .grip { cursor: grab; opacity: 0.6; }
  input.w { width: 5rem; }
  .meta { color: var(--fg-dim); font-size: 0.85em; }
</style>
