<script lang="ts" module>
  // Ids only have to be unique within a document, and a Field's label needs one
  // to point at. A counter is the whole requirement.
  let uid = 0;
</script>

<script lang="ts">
  import InlineMessage from "./InlineMessage.svelte";

  // The single largest deletion in Phase 1. `tauri.conf.json` sets no theme
  // override and `app.css` declares `color-scheme: dark`, so a native <select>,
  // <option>, <input> or checkbox renders light-on-light in this WebView2 shell
  // unless it is given explicit colours. Sixteen files discovered that
  // independently and wrote the same rule; fourteen comments across thirteen of
  // them record the separate rediscoveries. This component is that rule, once.
  let {
    kind = "text",
    value = $bindable(),
    label,
    ariaLabel,
    id,
    options = [],
    radioValue,
    placeholder,
    disabled = false,
    disabledReason,
    readonly = false,
    min,
    max,
    step,
    width,
    error,
    layout = "row",
    element = $bindable(),
    class: klass = "",
    controlClass = "",
    onchange,
    oninput,
    ...rest
  }: {
    kind?: "text" | "number" | "select" | "checkbox" | "radio" | "search" | "color";
    /** `any` because `kind` discriminates it: a select's value is a string, a
        checkbox's a boolean, a number field's a number. Spelling that as a
        discriminated union would make every one of ~40 call sites annotate a
        generic to buy nothing. */
    value?: any;
    label?: string;
    ariaLabel?: string;
    id?: string;
    /** `value` is deliberately not narrowed to string: Svelte's select binding
        stores an option's value on the element rather than stringifying it, so
        a null placeholder ("Choose a character…") round-trips as null. `disabled`
        is what makes such a placeholder unselectable once you have moved off it. */
    options?: { value: unknown; label: string; group?: string; disabled?: boolean }[];
    /** This radio's own value. `value` then holds the GROUP's selection, so
        `bind:value` across a set of radios behaves like Svelte's `bind:group`
        without the caller hand-rolling a native radio and its accent-color. */
    radioValue?: string;
    placeholder?: string;
    disabled?: boolean;
    disabledReason?: string;
    readonly?: boolean;
    min?: number;
    max?: number;
    step?: number;
    width?: string;
    error?: string;
    layout?: "row" | "column";
    /** The control node, for callers that manage focus. WindowPanel's parent
        focuses and selects the filter box from outside the component. */
    element?: HTMLInputElement | HTMLSelectElement;
    class?: string;
    /** Goes on the control itself rather than the wrapper. Three existing specs
        identify a select or input by a class and then read a property only that
        element has, so the hook has to land on the element. */
    controlClass?: string;
    onchange?: (e: Event) => void;
    oninput?: (e: Event) => void;
    [key: string]: unknown;
  } = $props();

  // Generated once per instance, so a caller that supplies no `id` still gets a
  // stable <label for> pairing across re-renders.
  const generated = `field-${++uid}`;
  const fid = $derived(id ?? generated);
  const eid = $derived(`${fid}-error`);
  const tip = $derived(disabled && disabledReason ? disabledReason : undefined);
  const box = $derived(kind === "checkbox" || kind === "radio");
  const grouped = $derived(kind === "radio" && radioValue !== undefined);
  const ticked = $derived(grouped ? value === radioValue : !!value);
  const tick = (on: boolean): void => {
    value = grouped ? (on ? radioValue : value) : on;
  };

  // Runs of the same `group` become an <optgroup>. Two views style `optgroup`
  // today, so this is a required capability rather than a speculative one.
  const groups = $derived.by(() => {
    const out: { name?: string; items: typeof options }[] = [];
    for (const o of options) {
      const last = out.at(-1);
      if (last && last.name === o.group) last.items.push(o);
      else out.push({ name: o.group, items: [o] });
    }
    return out;
  });

  const shared = $derived({
    id: fid,
    class: [kind === "color" ? "swatch" : "", controlClass].filter(Boolean).join(" ") || undefined,
    disabled,
    title: tip,
    "aria-label": label ? undefined : ariaLabel,
    "aria-invalid": error ? ("true" as const) : undefined,
    "aria-describedby": error ? eid : undefined,
  });
