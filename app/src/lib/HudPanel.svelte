<script lang="ts">
  import type { Hud, HudEntry, HudKind, NeocomBar } from "$lib/api";
  import type { FurnitureRect } from "$lib/layout";
  import NeocomButtons from "$lib/NeocomButtons.svelte";

  let {
    hud, readOnly, onSet, sharedNames = [], selectedKind = null, onSelectKind,
    neocom = null, neocomBusy = false, onNeocomReorder, onNeocomRemove, onNeocomAdd, onNeocomReset,
  }: {
    hud: Hud;
    readOnly: boolean;
    onSet: (name: string, text: string) => void;
    /** Other characters on this account, for the account-row legend. */
    sharedNames?: string[];
    /** The furniture selected on the canvas — its group is highlighted here. */
    selectedKind?: FurnitureRect["kind"] | null;
    /** Selecting a group here highlights the matching rectangle on the canvas,
     * the mirror of clicking that rectangle. */
    onSelectKind: (kind: FurnitureRect["kind"]) => void;
    /** The neocom bar, when a character file is open. Rendered inside the
     * Neocom group so the bar the user clicks on the canvas and the buttons
     * they edit are the same object. */
    neocom?: NeocomBar | null;
    /** True while a neocom command is in flight — disables the whole list so
     * a second click can't reuse a now-stale index before the re-projection
     * lands. */
    neocomBusy?: boolean;
    onNeocomReorder: (order: number[]) => void;
    onNeocomRemove: (index: number) => void;
    onNeocomAdd: (id: string, btnType: number, iconPath: string) => void;
    onNeocomReset: () => void;
  } = $props();

  // Display order and labels. `kind` ties a group to the canvas element it
  // edits, so selecting furniture highlights the fields that move it.
  // Account-scoped rows are badged because they change every character on the
  // account — the legend below names them.
  const GROUPS: { title: string; kind: FurnitureRect["kind"]; rows: { name: string; label: string }[] }[] = [
    { title: "Ship HUD", kind: "shipui", rows: [
      { name: "ship_offset", label: "Offset from centre" },
      { name: "ship_top", label: "Align to top" },
    ] },
    { title: "Fighter UI", kind: "fighter", rows: [
      { name: "fighter_x", label: "x" },
      { name: "fighter_y", label: "y" },
      { name: "fighter_detached", label: "Detached" },
      { name: "fighter_shown", label: "Shown" },
    ] },
    { title: "Neocom", kind: "neocom", rows: [{ name: "neocom_width", label: "Width" }] },
    { title: "Notification badge", kind: "badge", rows: [
      { name: "badge_x", label: "x" },
      { name: "badge_y", label: "y" },
    ] },
  ];

  // Only claim rows are shared when at least one of them is actually writable —
  // with no account file open they all read `unavailable`, and the claim would
  // be describing something the user cannot see or change.
  const anyAccountRow = $derived(
    hud.entries.some((e) => e.scope === "account" && e.set.how !== "unavailable"),
  );

  const find = (name: string): HudEntry | undefined => hud.entries.find((e) => e.name === name);
  const shown = (name: string) => find(name)?.value ?? find(name)?.default ?? "";
  const disabled = (e: HudEntry) => readOnly || e.set.how === "unavailable";
  const title = (e: HudEntry) =>
    e.set.how === "unavailable"
      ? "Not present in this file"
      : e.value === null
        ? `EVE's default (${e.default}) — editing stores a value`
        : e.scope === "account"
          ? "Account-wide: changes every character on this account"
          : "";

  // Int fields round before writing: <input type="number"> doesn't enforce
  // integrality (typing "1.5" is not blocked), and the backend's Int parser
  // rejects a fractional string outright.
  const numberEdit = (name: string, kind: HudKind) => (ev: Event) => {
    const raw = (ev.target as HTMLInputElement).value;
    if (raw.trim() === "" || !Number.isFinite(Number(raw))) return;
    onSet(name, kind === "float" ? raw : String(Math.round(Number(raw))));
  };
</script>

<div class="hud-panel">
  {#if anyAccountRow}
    <p class="account-legend">
      <span class="badge">account</span> rows are stored once for the whole account{sharedNames.length
        ? ` — editing one also changes ${sharedNames.join(", ")}`
        : " — every character on it"}.
    </p>
  {/if}
  {#each GROUPS as g (g.title)}
    <div class="group" class:selected={selectedKind === g.kind}>
      <h4><button class="group-title" onclick={() => onSelectKind(g.kind)}>{g.title}</button></h4>
      {#each g.rows as row (row.name)}
        {@const e = find(row.name)}
        {#if e}
          <label class="row" title={title(e)}>
            {#if e.kind === "bool"}
              <input
                type="checkbox"
                checked={shown(row.name) === "true"}
                disabled={disabled(e)}
                onchange={(ev) => onSet(row.name, (ev.target as HTMLInputElement).checked ? "true" : "false")} />
              <span class="label">{row.label}</span>
            {:else}
              <span class="label">{row.label}</span>
              <input
                type="number"
                step={e.kind === "float" ? undefined : "1"}
                value={shown(row.name)}
                disabled={disabled(e)}
                onchange={numberEdit(row.name, e.kind)} />
            {/if}
            {#if e.scope === "account"}<span class="badge">account</span>{/if}
            {#if e.value === null && e.set.how !== "unavailable"}<span class="badge">default</span>{/if}
          </label>
        {/if}
      {/each}
      {#if g.kind === "neocom" && neocom}
        <NeocomButtons
          bar={neocom}
          {readOnly}
          busy={neocomBusy}
          onReorder={onNeocomReorder}
          onRemove={onNeocomRemove}
          onAdd={onNeocomAdd}
          onReset={onNeocomReset} />
      {/if}
    </div>
  {/each}
</div>

<style>
  .hud-panel {
    border-bottom: 1px solid #333;
    padding: 0.4rem 0.5rem;
    font-size: 12px;
  }
  .account-legend {
    margin: 0 0 0.5rem;
    padding: 0.25rem 0.4rem;
    color: var(--fg-dim);
    background: var(--bg-panel);
    border-left: 2px solid var(--accent);
    font-size: 11px;
  }
  .group {
    margin-bottom: 0.4rem;
    /* Transparent by default so selecting a group doesn't shift the layout. */
    border-left: 2px solid transparent;
    padding-left: 0.3rem;
  }
  /* Matches the canvas's selected-furniture colour, so it's obvious which
     fields the highlighted rectangle edits. */
  .group.selected {
    border-left-color: #f59e0b;
    background: rgba(245, 158, 11, 0.08);
  }
  h4 {
    margin: 0.2rem 0;
  }
  /* A button so it's keyboard-reachable, styled as the heading it replaces
     (same pattern as WindowPanel's window-name button). */
  .group-title {
    padding: 0;
    background: none;
    border: none;
    color: #9aa4b2;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    cursor: pointer;
  }
  .group.selected .group-title {
    color: #fde68a;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 1px 0;
  }
  .label {
    color: #cbd5e1;
    min-width: 8.5rem;
  }
  /* Native controls render light in WebView2 unless told otherwise. */
  input[type="number"] {
    width: 5.5rem;
    background: #1b1f27;
    color: #e5e7eb;
    border: 1px solid #444;
  }
  input[type="number"]:disabled {
    color: #6b7280;
  }
  input[type="checkbox"] {
    accent-color: var(--accent);
  }
  .badge {
    color: #94a3b8;
    background: #262b36;
    border-radius: 3px;
    padding: 0 4px;
    font-size: 10px;
  }
</style>
