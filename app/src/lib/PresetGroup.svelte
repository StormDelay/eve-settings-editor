<script lang="ts">
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, errText, type Aspect, type PresetInfo } from "./api";
  import { allPresets, loadPresets, setPresets, summarise } from "./presetLibrary.svelte";
  import { ASPECT_LABELS } from "./aspects";
  import { confirmDialog } from "./ui/confirm.svelte";
  import { toast } from "./ui/toasts.svelte";
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

  const ASPECTS: { key: Aspect; label: string; needsUser: boolean }[] = ASPECT_LABELS.map((a) => ({
    ...a,
    needsUser: true,
  }));

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

  // One message slot for the whole group, four sentences. `run` took a dialog
  // TITLE ("Preset not created", "Rename failed"); it takes the noun now, and
  // the sentence is built to one grammar instead of four titles being invented
  // to two.
  let error = $state<{ text: string; detail: string } | null>(null);

  async function run(fn: () => Promise<PresetInfo[] | void>, verbed: string) {
    busy = true;
    error = null;
    try {
      const next = await fn();
      if (next) setPresets(next);
      return true;
    } catch (e) {
      error = { text: `The preset wasn't ${verbed} — ${errText(e)}`, detail: errMessage(e) };
      return false;
    } finally {
      busy = false;
    }
  }

  // Case-insensitive: the filesystem (NTFS) doesn't distinguish "Mining" from
  // "MINING", so these have to fire on the same cases the backend would
  // otherwise refuse outright.
  const sameName = (a: string, b: string | null) => b !== null && a.toLowerCase() === b.toLowerCase();

  // Both of these were dialogs raised AFTER the submit. The app knows each of
  // them the moment the name is typed, so it says so then — which is the rule
  // (say what an action costs before it is taken, in the control's own words)
  // applied literally. `nameWarning` also disables submit; `collides` only
  // relabels it, because replacing on purpose is a legitimate thing to do.
  //
  // `presets::create` writes the files directly, bypassing the save chain, so
  // replacing the OPEN preset would leave its loaded_mtime stale and dirtySlots
  // never cleared — the next Save would raise a conflict error blaming "the EVE
  // client", which cannot be true here.
  const nameOpen = $derived(sameName(newName.trim(), openPresetName));
  const collides = $derived(
    !nameOpen && allPresets().some((p) => sameName(newName.trim(), p.name)),
  );

  async function create() {
    const name = newName.trim();
    if (!name || picked.size === 0 || nameOpen) return;
    if (!(await run(() => api.settingsPresetCreate(name, [...picked], collides), "created"))) return;
    creating = false;
    newName = "";
  }

  async function importPreset() {
    const path = await openDialog({ filters: [{ name: "Preset", extensions: ["evepreset"] }] });
    if (typeof path !== "string") return;
    busy = true;
    error = null;
    try {
      const result = await api.settingsPresetImport(path);
      setPresets(result.presets);
      // The backend only reports the name it landed on, not the one the file
      // asked for, so this can only state the fact, never guess whether it
      // was deduped.
      toast(`Imported as “${result.name}”.`, { variant: "success" });
    } catch (e) {
      error = { text: `The preset wasn't imported — ${errText(e)}`, detail: errMessage(e) };
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
    // Survivor 6, and the only one of the six that guards a privacy disclosure
    // rather than a data loss. A `full` preset carries the account's autofill
    // history — station names, searches, anything typed into a box — and once
    // the file is shared it cannot be unshared. Nothing in the app can walk
    // that back, which is what a confirmation is for.
    if (p.full) {
      const ok = await confirmDialog({
        title: "This preset carries your typing history",
        body:
          "A full preset copies both settings files whole, including autofill — " +
          "station names, searches and anything you've typed into a box.",
        confirm: "Export anyway",
      });
      if (!ok) return;
    }
    if (await run(async () => { await api.settingsPresetExport(p.name, path); }, "exported")) {
      toast(`Exported “${p.name}”.`, { variant: "success" });
    }
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
    await run(() => api.settingsPresetRename(from, to), "renamed");
  }

  // Survivor 3. This is a SETTINGS preset — a directory on disk, removed with
  // `remove_dir_all` — not an overview preset inside the account document. Two
  // unrelated things share the word and they land on opposite sides of the
  // reversibility line: the overview one is a toast, this one keeps its confirm.
  //
  // "This cannot be undone" is replaced by the REASON it cannot, which is the
  // one place in the app where that sentence is true and the only way it earns
  // its keep.
  async function remove(p: PresetInfo) {
    const ok = await confirmDialog({
      title: `Delete “${p.name}”?`,
      body: "The preset's files are removed. This one isn't covered by the backup chain.",
      detail: p.dir,
      confirm: "Delete preset",
      danger: true,
    });
    if (!ok) return;
    if (await run(() => api.settingsPresetDelete(p.name), "deleted")) {
      // No Undo action: the document stack cannot reverse a directory removal,
      // and a toast that offered one would revert an unrelated document edit.
      toast(`Deleted “${p.name}”.`);
    }
  }

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  function openMenu(e: MouseEvent, p: PresetInfo) {
    e.preventDefault();
    // Renaming or deleting the OPEN preset would move/remove the directory the
    // backend's open document still points at by path — the next Save would
    // target a path that no longer exists. Export is unaffected, so it stays
    // live.
    //
    // The rows are DISABLED with the reason now, rather than live rows that
    // raise a dialog explaining why they did nothing. That dialog only ever
    // fired for a user who clicked a row that had already told them it would
    // not work, so it was a modal charged for reading a label twice. The
    // disabled variant also drops its ellipsis: it does not open anything.
    const isOpen = openPresetName !== null && p.name.toLowerCase() === openPresetName.toLowerCase();
    const closeFirst = isOpen ? "Close the preset first" : undefined;
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: isOpen ? "Rename (close the preset first)" : "Rename…",
          disabled: isOpen, hint: closeFirst,
          run: () => { renaming = p.name; renameTo = p.name; } },
        { label: "Export preset…", run: () => void exportPreset(p) },
        { label: isOpen ? "Delete (close the preset first)" : "Delete",
          disabled: isOpen, hint: closeFirst,
          run: () => void remove(p) },
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
      >New preset from this character…</Button>
    <Button onclick={importPreset} disabled={busy} disabledReason="A preset command is in flight">
      Import preset…
    </Button>
  </div>
  <!-- One slot for the group, four sentences into it. -->
  {#if error}
    <InlineMessage variant="error" detail={error.detail}>{error.text}</InlineMessage>
  {/if}

  {#if creating}
    <form class="new" onsubmit={(e) => { e.preventDefault(); void create(); }}>
      <Field placeholder="Preset name" ariaLabel="Preset name" bind:value={newName} />
      <!-- Live, at the field, as the name is typed. Both of these used to be
           dialogs raised after the submit, and the app knew each of them
           before it. -->
      {#if nameOpen}
        <InlineMessage variant="error">
          “{newName.trim()}” is open — close it first, then save over it.
        </InlineMessage>
      {:else if collides}
        <InlineMessage variant="warn">
          “{newName.trim()}” already exists. Saving replaces it.
        </InlineMessage>
      {/if}
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
          disabled={busy || !newName.trim() || picked.size === 0 || nameOpen}
          disabledReason={nameOpen
            ? "That preset is open — close it first"
            : !newName.trim()
              ? "Name the preset first"
              : "Pick at least one thing to copy"}
          >{collides ? "Replace preset" : "Save preset"}</Button>
        <Button type="button" onclick={() => (creating = false)}>Cancel</Button>
      </div>
    </form>
  {/if}

  {#if allPresets().length === 0}
    <EmptyState
      title="No presets yet"
      description="Save the open character's settings as a preset to reuse them." />
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
