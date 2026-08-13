<script lang="ts">
  import { untrack } from "svelte";
  import type { ErrDto, Mutation, NewValue, TreeNodeData } from "./api";
  import Button from "./ui/Button.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";

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
  // Defaults to appending, then becomes the user's to change — so a SNAPSHOT,
  // not a `$derived` of the target's child count. The form is mounted per
  // insert request (`{#if insertTarget !== null}` in +page), so `target` cannot
  // change under it; deriving would only serve to slam a typed index back to
  // the end on an unrelated re-render. `untrack` marks that as deliberate.
  let index = $state(untrack(() => target.children.length));

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
  type ErrorField = "key" | "value" | "index" | null;

  let error: string | null = $state(null);
  let errorField: ErrorField = $state(null);

  const FIELD_OF: Record<string, ErrorField> = {
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
    <div class="pair">
      <Field
        kind="select"
        label="key"
        bind:value={keyKind}
        onchange={clearError}
        options={[
          { value: "bytes", label: "bytes (text)" },
          { value: "str", label: "str" },
          { value: "int", label: "int" },
        ]} />
      <Field
        ariaLabel="key"
        placeholder="key"
        bind:value={keyText}
        oninput={clearError}
        error={errorField === "key" ? (error ?? undefined) : undefined} />
    </div>
  {:else}
    <Field
      kind="number"
      label="index"
      bind:value={index}
      min={0}
      max={target.children.length}
      oninput={clearError}
      error={errorField === "index" ? (error ?? undefined) : undefined} />
  {/if}
  <div class="pair">
    <Field
      kind="select"
      label="value"
      bind:value={valueKind}
      onchange={clearError}
      options={[
        { value: "str", label: "str" },
        { value: "str_ucs2", label: "str (UCS-2)" },
        { value: "int", label: "int" },
        { value: "float", label: "float" },
        { value: "bool", label: "bool" },
        { value: "none", label: "None" },
        { value: "bytes", label: "bytes (text)" },
        { value: "empty_dict", label: "empty dict" },
        { value: "empty_list", label: "empty list" },
        { value: "empty_tuple", label: "empty tuple" },
      ]} />
    {#if needsText(valueKind)}
      <Field
        ariaLabel="value"
        placeholder="value"
        bind:value={valueText}
        oninput={clearError}
        error={errorField === "value" ? (error ?? undefined) : undefined} />
    {/if}
  </div>
  {#if valueKind === "empty_tuple" || valueKind === "empty_list" || valueKind === "empty_dict"}
    <InlineMessage>Added empty — expand it in the tree and use + to fill it.</InlineMessage>
  {/if}
  {#if error !== null && errorField === null}
    <InlineMessage variant="error">{error}</InlineMessage>
  {/if}
  <div class="form-actions">
    <Button variant="primary" onclick={submit}>Add</Button>
    <Button onclick={onCancel}>Cancel</Button>
  </div>
</div>

<style>
  /* The four `.field-error` paragraphs are gone: an error now belongs to the
     Field it is about, which wires aria-invalid and aria-describedby with it.
     Before, they were bare <p>s with no live region and no association. */
  .pair {
    display: flex;
    gap: var(--s2);
    align-items: flex-end;
    flex-wrap: wrap;
    margin: var(--s2) 0;
  }
</style>
