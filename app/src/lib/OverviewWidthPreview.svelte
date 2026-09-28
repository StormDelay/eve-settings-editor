<script lang="ts">
  // The selected tab's header and sample rows, drawn the way EVE draws them.
  // Everything about WHAT shows — widths, cuts, fades, positions and the text's
  // pixels — comes from overview_fit.rs, the same code the assistant's
  // overview_column_fit tool reads, so the two can never disagree. This
  // component only paints it: one EVE pixel is `ui_scale` device pixels.
  // Sample cells are editable, a header click moves the sort arrow, header
  // edges drag to resize.
  import { api, errText, type Fit, type FitLabel, type OverviewTab } from "./api";
  import { OV, fadeStart } from "./overviewRender";

  let { tab, charOpen, onWidth }:
    { tab: OverviewTab; charOpen: boolean; onWidth: (column: string, width: number) => void } = $props();

  const HEADER_H = 26; // measured band, detail.ts OVERVIEW.headerBand
  // EVE gamma-corrects text coverage; 2.2 reproduces the capture's edge pixels.
  const TEXT_GAMMA = 2.2;

  let fit = $state<Fit | null>(null);
  let error = $state<string | null>(null);
  /** The sample rows; null until the backend hands over its defaults. */
  let rows = $state<Record<string, string>[] | null>(null);
  // Which header carries the sort arrow, and so loses 16 px to it. EVE's
  // default is Distance; ponytail: the stored sort (SortHeadersSettings2) isn't read.
  let sortedBy = $state("DISTANCE");
  let drag = $state<{ name: string; startX: number; startW: number; w: number } | null>(null);
  let editing = $state<{ row: number; name: string; x: number; w: number } | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);

  // Latest request wins: a drag fires one per pointer move.
  let seq = 0;
  $effect(() => {
    const req = {
      tab: tab.index,
      rows: rows ? $state.snapshot(rows) : null,
      widths: drag ? { [drag.name]: drag.w } : {},
      sorted_by: sortedBy,
      masks: true,
    };
    void JSON.stringify(tab.columns); void charOpen; // refit on any column edit
    const mine = ++seq;
    api.overviewFit(req).then((f) => {
      if (mine !== seq || !f) return;
      fit = f; error = null;
      if (rows === null) rows = f.rows.map((r) => ({ ...r }));
    }).catch((e) => { if (mine === seq) error = errText(e); });
  });

  const scale = $derived(fit?.ui_scale ?? 1);
  // GetEntryHeight: 17 with small text; 19 for a compact window, 24 otherwise.
  // ponytail: compact is window state the files don't expose; 19 is what the captures show.
  const ROW_H = $derived(fit?.use_small_text ? 17 : 19);
  const rowCount = $derived(fit?.rows.length ?? 0);
  const totalW = $derived(Math.max(fit?.total_width ?? 0, fit?.row_width ?? 0) + 24);
  const totalH = $derived(HEADER_H + ROW_H * rowCount);

  $effect(() => {
    if (!canvas || !fit) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return; // jsdom
    // One EVE pixel = `scale` device pixels, so the preview matches the game on screen.
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.ceil(totalW * scale);
    canvas.height = Math.ceil(totalH * scale);
    canvas.style.width = `${canvas.width / dpr}px`;
    canvas.style.height = `${canvas.height / dpr}px`;
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    ctx.clearRect(0, 0, totalW, totalH);
    const css = getComputedStyle(canvas);
    const tok = (name: string) => css.getPropertyValue(name).trim();

    for (const c of fit.columns) {
      drawLabel(ctx, c.header, 0, HEADER_H, tok("--text-muted"));
      if (c.name === fit.sorted_by) {
        // The sort caret: a 16 px icon, CENTERRIGHT with left 3.
        const cx = c.x + c.width - 3 - 8, cy = HEADER_H / 2;
        ctx.fillStyle = tok("--text-muted");
        ctx.beginPath(); ctx.moveTo(cx - 3, cy + 2); ctx.lineTo(cx + 3, cy + 2); ctx.lineTo(cx, cy - 2); ctx.closePath(); ctx.fill();
      }
      ctx.strokeStyle = tok("--muted-veil");
      ctx.beginPath(); ctx.moveTo(c.x + c.width - 0.5, 4); ctx.lineTo(c.x + c.width - 0.5, HEADER_H - 4); ctx.stroke();

      c.cells.forEach((cell, i) => {
        const y = HEADER_H + i * ROW_H;
        if (c.name === "ICON") {
          ctx.fillStyle = tok("--muted-line");
          ctx.fillRect(c.x + 3, y + (ROW_H - 12) / 2, 12, 12);
        } else {
          drawLabel(ctx, cell, y, ROW_H, tok("--text"));
        }
      });
    }

    if (fit.row_width !== null) {
      // Past the row's right edge the client clips everything.
      ctx.fillStyle = tok("--scrim");
      ctx.fillRect(fit.row_width, 0, totalW - fit.row_width, totalH);
      ctx.strokeStyle = tok("--warn");
      ctx.beginPath(); ctx.moveTo(fit.row_width + 0.5, 0); ctx.lineTo(fit.row_width + 0.5, totalH); ctx.stroke();
    }
  });

  const rgb = (color: string): [number, number, number] => {
    const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})/i.exec(color);
    return m ? [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)] : [255, 255, 255];
  };

  /** One label's pixels, clipped and faded as laid out, vertically centred in
   *  a band — on its own layer, in device pixels, so the fade erases only the
   *  text. */
  function drawLabel(ctx: CanvasRenderingContext2D, l: FitLabel, y: number, h: number, color: string) {
    const s = l.mask;
    if (l.clip_w <= 0 || !s || s.width === 0) return;
    const layer = document.createElement("canvas");
    layer.width = Math.ceil(l.clip_w * scale); layer.height = Math.ceil(h * scale);
    const g2 = layer.getContext("2d")!;
    const cov = Uint8Array.from(atob(s.mask), (ch) => ch.charCodeAt(0));
    const img = g2.createImageData(s.width, s.height);
    const [r, g, b] = rgb(color);
    for (let i = 0; i < cov.length; i++) {
      img.data[i * 4] = r; img.data[i * 4 + 1] = g; img.data[i * 4 + 2] = b;
      img.data[i * 4 + 3] = Math.round(255 * Math.pow(cov[i] / 255, 1 / TEXT_GAMMA));
    }
    g2.putImageData(img, s.left, Math.round((layer.height - s.height) / 2));
    if (l.fade_w > 0) {
      // ONE mask over the whole layer: destination-in erases everything the
      // fill doesn't cover, so a mask over the fade strip alone wiped the label.
      const cw = l.clip_w * scale;
      const grad = g2.createLinearGradient(0, 0, cw, 0);
      grad.addColorStop(fadeStart(l.clip_w, l.fade_w), "black"); grad.addColorStop(1, "transparent");
      g2.globalCompositeOperation = "destination-in";
      g2.fillStyle = grad; g2.fillRect(0, 0, cw, layer.height);
    }
    ctx.save(); ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.drawImage(layer, Math.round(l.text_x * scale), Math.round(y * scale));
    ctx.restore();
  }

  // --- interaction: EVE px from a pointer event ----------------------------
  function evePx(e: PointerEvent): { x: number; y: number } {
    const r = canvas!.getBoundingClientRect();
    return { x: ((e.clientX - r.left) / r.width) * totalW, y: ((e.clientY - r.top) / r.height) * totalH };
  }
  const columns = $derived(fit?.columns.filter((c) => c.name !== "ICON") ?? []);
  const edgeAt = (x: number) => columns.find((c) => Math.abs(c.x + c.width - x) <= 3) ?? null;

  let hoverEdge = $state(false);
  function down(e: PointerEvent) {
    const p = evePx(e);
    const edge = p.y < HEADER_H ? edgeAt(p.x) : null;
    if (edge && charOpen) {
      drag = { name: edge.name, startX: p.x, startW: edge.width, w: edge.width };
      canvas!.setPointerCapture(e.pointerId);
      return;
    }
    const hit = columns.find((c) => p.x >= c.x && p.x < c.x + c.width) ?? null;
    if (p.y < HEADER_H) { if (hit) sortedBy = hit.name; return; } // clicking a header sorts by it, as in game
    editing = hit ? { row: Math.floor((p.y - HEADER_H) / ROW_H), name: hit.name, x: hit.x, w: hit.width } : null;
  }
  function move(e: PointerEvent) {
    const p = evePx(e);
    if (drag) drag = { ...drag, w: Math.max(OV.minWidth, Math.round(drag.startW + p.x - drag.startX)) };
    else hoverEdge = charOpen && p.y < HEADER_H && edgeAt(p.x) !== null;
  }
  function up() {
    if (!drag) return;
    const { name, w, startW } = drag;
    drag = null;
    if (w !== startW) onWidth(name, w);
  }
  const cssPx = (v: number) => (v * scale) / (typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1);
