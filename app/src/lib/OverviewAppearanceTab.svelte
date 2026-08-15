<script lang="ts">
  import { api, errMessage, errText, type OverviewColumns, type Rgba } from "./api";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import {
    stateLabel, rgbaToHex, hexToRgba, moveInOrder, defaultColor,
    DEFAULT_BACKGROUND_ORDER, DEFAULT_BACKGROUND_STATES,
    DEFAULT_FLAG_ORDER, DEFAULT_FLAG_STATES,
  } from "./states";
  import Button from "./ui/Button.svelte";
  import Field from "./ui/Field.svelte";
  import ListRow from "./ui/ListRow.svelte";
  import Tabs from "./ui/Tabs.svelte";

  let { data, onChanged, onUserDirty }:
    { data: OverviewColumns | null;
      onChanged: (next: OverviewColumns) => void; onUserDirty: () => void } = $props();

  // EVE's own Appearance labels, in EVE's own order.
  const BOOL_LABELS: [string, string][] = [
    ["useSmallColorTags", "Use small colortags"],
    ["useSmallText", "Use small font"],
    ["applyToStructures", "Also apply to structures"],
    ["applyToOtherObjects", "Also apply to other objects in space"],
    ["overviewBroadcastsToTop", "Show fleet broadcasts at the top"],
    ["hideCorpTicker", "Hide corporation ticker"],
  ];

  // Last-resort swatch for a state with no stored colour AND no harvested
  // default. Dimmed, because unlike a real default it carries no information.
  const UNSET_HEX = "#808080";

  let surface = $state<"Background" | "Colortag">("Background");

  const appearance = $derived(data?.appearance ?? null);
  const isBg = $derived(surface === "Background");
  const stored = $derived(isBg ? appearance?.background : appearance?.flag);
  // Fall back to EVE's bundled defaults PER SURFACE, not on `appearance.defaulted`
  // (which is true only while all four keys are absent): editing Background
  // materialises its two keys and flips that flag false, and a global fallback
  // would then render Colortag as an empty, un-tickable list. A surface counts as
  // un-materialised only when BOTH its lists are empty — unticking every state
  // leaves `enabled` empty but `order` intact, which must not re-show defaults.
  const surfaceUnset = $derived(!stored?.order.length && !stored?.enabled.length);
  const order = $derived(
    surfaceUnset ? (isBg ? DEFAULT_BACKGROUND_ORDER : DEFAULT_FLAG_ORDER) : (stored?.order ?? []),
  );
  const enabled = $derived(
    surfaceUnset ? (isBg ? DEFAULT_BACKGROUND_STATES : DEFAULT_FLAG_STATES) : (stored?.enabled ?? []),
  );
  // Iterate `order` verbatim — it's a priority list and carries ids the client
  // never draws (68), which must keep their slots, so they stay in `rows` (and
  // thus in every reorder) even though the markup skips drawing them. An enabled
  // id somehow absent from it is appended so it stays reachable instead of being
  // invisible and impossible to untick.
  const rows = $derived([...order, ...enabled.filter((id) => !order.includes(id))]);
  const enabledSet = $derived(new Set(enabled));
  // Colours are per surface in the file — `(background, id)` and `(flag, id)`
  // are different entries — so they follow the sub-tab. Reading only the
  // background list is what left a pack's colortag colours (Z-S sets `flag_48`)
  // in the file with nothing on screen able to show, review or undo them.
  const colors = $derived(new Map((isBg ? appearance?.colors : appearance?.flag_colors) ?? []));
  const bools = $derived(new Map(appearance?.bools ?? []));

  // EVE's palette, from the model. A pack stores colours by NAME, and
  // `overview_pack::color_name` matches the floats EXACTLY, so a colour that is
  // not one of these is silently dropped from a pack export — which the row
  // below says out loud. INCOMPLETE: two of EVE's eight names (green, purple)
  // have no captured floats yet, so a colour of either reads as unnameable here.
  const palette = $derived(appearance?.palette ?? []);
  const paletteName = (c: Rgba): string | undefined =>
    palette.find(([, p]) => p.every((n, i) => n === c[i]))?.[0];

  // At the top of the sub-tab, which is the control group that owns every failure
  // this function can report. This tab is mounted-but-hidden when another
  // sub-tab is showing, so `escalate` matters here more than anywhere: an error
  // rendered into a panel nobody is looking at is a silent failure, and a modal
  // never was one.
  let error = $state<{ text: string; detail: string } | null>(null);

  async function edit(fn: () => Promise<OverviewColumns>) {
    error = null;
    try { onChanged(await fn()); onUserDirty(); }
    catch (e) {
      error = {
        text: `That appearance setting wasn't changed — ${errText(e)}`,
        detail: errMessage(e),
      };
    }
  }

  // Enabled and order are independent lists: a toggle writes only *States2, a
  // drag only *Order2 — never a coupled write. The one exception is the FIRST
  // edit on an un-materialised surface: both lists are showing EVE's bundled
  // defaults, so writing one alone leaves the other absent while `surfaceUnset`
  // flips false, and the list we never wrote would then render as empty (every
  // state unticked, or the priority order lost) instead of as the defaults the
  // user was just looking at.
  function toggleState(id: number, on: boolean) {
    const next = on ? [...enabled, id] : enabled.filter((n) => n !== id);
    const materialise = surfaceUnset;
    return edit(async () => {
      if (materialise) await api.overviewSetStates(isBg ? "backgroundOrder" : "flagOrder", order);
      return api.overviewSetStates(isBg ? "background" : "flag", next);
    });
  }

  let dragFrom = $state<number | null>(null);
  function drop(to: number) {
    if (dragFrom === null) return;
    const from = dragFrom;
    dragFrom = null;
    if (from === to) return;
    const next = moveInOrder(rows, from, to);
    const materialise = surfaceUnset;
    return edit(async () => {
      if (materialise) await api.overviewSetStates(isBg ? "background" : "flag", enabled);
      return api.overviewSetStates(isBg ? "backgroundOrder" : "flagOrder", next);
    });
  }

  // Alpha isn't exposed; carry the stored one through so a non-1.0 entry keeps it.
  function setColor(id: number, hex: string) {
    const alpha = colors.get(id)?.[3] ?? 1;
    // A hex inverts to n/255 — #bf0000 gives 0.74901…, not the 0.75 EVE stores
    // for `red` — so picking a palette colour off the swatch would still write
    // something no pack export could name. When the hex IS a known palette
    // colour's hex, write that palette entry's exact floats instead. Nothing
    // visible changes: both render as the same #rrggbb.
    const exact = palette.find(([, p]) => rgbaToHex(p) === hex)?.[1];
    const rgba: Rgba = exact ? [exact[0], exact[1], exact[2], alpha] : hexToRgba(hex, alpha);
    return edit(() => api.overviewSetStateColor(isBg ? "background" : "flag", id, rgba));
  }
  // Removing the entry is what restores EVE's default — writing a default-looking
  // colour is not the same thing.
  function resetColor(id: number) {
    return edit(() => api.overviewSetStateColor(isBg ? "background" : "flag", id, null));
  }
