<script lang="ts">
  import Sidebar from "$lib/Sidebar.svelte";
  import TreeNode from "$lib/TreeNode.svelte";
  import { api, errMessage, type OpenOutcome } from "$lib/api";
  import type { NodePath, TreeNodeData } from "$lib/api";
  import { message } from "@tauri-apps/plugin-dialog";

  let current: OpenOutcome | null = $state(null);
  let dirty = $state(false);
  let insertTarget: TreeNodeData | null = $state(null);

  async function openFile(path: string) {
    try {
      current = await api.open(path);
      dirty = false;
    } catch (e) {
      await message(errMessage(e), { title: "Open failed", kind: "error" });
    }
  }

  async function handleEdit(_path: NodePath, _text: string) {}
  async function handleRemove(_path: NodePath) {}
</script>

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
</main>
