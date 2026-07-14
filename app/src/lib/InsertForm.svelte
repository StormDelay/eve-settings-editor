<script lang="ts">
  import type { ErrDto, Mutation, NewValue, TreeNodeData } from "./api";

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
      case "empty_tuple": return { kind: "empty_tuple" };
      default: return { kind: "empty_list" };
    }
  }

  // The model rejects unparseable input (e.g. "df" as an int). Anchor its
  // complaint to the field that caused it and keep the form open, so the entry
  // being typed survives the mistake.
  type Field = "key" | "value" | "index" | null;

  let error: string | null = $state(null);
  let errorField: Field = $state(null);

  const FIELD_OF: Record<string, Field> = {
    parse_key: "key",
    parse: "value",
    bad_index: "index",
  };

  function clearError() {
    error = null;
    errorField = null;
  }

  async function submit() {
    clearError();
    const value = buildNew(valueKind, valueText);
    const m: Mutation = isDict
      ? {
          op: "insert_dict_entry",
          parent: target.path,
          key: buildNew(keyKind, keyText),
          value,
        }
      : { op: "insert_list_item", parent: target.path, index, value };
    try {
      await onSubmit(m);
    } catch (e) {
      const err = e as ErrDto;
      error = err?.message ?? String(e);
      errorField = FIELD_OF[err?.code] ?? null;
    }
  }

  const needsText = (k: string) =>
    !["none", "empty_dict", "empty_list", "empty_tuple"].includes(k);
</script>

<div class="insert-form">
  <h3>Add to {target.label ?? target.kind} ({target.kind})</h3>
  {#if isDict}
    <label>
      key
      <select bind:value={keyKind} onchange={clearError}>
        <option value="bytes">bytes (text)</option>
        <option value="str">str</option>
        <option value="int">int</option>
      </select>
      <input bind:value={keyText} placeholder="key" oninput={clearError} />
    </label>
    {#if errorField === "key"}<p class="field-error">{error}</p>{/if}
  {:else}
    <label>
      index
      <input
        type="number"
        bind:value={index}
        min="0"
        max={target.children.length}
        oninput={clearError} />
    </label>
    {#if errorField === "index"}<p class="field-error">{error}</p>{/if}
  {/if}
  <label>
    value
    <select bind:value={valueKind} onchange={clearError}>
      <option value="str">str</option>
      <option value="str_ucs2">str (UCS-2)</option>
      <option value="int">int</option>
      <option value="float">float</option>
      <option value="bool">bool</option>
      <option value="none">None</option>
      <option value="bytes">bytes (text)</option>
      <option value="empty_dict">empty dict</option>
      <option value="empty_list">empty list</option>
      <option value="empty_tuple">empty tuple</option>
    </select>
    {#if needsText(valueKind)}
      <input bind:value={valueText} placeholder="value" oninput={clearError} />
    {/if}
  </label>
  {#if errorField === "value"}<p class="field-error">{error}</p>{/if}
  {#if valueKind === "empty_tuple" || valueKind === "empty_list" || valueKind === "empty_dict"}
    <p class="hint">Added empty — expand it in the tree and use + to fill it.</p>
  {/if}
  {#if error !== null && errorField === null}
    <p class="field-error">{error}</p>
  {/if}
  <div class="form-actions">
    <button onclick={submit}>Add</button>
    <button onclick={onCancel}>Cancel</button>
  </div>
</div>
