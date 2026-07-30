<script lang="ts">
  import { toCanvas } from "$lib/layout";
  import type { DetailPart } from "$lib/detail";

  let { parts, scale }: { parts: DetailPart[]; scale: number } = $props();

  /** A label is only worth drawing when it has room to be read. Below this it
   * is dropped rather than ellipsised — a row of "…" is noise, not information. */
  const LABEL_MIN = 28;
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
             width: {w}px; height: {toCanvas(p.h, scale)}px;">
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
  .ring {
    border-radius: 50%;
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
