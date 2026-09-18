<script lang="ts">
  import { api, errMessage, errText, type Fleet, type HudEntry, type Rgb } from "./api";
  import { BROADCASTS, SHOW_OWN, listenField } from "./fleet";
  import { hexToRgb, rgbToHex, snapToPalette, UNSET_HEX } from "./colour";
  import { resolveNames } from "./names.svelte";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
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
</div>

<style>
  .fleet { display: flex; flex-direction: column; gap: var(--s4); max-width: 56rem; }
  .rows { display: flex; flex-direction: column; }
  .row { display: flex; align-items: center; gap: var(--s2); padding: var(--s1) 0; }
  .row :global(.box) { flex: 1; }
  .colour { display: flex; align-items: center; gap: var(--s1); }
  /* The one sanctioned opacity: a placeholder swatch is not content. */
  .colour :global(.unset) { opacity: var(--o-disabled); }
</style>
