<script lang="ts">
  import type { Mutation, NewValue, TreeNodeData } from "./api";

  let {
    target,
    onSubmit,
    onCancel,
  }: {
    target: TreeNodeData;
    onSubmit: (m: Mutation) => Promise<void>;
    onCancel: () => void;
  } = $props();

  const isDict = $derived(target.kind === "dict");

  let keyKind = $state("bytes"); // EVE dict keys are usually byte strings
  let keyText = $state("");
  let valueKind = $state("str");
  let valueText = $state("");
  let index = $state(target.children.length);

  function toHex(s: string): string {
    return Array.from(new TextEncoder().encode(s))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
  }

  function buildNew(kind: string, text: string): NewValue {
    switch (kind) {
      case "none": return { kind: "none" };
      case "bool": return { kind: "bool", v: text.trim() === "true" };
      case "int": return { kind: "int", v: text };
      case "float": return { kind: "float", v: text };
      case "str": return { kind: "str", v: text };
      case "str_ucs2": return { kind: "str_ucs2", v: text };
      case "bytes": return { kind: "bytes_hex", v: toHex(text) };
      case "empty_dict": return { kind: "empty_dict" };
      default: return { kind: "empty_list" };
    }
  }

  async function submit() {
    const value = buildNew(valueKind, valueText);
    const m: Mutation = isDict
      ? {
          op: "insert_dict_entry",
          parent: target.path,
          key: buildNew(keyKind, keyText),
          value,
        }
      : { op: "insert_list_item", parent: target.path, index, value };
    await onSubmit(m);
  }

  const needsText = (k: string) => !["none", "empty_dict", "empty_list"].includes(k);
</script>

<div class="insert-form">
  <h3>Add to {target.label ?? target.kind} ({target.kind})</h3>
  {#if isDict}
    <label>
      key
      <select bind:value={keyKind}>
        <option value="bytes">bytes (text)</option>
        <option value="str">str</option>
        <option value="int">int</option>
      </select>
      <input bind:value={keyText} placeholder="key" />
    </label>
  {:else}
    <label>
      index
      <input type="number" bind:value={index} min="0" max={target.children.length} />
    </label>
  {/if}
  <label>
    value
    <select bind:value={valueKind}>
      <option value="str">str</option>
      <option value="str_ucs2">str (UCS-2)</option>
      <option value="int">int</option>
      <option value="float">float</option>
      <option value="bool">bool</option>
      <option value="none">None</option>
      <option value="bytes">bytes (text)</option>
      <option value="empty_dict">empty dict</option>
      <option value="empty_list">empty list</option>
    </select>
    {#if needsText(valueKind)}
      <input bind:value={valueText} placeholder="value" />
    {/if}
  </label>
  <div class="form-actions">
    <button onclick={submit}>Add</button>
    <button onclick={onCancel}>Cancel</button>
  </div>
</div>
