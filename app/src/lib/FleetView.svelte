<script lang="ts">
  import { api, errMessage, errText, type Fleet, type FoundCharacter, type HudEntry, type Rgb } from "./api";
  import { BROADCASTS, SHOW_OWN, listenField } from "./fleet";
  import { hexToRgb, rgbToHex, snapToPalette, UNSET_HEX } from "./colour";
  import { names, resolveNames } from "./names.svelte";
  import { toast } from "./ui/toasts.svelte";
  import { undoAction } from "./undo.svelte";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";
  import Panel from "./ui/Panel.svelte";
  import PanelHeader from "./ui/PanelHeader.svelte";

  let {
    charOpen, userOpen, userId = null, charId = null, refreshToken = 0,
    onUserDirty, onCharDirty, onShowAccounts = () => {},
  }: {
    charOpen: boolean;
    userOpen: boolean;
    userId?: number | null;
    charId?: number | null;
    /** Bumped by every save, open, discard, backup restore and undo — the only
     *  signal that the documents changed underneath this view. */
    refreshToken?: number;
    onUserDirty: () => void;
    onCharDirty: () => void;
    onShowAccounts?: () => void;
  } = $props();

  let fleet = $state<Fleet | null>(null);
  let loadError = $state<string | null>(null);
  // One live message per panel — the panel is the control group that owns the
  // failure, and each handler nulls its own before it starts (05 §3.1).
  type Msg = { text: string; detail: string };
  type PanelId = "broadcast" | "watch" | "formation";
  let broadcastError = $state<Msg | null>(null);
  let watchError = $state<Msg | null>(null);
  let formationError = $state<Msg | null>(null);
  function setError(panel: PanelId, m: Msg | null) {
    if (panel === "broadcast") broadcastError = m;
    else if (panel === "watch") watchError = m;
    else formationError = m;
  }

  async function reload() {
    if (!charOpen && !userOpen) { fleet = null; return; }
    loadError = null;
    try {
      fleet = await api.fleet();
      void resolveNames(fleet.watchlist.map((w) => w.char_id));
    } catch (e) { loadError = errMessage(e); }
  }
  $effect(() => { void charOpen; void userOpen; void userId; void charId; void refreshToken; reload(); });

  const palette = $derived(fleet?.palette ?? []);
  const field = (name: string): HudEntry | undefined => fleet?.fields.find((f) => f.name === name);
  /** The stored value, or EVE's default when the key is absent. */
  const shown = (name: string): string => { const e = field(name); return e?.value ?? e?.default ?? ""; };
  const unavailable = (name: string): boolean => field(name)?.set.how === "unavailable";
  const NOT_EDITABLE = "This value has an unexpected type here";

  const colourEntry = (broadcast: string) => fleet?.colours.find((c) => c.broadcast === broadcast);
  /** What the swatch shows: the stored colour, else the type's default, else nothing. */
  function swatchRgb(broadcast: string): Rgb | null {
    const c = colourEntry(broadcast);
    if (!c) return null;
    if (c.state.state === "set") return c.state.rgb;
    if (c.state.state === "absent") return c.default;
    return null;
  }
  /** Whether ✕ has anything left to do. */
  function noColour(broadcast: string): boolean {
    const c = colourEntry(broadcast);
    return !c || c.state.state === "cleared" || (c.state.state === "absent" && c.default === null);
  }

  /** Run one write, replace the projection, mark the right file dirty; on
   *  failure put the R4 sentence on the panel that owns the control. */
  async function write(panel: PanelId, subject: string, fn: () => Promise<Fleet>, dirty: () => void): Promise<boolean> {
    setError(panel, null);
    try {
      fleet = await fn();
      dirty();
      return true;
    } catch (e) {
      setError(panel, { text: `${subject} — ${errText(e)}`, detail: errMessage(e) });
      return false;
    }
  }

  const setBroadcastField = (name: string, on: boolean) =>
    write("broadcast", "That broadcast setting wasn't changed", () => api.setFleetField(name, on ? "1" : "0"), onUserDirty);
  const pickColour = (broadcast: string, hex: string) =>
    write("broadcast", "That colour wasn't changed", () => api.setFleetColour(broadcast, snapToPalette(hex, palette) ?? hexToRgb(hex)), onUserDirty);
  const clearColour = (broadcast: string) =>
    write("broadcast", "That colour wasn't changed", () => api.setFleetColour(broadcast, null), onUserDirty);

  const checked = (e: Event) => (e.currentTarget as HTMLInputElement).checked;
  const picked = (e: Event) => (e.currentTarget as HTMLInputElement).value;

  // --- watch list ---------------------------------------------------------
  const nameOf = (id: number): string => names[id]?.name ?? String(id);
  let addQuery = $state("");
  // Blue is what 863 of the corpus's 1,382 entries chose; the other 519 are all
  // one other colour, so it is the right starting swatch.
  let addHex = $state(rgbToHex([0.2, 0.5, 1.0]));
  let adding = $state(false);

  const recolour = (id: number, hex: string) =>
    write("watch", `${nameOf(id)}'s colour wasn't changed`, () => api.setWatchlistColour(id, snapToPalette(hex, palette) ?? hexToRgb(hex)), onCharDirty);
  const remove = (id: number) =>
    write("watch", `${nameOf(id)} wasn't removed`, () => api.setWatchlistColour(id, null), onCharDirty);

  async function add(ev: SubmitEvent) {
    ev.preventDefault();
    const q = addQuery.trim();
    if (!q || adding) return;
    watchError = null;
    adding = true;
    try {
      let found: FoundCharacter | null;
      try { found = await api.lookupCharacter(q); }
      catch (e) { watchError = { text: `${q} wasn't looked up — couldn't reach ESI`, detail: errMessage(e) }; return; }
      if (!found) { watchError = { text: `No character called ${q}`, detail: "" }; return; }
      const id = found.id;
      if (fleet?.watchlist.some((w) => w.char_id === id)) { watchError = { text: `${found.name} is already in the list`, detail: "" }; return; }
      const rgb = snapToPalette(addHex, palette) ?? hexToRgb(addHex);
      const ok = await write("watch", `${found.name} wasn't added`, () => api.setWatchlistColour(id, rgb), onCharDirty);
      if (ok) {
        void resolveNames([id]);
        addQuery = "";
        // The new row can land below the fold, so the success is said out loud.
        toast(`Added ${found.name}`, { action: undoAction() });
      }
    } finally { adding = false; }
  }

  // --- formation ----------------------------------------------------------
  const setFormationField = (name: string, text: string) =>
    write("formation", "That formation setting wasn't changed", () => api.setFleetField(name, text), onCharDirty);

  // Int fields: round before writing, and put the input back in step with the
  // model whether or not the write landed — Svelte only patches `value` when
  // the expression changes, so a refused edit would otherwise sit on screen
  // beside a value that is not it (HudPanel's discipline).
  const numberEdit = (name: string) => async (ev: Event) => {
    const el = ev.target as HTMLInputElement;
    const text = el.value;
    if (text.trim() !== "" && Number.isFinite(Number(text))) {
      await setFormationField(name, String(Math.round(Number(text))));
    }
    el.value = shown(name);
  };
  const NUMBERS: { name: string; label: string; step: number }[] = [
    { name: "formation", label: "Formation", step: 1 },
    { name: "formation_size", label: "Size", step: 100 },
    { name: "formation_spacing", label: "Spacing", step: 100 },
  ];
