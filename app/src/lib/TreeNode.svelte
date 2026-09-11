<script lang="ts">
  import { untrack } from "svelte";
  import TreeNodeSelf from "./TreeNode.svelte";
  import type { TreeNodeData, NodePath } from "./api";
  import Button from "./ui/Button.svelte";
  import Field from "./ui/Field.svelte";

  let {
    node,
    depth = 0,
    autoExpand = false,
    searching = false,
    revealPath = null,
    revealNonce = 0,
    selectedPath = null,
    onSelect,
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
    /// The selected node, so the inspector and the tree agree on one.
    selectedPath?: NodePath | null;
    onSelect: (node: TreeNodeData) => void;
    onReveal: (path: NodePath) => void;
    onEdit: (path: NodePath, text: string) => Promise<void>;
    onRemove: (path: NodePath) => Promise<void>;
    onInsertRequest: (node: TreeNodeData) => void;
  } = $props();

  // Top-level nodes start open. A SNAPSHOT rather than a `$derived`: once the
  // user has clicked the twisty, `expanded` is theirs, and a derived would
  // reassert `depth < 1` and collapse a node they had opened (or reopen one
  // they had closed) on any re-render. `depth` is structural anyway — it is
  // fixed for a given node's position in the tree. The `$effect` below is the
  // one thing allowed to force it open, and only while a search is filtering.
  let expanded = $state(untrack(() => depth < 1));
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

  const isSelected = $derived(
    selectedPath !== null && JSON.stringify(selectedPath) === JSON.stringify(node.path),
  );
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
      <Button
        variant="ghost"
        size="sm"
        iconOnly
        class="twisty"
        title={expanded ? "Collapse" : "Expand"}
        onclick={() => (expanded = !expanded)}>{expanded ? "▾" : "▸"}</Button>
    {:else}
      <span class="twisty"></span>
    {/if}
    {#if node.label !== null}<span class="label">{node.label}:</span>{/if}
    {#if editing}
      <!-- svelte-ignore a11y_autofocus -->
      <Field
        class="edit"
        ariaLabel="Edit value"
        autofocus
        bind:value={draft}
        onkeydown={(e: KeyboardEvent) => {
          if (e.key === "Enter") commitEdit();
          if (e.key === "Escape") editing = false;
        }}
        onblur={commitEdit} />
    {:else}
      <!-- Single click selects, double click still edits. The tree had no
           selection at all, so its per-node metadata had nowhere to be shown
           and lived as a text colour and a one-character glyph. -->
      <span
        class="display kind-{node.kind}"
        class:editable={node.editable}
        class:selected={isSelected}
        role="button"
        tabindex="0"
        aria-pressed={isSelected}
        title={node.editable ? "click to select · double-click to edit" : "click to select"}
        onclick={() => onSelect(node)}
        onkeydown={(e) => {
          if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onSelect(node); }
        }}
        ondblclick={startEdit}>{node.display}</span>
    {/if}
    {#if node.in_shared}
      <span class="shared-mark" title="Inside a shared object — edits apply everywhere it is referenced">&</span>
    {/if}
    <!-- These three used to be `.mini`, hidden at opacity 0 and revealed only by
         `.row:hover`. They are ghost Buttons now and always visible — the row
         recedes them by COLOUR on hover instead, which is the treatment that
         cannot silently swallow a control the way the old one did to four
         buttons elsewhere. -->
    {#if container}
      <Button variant="ghost" size="sm" iconOnly class="row-act" title="Add entry…"
              onclick={() => onInsertRequest(node)}>+</Button>
    {/if}
    {#if node.removable}
      <Button variant="ghost" size="sm" iconOnly class="row-act danger-act" title="Remove entry"
              onclick={() => onRemove(node.path)}>×</Button>
    {/if}
    {#if searching}
      <Button variant="ghost" size="sm" iconOnly class="row-act" title="Show this in the full tree"
              onclick={() => onReveal(node.path)}>⌖</Button>
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
          {selectedPath}
          {onSelect}
          {onReveal}
          {onEdit}
          {onRemove}
          {onInsertRequest} />
      {/each}
    </div>
  {/if}
</div>

<style>
  /* Recede by colour, never by hiding. `.mini`'s opacity-0 trick is what left
     four buttons elsewhere invisible and still clickable. */
  .row :global(.row-act) {
    color: var(--text-muted);
  }
  .row:hover :global(.row-act) {
    color: var(--text);
  }
  .row :global(.danger-act:hover) {
    color: var(--danger);
  }
  .row :global(.edit) {
    flex: 1;
    min-width: 200px;
  }
  .display {
    cursor: pointer;
  }
  /* The same accent-dim ground ListRow's selected variant uses, so "selected"
     means one thing everywhere. */
  .display.selected {
    background: var(--accent-dim);
    border-radius: var(--r-sm);
  }
  .display:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    border-radius: var(--r-sm);
  }
</style>
