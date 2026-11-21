<script lang="ts">
  // Convert the whole layout to another screen. The player thinks in a monitor
  // resolution and EVE's UI-scale setting, so those are what is asked; EVE saves
  // every rect in resolution ÷ scale, and the line under the fields says what
  // that comes to. The conversion itself is the backend's (`rescale_layout`):
  // one undo step, nothing written until Save.
  import { onMount } from "svelte";
  import { api, errText, type RescaleReport } from "./api";
  import Button from "./ui/Button.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import Sheet from "./ui/Sheet.svelte";

  let {
    referenceW,
    referenceH,
    onApplied,
    onClose,
  }: {
    referenceW: number;
    referenceH: number;
    onApplied: (r: RescaleReport) => void;
    onClose: () => void;
  } = $props();

  // EVE's UI-scale steps.
  const SCALES = [0.9, 1, 1.1, 1.25, 1.5, 1.75, 2].map((s) => ({ value: s, label: `${Math.round(s * 100)}%` }));

  let width = $state(0);
  let height = $state(0);
  let scale = $state(1);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let done = $state<RescaleReport | null>(null);

  onMount(async () => {
    const d = await api.clientDisplay().catch(() => null);
    [width, height] = d?.resolution ?? [referenceW, referenceH];
    scale = SCALES.some((s) => s.value === d?.ui_scale) ? d!.ui_scale : 1;
  });

  // Mirrors Rust's `desktop_size` (truncated, as `UIRoot.UpdateSize` does).
  const desktop = $derived([Math.trunc(width / scale), Math.trunc(height / scale)]);
  const valid = $derived(width > 0 && height > 0);

  async function apply() {
    busy = true;
    error = null;
    try {
      done = await api.rescaleLayout(width, height, scale);
      onApplied(done);
    } catch (e) {
      error = `The layout wasn't changed — ${errText(e)}`;
    } finally {
      busy = false;
    }
  }
</script>

<Sheet title="Rescale layout" titled width="min(30rem, 92vw)" onclose={onClose}>
  <p>Saved for a {referenceW}×{referenceH} desktop.</p>
  <div class="fields">
    <Field kind="number" label="Screen width" min={1} bind:value={width} />
    <Field kind="number" label="Screen height" min={1} bind:value={height} />
    <Field kind="select" label="UI scale" options={SCALES} bind:value={scale} />
  </div>
  <p class="muted">EVE lays this out on a {desktop[0]}×{desktop[1]} desktop.</p>
  <p class="muted">
    Every window moves and resizes with the screen; docked windows stay docked. Undo takes it back, and
    nothing is written until you save.
  </p>
  {#if error}<InlineMessage variant="error">{error}</InlineMessage>{/if}
  {#if done}
    <InlineMessage variant="success">
      Rescaled {done.windows_changed} window{done.windows_changed === 1 ? "" : "s"} to {done.desktop_w}×{done.desktop_h}.
      {#if done.held.length}
        Kept at their minimum or fixed size: {done.held.join(", ")} — they may overlap their neighbours.
      {/if}
    </InlineMessage>
  {/if}
  {#snippet footer()}
    {#if done}
      <Button variant="primary" onclick={onClose}>Close</Button>
    {:else}
      <Button onclick={onClose}>Cancel</Button>
      <Button variant="primary" onclick={apply} disabled={busy || !valid}>Rescale</Button>
    {/if}
  {/snippet}
</Sheet>

<style>
  .fields {
    display: flex;
    gap: var(--s3);
    flex-wrap: wrap;
  }
  .muted {
    color: var(--text-muted);
    font-size: var(--t-caption);
  }
</style>
