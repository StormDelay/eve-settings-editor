<script lang="ts">
  import type { OverviewTab, TabPiece } from "./api";
  import { UNSET_HEX } from "./colour";
  import { cssColor, EVE_PALETTE, pieceStyle, TAB_FONT_PX, type TabNameEdit } from "./tabName";
  import Button from "./ui/Button.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import Popover from "./ui/Popover.svelte";

  // A tab name edited the way EVE draws it: a short list of pieces, each with
  // its own text, colour, weight, slant, underline, size and letter spacing.
  // The markup is parsed and written in Rust (tab_name.rs); this never builds
  // a tag. A name using markup the pieces can't carry (hint, font, uppercase,
  // unknown tags) opens as raw markup, the only mode that can't lose any of it.
  //
  // Enter, Escape and clicking away are the three exits; leaving commits, as
  // the plain rename box this replaces did.
  let {
    tab,
    parseName,
    formatName,
    oncommit,
    oncancel,
  }: {
    tab: OverviewTab;
    parseName: (raw: string) => Promise<{ pieces: TabPiece[]; editable: boolean; warnings?: string[] }>;
    formatName: (pieces: TabPiece[]) => Promise<string>;
    oncommit: (edit: TabNameEdit) => void;
    oncancel: () => void;
  } = $props();

  // Seeded once from the tab the editor opened on; later prop changes are the
  // result of this editor's own commit and must not reseed it mid-edit.
  // svelte-ignore state_referenced_locally
  let raw = $state(!tab.editable);
  // svelte-ignore state_referenced_locally
  let pieces = $state<TabPiece[]>(tab.editable ? tab.pieces.map((p) => ({ ...p })) : []);
  // svelte-ignore state_referenced_locally
  let markup = $state(tab.name);
  // svelte-ignore state_referenced_locally
  let preview = $state<{ pieces: TabPiece[]; editable: boolean; warnings?: string[] }>(tab);
  let switchError = $state<string | null>(null);
  let swatchFor = $state<number | null>(null);
  let swatchEls: HTMLDivElement[] = $state([]);
  let editorEl: HTMLDivElement | undefined = $state();
  let firstInput: HTMLInputElement | HTMLSelectElement | undefined = $state();
  let done = false;

  $effect(() => {
    if (!firstInput) return;
    firstInput.focus();
    if (firstInput instanceof HTMLInputElement) firstInput.select();
  });

  // The raw box previews through the real parser, so what it shows is what the
  // client will draw — warnings included.
  $effect(() => {
    if (!raw) return;
    const text = markup;
    void parseName(text).then((r) => { if (markup === text) preview = r; }).catch(() => {});
  });

  const shown = $derived(raw ? preview.pieces : pieces);
  const warnings = $derived(raw ? (preview.warnings ?? []) : []);

  function commit() {
    if (done) return;
    done = true;
    if (raw) {
      if (markup.trim()) oncommit({ raw: markup });
      else oncancel();
    } else if (pieces.some((p) => p.text.trim())) {
      oncommit({ pieces: pieces.map(clean) });
    } else {
      oncancel();
    }
  }
  function cancel() {
    if (done) return;
    done = true;
    oncancel();
  }

  /** Blank number boxes come back as null or NaN; neither is a setting. */
  function clean(p: TabPiece): TabPiece {
    const out: TabPiece = { text: p.text };
    if (p.color) out.color = p.color;
    if (p.bold) out.bold = true;
    if (p.italic) out.italic = true;
    if (p.underline) out.underline = true;
    if (typeof p.size === "number" && p.size > 0) out.size = p.size;
    if (typeof p.spacing === "number" && Number.isFinite(p.spacing) && p.spacing !== 0) out.spacing = p.spacing;
    return out;
  }

  // Leaving the editor commits, but its swatches, palettes and toggles are PART
  // of it — moving focus onto one of them must not close it.
  function focusOut(e: FocusEvent) {
    if (swatchFor !== null) return;
    const next = e.relatedTarget as Node | null;
    if (next && editorEl?.contains(next)) return;
    commit();
  }
  function keydown(e: KeyboardEvent) {
    if (e.key === "Enter") { e.preventDefault(); commit(); }
    else if (e.key === "Escape") { e.preventDefault(); cancel(); }
  }

  async function toRaw() {
    switchError = null;
    try {
      markup = await formatName(pieces.map(clean));
      raw = true;
    } catch (e) {
      switchError = (e as { message?: string })?.message ?? String(e);
    }
  }
  async function toPieces() {
    switchError = null;
    const r = await parseName(markup);
    if (!r.editable) {
      switchError = "This markup uses tags the piece editor can't write. Keep editing it as markup.";
      return;
    }
    pieces = r.pieces.length ? r.pieces.map((p) => ({ ...p })) : [{ text: "" }];
    raw = false;
  }

  function addPiece() {
    pieces.push({ text: "" });
  }
  function removePiece(i: number) {
    pieces.splice(i, 1);
  }
  /** `<input type=color>` speaks `#rrggbb`; a picked colour is opaque. */
  function fromPicker(hex: string): string {
    return `FF${hex.slice(1).toUpperCase()}`;
  }
  function toPicker(c: string | undefined): string {
    return c ? `#${c.slice(2).toLowerCase()}` : UNSET_HEX;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="editor" bind:this={editorEl} onfocusout={focusOut} onkeydown={keydown}>
  {#if raw}
    <Field bind:value={markup} bind:element={firstInput} ariaLabel="Tab name markup"
           placeholder="<color=0xFFFF4040>name</color>" class="markup" />
  {:else}
    {#each pieces as p, i (i)}
      <div class="piece">
        {#if i === 0}
          <Field bind:value={p.text} bind:element={firstInput} ariaLabel="Piece {i + 1} text"
                 placeholder="Text" class="text" style={pieceStyle(p, tab.color)} />
        {:else}
          <Field bind:value={p.text} ariaLabel="Piece {i + 1} text"
                 placeholder="Text" class="text" style={pieceStyle(p, tab.color)} />
        {/if}
        <div class="controls">
          <div class="swatch-wrap" bind:this={swatchEls[i]}>
            <!-- aria-label as well as title: the swatch's only content is a dash
                 or nothing at all. -->
            <Button class="swatch" title="Piece {i + 1} colour" aria-label="Piece {i + 1} colour"
                    style={p.color ? `background:${cssColor(p.color)}` : ""}
                    onclick={() => (swatchFor = swatchFor === i ? null : i)}>{p.color ? "" : "—"}</Button>
            {#if swatchFor === i && swatchEls[i]}
              <Popover anchor={swatchEls[i]} placement="bottom-start" ariaLabel="Piece {i + 1} colour"
                       class="palette" onclose={() => (swatchFor = null)}>
                <div class="palette-grid">
                  {#each EVE_PALETTE as c (c)}
                    <button style="background:#{c}" title="#{c}" aria-label="#{c}"
                            onclick={() => { p.color = `FF${c.toUpperCase()}`; swatchFor = null; }}></button>
                  {/each}
                </div>
                <Field kind="color" label="Custom" value={toPicker(p.color)}
                       onchange={(e: Event) => { p.color = fromPicker((e.target as HTMLInputElement).value); }} />
                <Button variant="ghost" size="sm" class="palette-none"
                        onclick={() => { p.color = undefined; swatchFor = null; }}>No colour</Button>
              </Popover>
            {/if}
          </div>
          <Button class="toggle b" pressed={!!p.bold} title="Bold" aria-label="Piece {i + 1} bold"
                  onclick={() => (p.bold = !p.bold)}>B</Button>
          <Button class="toggle i" pressed={!!p.italic} title="Italic" aria-label="Piece {i + 1} italic"
                  onclick={() => (p.italic = !p.italic)}>I</Button>
          <Button class="toggle u" pressed={!!p.underline} title="Underline" aria-label="Piece {i + 1} underline"
                  onclick={() => (p.underline = !p.underline)}>U</Button>
          <Field kind="number" bind:value={p.size} ariaLabel="Piece {i + 1} size" title="Font size, pixels"
                 placeholder={String(TAB_FONT_PX)} min={1} max={64} width="3.5rem" />
          <Field kind="number" bind:value={p.spacing} ariaLabel="Piece {i + 1} letter spacing"
                 title="Extra space after each letter, pixels" placeholder="0" min={-10} max={50} width="3.5rem" />
          <Button variant="ghost" size="sm" title="Remove piece" aria-label="Remove piece {i + 1}"
                  disabled={pieces.length === 1} onclick={() => removePiece(i)}>✕</Button>
        </div>
      </div>
    {/each}
  {/if}

  <div class="foot">
    <!-- The name as the tab will draw it, the tab's own colour underneath. -->
    <span class="preview" aria-label="Preview">
      {#each shown as p, i (i)}{#if !p.hidden}<span style={pieceStyle(p, tab.color)}>{p.text}</span>{/if}{/each}
    </span>
    {#if !raw}
      <Button size="sm" onclick={addPiece}>+ Piece</Button>
      <Button size="sm" variant="ghost" onclick={toRaw}>Edit markup</Button>
    {:else}
      <Button size="sm" variant="ghost" onclick={toPieces}>Edit pieces</Button>
    {/if}
  </div>
  {#each warnings as w (w)}
    <InlineMessage variant="warn">{w}</InlineMessage>
  {/each}
  {#if switchError}
    <InlineMessage variant="error">{switchError}</InlineMessage>
  {/if}
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: var(--s1);
    padding: var(--s1) var(--s2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
  }
  .piece { display: flex; flex-direction: column; gap: var(--s1); }
  .piece :global(.text), .editor :global(.markup) { width: 100%; }
  .piece :global(.text input), .editor :global(.markup input) { width: 100%; }
  .controls { display: flex; flex-wrap: wrap; align-items: center; gap: var(--s1); }
  .swatch-wrap { position: relative; display: inline-flex; }
  .controls :global(.swatch) { width: 1.9rem; }
  .controls :global(.toggle) { min-width: 1.9rem; }
  .controls :global(.toggle.b) { font-weight: 700; }
  .controls :global(.toggle.i) { font-style: italic; }
  .controls :global(.toggle.u) { text-decoration: underline; }
  :global(.palette) { display: block; }
  .palette-grid { display: grid; grid-template-columns: repeat(8, 1.1rem); gap: var(--s1); margin-bottom: var(--s1); }
  /* --border-strong, not --border: this outline has to read against an
     arbitrary user colour on either side of it. */
  .palette-grid button {
    width: 1.1rem; height: 1.1rem; border: 1px solid var(--border-strong);
    border-radius: var(--r-sm); padding: 0; cursor: pointer;
  }
  .palette-grid button:hover { outline: 1px solid var(--text); }
  :global(.palette-none) { display: block; width: 100%; margin-top: var(--s1); }
  .foot { display: flex; flex-wrap: wrap; align-items: center; gap: var(--s1); }
  .preview {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: pre;
    text-overflow: ellipsis;
  }
</style>
