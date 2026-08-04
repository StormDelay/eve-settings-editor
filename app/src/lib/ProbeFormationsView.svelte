<script lang="ts">
  import { api, errMessage, type Formation, type Formations, type FormationSpec } from "./api";
  import { fromUnit, toSpherical, toCartesian, cubeFormation, formatUnit,
           DEFAULT_RANGE_M, MAX_PROBES, RANGE_STEPS_AU, RANGE_STEPS_M,
           type Unit, type Vec3 } from "./probes";
  import { message } from "@tauri-apps/plugin-dialog";
  import ProbeViewer from "./ProbeViewer.svelte";

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

  /** The formation set as the user currently sees it: the loaded projection
   * with the selected formation's uncommitted draft substituted in.
   *
   * Copy, Export and the export picker all read this, so what leaves the app is
   * what is on screen (spec §5.1). Reading the backend's projection instead
   * would race the blur-commit that the Copy button's own click fires, and
   * could return either side of it depending on timing. */
  const visible = $derived<FormationSpec[]>(
    (loaded?.formations ?? []).map((f) =>
      f.id === selectedId
        ? { name: draftName, probes: draftProbes, ranges: draftRanges }
        : { name: f.name, probes: f.probes, ranges: f.ranges },
    ),
  );
  const visibleIndex = $derived(loaded?.formations.findIndex((f) => f.id === selectedId) ?? -1);

  /** Whether the account file is open, so Copy/Paste have something to act on.
   * The `<svelte:window>` listeners below outlive the markup's `{#if}`
   * branches — they're live even on the "pair this character" hint screen —
   * so they need their own gate rather than relying on the buttons' absence. */
  const canShare = $derived(userOpen && loaded !== null);

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
    // Index into the OLD formation. Left standing it drops the drag gizmo onto
    // an arbitrary probe of the one just opened.
    selectedProbe = null;
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

  /** A probe moved in the viewer. Writes only that probe — every other one
   * keeps its exact f64 from the file. */
  function moveProbe(i: number, p: Vec3) {
    draftProbes = draftProbes.map((q, j) => (j === i ? p : q));
    const s = toSpherical(p);
    if (s.r !== 0) lastAngles[i] = { az: s.az, el: s.el };
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
    selectedProbe = null; // every index at or past `i` just shifted under it
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

  /** Add formations from shared text — the one path Paste and Import both end
   * on, so the collision rule (in Rust, spec §4.3) applies to both. Throws;
   * each caller reports under its own title. */
  async function addShared(specs: FormationSpec[]) {
    if (specs.length === 0) return;
    const before = new Set(loaded?.formations.map((f) => f.id) ?? []);
    loaded = await api.addProbeFormations(specs);
    onUserDirty();
    // next_id fills the lowest free gap, so an added formation can land in the
    // MIDDLE of the sorted response — diff the ids rather than reading the end.
    const added = loaded.formations.filter((f) => !before.has(f.id));
    if (added.length) select(added[added.length - 1]);
  }

  async function copyFormation() {
    if (visibleIndex < 0) return;
    try {
      await navigator.clipboard.writeText(await api.probeYaml([visible[visibleIndex]]));
    } catch (e) {
      await message(errMessage(e), { title: "Could not copy the formation", kind: "error" });
    }
  }

  async function pasteText(text: string) {
    if (!text.trim()) return;
    try {
      await addShared(await api.probeParseYaml(text));
    } catch (e) {
      await message(errMessage(e), { title: "Could not paste the formation", kind: "error" });
    }
  }

  async function pasteFormation() {
    let text: string;
    try {
      text = await navigator.clipboard.readText();
    } catch {
      // WebView2 can refuse a clipboard READ without showing a prompt. Ctrl-V
      // needs no permission — the keypress is the grant — so point the user at
      // it rather than reporting a failure they cannot act on (spec §5.4).
      await message("Press Ctrl+V to paste a formation instead.", {
        title: "The clipboard could not be read",
      });
      return;
    }
    await pasteText(text);
  }

  /** True when the event came from somewhere the OS clipboard must keep
   * behaving normally. A tab full of coordinate fields is exactly where Ctrl-C
   * has to go on copying the digits the user just selected. */
  function inAField(t: EventTarget | null): boolean {
    const el = t as HTMLElement | null;
    const tag = el?.tagName;
    return tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA" || !!el?.isContentEditable;
  }

  function onKeyDown(e: KeyboardEvent) {
    // Ctrl-V needs no branch here: the browser fires `paste`, which carries the
    // data and asks no permission.
    if (!(e.ctrlKey || e.metaKey) || e.key !== "c" || inAField(e.target)) return;
    if (!canShare || visibleIndex < 0) return;
    // A formation copy is the fallback for when there is nothing selected,
    // never an override of it — if the user has text selected (the hint
    // paragraph, the shared-account banner), let the browser's own copy win.
    if (!window.getSelection()?.isCollapsed) return;
    e.preventDefault();
    void copyFormation();
  }

  function onPaste(e: ClipboardEvent) {
    if (inAField(e.target) || !canShare) return;
    const text = e.clipboardData?.getData("text/plain") ?? "";
    if (!text.trim()) return;
    e.preventDefault();
    void pasteText(text);
  }
</script>

<!-- The Probes tab is conditionally mounted (+page.svelte), so this listener
     does not exist while another view is open and cannot leak into it. -->
<svelte:window onkeydown={onKeyDown} onpaste={onPaste} />

{#if !userOpen}
  <p class="hint">
    Probe formations live in the account file.
    <button class="link" onclick={onShowAccounts}>Pair this character with its account</button>
    to edit them.
  </p>
{:else if error}
  <p class="error">{error}</p>
{:else if loaded}
  <!-- One column filling the tab, so the banner takes its own height off the
       top instead of the editor below assuming it has the whole tab and
       running that much past the bottom. -->
  <div class="probes-tab">
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
        <button onclick={pasteFormation} title="Add a formation from the clipboard (Ctrl+V)">Paste</button>
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
          <button onclick={copyFormation} title="Copy this formation to the clipboard (Ctrl+C)">Copy</button>
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
        <!-- Wrapped, so the column layout below keeps these two on one line. -->
        <div class="probe-actions">
          <button onclick={() => { addProbe(); commit(); }}
                  disabled={draftProbes.length >= MAX_PROBES}>
            + probe
          </button>
          <span class="meta">{draftProbes.length} of {MAX_PROBES}</span>
        </div>

        <ProbeViewer probes={draftProbes} ranges={draftRanges} formationId={selectedId}
                     selected={selectedProbe}
                     onselect={(i) => (selectedProbe = i)}
                     onmove={moveProbe}
                     oncommit={() => { if (draftChanged()) commit(); }} />
      </section>
    {:else}
      <p class="hint">This account has no custom probe formations yet.</p>
    {/if}
  </div>
  </div>
{/if}

<style>
  /* Native controls render light in the dark WebView2 shell unless told
     otherwise — see the dark-native-controls note in the repo memory. */
  input, select {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 6px; font: inherit;
  }
  .probes-tab { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .probes { display: flex; gap: 1rem; align-items: flex-start; flex: 1; min-height: 0; }
  .formation-list {
    flex: 0 0 14rem; display: flex; flex-direction: column; gap: 0.5rem;
    border-right: 1px solid var(--border); padding-right: 1rem;
  }
  .formation-list ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .formation-list li button { width: 100%; text-align: left; border: none; background: none; padding: 3px 6px; }
  .formation-list li button.active { background: var(--accent); color: var(--bg); border-radius: 3px; }
  .list-actions { display: flex; flex-wrap: wrap; gap: 4px; }
  .list-actions .danger { border-color: #a33; }
  /* A column, so the viewer can take exactly the height the table leaves it
     rather than a fixed box that runs off the bottom of the window. Everything
     above it keeps its natural height; only the viewer flexes. */
  .formation {
    flex: 1; min-width: 0; align-self: stretch;
    display: flex; flex-direction: column; min-height: 0;
    /* Not the default `stretch`: that pulled the table out to the full width
       of the panel, and a full-width table hands the slack to its columns and
       puts the X/Y/Z fields inches apart. Each row keeps its own width. */
    align-items: flex-start;
  }
  .formation > :not(:last-child) { flex: none; }
  .probe-actions { display: flex; align-items: center; }
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
</style>
