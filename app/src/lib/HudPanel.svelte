<script lang="ts">
  import type { Hud, HudEntry } from "$lib/api";

  let { hud, readOnly, onSet }: {
    hud: Hud;
    readOnly: boolean;
    onSet: (name: string, text: string) => void;
  } = $props();

  // Display order and labels. Account-scoped rows are flagged in the UI because
  // they change every character on the account.
  const GROUPS: { title: string; rows: { name: string; label: string }[] }[] = [
    { title: "Ship HUD", rows: [
      { name: "ship_offset", label: "Offset from centre" },
      { name: "ship_top", label: "Align to top" },
    ] },
    { title: "Fighter UI", rows: [
      { name: "fighter_x", label: "x" },
      { name: "fighter_y", label: "y" },
      { name: "fighter_detached", label: "Detached" },
      { name: "fighter_shown", label: "Shown" },
    ] },
    { title: "Neocom", rows: [{ name: "neocom_width", label: "Width" }] },
    { title: "Notification badge", rows: [
      { name: "badge_x", label: "x" },
      { name: "badge_y", label: "y" },
    ] },
  ];

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

  const numberEdit = (name: string) => (ev: Event) => {
    const raw = (ev.target as HTMLInputElement).value;
    if (raw.trim() !== "" && Number.isFinite(Number(raw))) onSet(name, raw);
  };
</script>

<div class="hud-panel">
  {#each GROUPS as g (g.title)}
    <div class="group">
      <h4>{g.title}</h4>
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
                value={shown(row.name)}
                disabled={disabled(e)}
                onchange={numberEdit(row.name)} />
            {/if}
            {#if e.scope === "account"}<span class="badge">account</span>{/if}
            {#if e.value === null && e.set.how !== "unavailable"}<span class="badge">default</span>{/if}
          </label>
        {/if}
      {/each}
    </div>
  {/each}
</div>

<style>
  .hud-panel {
    border-bottom: 1px solid #333;
    padding: 0.4rem 0.5rem;
    font-size: 12px;
  }
  .group {
    margin-bottom: 0.4rem;
  }
  h4 {
    margin: 0.2rem 0;
    color: #9aa4b2;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
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
