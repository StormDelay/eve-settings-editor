<script lang="ts">
  import { open as openDialog, save as saveDialog, confirm, message } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, type Aspect, type PresetInfo } from "./api";
  import { allPresets, loadPresets, setPresets, summarise } from "./presetLibrary.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";

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
    { key: "probe_formations", label: "Probe formations", needsUser: true },
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
    <Button onclick={() => (creating = !creating)} disabled={!charOpen && !userOpen}
      disabledReason="Open a character first"
      title={charOpen || userOpen ? "Save the open character's settings as a preset" : "Open a character first"}
      >New from open character…</Button>
    <Button onclick={importPreset} disabled={busy} disabledReason="A preset command is in flight">
      Import…
    </Button>
  </div>

  {#if creating}
    <form class="new" onsubmit={(e) => { e.preventDefault(); void create(); }}>
      <Field placeholder="Preset name" ariaLabel="Preset name" bind:value={newName} />
      {#each ASPECTS as a}
        {@const off = (everything && a.key !== "everything") || (a.needsUser && !userOpen)}
        <Field
          kind="checkbox"
          class="aspect"
          label={a.label}
          value={picked.has(a.key)}
          disabled={off}
          disabledReason={everything && a.key !== "everything"
            ? "Everything already covers this"
            : "Open an account file first"}
          onchange={() => toggle(a.key)} />
      {/each}
      <!-- Sits under the Everything checkbox, which is last in ASPECTS. The
           export confirm guards SHARING a full preset; this guards choosing to
           capture one, which is the moment the history gets snapshotted. -->
      <InlineMessage class="everything-note">
        Everything copies both settings files whole, including your autofill history — station names,
        searches and typed text.
      </InlineMessage>
      <div class="actions">
        <Button
          variant="primary"
          type="submit"
          disabled={busy || !newName.trim() || picked.size === 0}
          disabledReason={!newName.trim() ? "Name the preset first" : "Pick at least one thing to copy"}
          >Save</Button>
        <Button type="button" onclick={() => (creating = false)}>Cancel</Button>
      </div>
    </form>
  {/if}

  {#if allPresets().length === 0}
    <EmptyState title="No presets yet." description="Open a character and save one." />
  {:else}
    <ul>
      {#each allPresets() as p (p.dir)}
        <li>
          {#if renaming === p.name}
            <Field
              class="rename"
              ariaLabel="Rename preset"
              bind:value={renameTo}
              onblur={commitRename}
              onkeydown={(e: KeyboardEvent) => {
                if (e.key === "Enter") void commitRename();
                if (e.key === "Escape") renaming = null; }} />
          {:else}
            <!-- Right-click only, deliberately. ListRow's `actions` would put a
                 visible "⋯" here, and adding a control is a behaviour change —
                 that is Phase 4's job, and it is a one-argument change then. -->
            <ListRow
              onclick={() => onOpenPreset(p)}
              oncontextmenu={(e) => openMenu(e, p)}
              disabled={p.error !== null}
              disabledReason={p.error ?? undefined}
              title={p.error ?? p.dir}>
              {p.name}
              <span class="meta">{summarise(p)}</span>
            </ListRow>
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
  /* The two "give the native control explicit dark colours" rules are gone —
     Field owns that now, in one place. */
  .new { display: flex; flex-direction: column; gap: var(--s1); padding: var(--s1) 0; }
  .new :global(.aspect label) { font-size: var(--t-body); }
  .actions { display: flex; gap: var(--s2); flex-wrap: wrap; padding: var(--s1) 0; }
  .new :global(.everything-note) { margin: var(--s1) 0; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { list-style: none; }
  .meta { color: var(--text-muted); font-size: var(--t-caption); margin-left: var(--s1); }
</style>
