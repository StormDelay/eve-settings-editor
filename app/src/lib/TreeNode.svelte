<script lang="ts">
  import TreeNodeSelf from "./TreeNode.svelte";
  import type { TreeNodeData, NodePath } from "./api";

  let {
    node,
    depth = 0,
    autoExpand = false,
    searching = false,
    revealPath = null,
    revealNonce = 0,
    onReveal,
    onEdit,
    onRemove,
    onInsertRequest,
  }: {
    node: TreeNodeData;
    depth?: number;
    /// Set while a search is filtering the tree: everything still standing is
    /// on the way to a hit, so open it. The twisty keeps working afterwards.
    autoExpand?: boolean;
    searching?: boolean;
    /// A node path to expand-to and scroll-to; `revealNonce` bumps to re-fire.
    revealPath?: NodePath | null;
    revealNonce?: number;
    onReveal: (path: NodePath) => void;
    onEdit: (path: NodePath, text: string) => Promise<void>;
    onRemove: (path: NodePath) => Promise<void>;
    onInsertRequest: (node: TreeNodeData) => void;
  } = $props();

  let expanded = $state(depth < 1);
  $effect(() => {
    if (autoExpand) expanded = true;
  });

  // Reveal: expand this node if it is an ancestor of (or is) the target, and
  // scroll+highlight the target itself. Runs once per reveal request (nonce).
  let rowEl: HTMLDivElement | undefined = $state();
  let highlighted = $state(false);
  let lastReveal = -1;
  $effect(() => {
    const nonce = revealNonce;
    const path = revealPath;
    if (nonce === lastReveal) return;
    lastReveal = nonce;
    if (!path || path.length < node.path.length) return;
    if (JSON.stringify(path.slice(0, node.path.length)) !== JSON.stringify(node.path)) return;
    expanded = true;
    if (path.length === node.path.length) {
      highlighted = true;
      setTimeout(() => (highlighted = false), 1500);
      setTimeout(() => rowEl?.scrollIntoView({ block: "center" }), 0);
    }
  });
  let editing = $state(false);
  let draft = $state("");

  const hasChildren = $derived(node.children.length > 0);
  const container = $derived(
    node.kind === "dict" || node.kind === "list" || node.kind === "tuple",
  );

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
  <div class="row" class:reveal-hit={highlighted} bind:this={rowEl}>
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
    {#if searching}
      <button class="mini" title="show here in the full tree" onclick={() => onReveal(node.path)}>⌖</button>
    {/if}
  </div>
  {#if expanded && hasChildren}
    <div class="children">
      {#each node.children as child (JSON.stringify(child.path))}
        <TreeNodeSelf
          node={child}
          depth={depth + 1}
          {autoExpand}
          {searching}
          {revealPath}
          {revealNonce}
          {onReveal}
          {onEdit}
          {onRemove}
          {onInsertRequest} />
      {/each}
    </div>
  {/if}
</div>