</script>

<div class="fleet">
  {#if loadError}
    <InlineMessage variant="error">{loadError}</InlineMessage>
  {/if}
  <!-- The palette as suggestions in the native colour picker: a hint, not a
       constraint. A picked palette colour is snapped to EVE's exact floats. -->
  <datalist id="fleet-palette">
    {#each palette as [name, c] (name)}<option value={rgbToHex(c)}></option>{/each}
  </datalist>

  <Panel class="broadcasts">
    <PanelHeader title="Broadcast settings" subtitle="Which broadcasts you receive, and the colour each shows in">
      {#snippet actions()}<Chip size="sm">account file</Chip>{/snippet}
    </PanelHeader>
    {#if !userOpen}
      <EmptyState title="No account paired" description="Broadcast settings live in the account file.">
        {#snippet action()}<Button onclick={onShowAccounts}>Pair this character…</Button>{/snippet}
      </EmptyState>
    {:else if fleet}
      {#if broadcastError}
        <InlineMessage variant="error" detail={broadcastError.detail}>{broadcastError.text}</InlineMessage>
      {/if}
      <div class="rows">
        <!-- EVE's own layout: checkbox, label, then the swatch. Field's label
             wraps the checkbox, so the whole caption is the hit target. -->
        <div class="row">
          <Field kind="checkbox" label={SHOW_OWN.label} value={shown(SHOW_OWN.field) === "1"}
            disabled={unavailable(SHOW_OWN.field)} disabledReason={NOT_EDITABLE}
            onchange={(e) => setBroadcastField(SHOW_OWN.field, checked(e))} />
        </div>
        {#each BROADCASTS as b (b.type)}
          {@const name = listenField(b.type)}
          {@const rgb = swatchRgb(b.type)}
          {@const unreadable = colourEntry(b.type)?.state.state === "unreadable"}
          <div class="row">
            <Field kind="checkbox" label={b.label} value={shown(name) === "1"}
              disabled={unavailable(name)} disabledReason={NOT_EDITABLE}
              onchange={(e) => setBroadcastField(name, checked(e))} />
            <span class="colour">
              <!-- An unset swatch shows a placeholder and takes the one disabled
                   treatment, so "no colour" and "black" cannot be confused —
                   the appearance tab's convention. -->
              <Field kind="color" list="fleet-palette" controlClass={rgb ? "" : "unset"}
                value={rgb ? rgbToHex(rgb) : UNSET_HEX}
                ariaLabel="Colour for {b.label}" title={rgb ? undefined : "No colour"}
                disabled={unreadable} disabledReason={NOT_EDITABLE}
                onchange={(e) => pickColour(b.type, picked(e))} />
              <Button variant="ghost" size="sm" iconOnly title="No colour"
                disabled={noColour(b.type)} disabledReason="Already no colour"
                onclick={() => clearColour(b.type)}>✕</Button>
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </Panel>

  <Panel class="watchlist">
    <PanelHeader title="Watch list colours" subtitle="The colour a fleet-mate shows in your watch list">
      {#snippet actions()}<Chip size="sm">character file</Chip>{/snippet}
    </PanelHeader>
    {#if !charOpen}
      <EmptyState title="No character open" description="Watch-list colours live in the character file." />
    {:else if fleet}
      {#if watchError}
        <InlineMessage variant="error" detail={watchError.detail || undefined}>{watchError.text}</InlineMessage>
      {/if}
      {#if fleet.watchlist.length === 0}
        <EmptyState title="No watch-list colours"
          description="Colours you set on watch-list members in-game appear here. Add one below to colour a character before you next fleet with them." />
      {:else}
        <ul class="watch-list">
          {#each fleet.watchlist as w (w.char_id)}
            {@const label = nameOf(w.char_id)}
            <li>
              <ListRow title={String(w.char_id)}>
                {#snippet leading()}
                  <Field kind="color" list="fleet-palette" controlClass={w.rgb ? "" : "unset"}
                    value={w.rgb ? rgbToHex(w.rgb) : UNSET_HEX} ariaLabel="Colour for {label}"
                    disabled={w.rgb === null} disabledReason={NOT_EDITABLE}
                    onchange={(e) => recolour(w.char_id, picked(e))} />
                {/snippet}
                <span class="label">{label}</span>
                {#snippet trailing()}
                  {#if w.rgb === null}<Chip tone="warn" size="sm">unreadable</Chip>{/if}
                  {#if label !== String(w.char_id)}<span class="meta">{w.char_id}</span>{/if}
                  <Button variant="ghost" size="sm" iconOnly title="Remove from the list"
                    onclick={() => remove(w.char_id)}>✕</Button>
                {/snippet}
              </ListRow>
            </li>
          {/each}
        </ul>
      {/if}
      <form class="add" onsubmit={add}>
        <Field kind="text" label="Add a character" placeholder="Name or ID" width="18rem" bind:value={addQuery} />
        <Field kind="color" list="fleet-palette" ariaLabel="Colour for the new entry" bind:value={addHex} />
        <Button variant="primary" type="submit" disabled={addQuery.trim() === "" || adding}
          disabledReason={adding ? "Looking the character up…" : "Type a character name or ID"}>Add</Button>
      </form>
    {/if}
  </Panel>

  <Panel class="formation">
    <PanelHeader title="Formation" subtitle="Fleet-warp formation, and the fleet finder">
      {#snippet actions()}<Chip size="sm">character file</Chip>{/snippet}
    </PanelHeader>
    {#if !charOpen}
      <EmptyState title="No character open" description="Formation settings live in the character file." />
    {:else if fleet}
      {#if formationError}
        <InlineMessage variant="error" detail={formationError.detail}>{formationError.text}</InlineMessage>
      {/if}
      <div class="rows">
        {#each NUMBERS as n (n.name)}
          <div class="row">
            <Field kind="number" label={n.label} min={0} step={n.step} width="8rem"
              value={shown(n.name)} disabled={unavailable(n.name)} disabledReason={NOT_EDITABLE}
              onchange={numberEdit(n.name)} />
            {#if n.name !== "formation"}<span class="meta">m</span>{/if}
          </div>
        {/each}
        <div class="row">
          <Field kind="checkbox" label="Show only my corp, alliance and high-standing fleets"
            value={shown("finder_group_only") === "1"}
            disabled={unavailable("finder_group_only")} disabledReason={NOT_EDITABLE}
            onchange={(e) => setFormationField("finder_group_only", checked(e) ? "1" : "0")} />
        </div>
      </div>
      <p class="meta">Saved fleet setups live on CCP's servers and can't be edited here.</p>
    {/if}
  </Panel>
</div>

<style>
  .fleet { display: flex; flex-direction: column; gap: var(--s4); max-width: 56rem; }
  .rows { display: flex; flex-direction: column; }
  .row { display: flex; align-items: center; gap: var(--s2); padding: var(--s1) 0; }
  .row :global(.box) { flex: 1; }
  .colour { display: flex; align-items: center; gap: var(--s1); }
  /* The one sanctioned opacity: a placeholder swatch is not content. */
  .colour :global(.unset) { opacity: var(--o-disabled); }

  .watch-list { list-style: none; margin: 0; padding: 0; max-width: 32rem; }
  .watch-list .label { flex: 1; }
  .meta { color: var(--text-muted); font-size: var(--t-caption); }
  .add { display: flex; align-items: flex-end; gap: var(--s2); margin-top: var(--s3); }
</style>
