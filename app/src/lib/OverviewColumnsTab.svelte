<script lang="ts">
  import { api, errMessage, type OverviewColumns, type OverviewTab } from "./api";
  import { plainTabName } from "./tabName";
  import { message } from "@tauri-apps/plugin-dialog";

  let { data, tabIndex, charOpen, onChanged, onUserDirty, onCharDirty }:
    { data: OverviewColumns | null; tabIndex: number | null; charOpen: boolean;
      onChanged: (next: OverviewColumns) => void; onUserDirty: () => void; onCharDirty: () => void } = $props();

  const tab = $derived(data?.tabs.find((t) => t.index === tabIndex) ?? null);

  async function toggle(column: string, visible: boolean) {
    try { onChanged(await api.setOverviewVisible(tabIndex!, column, visible)); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function setWidth(column: string, raw: string) {
    const width = Number(raw);
    if (!charOpen || raw.trim() === "" || Number.isNaN(width)) return;
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

  // Copy this tab's columns onto others. The panel is inline rather than a
  // modal — the app has no modal, and OverviewView's name-entry rows set the
  // precedent. Ticking the targets by hand IS the confirmation step, so there
  // is no second confirm dialog on top of it.
  let copyOpen = $state(false);
  let picked = $state<Record<number, boolean>>({});
  // Copying a PART of a layout is the unusual ask, so all three start on;
  // targets start empty, because this overwrites whatever the target had.
  let parts = $state({ order: true, visible: true, widths: true });

  // Same window grouping the tab selector shows, minus the source tab itself.
  const targetGroups = $derived.by(() => {
    const others = (data?.tabs ?? []).filter((t) => t.index !== tabIndex);
    const windows = data?.windows ?? [];
    if (windows.length === 0) return [{ label: "", tabs: others }];
    const byIndex = new Map(others.map((t) => [t.index, t]));
    const grouped = new Set(windows.flatMap((w) => w.tab_indices));
    const groups = windows
      .map((w) => ({
        label: `Overview ${w.index + 1}`,
        tabs: w.tab_indices.map((i) => byIndex.get(i)).filter((t): t is OverviewTab => !!t),
      }))
      .filter((g) => g.tabs.length > 0);
    const orphans = others.filter((t) => !grouped.has(t.index));
    if (orphans.length > 0) groups.push({ label: "Other", tabs: orphans });
    return groups;
  });

  const chosen = $derived(
    targetGroups.flatMap((g) => g.tabs).filter((t) => picked[t.index]).map((t) => t.index),
  );
  // Widths live in the character file, so there is nothing to copy without one.
  const copyWidths = $derived(parts.widths && charOpen);

  function openCopy() {
    picked = {};
    copyOpen = true;
  }
  function pickAll(on: boolean) {
    picked = on ? Object.fromEntries(targetGroups.flatMap((g) => g.tabs).map((t) => [t.index, true])) : {};
  }
  async function runCopy() {
    if (chosen.length === 0) return;
    try {
      onChanged(await api.overviewCopyColumns(tabIndex!, chosen, parts.order, parts.visible, copyWidths));
      // Two files, two dirty flags — miss the char one and the copied widths
      // are dropped on save.
      if (parts.order || parts.visible) onUserDirty();
      if (copyWidths) onCharDirty();
      copyOpen = false;
    } catch (e) { await message(errMessage(e), { title: "Copy failed", kind: "error" }); }
  }
</script>

{#if tab}
  <div class="col-actions">
    <button onclick={openCopy} disabled={copyOpen || (data?.tabs.length ?? 0) < 2}
            title="Copy this tab's column settings onto other tabs">Copy columns…</button>
  </div>
  {#if copyOpen}
    <div class="copy-panel">
      <div class="copy-head">
        <span>Copy <strong>{plainTabName(tab.name)}</strong>'s columns to:</span>
        <button onclick={() => pickAll(true)}>Select all</button>
        <button onclick={() => pickAll(false)}>None</button>
      </div>
      <div class="copy-targets">
        {#each targetGroups as g (g.label)}
          {#if g.label}<span class="copy-group">{g.label}</span>{/if}
          {#each g.tabs as t (t.index)}
            <label><input type="checkbox" bind:checked={picked[t.index]} /> {plainTabName(t.name)}</label>
          {/each}
        {/each}
      </div>
      <div class="copy-parts">
        <label><input type="checkbox" bind:checked={parts.order} /> Column order</label>
        <label><input type="checkbox" bind:checked={parts.visible} /> Visible columns</label>
        <label title={charOpen ? "" : "Widths are per character — open one to copy them"}>
          <input type="checkbox" checked={copyWidths} disabled={!charOpen}
                 onchange={(e) => (parts.widths = (e.currentTarget as HTMLInputElement).checked)} />
          Widths{charOpen ? "" : " (no character open)"}
        </label>
      </div>
      <div class="copy-actions">
        <button onclick={runCopy} disabled={chosen.length === 0 || !(parts.order || parts.visible || copyWidths)}>
          Copy to {chosen.length} tab{chosen.length === 1 ? "" : "s"}
        </button>
        <button onclick={() => (copyOpen = false)}>Cancel</button>
      </div>
    </div>
  {/if}
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
        <input class="w" type="number" min="0" disabled={!charOpen}
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
  .col-actions { display: flex; gap: 0.4rem; margin-bottom: 0.4rem; }
  .col-actions button, .copy-panel button {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 4px; padding: 4px 10px; font: inherit; cursor: pointer;
  }
  .copy-panel {
    display: flex; flex-direction: column; gap: 0.5rem;
    margin-bottom: 0.6rem; padding: 0.5rem;
    border: 1px solid var(--border); border-radius: 4px; background: var(--bg-panel);
  }
  .copy-head { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; }
  .copy-head button { padding: 1px 8px; font-size: 0.85em; }
  .copy-targets { display: grid; grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr)); gap: 0.15rem 0.8rem; }
  /* A window heading owns its own full row above the tabs it groups. */
  .copy-group { grid-column: 1 / -1; color: var(--fg-dim); font-size: 0.85em; margin-top: 0.2rem; }
  .copy-parts { display: flex; gap: 1rem; flex-wrap: wrap; }
  .copy-targets label, .copy-parts label { display: flex; gap: 0.35rem; align-items: center; }
  .copy-parts label:has(input:disabled) { color: var(--fg-dim); }
  .copy-actions { display: flex; gap: 0.4rem; }
  .copy-panel input[type="checkbox"] { accent-color: var(--accent); }
</style>
