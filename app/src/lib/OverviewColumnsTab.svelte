<script lang="ts">
  import { api, errMessage, type OverviewColumns } from "./api";
  import { message } from "@tauri-apps/plugin-dialog";

  let { data, tabIndex, charId, onChanged, onUserDirty, onCharDirty }:
    { data: OverviewColumns | null; tabIndex: number | null; charId: number | null;
      onChanged: (next: OverviewColumns) => void; onUserDirty: () => void; onCharDirty: () => void } = $props();

  const tab = $derived(data?.tabs.find((t) => t.index === tabIndex) ?? null);

  async function toggle(column: string, visible: boolean) {
    try { onChanged(await api.setOverviewVisible(tabIndex!, column, visible)); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function setWidth(column: string, raw: string) {
    const width = Number(raw);
    if (charId === null || raw.trim() === "" || Number.isNaN(width)) return;
    try { onChanged(await api.setOverviewWidth(tabIndex!, column, width)); onCharDirty(); }
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
    try { onChanged(await api.setOverviewOrder(tabIndex!, order)); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
</script>

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

<style>
  .ov-cols { list-style: none; padding: 0; }
  .ov-cols li { display: flex; align-items: center; gap: 0.5rem; padding: 0.15rem 0; }
  .grip { cursor: grab; opacity: 0.6; }
  /* Dark native controls: the app runs in a dark WebView2; give the width input
     explicit dark colors (see the dark-native-controls memo). */
  input.w {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 4px; font: inherit;
  }
  input.w { width: 5rem; }
  .meta { color: var(--fg-dim); font-size: 0.85em; }
</style>
