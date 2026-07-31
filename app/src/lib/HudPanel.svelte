<script lang="ts">
  import type { Hud, HudEntry, HudKind, NeocomBar } from "$lib/api";
  import type { FurnitureRect } from "$lib/layout";
  import NeocomButtons from "$lib/NeocomButtons.svelte";

  let {
    hud, readOnly, accountReadOnly = false, onSet, sharedNames = [], selectedKind = null, onSelectKind,
    neocom = null, neocomBusy = false, onNeocomReorder, onNeocomRemove, onNeocomAdd, onNeocomReset,
  }: {
    hud: Hud;
    readOnly: boolean;
    /** The ACCOUNT document's read-only flag, which only the account-scoped
     * rows care about. False when no account file is open — those rows are
     * already `unavailable` then. */
    accountReadOnly?: boolean;
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
    // The target list's anchor is stored as a FRACTION of the screen, not in
    // pixels like every other row here, so that is what these two show. x is a
    // fraction of the width right of the neocom (see layout.ts) — the labels
    // say so, because a bare 0.54 next to a 2519 invites the reader to assume
    // they are the same kind of number. Dragging the slot on the canvas is the
    // ergonomic path; these are for typing an exact value back.
    { title: "Target list", kind: "target", rows: [
      { name: "target_x", label: "x (fraction)" },
      { name: "target_y", label: "y (fraction)" },
      { name: "target_horizontal", label: "Horizontal" },
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
  // `readOnly` is the CHARACTER document's flag. An account-scoped row writes
  // the account file, so a read-only account left those four rows clickable and
  // the backend's refusal arrived as a dialog — stricter to say so up front.
  const disabled = (e: HudEntry) =>
    readOnly || e.set.how === "unavailable" || (e.scope === "account" && accountReadOnly);
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
  //
  // The field is put back in step with the model on every commit, whatever
  // happens next. Svelte only patches `value` when the EXPRESSION changes, so an
  // edit that does not move the model — one this refuses, one the backend
  // refuses, or an int rounding back to what it already was (326.4 -> 326) —
  // left the typed text sitting on screen beside a value that is not it. If the
  // write does land, the parent's re-render overwrites this a moment later.
  const numberEdit = (name: string, kind: HudKind) => (ev: Event) => {
    const el = ev.target as HTMLInputElement;
    const raw = el.value;
    const resync = () => { el.value = shown(name); };
    if (raw.trim() === "" || !Number.isFinite(Number(raw))) {
      resync();
      return;
    }
    onSet(name, kind === "float" ? raw : String(Math.round(Number(raw))));
    resync();
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
    border-bottom: 1px solid var(--border);
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
  /* The two ambers below are deliberately NOT app.css variables: they match
     the canvas's selected-furniture colour (LayoutView's own #f59e0b/#fde68a),
     and the pair has to move together or the panel stops agreeing with the
     rectangle it describes. Everything else here now follows the palette. */
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
    color: var(--fg-dim);
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
    color: var(--fg);
    min-width: 8.5rem;
  }
  /* Native controls render light in WebView2 unless told otherwise. */
  input[type="number"] {
    width: 5.5rem;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--border);
  }
  input[type="number"]:disabled {
    color: var(--fg-dim);
  }
  input[type="checkbox"] {
    accent-color: var(--accent);
  }
  .badge {
    color: var(--fg-dim);
    background: var(--bg-panel);
    border-radius: 3px;
    padding: 0 4px;
    font-size: 10px;
  }
</style>
