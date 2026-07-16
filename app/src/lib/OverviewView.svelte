<script lang="ts">
  import { api, errMessage, type OverviewColumns } from "./api";
  import { message } from "@tauri-apps/plugin-dialog";
  import { names } from "./names.svelte";

  let { userOpen, charId, characters, onLoadCharacter, onUserDirty, onCharDirty }:
    { userOpen: boolean; charId: number | null; characters: number[];
      onLoadCharacter: (id: number) => void; onUserDirty: () => void; onCharDirty: () => void } = $props();

  let data = $state<OverviewColumns | null>(null);
  let tabIndex = $state<number | null>(null);
  let error = $state<string | null>(null);

  async function reload() {
    if (!userOpen) { data = null; return; }
    try {
      data = await api.overviewColumns();
      if (tabIndex === null && data.tabs.length > 0) tabIndex = data.tabs[0].index;
    } catch (e) { error = errMessage(e); }
  }
  $effect(() => { void userOpen; void charId; reload(); });

  const tab = $derived(data?.tabs.find((t) => t.index === tabIndex) ?? null);

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

{#if !userOpen}
  <p class="hint">Open an account (core_user) file to edit overview columns.</p>
{:else if error}
  <p class="error">{error}</p>
{:else if data && data.tabs.length === 0}
  <p class="hint">This account file has no overview tabs.</p>
{:else if data}
  <div class="ov-controls">
    <label>Tab
      <select bind:value={tabIndex}>
        {#each data.tabs as t (t.index)}<option value={t.index}>{t.name}</option>{/each}
      </select>
    </label>
    <label>Character (for widths)
      <select value={charId ?? ""} onchange={(e) => onLoadCharacter(Number((e.target as HTMLSelectElement).value))}>
        <option value="" disabled>Select…</option>
        {#each characters as c (c)}<option value={c}>{names[c]?.name ?? c}</option>{/each}
      </select>
    </label>
  </div>
  {#if characters.length === 0}
    <p class="hint">No characters associated with this account yet — pair one in Accounts to edit widths.</p>
  {/if}
  {#if tab}
    <ul class="ov-cols">
      {#each tab.columns as col, i (col.name)}
        <li draggable="true"
            ondragstart={() => (dragFrom = i)}
            ondragover={(e) => e.preventDefault()}
            ondrop={() => drop(i)}
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
    {#if tab.inherits}<p class="meta">This tab inherits the account default columns; editing it will give it its own copy.</p>{/if}
  {/if}
{/if}

<style>
  .ov-controls { display: flex; gap: 1rem; margin-bottom: 0.5rem; }
  .ov-controls label { display: flex; gap: 0.4rem; align-items: center; }
  /* Dark native controls: the app runs in a dark WebView2; give selects, their
     options, and inputs explicit dark colors (see the dark-native-controls memo). */
  select, option, input.w {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 4px; font: inherit;
  }
  .ov-cols { list-style: none; padding: 0; }
  .ov-cols li { display: flex; align-items: center; gap: 0.5rem; padding: 0.15rem 0; }
  .grip { cursor: grab; opacity: 0.6; }
  input.w { width: 5rem; }
  .meta { color: var(--fg-dim); font-size: 0.85em; }
</style>