</script>

<div class="field {klass}" class:column={layout === "column"} class:inline={box}>
  {#if label && box}
    <!-- The label WRAPS the control for a checkbox or radio. That is how every
         checkbox in this codebase was already written, it makes the whole row
         the hit target, and three existing specs reach the caption through
         `checkbox.closest("label")` — a `for`-paired sibling would return null
         there and break them. -->
    <label class="box">
      <input
        type={kind === "checkbox" ? "checkbox" : "radio"}
        checked={ticked}
        onchange={(e) => {
          tick(e.currentTarget.checked);
          onchange?.(e);
        }}
        {...shared}
        {...rest} />
      {label}
    </label>
  {:else}
    {#if label}<label for={fid}>{label}</label>{/if}
    {#if kind === "select"}
      <select
        bind:value
        bind:this={element}
        style={width ? `width: ${width}` : undefined}
        {onchange}
        {...shared}
        {...rest}>
        {#each groups as g (g.name ?? g.items[0].label)}
          {#if g.name}
            <optgroup label={g.name}>
              {#each g.items as o (o.label)}
              <option value={o.value} disabled={o.disabled}>{o.label}</option>
            {/each}
            </optgroup>
          {:else}
            {#each g.items as o (o.label)}
              <option value={o.value} disabled={o.disabled}>{o.label}</option>
            {/each}
          {/if}
        {/each}
      </select>
    {:else if kind === "number"}
      <input
        type="number"
        bind:value
        {min}
        {max}
        {step}
        {placeholder}
        {readonly}
        style={width ? `width: ${width}` : undefined}
        {onchange}
        {oninput}
        {...shared}
        {...rest} />
    {:else if kind === "color"}
      <input
        type="color"
        bind:value
        style={width ? `width: ${width}` : undefined}
        {onchange}
        {oninput}
        {...shared}
        {...rest} />
    {:else if box}
      <!-- No `label` prop: the caller has wrapped this in its own label, or is
           naming it some other way. HudPanel's rows are exactly that shape. -->
      <input
        type={kind}
        checked={ticked}
        onchange={(e) => {
          tick(e.currentTarget.checked);
          onchange?.(e);
        }}
        {...shared}
        {...rest} />
    {:else}
      <input
        type={kind}
        bind:value
        bind:this={element}
        {placeholder}
        {readonly}
        style={width ? `width: ${width}` : undefined}
        {onchange}
        {oninput}
        {...shared}
        {...rest} />
    {/if}
  {/if}
</div>

{#if error}
  <InlineMessage variant="error" class="field-error" id={eid}>{error}</InlineMessage>
{/if}

<style>
  .field {
    display: flex;
    align-items: center;
    gap: var(--s2);
    min-width: 0;
  }
  .field.column {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--s1);
  }
  label {
    color: var(--text-secondary);
    font-size: var(--t-body);
  }
  label.box {
    display: flex;
    align-items: center;
    gap: var(--s1);
    min-width: 0;
    cursor: pointer;
  }

  /* The one place in the app that styles a native control. */
  input,
  select {
    background: var(--surface-raised);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: var(--s1) var(--s2);
    font: inherit;
    font-size: var(--t-ui);
    min-width: 0;
  }
  select option,
  select optgroup {
    background: var(--surface-raised);
    color: var(--text);
  }
  input[type="checkbox"],
  input[type="radio"] {
    accent-color: var(--accent);
    padding: 0;
  }
  input[type="color"] {
    padding: 0;
    width: var(--s5);
    height: var(--s4);
    cursor: pointer;
  }
  input:focus-visible,
  select:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  input:disabled,
  select:disabled {
    opacity: var(--o-disabled);
    cursor: default;
  }
  input::placeholder {
    color: var(--text-muted);
  }
</style>
