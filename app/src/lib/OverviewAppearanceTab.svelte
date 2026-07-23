<script lang="ts">
  import { api, errMessage, type OverviewColumns } from "./api";
  import { message } from "@tauri-apps/plugin-dialog";
  import {
    stateLabel, rgbaToHex, hexToRgba, moveInOrder,
    DEFAULT_BACKGROUND_ORDER, DEFAULT_BACKGROUND_STATES,
    DEFAULT_FLAG_ORDER, DEFAULT_FLAG_STATES,
  } from "./states";

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

  // Shown in the swatch for a state with no stored colour. EVE's built-in
  // per-state defaults live in client script and aren't in any file we read, so
  // this is a neutral placeholder — the "default" marker beside it, not the
  // colour, is what tells the user the row is unset.
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
  // Render `order` verbatim — it's a priority list and carries ids the client
  // never draws (68), which must keep their slots. An enabled id somehow absent
  // from it is appended so it stays reachable instead of being invisible and
  // impossible to untick.
  const rows = $derived([...order, ...enabled.filter((id) => !order.includes(id))]);
  const enabledSet = $derived(new Set(enabled));
  const colors = $derived(new Map(appearance?.colors ?? []));
  const bools = $derived(new Map(appearance?.bools ?? []));

  async function edit(fn: () => Promise<OverviewColumns>) {
    try { onChanged(await fn()); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
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
    return edit(() => api.overviewSetStateColor(id, hexToRgba(hex, alpha)));
  }
  // Removing the entry is what restores EVE's default — writing a default-looking
  // colour is not the same thing.
  function resetColor(id: number) {
    return edit(() => api.overviewSetStateColor(id, null));
  }
</script>

{#if appearance}
  <div class="bools">
    {#each BOOL_LABELS as [key, label] (key)}
      {#if key === "applyToStructures"}
        <p class="apply-note">The Colortag and Background settings apply to ships and drones by default.</p>
      {/if}
      <label class="bool-row">
        <input type="checkbox" checked={bools.get(key) ?? false}
               onchange={(e) => edit(() => api.overviewSetBool(key, (e.currentTarget as HTMLInputElement).checked))} />
        {label}
      </label>
    {/each}
  </div>

  <div class="subtabs" role="tablist">
    <!-- EVE's own Appearance tab lists Colortag first, Background second. -->
    {#each ["Colortag", "Background"] as name}
      <button role="tab" aria-selected={surface === name} class:active={surface === name}
              onclick={() => (surface = name as "Background" | "Colortag")}>{name}</button>
    {/each}
  </div>

  {#if surfaceUnset}
    <p class="meta">This account has never customised its {surface} states, so these are EVE's
      defaults and aren't saved yet. Your first change here writes them to the file.</p>
  {/if}

  <ul class="state-list">
    {#each rows as id, i (id)}
      <li draggable="true"
          ondragstart={(e) => { dragFrom = i;
            // WebView2/Chromium won't fire `drop` unless dragstart sets data.
            e.dataTransfer?.setData("text/plain", String(i));
            if (e.dataTransfer) e.dataTransfer.effectAllowed = "move"; }}
          ondragover={(e) => { e.preventDefault();
            if (e.dataTransfer) e.dataTransfer.dropEffect = "move"; }}
          ondrop={(e) => { e.preventDefault(); drop(i); }}
          ondragend={() => (dragFrom = null)}>
        <span class="grip" title="Drag to reorder (priority — first match wins)">⠿</span>
        <label class="state-label">
          <input type="checkbox" checked={enabledSet.has(id)}
                 onchange={(e) => toggleState(id, (e.currentTarget as HTMLInputElement).checked)} />
          {stateLabel(id) ?? `#${id}`}
        </label>
        {#if isBg}
          {@const c = colors.get(id)}
          <input class="swatch" class:unset={!c} type="color" value={c ? rgbaToHex(c) : UNSET_HEX}
                 aria-label="Background colour"
                 onchange={(e) => setColor(id, (e.currentTarget as HTMLInputElement).value)} />
          {#if c}
            <button class="reset" onclick={() => resetColor(id)}
                    title="Remove the stored colour, restoring EVE's default">Reset</button>
          {:else}
            <span class="default-note" title="No stored colour — EVE uses its built-in default for this state">default</span>
          {/if}
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .bools { display: flex; flex-direction: column; gap: 0.15rem; margin-bottom: 0.6rem; }
  .bool-row { display: flex; gap: 0.35rem; align-items: center; }
  .apply-note { color: var(--fg-dim); font-size: 0.85em; margin: 0.4rem 0 0.15rem; }
  .meta { color: var(--fg-dim); font-size: 0.85em; }
  /* Same tab strip the parent view uses, scoped locally for Background/Colortag. */
  .subtabs { display: flex; gap: 0.3rem; margin: 0.2rem 0 0.5rem; border-bottom: 1px solid var(--border); }
  .subtabs button {
    background: none; border: none; border-bottom: 2px solid transparent;
    color: var(--fg-dim); padding: 0.3rem 0.7rem; font: inherit; cursor: pointer;
  }
  .subtabs button.active { color: var(--fg); border-bottom-color: var(--accent); }
  .state-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 0.1rem; }
  .state-list li { display: flex; align-items: center; gap: 0.5rem; padding: 0.15rem 0; }
  .state-label { display: flex; gap: 0.35rem; align-items: center; flex: 1; }
  .grip { cursor: grab; opacity: 0.6; }
  /* Dark native controls: the app runs in a dark WebView2, which renders bare
     checkboxes and colour inputs light (see the dark-native-controls memo). */
  input[type="checkbox"] { accent-color: var(--accent); }
  .swatch {
    background: var(--bg-panel); border: 1px solid var(--border); border-radius: 3px;
    width: 2.2rem; height: 1.3rem; padding: 1px; cursor: pointer;
  }
  /* An unset row shows a placeholder colour, so dim it to keep "unset" and
     "explicitly set" visually distinct. */
  .swatch.unset { opacity: 0.4; }
  .default-note { color: var(--fg-dim); font-size: 0.85em; width: 3.4rem; }
  .reset {
    background: var(--bg-panel); color: var(--fg); border: 1px solid var(--border);
    border-radius: 3px; padding: 1px 6px; font: inherit; font-size: 0.85em; cursor: pointer; width: 3.4rem;
  }
</style>
