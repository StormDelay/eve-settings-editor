<script lang="ts">
  import { api, errMessage, type Formation, type Formations } from "./api";
  import { fromUnit, toSpherical, toCartesian, cubeFormation, formatUnit,
           DEFAULT_RANGE_M, paneScale, project, DRIFTER, drifterHole,
           type Unit, type Plane } from "./probes";
  import { message } from "@tauri-apps/plugin-dialog";

  let { userOpen, userId = null, onUserDirty, onShowAccounts = () => {}, sharedLabel = "" }:
    { userOpen: boolean; userId?: number | null; onUserDirty: () => void;
      onShowAccounts?: () => void; sharedLabel?: string } = $props();

  /** The projection as loaded. `null` before the first load. */
  let loaded = $state<Formations | null>(null);
  let error = $state<string | null>(null);
  let selectedId = $state<number | null>(null);
  let unit = $state<Unit>("au");
  /** Off by default: a hardcoded, single-scenario overlay, not a feature the
   * user asked to configure. */
  let showDrifter = $state(false);

  // THE EDIT BUFFER, IN METRES. Every displayed number is derived from this;
  // nothing derived is ever read back into it. A field the user has not typed
  // into therefore keeps its exact f64 from the file, which is the whole reason
  // this is not bound straight to the inputs (spec §4.2).
  let draftName = $state("");
  let draftRange = $state(0);
  let draftProbes = $state<[number, number, number][]>([]);
  /** Last angles entered per probe, so a probe pulled to r == 0 and back does
   * not silently snap onto the X axis. */
  let lastAngles = $state<{ az: number; el: number }[]>([]);

  /** Which probe row is focused, so a row and its dots highlight together in
   * both panes (Task 7). */
  let selectedProbe = $state<number | null>(null);

  const PANE = 320; // px, both panes square and identical
  // One scale for both panes, so a distance that looks longer in one pane
  // genuinely is longer — never derive each pane's scale separately.
  const scale = $derived(paneScale(draftProbes, draftRange, PANE));

  /** Pane pixel coordinates for a probe, origin at the pane centre. SVG y
   * grows downward, so the vertical data axis is negated. */
  const at = (p: [number, number, number], plane: Plane) => {
    const [a, b] = project(p, plane);
    return { cx: PANE / 2 + a / scale, cy: PANE / 2 - b / scale };
  };

  const PANES: { plane: Plane; label: string }[] = [
    { plane: "top", label: "top-down (X/Z)" },
    { plane: "side", label: "side (X/Y)" },
  ];

  /** While a field has focus, show exactly what's typed rather than a fresh
   * formatted re-derivation of the committed value on every keystroke —
   * otherwise reformatting (trailing-zero stripping, a forced decimal place)
   * fights the user's own typing and displaces the caret. `rawText` holds
   * what's currently in the box; `focusedField` which key, if any, is live. */
  let focusedField = $state<string | null>(null);
  let rawText = $state<Record<string, string>>({});
  function focusField(key: string, computedNow: string) {
    focusedField = key;
    rawText[key] = computedNow;
  }
  function typeField(key: string, text: string) {
    rawText[key] = text;
  }
  function blurField() {
    focusedField = null;
    commit();
  }
  function shown(key: string, computed: string): string {
    return focusedField === key ? (rawText[key] ?? computed) : computed;
  }

  const current = $derived(loaded?.formations.find((f) => f.id === selectedId) ?? null);

  /** The probes whose range differs from the first, 1-based to match the table.
   * `ranges` is the file's own per-probe values, which is why this can name
   * rows rather than just reporting that they disagree (spec §4.3). */
  const mixedProbeLabel = $derived.by(() => {
    const rs = current?.ranges ?? [];
    const odd = rs.map((r, n) => (r === rs[0] ? null : n + 1)).filter((n) => n !== null);
    return odd.length ? `probes ${odd.join(", ")}` : "";
  });

  async function reload() {
    if (!userOpen) { loaded = null; return; }
    error = null;
    try {
      loaded = await api.probeFormations();
    } catch (e) {
      const code = (e as { code?: string }).code;
      // No formations key at all is an empty list you can add to, not an error:
      // set_probe_formation mints the key.
      if (code === "no_formations") { loaded = { formations: [], selected: null }; }
      else { error = errMessage(e); loaded = null; return; }
    }
    if (!loaded.formations.some((f) => f.id === selectedId)) {
      select(loaded.formations[0] ?? null);
    }
  }
  $effect(() => { void userOpen; void userId; reload(); });

  function select(f: Formation | null) {
    selectedId = f?.id ?? null;
    draftName = f?.name ?? "";
    draftRange = f?.range ?? 0;
    draftProbes = f ? f.probes.map((p) => [...p] as [number, number, number]) : [];
    lastAngles = draftProbes.map((p) => { const s = toSpherical(p); return { az: s.az, el: s.el }; });
  }

  async function commit(id: number | null = selectedId) {
    // `next_id` fills the lowest free gap, not the end of the list, so a
    // freshly minted id can land in the MIDDLE of the sorted response — never
    // identify it by position.
    const before = new Set(loaded?.formations.map((f) => f.id) ?? []);
    try {
      loaded = await api.setProbeFormation(id, draftName, draftProbes, draftRange);
      onUserDirty();
      if (id === null) select(loaded.formations.find((f) => !before.has(f.id)) ?? null);
    } catch (e) {
      await message(errMessage(e), { title: "Could not save the formation", kind: "error" });
      await reload();
      // `reload` only re-selects when `selectedId` vanished. On an id===null
      // failure (createNew/duplicate/copy) selectedId is still the pre-existing
      // formation that was never touched, but the draft is left holding the
      // failed attempt's name/probes — re-sync it here so the NEXT blur can't
      // commit that draft over the still-selected, untouched formation.
      select(loaded?.formations.find((f) => f.id === selectedId) ?? null);
    }
  }

  /** Replace one cartesian component from a typed display value. */
  function setAxis(i: number, axis: 0 | 1 | 2, text: string) {
    const v = Number(text);
    if (!Number.isFinite(v)) return;
    const next = draftProbes.map((p) => [...p] as [number, number, number]);
    next[i][axis] = fromUnit(v, unit);
    draftProbes = next;
    const s = toSpherical(next[i]);
    if (s.r !== 0) lastAngles[i] = { az: s.az, el: s.el };
  }

  /** Scale a probe to a new distance, preserving its angles. */
  function setDistance(i: number, text: string) {
    const v = Number(text);
    if (!Number.isFinite(v)) return;
    const { az, el } = lastAngles[i] ?? toSpherical(draftProbes[i]);
    const next = draftProbes.map((p) => [...p] as [number, number, number]);
    next[i] = toCartesian({ r: fromUnit(v, unit), az, el });
    draftProbes = next;
  }

  /** Rotate a probe, preserving its distance. */
  function setAngle(i: number, which: "az" | "el", text: string) {
    const v = Number(text);
    if (!Number.isFinite(v)) return;
    const s = toSpherical(draftProbes[i]);
    const angles = { ...(lastAngles[i] ?? { az: s.az, el: s.el }), [which]: v };
    lastAngles[i] = angles;
    const next = draftProbes.map((p) => [...p] as [number, number, number]);
    next[i] = toCartesian({ r: s.r, ...angles });
    draftProbes = next;
  }

  function addProbe() {
    if (draftProbes.length >= 8) return;
    draftProbes = [...draftProbes, [draftRange / 2, 0, 0]];
    lastAngles = [...lastAngles, { az: 0, el: 0 }];
  }

  function removeProbe(i: number) {
    if (draftProbes.length <= 1) return;
    draftProbes = draftProbes.filter((_, j) => j !== i);
    lastAngles = lastAngles.filter((_, j) => j !== i);
  }

  async function createNew() {
    draftName = "New formation";
    draftRange = DEFAULT_RANGE_M;
    draftProbes = cubeFormation(DEFAULT_RANGE_M);
    lastAngles = draftProbes.map((p) => { const s = toSpherical(p); return { az: s.az, el: s.el }; });
    await commit(null);
  }

  async function duplicate() {
    if (!current) return;
    draftName = `${current.name} copy`;
    await commit(null);
  }

  async function remove() {
    if (selectedId === null) return;
    try {
      loaded = await api.removeProbeFormation(selectedId);
      onUserDirty();
      select(loaded.formations[0] ?? null);
    } catch (e) {
      await message(errMessage(e), { title: "Could not delete the formation", kind: "error" });
    }
  }
