<script lang="ts">
  import { api, errMessage, type OverviewColumns } from "./api";
  import defaultPresetNames from "./data/default-preset-names.json";
  import defaultPresetsBundle from "./data/default-presets.json";
  import overviewGroups from "./data/overview-groups.json";
  import { mergeCatalog, filterCatalog, toggleGroup, toggleGroups, unknownGroups, type Category, type CatalogBundle } from "./groups";
  import { isDefaultKey, accountFormat, defaultsForFormat, mergePresetOptions, forkName, findDefault, LEGACY_NAMES, type DefaultsBundle, type DefaultProfile } from "./presets";
  import { stateLabel, EXCEPTION_STATES, exceptionOf, applyException, type Exception } from "./states";
  import { message, confirm } from "@tauri-apps/plugin-dialog";

  let { data, tabIndex, onChanged, onUserDirty }:
    { data: OverviewColumns | null; tabIndex: number | null;
      onChanged: (next: OverviewColumns) => void; onUserDirty: () => void } = $props();

  const tab = $derived(data?.tabs.find((t) => t.index === tabIndex) ?? null);

  // The preset dropdown's default-profile options: EVE's built-in bundle for
  // this account's on-disk regime (modern DefaultPreset_<id> vs legacy
  // default* literals), merged with any stored presets so nothing is missed.
  const fmt = $derived(accountFormat((data?.tabs ?? []).map((t) => t.preset)));
  const bundledDefaults = $derived(defaultsForFormat(defaultPresetsBundle as DefaultsBundle, fmt));
  const storedNames = $derived((data?.presets ?? []).map((p) => p.name));
  const grouped = $derived(mergePresetOptions(storedNames, bundledDefaults));

  // Preset-contents catalog: seed synchronously from the bundled tree so the
  // checklist renders immediately (the app's core path is editing files offline);
  // then upgrade it once on mount with any ESI-synced additions (the backend
  // server_version-gates the sync, so a repeat call is cheap).
  let catalog = $state<Category[]>(mergeCatalog(overviewGroups as CatalogBundle, []));
  $effect(() => {
    const b = overviewGroups as CatalogBundle;
    api
      .syncGroupCatalog(b.all_group_ids, b.categories.map((c) => c.id))
      .then((additions) => (catalog = mergeCatalog(b, additions)))
      .catch(() => (catalog = mergeCatalog(b, [])));
  });

  // What the box shows, and what the list actually filters on. They are separate
  // because applying a query expands every category it matches, and doing that
  // per keystroke re-renders the whole expanded set while the user is still
  // typing. 150ms is below the threshold where a pause feels like lag.
  let typedFilter = $state("");
  let groupFilter = $state("");
  $effect(() => {
    const next = typedFilter;
    const t = setTimeout(() => (groupFilter = next), 150);
    return () => clearTimeout(t);
  });
  // Which categories are expanded. A `<details>` hides its children but Svelte
  // still builds every one and tracks it reactively, so 649 checkboxes stayed
  // live at all times — and each backend round trip behind a tick re-evaluated
  // all of them. Rows are rendered only while their category is open.
  //
  // Unset means "follow the filter": a query expands its matches, which is what
  // makes filtering useful. Once the user toggles a category by hand that choice
  // sticks, so a deliberately-collapsed Entity (400 rows) stays collapsed even
  // while a broad query matches it.
  //
  // Only a DIVERGENCE from the filter's default is recorded, and that subtlety
  // is load-bearing: assigning `details.open` fires `toggle` exactly as a click
  // does, so `open={isOpen(...)}` echoes back through the handler. Storing that
  // echo would let a query's auto-expand pin its categories open for good, and
  // clearing the box would then render 450 rows (Entity + Ship) under an empty
  // filter — the very cost this exists to avoid, on the ordinary type-then-clear
  // flow, since any one-letter query matches Entity.
  let openCats = $state<Record<number, boolean>>({});
  const isOpen = (id: number) => openCats[id] ?? !!groupFilter.trim();
  function noteToggle(id: number, open: boolean) {
    if (open === !!groupFilter.trim()) delete openCats[id];
    else openCats[id] = open;
  }
  // A default profile that isn't (yet) stored on the account resolves its
  // contents from the bundled snapshot instead — that's what lets a clean
  // account edit a built-in's groups before any fork exists.
  const storedPreset = $derived(data?.presets.find((p) => p.name === tab?.preset));
  const currentDefault = $derived(tab ? findDefault(bundledDefaults, tab.preset) : undefined);
  const presetGroups = $derived(storedPreset?.groups ?? currentDefault?.groups ?? []);
  const editable = $derived(!!tab && (!!storedPreset || !!currentDefault));
  const presetGroupSet = $derived(new Set(presetGroups));
  const visibleCategories = $derived(filterCatalog(catalog, groupFilter));
  const unknownIds = $derived(unknownGroups(catalog, presetGroups));

  // Exceptions: EVE's own Filters sub-tab renders these sorted alphabetically
  // by label (not priority order — that's the account-wide Appearance lists).
  // Any id a preset stores but EXCEPTION_STATES doesn't know about (raw
  // #<id>) is still included so it round-trips instead of being dropped.
  const presetFiltered = $derived(storedPreset?.filtered_states ?? currentDefault?.filteredStates ?? []);
  const presetAlwaysShown = $derived(storedPreset?.always_shown_states ?? currentDefault?.alwaysShownStates ?? []);
  const exceptionRows = $derived(
    Array.from(new Set([...EXCEPTION_STATES, ...presetFiltered, ...presetAlwaysShown]))
      .map((id) => ({ id, label: stateLabel(id) ?? `#${id}` }))
      .sort((a, b) => a.label.localeCompare(b.label)),
  );

  // One write of the whole membership list, whether it came from a single
  // checkbox or a category's All/None. `presetSetGroups` has always taken the
  // complete list, so ticking Entity's ~400 groups is one round trip, not 400.
  async function applyGroups(next: number[]) {
    if (!tab) return;
    const t = tab;
    try {
      if (isDefaultKey(t.preset)) {
        const def = currentDefault;
        const name = forkName(labelFor(t.preset), storedNames);
        onChanged(await api.presetFork(t.index, name, next, def?.filteredStates ?? [], def?.alwaysShownStates ?? []));
      } else {
        onChanged(await api.presetSetGroups(t.preset, next));
      }
      onUserDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }

  const setPresetGroup = (id: number, on: boolean) => applyGroups(toggleGroup(presetGroups, id, on));
  // All/None act on the groups CURRENTLY SHOWN, not on the category's full
  // membership: with a query active `filterCatalog` has already narrowed each
  // category to its matches, and ticking what is on screen is both the obvious
  // reading of a button sitting in that header and what makes the filter box
  // useful as a bulk-select tool. With no query the two are the same thing.
  const setCategory = (cat: Category, on: boolean) =>
    applyGroups(toggleGroups(presetGroups, cat.groups.map((g) => g.id), on));

  async function setException(id: number, choice: Exception) {
    if (!tab) return;
    const t = tab;
    const next = applyException(presetFiltered, presetAlwaysShown, id, choice);
    try {
      if (isDefaultKey(t.preset)) {
        const name = forkName(labelFor(t.preset), storedNames);
        onChanged(await api.presetFork(t.index, name, presetGroups, next.filtered, next.alwaysShown));
      } else {
        onChanged(await api.presetSetStates(t.preset, next.filtered, next.alwaysShown));
      }
      onUserDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }

  // Display label for a preset. EVE's built-in presets are keyed
  // `DefaultPreset_<localizationId>` with no readable name in the file; map the id
  // to its en-US label from the bundled snapshot (see tools/gen-default-preset-names.py).
  // The raw key is still what every edit/API call uses — this only changes shown text.
  function labelFor(name: string): string {
    if (!name) return "(default)";
    const m = /^DefaultPreset_(\d+)$/.exec(name);
    if (m) return (defaultPresetNames as Record<string, string>)[m[1]] ?? name;
    return LEGACY_NAMES[name.toLowerCase()] ?? name;
  }

  // Name entry is an inline input (see the markup below), NOT window.prompt —
  // which the WebView2 renders as an ugly "localhost:1420 says …" dialog.
  let pending = $state<{ value: string; old: string } | null>(null);
  function focusInput(node: HTMLInputElement) { node.focus(); node.select(); }

  function startRenamePreset() {
    if (!tab) return;
    pending = { value: labelFor(tab.preset), old: tab.preset };
  }
  async function submitPending() {
    if (!pending) return;
    const p = pending;
    const name = p.value.trim();
    pending = null;
    if (!name) return;
    try {
      // Compare against the shown label: the rename box is prefilled with
      // labelFor(old), so an unedited submit on a DefaultPreset_<id> (label
      // "Carriers") must be a no-op, not a rename of the raw key to "Carriers".
      if (name === labelFor(p.old)) return;
      onChanged(await api.presetRename(p.old, name));
      onUserDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function setTabPreset(preset: string) {
    if (!tab || preset === tab.preset) return;
    try { onChanged(await api.tabSetPreset(tab.index, preset)); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function duplicatePreset() {
    if (!tab) return;
    const t = tab;
    const name = forkName(labelFor(t.preset), storedNames);
    try {
      if (isDefaultKey(t.preset)) {
        const def = currentDefault;
        onChanged(await api.presetFork(t.index, name, presetGroups, def?.filteredStates ?? [], def?.alwaysShownStates ?? []));
      } else {
        onChanged(await api.presetCreate(t.preset, name));
        onChanged(await api.tabSetPreset(t.index, name));
      }
      onUserDirty();
    } catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function deletePreset() {
    if (!tab || !data) return;
    const name = tab.preset;
    const list = data.presets.map((p) => p.name);
    const pos = list.indexOf(name);
    if (pos < 0 || list.length <= 1) return;
    const neighbour = pos > 0 ? list[pos - 1] : list[pos + 1];
    const ok = await confirm(
      `Delete preset "${labelFor(name)}"? Tabs using it will move to "${labelFor(neighbour)}".`,
      { title: "Delete preset", kind: "warning" },
    );
    if (!ok) return;
    try { onChanged(await api.presetDelete(name)); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
</script>

{#if tab}
  <div class="filters-controls">
    <label>Preset
      <select value={tab.preset} onchange={(e) => setTabPreset((e.currentTarget as HTMLSelectElement).value)}>
        {#if !grouped.defaults.includes(tab.preset) && !grouped.user.includes(tab.preset)}
          <option value={tab.preset}>{labelFor(tab.preset)}</option>
        {/if}
        <optgroup label="Default profiles">
          {#each grouped.defaults as k (k)}<option value={k}>{labelFor(k)}</option>{/each}
        </optgroup>
        {#if grouped.user.length}
          <optgroup label="Your profiles">
            {#each grouped.user as k (k)}<option value={k}>{labelFor(k)}</option>{/each}
          </optgroup>
        {/if}
      </select>
    </label>
    <div class="preset-actions">
      <button onclick={duplicatePreset} disabled={!editable} title="Duplicate this preset">Duplicate preset</button>
      <button onclick={startRenamePreset} disabled={!storedPreset || isDefaultKey(tab.preset)} title="Rename this preset">Rename preset</button>
      <button class="danger" onclick={deletePreset}
              disabled={!storedPreset || isDefaultKey(tab.preset) || (data?.presets.length ?? 0) <= 1}
              title="Delete this preset">Delete preset</button>
    </div>
    {#if editable}
      <div class="preset-contents">
        <div class="contents-head">
          <span class="contents-title">Shows: {labelFor(tab.preset)}</span>
          <input class="group-filter" type="text" placeholder="Filter groups…" bind:value={typedFilter} />
        </div>

        <h4 class="section-heading">Types Shown</h4>

        {#if unknownIds.length}
          <div class="unknown-groups">
            Unrecognized groups (not in the catalog):
            {#each unknownIds as id}
              <label><input type="checkbox" checked onchange={() => setPresetGroup(id, false)} /> #{id}</label>
            {/each}
          </div>
        {/if}

        {#each visibleCategories as cat (cat.id)}
          <details
            class="group-cat"
            open={isOpen(cat.id)}
            ontoggle={(e) => noteToggle(cat.id, (e.currentTarget as HTMLDetailsElement).open)}>
            <!-- preventDefault, or the click's default action on the ancestor
                 <summary> toggles the category shut as you bulk-select it. -->
            <summary>
              <span class="cat-name">{cat.name}</span>
              <span class="cat-bulk">
                <button onclick={(e) => { e.preventDefault(); setCategory(cat, true); }}
                        title="Select every group shown in {cat.name}">All</button>
                <button onclick={(e) => { e.preventDefault(); setCategory(cat, false); }}
                        title="Deselect every group shown in {cat.name}">None</button>
              </span>
            </summary>
            {#if isOpen(cat.id)}
              <div class="group-grid">
                {#each cat.groups as g (g.id)}
                  <label class="group-item">
                    <input type="checkbox" checked={presetGroupSet.has(g.id)}
                           onchange={(e) => setPresetGroup(g.id, (e.currentTarget as HTMLInputElement).checked)} />
                    {g.name}
                  </label>
                {/each}
              </div>
            {/if}
          </details>
        {/each}

        <h4 class="section-heading">Exceptions</h4>
        <div class="exceptions-list">
          {#each exceptionRows as row (row.id)}
            {@const choice = exceptionOf(presetFiltered, presetAlwaysShown, row.id)}
            <div class="exception-row">
              <span class="exception-label">{row.label}</span>
              <label><input type="radio" name={`exc-${row.id}`} checked={choice === "show"}
                            onchange={() => setException(row.id, "show")} /> Show</label>
              <label><input type="radio" name={`exc-${row.id}`} checked={choice === "hide"}
                            onchange={() => setException(row.id, "hide")} /> Hide</label>
              <label><input type="radio" name={`exc-${row.id}`} checked={choice === "always"}
                            onchange={() => setException(row.id, "always")} /> Always show</label>
            </div>
          {/each}
        </div>
      </div>
    {/if}
    {#if pending}
      <div class="name-entry">
        <input type="text" bind:value={pending.value} use:focusInput placeholder="Preset name"
               onkeydown={(e) => {
                 if (e.key === "Enter") { e.preventDefault(); submitPending(); }
                 else if (e.key === "Escape") pending = null;
               }} />
        <button onclick={submitPending}>Rename preset</button>
        <button onclick={() => (pending = null)}>Cancel</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  /* Same flex-wrap row layout the shared tab strip used, kept local now that
     the preset controls have their own panel instead of sharing a row with it. */
  .filters-controls { display: flex; gap: 1rem; margin-bottom: 0.5rem; align-items: center; flex-wrap: wrap; }
  .filters-controls label { display: flex; gap: 0.4rem; align-items: center; }
  .preset-actions { display: flex; gap: 0.4rem; align-items: center; flex-wrap: wrap; }
  .name-entry { display: flex; gap: 0.4rem; align-items: center; margin-bottom: 0.5rem; }
  .name-entry input { flex: 1; max-width: 16rem; }
  button.danger { border-color: #a33; }
  /* Dark native controls: the app runs in a dark WebView2; give selects, their
     options, and inputs explicit dark colors (see the dark-native-controls memo). */
  select, option, optgroup, .name-entry input, .group-filter {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 4px; font: inherit;
  }
  /* Full-width so the box below can size a real column grid — it's a flex item
     inside the wrapping .filters-controls row otherwise. */
  .preset-contents { flex-basis: 100%; margin-top: 0.6rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .contents-head { display: flex; gap: 0.6rem; align-items: center; flex-wrap: wrap; }
  .contents-title { font-weight: 600; }
  .section-heading { margin: 0.2rem 0 0; font-size: 0.9em; }
  .group-cat > summary { cursor: pointer; padding: 0.2rem 0; }
  /* The bulk buttons stay dim until the category is hovered — one pair per
     category, and 20-odd always-lit pairs would out-shout the group names. */
  .cat-bulk { margin-left: 0.6rem; opacity: 0; transition: opacity 0.1s; }
  .group-cat > summary:hover .cat-bulk, .group-cat > summary:focus-within .cat-bulk { opacity: 1; }
  .cat-bulk button {
    background: var(--bg-panel); color: var(--fg-dim);
    border: 1px solid var(--border); border-radius: 3px;
    padding: 0 0.35rem; margin-left: 0.2rem; font: inherit; font-size: 0.85em; cursor: pointer;
  }
  .cat-bulk button:hover { color: var(--fg); border-color: var(--accent); }
  .group-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr)); gap: 0.15rem 0.8rem; padding: 0.2rem 0 0.4rem 1rem; }
  .group-item { display: flex; gap: 0.35rem; align-items: center; }
  .preset-contents input[type="checkbox"] { accent-color: var(--accent); }
  .unknown-groups { display: flex; gap: 0.6rem; flex-wrap: wrap; align-items: center; color: var(--warn); }
  .exceptions-list { display: flex; flex-direction: column; gap: 0.15rem; padding: 0.2rem 0 0.4rem 1rem; }
  .exception-row { display: flex; gap: 0.8rem; align-items: center; }
  .exception-label { min-width: 14rem; }
  .exception-row label { display: flex; gap: 0.3rem; align-items: center; }
  /* Radios render as a light circle in this app's dark WebView2 shell unless
     given explicit dark colors (see the dark-native-controls memo). */
  .exceptions-list input[type="radio"] {
    background: var(--bg-panel); color: var(--fg); accent-color: var(--accent);
  }
</style>
