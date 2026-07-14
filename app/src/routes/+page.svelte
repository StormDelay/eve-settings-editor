<script lang="ts">
  import Sidebar from "$lib/Sidebar.svelte";
  import TreeNode from "$lib/TreeNode.svelte";
  import InsertForm from "$lib/InsertForm.svelte";
  import BackupsPanel from "$lib/BackupsPanel.svelte";
  import { api, errMessage, type OpenOutcome } from "$lib/api";
  import type { Mutation, NodePath, TreeNodeData, ErrDto } from "$lib/api";
  import { ask, message } from "@tauri-apps/plugin-dialog";

  let current: OpenOutcome | null = $state(null);
  let dirty = $state(false);
  let insertTarget: TreeNodeData | null = $state(null);
  let savedAt = $state(0); // bumped after each save; BackupsPanel refetches on change

  async function openFile(path: string) {
    try {
      current = await api.open(path);
      dirty = false;
    } catch (e) {
      await message(errMessage(e), { title: "Open failed", kind: "error" });
    }
  }

  async function runMutation(m: Mutation) {
    if (current?.status !== "opened") return;
    try {
      current.tree = await api.mutate(m);
      dirty = true;
    } catch (e) {
      await message(errMessage(e), { title: "Edit failed", kind: "error" });
    }
  }

  const handleEdit = (path: NodePath, text: string) =>
    runMutation({ op: "set_scalar", path, text });
  const handleRemove = (path: NodePath) =>
    runMutation({ op: "remove_entry", path });

  async function saveFile(force = false) {
    if (current?.status !== "opened" || current.fidelity.state !== "editable") return;
    try {
      const report = await api.save(force);
      dirty = false;
      savedAt += 1;
      let note = `Saved ${report.bytes_written} bytes.\nBackup: ${report.backup_path}`;
      if (report.recent_sibling_writes.length > 0) {
        note +=
          `\n\nWarning: other files in this profile changed in the last 5 minutes` +
          ` — the EVE client may be running and can overwrite your changes on logout:` +
          `\n${report.recent_sibling_writes.join("\n")}`;
      }
      await message(note, { title: "Saved", kind: "info" });
    } catch (e) {
      const err = e as ErrDto;
      if (err.code === "conflict") {
        const overwrite = await ask(
          "The file changed on disk after it was loaded (the EVE client may have " +
            "written it). Overwrite anyway?\n\nA backup of the current on-disk file " +
            "is taken first either way, so nothing is lost.",
          { title: "File changed on disk", kind: "warning" },
        );
        if (overwrite) await saveFile(true);
      } else {
        await message(errMessage(e), { title: "Save failed — file untouched", kind: "error" });
      }
    }
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      saveFile();
    }
  }}
/>

<main class="layout">
  <Sidebar onOpen={openFile} />
  <section class="editor">
    {#if current === null}
      <p class="hint">Open a settings file to begin.</p>
    {:else if current.status === "opened"}
      <header class="filebar">
        <span class="filename">{current.file_name}</span>
        {#if current.fidelity.state === "read_only"}
          <span class="badge read-only" title={current.fidelity.reason}>read-only</span>
        {:else}
          <span class="badge editable">editable</span>
        {/if}
        {#if dirty}<span class="badge dirty">unsaved changes</span>{/if}
        <span class="spacer"></span>
        <button
          class="save"
          disabled={!dirty || current.fidelity.state !== "editable"}
          onclick={() => saveFile()}>Save</button>
      </header>
      <div class="tree-area">
        <TreeNode
          node={current.tree}
          onEdit={handleEdit}
          onRemove={handleRemove}
          onInsertRequest={(n) => (insertTarget = n)}
        />
      </div>
    {:else}
      <p class="error">Cannot edit: {current.message} (offset {current.offset})</p>
      <pre class="hex">{current.hex_preview}</pre>
    {/if}
  </section>
  {#if current?.status === "opened"}
    <BackupsPanel
      {savedAt}
      onRestored={(outcome) => {
        current = outcome;
        dirty = false;
        savedAt += 1;
      }}
    />
  {/if}
  {#if insertTarget !== null}
    <div class="overlay" role="none" onclick={() => (insertTarget = null)}>
      <div class="modal" role="none" onclick={(e) => e.stopPropagation()}>
        <InsertForm
          target={insertTarget}
          onSubmit={async (m) => {
            await runMutation(m);
            insertTarget = null;
          }}
          onCancel={() => (insertTarget = null)}
        />
      </div>
    </div>
  {/if}
</main>
