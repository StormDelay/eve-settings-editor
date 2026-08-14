<script lang="ts">
  import type { NodePath, TreeNodeData } from "./api";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import PanelHeader from "./ui/PanelHeader.svelte";

  // The tree's per-node metadata was invisible or hover-only: `kind` was encoded
  // as a text colour, `in_shared` as a single "&" glyph, `path` not shown at
  // all, and the three row actions sat at opacity 0 until the row was hovered.
  // Right-click was suppressed outright with a comment saying tree actions would
  // take its place. This is that place.
  //
  // It resolves the selection from the CURRENT tree by path rather than holding
  // the node object: an edit rebuilds the tree, and a held node would go on
  // showing the value it had before the edit.
  let {
    root,
    path,
    file,
    onReveal,
    onRemove,
    onInsertRequest,
  }: {
    root: TreeNodeData | null;
    path: NodePath | null;
    file: "char" | "user";
    onReveal: (path: NodePath) => void;
    onRemove: (path: NodePath) => Promise<void>;
    onInsertRequest: (node: TreeNodeData) => void;
  } = $props();

  function resolve(r: TreeNodeData | null, p: NodePath | null): TreeNodeData | null {
    if (!r || !p) return null;
    let cur = r;
    for (let d = r.path.length; d < p.length; d++) {
      const want = JSON.stringify(p.slice(0, d + 1));
      const next = cur.children.find((c) => JSON.stringify(c.path) === want);
      if (!next) return null;
      cur = next;
    }
    return JSON.stringify(cur.path) === JSON.stringify(p) ? cur : null;
  }

  const node = $derived(resolve(root, path));

  // The tree paints six kinds in six colours and says the word nowhere. Colour
  // as the only carrier of a distinction is the thing to fix, not to restate.
  const KINDS: Record<string, string> = {
    int: "integer",
    long: "long integer",
    float: "float",
    str: "string",
    str_ucs2: "string (UCS-2)",
    str_table: "string (from the file's string table)",
    bytes: "bytes",
    none: "none",
    bool: "boolean",
    dict: "dictionary",
    list: "list",
    tuple: "tuple",
    ref: "reference",
    shared: "shared object",
  };
  const kindWord = $derived(node ? (KINDS[node.kind] ?? node.kind) : "");
  const container = $derived(node?.kind === "dict" || node?.kind === "list" || node?.kind === "tuple");

  // Steps are `{s: "dict_value", i: 3}` or `{s: "shared_inner"}` — indices, not
  // keys, because dict keys in these files are arbitrary values. Rendered the
  // way the backend spells them so a path pasted into an issue means something.
  const pathText = $derived(
    (node?.path ?? []).map((s) => (s.i === undefined ? s.s : `${s.s}[${s.i}]`)).join(" › ") || "(root)",
  );

  let copied = $state(false);
  function copyPath() {
    // Best-effort: a clipboard refusal must not throw into the click handler.
    void navigator.clipboard.writeText(pathText).then(
      () => {
        copied = true;
        setTimeout(() => (copied = false), 1500);
      },
      () => {},
    );
  }
</script>

<div class="raw-inspect">
  {#if !node}
    <EmptyState title="Select a node to see its type, its value and its path." />
  {:else}
    <PanelHeader title={node.label ?? kindWord} level={4}>
      {#snippet actions()}
        <Chip size="sm">{file === "char" ? "character file" : "account file"}</Chip>
      {/snippet}
    </PanelHeader>

    <!-- Was a single "&" glyph beside the value. It is the warning that an edit
         here is an edit everywhere the object is referenced. -->
    {#if node.in_shared}
      <InlineMessage variant="warn">
        Inside a shared object — editing this changes every place it is referenced.
      </InlineMessage>
    {/if}

    <Field label="Type" layout="column" readonly value={kindWord} />
    <Field label="Value" layout="column" readonly value={node.display} />
    <!-- Only when they differ: the tree shows a rendered `display` and edits an
         `edit_text`, and for most nodes they are the same string. -->
    {#if node.edit_text !== null && node.edit_text !== node.display}
      <Field label="Raw value" layout="column" readonly value={node.edit_text} />
    {/if}

    <Field label="Path" layout="column" readonly value={pathText} />
    <div class="acts">
      <Button size="sm" onclick={copyPath}>{copied ? "Copied" : "Copy path"}</Button>
      <Button size="sm" variant="ghost" onclick={() => onReveal(node.path)}>Show in tree</Button>
    </div>

    {#if container || node.removable}
      <div class="acts">
        {#if container}
          <Button size="sm" onclick={() => onInsertRequest(node)}>Add entry…</Button>
        {/if}
        {#if node.removable}
          <Button size="sm" variant="danger" onclick={() => onRemove(node.path)}>Remove entry</Button>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .raw-inspect { display: flex; flex-direction: column; gap: var(--s2); padding: var(--s3); }
  .acts { display: flex; gap: var(--s1); flex-wrap: wrap; }
</style>
