<script lang="ts">
  import { api, errMessage, type OverviewColumns } from "./api";
  import { message, confirm } from "@tauri-apps/plugin-dialog";
  import { names } from "./names.svelte";

  let { userOpen, charId, characters, refreshToken, onLoadCharacter, onUserDirty, onCharDirty, onWindowAdded, onShowAccounts }:
    { userOpen: boolean; charId: number | null; characters: number[]; refreshToken: number;
      onLoadCharacter: (id: number) => void; onUserDirty: () => void; onCharDirty: () => void;
      onWindowAdded: (windowId: string) => void; onShowAccounts: () => void } = $props();

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
  $effect(() => { void userOpen; void charId; void refreshToken; reload(); });

  const tab = $derived(data?.tabs.find((t) => t.index === tabIndex) ?? null);
  // The window strip whose tab_indices contains the selected tab (null for an
  // orphan tab that isn't listed under any window).
  const currentWindow = $derived(data?.windows.find((w) => w.tab_indices.includes(tabIndex ?? -1)) ?? null);
  const currentWindowIndex = $derived(currentWindow?.index ?? null);

  // Name entry is an inline input (see the markup below), NOT window.prompt —
  // which the WebView2 renders as an ugly "localhost:1420 says …" dialog. One
  // pending action drives all three name-entry flows.
  let pending = $state<
    | { kind: "createTab"; value: string }
    | { kind: "renameTab"; value: string; tabIdx: number }
    | { kind: "addWindow"; value: string }
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
      } else {
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
  <div class="hint">
    <p>This character isn't linked to an account yet. Overview columns live in the account
      file — associate it to edit them.</p>
    <button onclick={onShowAccounts}>Open Accounts</button>
  </div>
{:else if !userOpen}
  <p class="hint">Open a character or account file to edit overview columns.</p>
{:else if error}
  <p class="error">{error}</p>
{:else if data && data.tabs.length === 0}
  <p class="hint">This account file has no overview tabs.</p>
{:else if data}
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
    {#if pending}
      <div class="name-entry">
        <input type="text" bind:value={pending.value} use:focusInput
               placeholder={pending.kind === "addWindow" ? "First tab name" : "Tab name"}
               onkeydown={(e) => {
                 if (e.key === "Enter") { e.preventDefault(); submitPending(); }
                 else if (e.key === "Escape") pending = null;
               }} />
        <button onclick={submitPending}>
          {pending.kind === "addWindow" ? "Add window" : pending.kind === "renameTab" ? "Rename" : "Add tab"}
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
  .ov-controls { display: flex; gap: 1rem; margin-bottom: 0.5rem; align-items: center; }
  .ov-controls label { display: flex; gap: 0.4rem; align-items: center; }
  .tab-actions { display: flex; gap: 0.4rem; align-items: center; flex-wrap: wrap; }
  .name-entry { display: flex; gap: 0.4rem; align-items: center; margin-bottom: 0.5rem; }
  .name-entry input { flex: 1; max-width: 16rem; }
  button.danger { border-color: #a33; }
  /* Dark native controls: the app runs in a dark WebView2; give selects, their
     options, and inputs explicit dark colors (see the dark-native-controls memo). */
  select, option, optgroup, input.w, .name-entry input {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 4px; font: inherit;
  }
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
