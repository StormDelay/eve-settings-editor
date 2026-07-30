<script lang="ts">
  import { chatStackTargets, historyArea } from "$lib/detail";
  import type { ChatPanel, Stack } from "$lib/api";

  let { windowId, geom, panel, stack, readOnly, sharedNames, onSet }: {
    windowId: string;
    geom: { w: number; h: number } | null;
    panel: ChatPanel | undefined;
    stack: Stack | null;
    readOnly: boolean;
    sharedNames: string[];
    onSet: (ids: string[], userlistWidth: number | null, inputHeight: number | null) => void;
  } = $props();

  // The stack apply writes both current values to every channel, so it needs
  // both — a channel that has never been resized has nothing to copy out.
  const targets = $derived(stack ? chatStackTargets(stack) : []);
  const area = $derived(geom ? historyArea(geom, panel) : null);

  /** Commit one field. A blank or non-numeric input writes NOTHING and snaps
   * back to the stored value — the same rule HudPanel documents, and the reason
   * it exists: an empty box is a half-typed number, not a request to store one. */
  function edit(field: "userlist" | "input") {
    return (e: Event) => {
      const el = e.currentTarget as HTMLInputElement;
      const v = Number(el.value);
      if (el.value.trim() !== "" && Number.isFinite(v)) {
        // Rounded because <input type="number"> does not enforce integrality
        // and the backend stores an Int.
        onSet([windowId], field === "userlist" ? Math.round(v) : null, field === "input" ? Math.round(v) : null);
      } else {
        el.value = String((field === "userlist" ? panel?.userlist_width : panel?.input_height) ?? "");
      }
    };
  }

  const nothingToCopy = $derived(panel?.userlist_width == null && panel?.input_height == null);

  const applyToStack = () =>
    onSet(targets, panel?.userlist_width ?? null, panel?.input_height ?? null);
</script>

<div class="chat-split">
  <div class="legend">
    Chat layout — account-wide{#if sharedNames.length > 0}, shared with {sharedNames.join(", ")}{/if}
  </div>
  <div class="fields">
    <label>
      Member list
      <input type="number" min="0" value={panel?.userlist_width ?? ""} disabled={readOnly}
        title={readOnly ? "Not present in this file" : undefined}
        onchange={edit("userlist")} />
    </label>
    <label>
      Input box
      <input type="number" min="0" value={panel?.input_height ?? ""} disabled={readOnly}
        title={readOnly ? "Not present in this file" : undefined}
        onchange={edit("input")} />
    </label>
  </div>
  {#if area}
    <!-- Unclamped on purpose: a negative area means this account-wide split does
         not fit THIS character's window. See detail.ts's historyArea. -->
    <div class="area" class:bad={area.w <= 0 || area.h <= 0}>
      history area {area.w} × {area.h}
    </div>
  {/if}
  {#if targets.length > 1}
    <!-- Disabled when this channel has neither value stored: there would be
         nothing to copy out, and the click would be a silent no-op. -->
    <button
      class="stack-apply"
      disabled={readOnly || nothingToCopy}
      title={nothingToCopy ? "This channel has no stored sizes to copy" : undefined}
      onclick={applyToStack}>
      Apply to all {targets.length} channels in this stack
    </button>
  {/if}
</div>

<style>
  .chat-split {
    border-top: 1px solid #333;
    margin-top: 0.3rem;
    padding-top: 0.3rem;
  }
  .legend {
    color: var(--warn);
    font-size: 10px;
    margin-bottom: 0.2rem;
  }
  .fields {
    display: flex;
    gap: 0.5rem;
  }
  .fields label {
    color: #aaa;
    display: flex;
    flex-direction: column;
    font-size: 10px;
    gap: 1px;
  }
  /* Explicit dark styling per the repo's dark-native-controls note: an unstyled
     number input renders light-on-light in this theme. */
  .fields input {
    background: #11141a;
    border: 1px solid #444;
    color: #dbeafe;
    font: inherit;
    padding: 1px 3px;
    width: 5rem;
  }
  .area {
    color: #888;
    font-size: 10px;
    margin-top: 0.2rem;
  }
  .area.bad {
    color: var(--warn);
  }
  .stack-apply {
    background: #2a2f3a;
    border: 1px solid #444;
    color: #dbeafe;
    cursor: pointer;
    font: inherit;
    font-size: 10px;
    margin-top: 0.3rem;
    padding: 2px 6px;
  }
  .stack-apply:disabled {
    cursor: default;
    opacity: 0.5;
  }
</style>
