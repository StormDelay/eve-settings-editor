<script lang="ts">
  import { open as openDialog, save as saveDialog, confirm, message } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, type Aspect, type PresetInfo } from "./api";
  import { allPresets, loadPresets, setPresets, summarise } from "./presetLibrary.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

  let { onOpenPreset, charOpen, userOpen, openPresetName }: {
    onOpenPreset: (p: PresetInfo) => void;
    charOpen: boolean;
    userOpen: boolean;
    openPresetName: string | null;
  } = $props();

  loadPresets();

  const ASPECTS: { key: Aspect; label: string; needsUser: boolean }[] = [
    { key: "layout", label: "Window layout", needsUser: true },
    { key: "overview", label: "Overview", needsUser: true },
    { key: "autofill", label: "Autofill", needsUser: true },
    { key: "keybinds", label: "Keybindings", needsUser: true },
    { key: "everything", label: "Everything", needsUser: true },
  ];

  let creating = $state(false);
  let newName = $state("");
  let picked = $state<Set<Aspect>>(new Set(["layout"]));
  let busy = $state(false);
  const everything = $derived(picked.has("everything"));

  function toggle(a: Aspect) {
    const next = new Set(picked);
    next.has(a) ? next.delete(a) : next.add(a);
    picked = next;
  }

  async function run(fn: () => Promise<PresetInfo[] | void>, title: string) {
    busy = true;
    try {
      const next = await fn();
      if (next) setPresets(next);
    } catch (e) {
      await message(errMessage(e), { title, kind: "error" });
    } finally {
      busy = false;
    }
  }

  async function create() {
    const name = newName.trim();
    if (!name || picked.size === 0) return;
    // `presets::create` writes the files directly, bypassing the save chain,
    // so replacing the OPEN preset leaves its loaded_mtime stale and
    // dirtySlots never cleared -- the next Save would then raise a conflict
    // error blaming "the EVE client", which cannot be true here. Refuse up
    // front instead, same as Rename/Delete already do for the open preset.
    if (openPresetName !== null && name.toLowerCase() === openPresetName.toLowerCase()) {
      await message(`“${name}” is currently open — close it first, then save over it.`, {
        title: "Preset is open",
      });
      return;
    }
    // Case-insensitive: the filesystem (NTFS) doesn't distinguish "Mining" from
    // "MINING", so the confirm has to trigger on the same cases the backend
    // would otherwise refuse outright.
    const exists = allPresets().some((p) => p.name.toLowerCase() === name.toLowerCase());
    if (exists && !(await confirm(`Replace the existing preset “${name}”?`, { title: "Preset exists" })))
      return;
    await run(() => api.settingsPresetCreate(name, [...picked], exists), "Preset not created");
    creating = false;
    newName = "";
  }

  async function importPreset() {
    const path = await openDialog({ filters: [{ name: "Preset", extensions: ["evepreset"] }] });
    if (typeof path !== "string") return;
    busy = true;
    try {
      const result = await api.settingsPresetImport(path);
      setPresets(result.presets);
      // The backend only reports the name it landed on, not the one the file
      // asked for, so this can only state the fact, never guess whether it
      // was deduped.
      await message(`Imported as “${result.name}”.`, { title: "Imported", kind: "info" });
    } catch (e) {
      await message(errMessage(e), { title: "Import failed", kind: "error" });
    } finally {
      busy = false;
    }
  }

  async function exportPreset(p: PresetInfo) {
    const path = await saveDialog({
      defaultPath: `${p.name}.evepreset`,
      filters: [{ name: "Preset", extensions: ["evepreset"] }],
    });
    if (typeof path !== "string") return;
    if (p.full) {
      const ok = await confirm(
        "This preset is a complete copy of both settings files. It carries everything the editor does not model, including your autofill history — station names, searches and typed text. Share it anyway?",
        { title: "Share a full preset?" },
      );
      if (!ok) return;
    }
    await run(async () => { await api.settingsPresetExport(p.name, path); }, "Export failed");
  }

  // Rename uses an inline input rather than window.prompt, matching the pattern
  // the overview-window slice introduced.
  let renaming = $state<string | null>(null);
  let renameTo = $state("");
  async function commitRename() {
    const from = renaming;
    const to = renameTo.trim();
    renaming = null;
    if (!from || !to || to === from) return;
    await run(() => api.settingsPresetRename(from, to), "Rename failed");
  }

  async function remove(p: PresetInfo) {
    const ok = await confirm(`Delete the preset “${p.name}”? This cannot be undone.`, {
      title: "Delete preset",
    });
    if (!ok) return;
    await run(() => api.settingsPresetDelete(p.name), "Delete failed");
  }

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  function openMenu(e: MouseEvent, p: PresetInfo) {
    e.preventDefault();
    // Renaming or deleting the OPEN preset would move/remove the directory the
    // backend's open document still points at by path — the next Save would
    // target a path that no longer exists. Export is unaffected, so it stays
    // live; Rename/Delete stay in the menu (ContextMenu has no disabled-item
    // concept) but explain themselves instead of acting.
    const isOpen = openPresetName !== null && p.name.toLowerCase() === openPresetName.toLowerCase();
    const explainOpen = () =>
      void message(`“${p.name}” is currently open — close it first to rename or delete it.`, {
        title: "Preset is open",
      });
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        // The label carries the reason, so the restriction is visible before
        // the click rather than only after it — ContextMenu has no disabled
        // state, and a menu that silently loses rows depending on which preset
        // you right-click just reads as broken.
        { label: isOpen ? "Rename… (close first)" : "Rename…",
          run: isOpen ? explainOpen : () => { renaming = p.name; renameTo = p.name; } },
        { label: "Export…", run: () => void exportPreset(p) },
        { label: isOpen ? "Delete… (close first)" : "Delete…",
          run: isOpen ? explainOpen : () => void remove(p) },
      ],
    };
  }
