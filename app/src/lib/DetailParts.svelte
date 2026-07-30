<script lang="ts">
  import { toCanvas } from "$lib/layout";
  import type { DetailPart } from "$lib/detail";

  let { parts, scale }: { parts: DetailPart[]; scale: number } = $props();

  /** A label is only worth drawing when it has room to be read. Below this it
   * is dropped rather than ellipsised — a row of "…" is noise, not information. */
  const LABEL_MIN = 28;

  /** The capacitor gauge band's thickness as a fraction of the arc part's own
   * diameter: the measured tick band spans r 48..74, so 26 of the 148 across.
   * See detail.ts's CAP. */
  const ARC_BAND = 26 / 148;
</script>

<!-- pointer-events: none is the ONE mechanism that keeps this layer decoration:
     no part can swallow a drag on the rectangle it decorates, be hit-tested, or
     reach any of the canvas's gesture code. Do not remove it. -->
<div class="detail">
  {#each parts as p, i (i)}
    {@const w = toCanvas(p.w, scale)}
    <div
      class="part {p.kind}"
      style="left: {toCanvas(p.x, scale)}px; top: {toCanvas(p.y, scale)}px;
             width: {w}px; height: {toCanvas(p.h, scale)}px;
             {p.kind === 'arc' ? `border-width: ${Math.max(1, w * ARC_BAND)}px;` : ''}">
      {#if p.label && w > LABEL_MIN}<span>{p.label}</span>{/if}
    </div>
  {/each}
</div>

<style>
  .detail {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .part {
    position: absolute;
    box-sizing: border-box;
    border: 1px solid rgba(148, 163, 184, 0.45);
    color: #94a3b8;
    font-size: 9px;
    line-height: 1;
    overflow: hidden;
    white-space: nowrap;
  }
  /* EVE's HUD buttons are round — module slots and the ship-control cluster
     alike. Drawing them as rectangles was most of why the HUD read as "boxes in
     a box" rather than a ship HUD. */
  .ring,
  .slot,
  .core {
    border-radius: 50%;
  }
  /* The capacitor's gauge band: shield/armour/hull ticks sweeping 9 o'clock
     through 12 to 3, with the dark speed dial below. A thick-bordered circle
     clipped to its top half — the border thickness IS the annulus, so no SVG
     and no second element. The thickness is set inline, because border-width
     cannot take a percentage and this has to scale with the canvas. */
  .arc {
    border-style: solid;
    border-color: rgba(148, 163, 184, 0.5);
    border-radius: 50%;
    clip-path: inset(0 0 50% 0);
  }
  /* The capacitor core reads as the one lit thing on the element, which is what
     the eye finds first on the real HUD. */
  .core {
    background: rgba(245, 158, 11, 0.45);
    border-color: rgba(245, 158, 11, 0.7);
  }
  /* The two data-driven bands read as panels, not outlines — they are the parts
     whose SIZE is the information. */
  .band,
  .column {
    background: rgba(148, 163, 184, 0.14);
  }
  .part span {
    padding: 0 2px;
  }
</style>