</script>

{#if appearance}
  {#if error}
    <InlineMessage variant="error" detail={error.detail}>{error.text}</InlineMessage>
  {/if}
  <div class="bools">
    {#each BOOL_LABELS as [key, label] (key)}
      {#if key === "applyToStructures"}
        <p class="apply-note">The Colortag and Background settings apply to ships and drones by default.</p>
      {/if}
      <Field
        kind="checkbox"
        class="bool-row"
        {label}
        value={bools.get(key) ?? false}
        onchange={(e) => edit(() => api.overviewSetBool(key, (e.currentTarget as HTMLInputElement).checked))} />
    {/each}
  </div>

  <!-- EVE's own Appearance tab lists Colortag first, Background second.
       Tabs brings the roving tabindex and arrow-key movement this strip never
       had: it set role="tab" and aria-selected and stopped there. -->
  <Tabs
    variant="underline"
    class="surfaces"
    ariaLabel="Appearance surface"
    tabs={[
      { id: "Colortag", label: "Colortag" },
      { id: "Background", label: "Background" },
    ]}
    bind:value={surface} />

  {#if surfaceUnset}
    <p class="meta">This account has never customised its {surface} states, so these are EVE's
      defaults and aren't saved yet. Your first change here writes them to the file.</p>
  {/if}

  <!-- The palette as suggestions in the native colour picker. It is a hint, not
       a constraint: free-form stays available, and a picked palette colour is
       snapped to EVE's exact floats by `setColor` so it survives a pack export.
       Two of EVE's eight names have no captured floats, so this list is short
       by two — which is why it is offered as suggestions rather than as the
       only choices. -->
  <datalist id="eve-palette">
    {#each palette as [name, c] (name)}<option value={rgbaToHex(c)}></option>{/each}
  </datalist>

  <ul class="state-list">
    {#each rows as id, i (id)}
      <!-- An id with no label is one the client never draws (68): EVE's own
           Appearance list has no row for it, so neither do we. It stays in
           `rows`, keeping its slot through every reorder. -->
      {#if stateLabel(id)}
      <li>
        <ListRow
          draggable
          ondragstart={(e) => { dragFrom = i;
            // WebView2/Chromium won't fire `drop` unless dragstart sets data.
            e.dataTransfer?.setData("text/plain", String(i));
            if (e.dataTransfer) e.dataTransfer.effectAllowed = "move"; }}
          ondragover={(e) => { e.preventDefault();
            if (e.dataTransfer) e.dataTransfer.dropEffect = "move"; }}
          ondrop={(e) => { e.preventDefault(); drop(i); }}
          ondragend={() => (dragFrom = null)}>
          <Field
            kind="checkbox"
            class="state-label"
            label={stateLabel(id) ?? undefined}
            value={enabledSet.has(id)}
            onchange={(e) => toggleState(id, (e.currentTarget as HTMLInputElement).checked)} />
          {#snippet trailing()}
            {@const c = colors.get(id)}
            <!-- The bundled defaults were sampled off EVE's *Background* list and
                 are reused for Colortag, which is an assumption (EVE tints a
                 state's tag and its row alike) rather than a measurement. They
                 are display only and never written, so if it is wrong it costs a
                 swatch, not a file. -->
            {@const fallback = defaultColor(id)}
            <Field
              kind="color"
              list="eve-palette"
              controlClass={!c && !fallback ? "unset" : ""}
              value={c ? rgbaToHex(c) : (fallback ?? UNSET_HEX)}
              ariaLabel={isBg ? "Background colour" : "Colortag colour"}
              onchange={(e) => setColor(id, (e.currentTarget as HTMLInputElement).value)} />
            <!-- `palette.length` guards the empty case: no palette means we
                 cannot tell, and "we cannot tell" must not be shown as "this
                 will be dropped". -->
            {#if c && palette.length > 0 && !paletteName(c)}
              <span class="off-palette"
                    title="This exact colour is not one of the palette names a pack can carry, so exporting a pack will leave this state out. EVE's palette has eight names and this build has captured {palette.length} of them — a colour that is really green or purple reads as off-palette here too.">off-palette</span>
            {/if}
            {#if c}
              <Button size="sm" class="reset" onclick={() => resetColor(id)}
                      title="Remove the stored colour, restoring EVE's default">Reset</Button>
            {:else}
              <span class="default-note"
                    title={fallback
                      ? "No stored colour — this is EVE's built-in default for this state"
                      : "No stored colour, and EVE's built-in default for this state is unknown"}>default</span>
            {/if}
          {/snippet}
        </ListRow>
      </li>
      {/if}
    {/each}
  </ul>
{/if}

<style>
  /* The checkbox and colour-input dark-control rules are gone — Field owns
     both, and the local surface strip is Tabs.

     This strip was `.subtabs` until 2026-08-15, the same class OverviewView uses
     for the view-level strip — and since only this file declared the rule, and
     `:global` is required to reach into a child component, the PARENT's margin
     was being supplied by a child that merely happens to always be mounted
     (the sub-tabs render with `hidden`, never unmounted). Distinct names now, one
     rule each, same computed margin. */
  .bools { display: flex; flex-direction: column; gap: 0; margin-bottom: var(--s2); }
  .apply-note { color: var(--text-muted); font-size: var(--t-caption); margin: var(--s1) 0 0; }
  .meta { color: var(--text-muted); font-size: var(--t-caption); }
  :global(.surfaces) { margin: var(--s1) 0 var(--s2); }
  /* A reading width, for the same reason `.ov-cols` has one: ListRow pushes the
     swatch and its Reset button to the container's right edge, and the
     container is a wide work area rather than a 20rem panel. */
  .state-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 0; max-width: 26rem; }
  .state-list li { list-style: none; }
  .state-list :global(.state-label) { flex: 1; }
  /* An unset row shows a placeholder colour, so it takes the one disabled
     treatment to keep "unset" and "explicitly set" visually distinct. */
  .state-list :global(.unset) { opacity: var(--o-disabled); }
  .default-note { color: var(--text-muted); font-size: var(--t-caption); width: 3.4rem; }
  .off-palette { color: var(--warn); font-size: var(--t-caption); white-space: nowrap; }
  .state-list :global(.reset) { width: 3.4rem; }
</style>