</script>

<details open>
  <summary>Presets</summary>
  <div class="actions">
    <button onclick={() => (creating = !creating)} disabled={!charOpen && !userOpen}
      title={charOpen || userOpen ? "Save the open character's settings as a preset" : "Open a character first"}
      >New from open character…</button>
    <button onclick={importPreset} disabled={busy}>Import…</button>
  </div>

  {#if creating}
    <form class="new" onsubmit={(e) => { e.preventDefault(); void create(); }}>
      <input placeholder="Preset name" bind:value={newName} />
      {#each ASPECTS as a}
        <label
          class:disabled={(everything && a.key !== "everything") || (a.needsUser && !userOpen)}>
          <input type="checkbox" checked={picked.has(a.key)}
            disabled={(everything && a.key !== "everything") || (a.needsUser && !userOpen)}
            onchange={() => toggle(a.key)} />
          {a.label}
        </label>
      {/each}
      <!-- Sits under the Everything checkbox, which is last in ASPECTS. The
           export confirm guards SHARING a full preset; this guards choosing to
           capture one, which is the moment the history gets snapshotted. -->
      <p class="hint">Everything copies both settings files whole, including your autofill history — station names, searches and typed text.</p>
      <div class="actions">
        <button type="submit" disabled={busy || !newName.trim() || picked.size === 0}>Save</button>
        <button type="button" onclick={() => (creating = false)}>Cancel</button>
      </div>
    </form>
  {/if}

  {#if allPresets().length === 0}
    <p class="hint">No presets yet. Open a character and save one.</p>
  {:else}
    <ul>
      {#each allPresets() as p (p.dir)}
        <li>
          {#if renaming === p.name}
            <input bind:value={renameTo} onblur={commitRename}
              onkeydown={(e) => { if (e.key === "Enter") void commitRename(); if (e.key === "Escape") renaming = null; }} />
          {:else}
            <button class="file" onclick={() => onOpenPreset(p)} oncontextmenu={(e) => openMenu(e, p)}
              disabled={p.error !== null} title={p.error ?? p.dir}>
              {p.name}
              <span class="meta">{summarise(p)}</span>
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</details>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  /* Native controls render light in the dark WebView2 app unless told otherwise.
     Covers both the create-form name field and the inline rename field. */
  input:not([type]) {
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 3px 6px;
    font: inherit;
  }
  input[type="checkbox"] { accent-color: var(--accent); }
  .new { display: flex; flex-direction: column; gap: 0.25rem; padding: 0.35rem 0.1rem; }
  .new label { display: flex; align-items: center; gap: 0.4em; font-size: 0.9em; }
  .new label.disabled { opacity: 0.5; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; padding: 0.25rem 0; }
  .hint { opacity: 0.7; font-size: 0.85em; padding: 0.25rem 0.1rem; }
  .meta { color: var(--fg-dim); font-size: 0.85em; margin-left: 0.4em; }
</style>
