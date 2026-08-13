<script lang="ts">
  import { chatStackTargets, historyArea } from "$lib/detail";
  import type { ChatPanel, Stack } from "$lib/api";
  import Button from "./ui/Button.svelte";
  import Field from "./ui/Field.svelte";
  import ScopeBanner from "./ui/ScopeBanner.svelte";

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
  <ScopeBanner
    compact
    label="Chat layout — account-wide{sharedNames.length > 0
      ? `, shared with ${sharedNames.join(', ')}`
      : ''}" />
  <div class="fields">
    <Field
      kind="number"
      label="Member list"
      layout="column"
      width="5rem"
      min={0}
      value={panel?.userlist_width ?? ""}
      disabled={readOnly}
      disabledReason="Not present in this file"
      onchange={edit("userlist")} />
    <Field
      kind="number"
      label="Input box"
      layout="column"
      width="5rem"
      min={0}
      value={panel?.input_height ?? ""}
      disabled={readOnly}
      disabledReason="Not present in this file"
      onchange={edit("input")} />
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
    <Button
      size="sm"
      class="stack-apply"
      disabled={readOnly || nothingToCopy}
      disabledReason={nothingToCopy
        ? "This channel has no stored sizes to copy"
        : "Not present in this file"}
      onclick={applyToStack}>
      Apply to all {targets.length} channels in this stack
    </Button>
  {/if}
</div>

<style>
  /* The scope legend moved from --warn to --info. --warn was carrying both
     meanings inside this one file: the legend below, which is a statement of
     scope, and `.area.bad`, which is a real warning that the computed split
     leaves a negative area. Same token, one file, two meanings, and no way for
     a reader to tell which. --warn now means exactly one thing again.

     The 10px type went with it. §4.3 listed these four lines as canvas-scale,
     but this component renders inside WindowPanel's side panel, not on the
     canvas, so it is chrome and takes the scale. */
  .chat-split {
    border-top: 1px solid var(--border);
    margin-top: var(--s1);
    padding-top: var(--s1);
  }
  .chat-split :global(.scope) {
    margin-bottom: var(--s1);
  }
  .fields {
    display: flex;
    gap: var(--s2);
  }
  .area {
    color: var(--text-muted);
    font-size: var(--t-caption);
    margin-top: var(--s1);
  }
  .area.bad {
    color: var(--warn);
  }
  .chat-split :global(.stack-apply) {
    margin-top: var(--s1);
  }
</style>