</script>

{#if !userOpen}
  <p class="hint">
    Probe formations live in the account file.
    <button class="link" onclick={onShowAccounts}>Pair this character with its account</button>
    to edit them.
  </p>
{:else if error}
  <p class="error">{error}</p>
{:else if loaded}
  {#if sharedLabel}<p class="shared-banner">{sharedLabel}</p>{/if}
  <div class="probes">
    <aside class="formation-list">
      <ul>
        {#each loaded.formations as f (f.id)}
          <li>
            <button class:active={f.id === selectedId} onclick={() => select(f)}>{f.name}</button>
          </li>
        {/each}
      </ul>
      <div class="list-actions">
        <button onclick={createNew}>New</button>
        <button onclick={duplicate} disabled={!current}>Duplicate</button>
        <button class="danger" onclick={remove} disabled={!current}>Delete</button>
      </div>
    </aside>

    {#if current}
      <section class="formation">
        <div class="row">
          <label>
            Name
            <input value={draftName}
                   disabled={current.mixed_range}
                   oninput={(e) => (draftName = e.currentTarget.value)}
                   onblur={() => commit()} />
          </label>
          <label>
            Range
            <input aria-label="formation range"
                   value={shown("range", formatUnit(draftRange, unit))}
                   disabled={current.mixed_range}
                   oninput={(e) => {
                     typeField("range", e.currentTarget.value);
                     const v = Number(e.currentTarget.value);
                     // A range of zero or less is meaningless in EVE and would
                     // otherwise be written straight to the user's settings file.
                     if (Number.isFinite(v) && v > 0) draftRange = fromUnit(v, unit);
                   }}
                   onfocus={() => focusField("range", formatUnit(draftRange, unit))}
                   onblur={blurField} />
          </label>
          <span class="units">
            <button class:active={unit === "au"} onclick={() => (unit = "au")}>AU</button>
            <button class:active={unit === "km"} onclick={() => (unit = "km")}>km</button>
          </span>
        </div>

        {#if current.mixed_range}
          <p class="warn">
            This formation's probes carry different ranges
            ({mixedProbeLabel}). The editor writes one range for the whole
            formation, so it is shown read-only here rather than silently
            flattening them on your next edit.
            <button class="link" onclick={duplicate}>Copy with uniform range</button>
            to get a copy you can edit — the original is left untouched.
          </p>
        {/if}

        <table>
          <thead>
            <tr>
              <th>#</th><th>X</th><th>Y</th><th>Z</th>
              <th>dist</th><th>az°</th><th>el°</th><th></th>
            </tr>
          </thead>
          <tbody>
            {#each draftProbes as p, n}
              {@const s = toSpherical(p)}
              <tr class:selected={selectedProbe === n} onfocusin={() => (selectedProbe = n)}>
                <td>{n + 1}</td>
                {#each [0, 1, 2] as axis}
                  <td>
                    <input aria-label={`probe ${n + 1} ${"XYZ"[axis]}`}
                           value={shown(`${n}:${axis}`, formatUnit(p[axis], unit))}
                           disabled={current.mixed_range}
                           oninput={(e) => { typeField(`${n}:${axis}`, e.currentTarget.value);
                             setAxis(n, axis as 0 | 1 | 2, e.currentTarget.value); }}
                           onfocus={() => focusField(`${n}:${axis}`, formatUnit(p[axis], unit))}
                           onblur={blurField} />
                  </td>
                {/each}
                <td>
                  <input aria-label={`probe ${n + 1} distance`}
                         value={shown(`${n}:dist`, formatUnit(s.r, unit))}
                         disabled={current.mixed_range}
                         oninput={(e) => { typeField(`${n}:dist`, e.currentTarget.value);
                           setDistance(n, e.currentTarget.value); }}
                         onfocus={() => focusField(`${n}:dist`, formatUnit(s.r, unit))}
                         onblur={blurField} />
                </td>
                <td>
                  <input aria-label={`probe ${n + 1} azimuth`}
                         value={shown(`${n}:az`, s.az.toFixed(1))}
                         disabled={current.mixed_range}
                         oninput={(e) => { typeField(`${n}:az`, e.currentTarget.value);
                           setAngle(n, "az", e.currentTarget.value); }}
                         onfocus={() => focusField(`${n}:az`, s.az.toFixed(1))}
                         onblur={blurField} />
                </td>
                <td>
                  <input aria-label={`probe ${n + 1} elevation`}
                         value={shown(`${n}:el`, s.el.toFixed(1))}
                         disabled={current.mixed_range}
                         oninput={(e) => { typeField(`${n}:el`, e.currentTarget.value);
                           setAngle(n, "el", e.currentTarget.value); }}
                         onfocus={() => focusField(`${n}:el`, s.el.toFixed(1))}
                         onblur={blurField} />
                </td>
                <td>
                  <button class="mini" title="Remove this probe"
                          disabled={draftProbes.length <= 1 || current.mixed_range}
                          onclick={() => { removeProbe(n); commit(); }}>×</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
        <button onclick={() => { addProbe(); commit(); }}
                disabled={draftProbes.length >= 8 || current.mixed_range}>
          + probe
        </button>
        <span class="meta">{draftProbes.length} of 8</span>

        <label class="drifter-toggle">
          <input type="checkbox" bind:checked={showDrifter} />
          Drifter wormhole
        </label>
        {#if showDrifter}
          <p class="hint">
            The drifter geometry is 89 km across; a 0.5 AU formation is about
            800 times wider — the panes stay scaled to the probes, so the
            overlay draws near the origin rather than resizing to it.
          </p>
        {/if}

        <div class="panes">
          {#each PANES as { plane, label } (plane)}
            <figure class="pane">
              <figcaption>{label}</figcaption>
              <svg viewBox="0 0 {PANE} {PANE}" width={PANE} height={PANE} role="img"
                   aria-label="{label} view of the formation">
                <line x1={PANE / 2} y1="0" x2={PANE / 2} y2={PANE} class="axis" />
                <line x1="0" y1={PANE / 2} x2={PANE} y2={PANE / 2} class="axis" />
                {#each draftProbes as p, n}
                  {@const c = at(p, plane)}
                  <circle cx={c.cx} cy={c.cy} r={Math.max(0, draftRange) / scale} class="range" />
                  <circle cx={c.cx} cy={c.cy} r="4" class="probe" class:selected={selectedProbe === n} />
                {/each}
                {#if showDrifter}
                  {@const h = at(drifterHole(), plane)}
                  <circle cx={PANE / 2} cy={PANE / 2} r="5" class="warp-in" />
                  <line x1={PANE / 2} y1={PANE / 2} x2={h.cx} y2={h.cy} class="drifter-axis" />
                  <circle cx={h.cx} cy={h.cy} r={DRIFTER.jumpRange / scale} class="jump-range" />
                  <circle cx={h.cx} cy={h.cy} r="4" class="hole" />
                {/if}
              </svg>
            </figure>
          {/each}
        </div>
      </section>
    {:else}
      <p class="hint">This account has no custom probe formations yet.</p>
    {/if}
  </div>
{/if}

<style>
  /* Native controls render light in the dark WebView2 shell unless told
     otherwise — see the dark-native-controls note in the repo memory. */
  input {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 6px; font: inherit;
  }
  .probes { display: flex; gap: 1rem; align-items: flex-start; height: 100%; }
  .formation-list {
    flex: 0 0 14rem; display: flex; flex-direction: column; gap: 0.5rem;
    border-right: 1px solid var(--border); padding-right: 1rem;
  }
  .formation-list ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .formation-list li button { width: 100%; text-align: left; border: none; background: none; padding: 3px 6px; }
  .formation-list li button.active { background: var(--accent); color: var(--bg); border-radius: 3px; }
  .list-actions { display: flex; flex-wrap: wrap; gap: 4px; }
  .list-actions .danger { border-color: #a33; }
  .formation { flex: 1; min-width: 0; }
  .formation .row { display: flex; align-items: flex-end; gap: 1rem; margin-bottom: 0.5rem; }
  .formation label { display: flex; flex-direction: column; gap: 2px; font-size: 0.85em; color: var(--fg-dim); }
  .units { display: flex; gap: 2px; }
  .units button { padding: 1px 8px; font-size: 0.85em; }
  .units button.active { background: var(--accent); color: var(--bg); border-color: var(--accent); }
  .warn { color: var(--warn); font-size: 0.85em; }
  table { border-collapse: collapse; width: 100%; margin: 0.5rem 0; }
  th, td { padding: 2px 6px; text-align: left; }
  th { color: var(--fg-dim); font-weight: 400; font-size: 0.85em; }
  tr.selected td { background: rgba(79, 156, 240, 0.12); }
  td input { width: 8rem; }
  .meta { opacity: 0.7; font-size: 0.85em; margin-left: 0.5rem; }
  .shared-banner {
    margin: 0 0 0.6rem; padding: 0.3rem 0.5rem; font-size: 0.85em;
    color: var(--fg-dim); border-left: 2px solid var(--accent); background: var(--bg-panel);
  }

  .panes { display: flex; gap: 1rem; margin-top: 0.75rem; }
  .pane { margin: 0; display: flex; flex-direction: column; align-items: center; gap: 0.25rem; }
  .pane figcaption { font-size: 0.85em; color: var(--fg-dim); }
  .pane svg { background: var(--bg-panel); border: 1px solid var(--border); border-radius: 3px; }
  .axis { stroke: var(--border); stroke-width: 1; }
  .range { fill: rgba(79, 156, 240, 0.08); stroke: rgba(79, 156, 240, 0.4); stroke-width: 1; }
  .probe { fill: var(--accent); }
  .probe.selected { fill: var(--warn); stroke: var(--fg); stroke-width: 1; }

  .formation .drifter-toggle {
    display: flex; flex-direction: row; align-items: center; gap: 0.4rem;
    margin-top: 0.5rem; font-size: 0.85em; color: var(--fg-dim); width: fit-content;
  }
  /* The beacon a formation is dropped on: a hollow ring at the origin, so it
     reads as "you are here" rather than a probe. */
  .warp-in { fill: none; stroke: var(--ok); stroke-width: 1.5; }
  /* The (assumed) downward line from the beacon to the hole — dashed so it
     never reads as one of the solid crosshair axes. */
  .drifter-axis { stroke: var(--fg-dim); stroke-width: 1; stroke-dasharray: 3 2; }
  /* The hole's 16 km jump sphere — same idea as .range but in the danger
     colour and dashed, so a probe's range circle is never mistaken for it. */
  .jump-range {
    fill: rgba(224, 108, 96, 0.08); stroke: rgba(224, 108, 96, 0.5);
    stroke-width: 1; stroke-dasharray: 3 2;
  }
  .hole { fill: var(--danger); stroke: var(--fg); stroke-width: 1; }
</style>