</script>

<section class="preview" aria-label="Overview preview">
  <div class="stage">
    <canvas bind:this={canvas} class:resize={hoverEdge || drag}
            onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={() => (drag = null)}></canvas>
    {#if editing && rows}
      <!-- svelte-ignore a11y_autofocus -->
      <input class="sample" autofocus aria-label="{editing.name} sample value"
             style="left: {cssPx(editing.x)}px; width: {cssPx(editing.w)}px; top: {cssPx(HEADER_H + editing.row * ROW_H)}px; height: {cssPx(ROW_H)}px"
             bind:value={rows[editing.row][editing.name]}
             onblur={() => (editing = null)}
             onkeydown={(e) => { if (e.key === "Enter" || e.key === "Escape") editing = null; }} />
    {/if}
  </div>
  <p class="meta">
    {#if fit}
      {fit.font_size} font{fit.use_small_text ? ", small overview text" : ""},
      UI scale {Math.round(scale * 100)}%, read from the client's settings.
      {#if fit.row_width !== null}The orange line is this window's right edge.{:else if !charOpen}Open a character to see the window's edge.{/if}
      Click a sample to try your own value, or a header to sort by it{charOpen ? "; drag a header edge to resize" : ""}.
    {/if}
    {#if error}<span class="warn">No preview: {error}</span>{/if}
  </p>
</section>

<style>
  .preview { margin: 0 0 var(--s3); }
  .stage { position: relative; overflow-x: auto; background: var(--bg); border: 1px solid var(--border);
           border-radius: var(--r-sm); padding: 0; }
  canvas { display: block; }
  canvas.resize { cursor: col-resize; }
  .sample { position: absolute; box-sizing: border-box; margin: 0; padding: 0 var(--s1); border: 1px solid var(--accent);
            background: var(--surface-raised); color: var(--text); font: inherit; font-size: var(--t-caption); }
  .meta { color: var(--text-muted); font-size: var(--t-caption); max-width: 60ch; margin: var(--s1) 0 0; }
  .warn { color: var(--warn); }
</style>
