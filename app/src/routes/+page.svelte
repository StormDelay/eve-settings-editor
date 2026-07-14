<script lang="ts">
  import Sidebar from "$lib/Sidebar.svelte";
  import TreeNode from "$lib/TreeNode.svelte";
  import InsertForm from "$lib/InsertForm.svelte";
  import { api, errMessage, type OpenOutcome } from "$lib/api";
  import type { Mutation, NodePath, TreeNodeData } from "$lib/api";
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
