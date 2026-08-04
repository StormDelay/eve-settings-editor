<script lang="ts">
  import { api, errMessage, type Formation, type Formations } from "./api";
  import { fromUnit, toSpherical, toCartesian, cubeFormation, formatUnit,
           DEFAULT_RANGE_M, MAX_PROBES, RANGE_STEPS_AU, RANGE_STEPS_M,
           paneScale, project, type Unit, type Plane } from "./probes";
  import { message } from "@tauri-apps/plugin-dialog";

  let { userOpen, userId = null, onUserDirty, onShowAccounts = () => {}, sharedLabel = "" }:
    { userOpen: boolean; userId?: number | null; onUserDirty: () => void;
      onShowAccounts?: () => void; sharedLabel?: string } = $props();

  /** The projection as loaded. `null` before the first load. */
  let loaded = $state<Formations | null>(null);
  let error = $state<string | null>(null);
  let selectedId = $state<number | null>(null);
  let unit = $state<Unit>("au");
  /** The unit as it reads in a column header. */
  const unitLabel = $derived(unit === "au" ? "AU" : "km");

  // THE EDIT BUFFER, IN METRES. Every displayed number is derived from this;
  // nothing derived is ever read back into it. A field the user has not typed
  // into therefore keeps its exact f64 from the file, which is the whole reason
  // this is not bound straight to the inputs (spec §4.2).
  let draftName = $state("");
  let draftRanges = $state<number[]>([]);
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
  const scale = $derived(paneScale(draftProbes, Math.max(0, ...draftRanges), PANE));

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
    if (draftChanged()) commit();
  }
  function shown(key: string, computed: string): string {
    return focusedField === key ? (rawText[key] ?? computed) : computed;
  }

  const current = $derived(loaded?.formations.find((f) => f.id === selectedId) ?? null);

  /** The range every probe shares, or `null` when they differ — the header
   * picker shows blank rather than claiming one of the values applies to all. */
  const uniformRange = $derived(
    draftRanges.length && draftRanges.every((r) => r === draftRanges[0]) ? draftRanges[0] : null,
  );

  /** Whether the draft differs from the loaded formation. A blur that changed
   * nothing must not write the file or light the "unsaved" badge — and since
   * reading accepts `List | Tuple` for the probe list but writing always emits
   * `List`, a no-op commit could otherwise normalise a shape the client wrote. */
  function draftChanged(): boolean {
    if (!current) return false;
    return (
      draftName !== current.name ||
      draftProbes.length !== current.probes.length ||
      draftRanges.some((r, i) => r !== current.ranges[i]) ||
      draftProbes.some((p, i) => p.some((v, j) => v !== current.probes[i][j]))
    );
  }

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
      const l = loaded; // narrowed non-null; `loaded` itself doesn't narrow inside a closure
      select(l.formations.find((f) => f.id === l.selected) ?? l.formations[0] ?? null);
    }
  }
  $effect(() => { void userOpen; void userId; reload(); });

  function select(f: Formation | null) {
    selectedId = f?.id ?? null;
    draftName = f?.name ?? "";
    draftProbes = f ? f.probes.map((p) => [...p] as [number, number, number]) : [];
    draftRanges = f ? [...f.ranges] : [];
    lastAngles = draftProbes.map((p) => { const s = toSpherical(p); return { az: s.az, el: s.el }; });
  }

  async function commit(id: number | null = selectedId) {
    // `next_id` fills the lowest free gap, not the end of the list, so a
    // freshly minted id can land in the MIDDLE of the sorted response — never
    // identify it by position.
    const before = new Set(loaded?.formations.map((f) => f.id) ?? []);
    try {
      loaded = await api.setProbeFormation(id, draftName, draftProbes, draftRanges);
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

  /** One probe's scan range. */
  function setRange(i: number, metres: number) {
    draftRanges = draftRanges.map((r, j) => (j === i ? metres : r));
    commit();
  }

  /** Every probe's scan range — uniform is the common case, and eight pickers
   * would be a regression on the single field this replaces. */
  function setAllRanges(metres: number) {
    draftRanges = draftRanges.map(() => metres);
    commit();
  }

  function addProbe() {
    if (draftProbes.length >= MAX_PROBES) return;
    // The new probe inherits the last probe's range rather than the default:
    // a formation is normally uniform, and inheriting keeps it that way
    // without the user having to notice a picker.
    const r = draftRanges[draftRanges.length - 1] ?? DEFAULT_RANGE_M;
    draftProbes = [...draftProbes, [r / 2, 0, 0]];
    draftRanges = [...draftRanges, r];
    lastAngles = [...lastAngles, { az: 0, el: 0 }];
  }

  function removeProbe(i: number) {
    if (draftProbes.length <= 1) return;
    draftProbes = draftProbes.filter((_, j) => j !== i);
    draftRanges = draftRanges.filter((_, j) => j !== i);
    lastAngles = lastAngles.filter((_, j) => j !== i);
  }

  async function createNew() {
    draftName = "New formation";
    draftProbes = cubeFormation(DEFAULT_RANGE_M);
    draftRanges = draftProbes.map(() => DEFAULT_RANGE_M);
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
                   oninput={(e) => (draftName = e.currentTarget.value)}
                   onblur={blurField} />
          </label>
          <label>
            Range (all probes)
            <!-- Always AU, and always one of EVE's slider stops: the in-game
                 control has no free value, so neither does this. A picker also
                 makes a non-positive range unwritable by construction. -->
            <select aria-label="range for every probe"
                    value={uniformRange}
                    onchange={(e) => setAllRanges(Number(e.currentTarget.value))}>
              {#each RANGE_STEPS_M as m, i}
                <option value={m}>{RANGE_STEPS_AU[i]} AU</option>
              {/each}
            </select>
          </label>
          <span class="units">
            <span class="meta">probe positions in</span>
            <button class:active={unit === "au"} onclick={() => (unit = "au")}>AU</button>
            <button class:active={unit === "km"} onclick={() => (unit = "km")}>km</button>
          </span>
        </div>

        <table>
          <thead>
            <tr>
              <th>#</th>
              <th>X</th><th>Y</th><th>Z</th>
              <th>distance</th><th>azimuth</th><th>elevation</th>
              <th>range</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {#each draftProbes as p, n}
              {@const s = toSpherical(p)}
              <tr class:selected={selectedProbe === n} onfocusin={() => (selectedProbe = n)}>
                <td>{n + 1}</td>
                {#each [0, 1, 2] as axis}
                  <td class="u" data-unit={unitLabel}>
                    <input aria-label={`probe ${n + 1} ${"XYZ"[axis]}`}
                           value={shown(`${n}:${axis}`, formatUnit(p[axis], unit))}
                           oninput={(e) => { typeField(`${n}:${axis}`, e.currentTarget.value);
                             setAxis(n, axis as 0 | 1 | 2, e.currentTarget.value); }}
                           onfocus={() => focusField(`${n}:${axis}`, formatUnit(p[axis], unit))}
                           onblur={blurField} />
                  </td>
                {/each}
                <td class="u" data-unit={unitLabel}>
                  <input aria-label={`probe ${n + 1} distance`}
                         value={shown(`${n}:dist`, formatUnit(s.r, unit))}
                         oninput={(e) => { typeField(`${n}:dist`, e.currentTarget.value);
                           setDistance(n, e.currentTarget.value); }}
                         onfocus={() => focusField(`${n}:dist`, formatUnit(s.r, unit))}
                         onblur={blurField} />
                </td>
                <td class="u" data-unit="°">
                  <input aria-label={`probe ${n + 1} azimuth`}
                         value={shown(`${n}:az`, s.az.toFixed(1))}
                         oninput={(e) => { typeField(`${n}:az`, e.currentTarget.value);
                           setAngle(n, "az", e.currentTarget.value); }}
                         onfocus={() => focusField(`${n}:az`, s.az.toFixed(1))}
                         onblur={blurField} />
                </td>
                <td class="u" data-unit="°">
                  <input aria-label={`probe ${n + 1} elevation`}
                         value={shown(`${n}:el`, s.el.toFixed(1))}
                         oninput={(e) => { typeField(`${n}:el`, e.currentTarget.value);
                           setAngle(n, "el", e.currentTarget.value); }}
                         onfocus={() => focusField(`${n}:el`, s.el.toFixed(1))}
                         onblur={blurField} />
                </td>
                <td>
                  <select aria-label={`probe ${n + 1} range`}
                          value={draftRanges[n]}
                          onchange={(e) => setRange(n, Number(e.currentTarget.value))}>
                    {#each RANGE_STEPS_M as m, i}
                      <option value={m}>{RANGE_STEPS_AU[i]} AU</option>
                    {/each}
                    {#if !RANGE_STEPS_M.includes(draftRanges[n])}
                      <!-- A range this file holds that EVE's slider cannot
                           produce. Offered so the value is shown rather than
                           silently snapped to a neighbour. -->
                      <option value={draftRanges[n]}>
                        {formatUnit(draftRanges[n], "au")} AU (not a slider stop)
                      </option>
                    {/if}
                  </select>
                </td>
                <td>
                  <button class="mini-visible" title="Remove this probe"
                          disabled={draftProbes.length <= 1}
                          onclick={() => { removeProbe(n); commit(); }}>×</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
        <button onclick={() => { addProbe(); commit(); }}
                disabled={draftProbes.length >= MAX_PROBES}>
          + probe
        </button>
        <span class="meta">{draftProbes.length} of {MAX_PROBES}</span>

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
                  <circle cx={c.cx} cy={c.cy} r={Math.max(0, draftRanges[n] ?? 0) / scale} class="range" />
                  <circle cx={c.cx} cy={c.cy} r="4" class="probe" class:selected={selectedProbe === n} />
                {/each}
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
  input, select {
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
  /* `width: auto` and not 100%: a full-width table spreads the leftover space
     between the columns, which put the X/Y/Z fields an inch apart. Let the
     columns hug their inputs instead. */
  table { border-collapse: collapse; width: auto; margin: 0.5rem 0; }
  th, td { padding: 2px 4px; text-align: left; }
  th { color: var(--fg-dim); font-weight: 400; font-size: 0.85em; white-space: nowrap; }
  tr.selected td { background: rgba(79, 156, 240, 0.12); }
  td input { width: 7rem; }
  /* The angle columns hold at most "-180.0". */
  td:nth-child(6) input, td:nth-child(7) input { width: 5.5rem; }
  /* The unit rides inside the field, dimmed, as a pseudo-element: it cannot be
     selected or clicked, so it never lands in a copied value or steals focus
     from the input it labels. `padding-right` keeps the digits clear of it. */
  td.u { position: relative; }
  td.u::after {
    content: attr(data-unit);
    position: absolute; right: 10px; top: 50%; transform: translateY(-50%);
    color: var(--fg-dim); font-size: 0.85em;
    pointer-events: none; user-select: none;
  }
  td.u input { padding-right: 2.1rem; }
  td.u[data-unit="°"] input { padding-right: 1.3rem; }
  /* `.mini` is opacity 0 unless it sits inside a `.node .row`, which only the
     tree view has — so the remove button was invisible here. `.mini-visible`
     is the codebase's own always-shown variant; it just lacks the danger tint. */
  td .mini-visible:hover { border-color: var(--danger); color: var(--danger); }
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
</style>
