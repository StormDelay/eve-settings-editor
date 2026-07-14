<script lang="ts">
  import TreeNodeSelf from "./TreeNode.svelte";
  import type { TreeNodeData, NodePath } from "./api";

  let {
    node,
    depth = 0,
    onEdit,
    onRemove,
    onInsertRequest,
  }: {
    node: TreeNodeData;
    depth?: number;
    onEdit: (path: NodePath, text: string) => Promise<void>;
    onRemove: (path: NodePath) => Promise<void>;
    onInsertRequest: (node: TreeNodeData) => void;
  } = $props();

  let expanded = $state(depth < 1);
  let editing = $state(false);
  let draft = $state("");

  const hasChildren = $derived(node.children.length > 0);
  const container = $derived(node.kind === "dict" || node.kind === "list");

  function startEdit() {
    if (!node.editable) return;
    draft = node.edit_text ?? "";
    editing = true;
  }

  async function commitEdit() {
    if (!editing) return;
    editing = false;
    if (draft !== (node.edit_text ?? "")) await onEdit(node.path, draft);
  }
</script>

<div class="node">
  <div class="row">
    {#if hasChildren}
      <button class="twisty" onclick={() => (expanded = !expanded)}
        >{expanded ? "▾" : "▸"}</button>
    {:else}
      <span class="twisty"></span>
    {/if}
    {#if node.label !== null}<span class="label">{node.label}:</span>{/if}
    {#if editing}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="edit"
        autofocus
        bind:value={draft}
        onkeydown={(e) => {
          if (e.key === "Enter") commitEdit();
          if (e.key === "Escape") editing = false;
        }}
        onblur={commitEdit}
      />
    {:else}
      <span
        class="display kind-{node.kind}"
        class:editable={node.editable}
        role="none"
        title={node.editable ? "double-click to edit" : undefined}
        ondblclick={startEdit}>{node.display}</span>
    {/if}
    {#if node.in_shared}
      <span class="shared-mark" title="inside a shared object: edits apply everywhere it is referenced">&</span>
    {/if}
    {#if container}
      <button class="mini" title="add entry" onclick={() => onInsertRequest(node)}>+</button>
    {/if}
    {#if node.removable}
      <button class="mini danger" title="remove entry" onclick={() => onRemove(node.path)}>×</button>
    {/if}
  </div>
  {#if expanded && hasChildren}
    <div class="children">
      {#each node.children as child (JSON.stringify(child.path))}
        <TreeNodeSelf node={child} depth={depth + 1} {onEdit} {onRemove} {onInsertRequest} />
      {/each}
    </div>
  {/if}
</div>
