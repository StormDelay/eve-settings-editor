<script lang="ts">
  import { api, errMessage, errText, type OverviewColumns } from "./api";
  import defaultPresetNames from "./data/default-preset-names.json";
  import defaultPresetsBundle from "./data/default-presets.json";
  import overviewGroups from "./data/overview-groups.json";
  import { mergeCatalog, filterCatalog, toggleGroup, toggleGroups, unknownGroups, type Category, type CatalogBundle } from "./groups";
  import { isDefaultKey, accountFormat, defaultsForFormat, mergePresetOptions, forkName, findDefault, LEGACY_NAMES, type DefaultsBundle, type DefaultProfile } from "./presets";
  import { stateLabel, EXCEPTION_STATES, exceptionOf, applyException, type Exception } from "./states";
  import { revealAndFocus } from "./keymap";
  import Button from "./ui/Button.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import { toast } from "./ui/toasts.svelte";
  import { undoAction } from "./undo.svelte";

  let { data, tabIndex, onChanged, onUserDirty, focusSearch = $bindable(undefined) }:
    { data: OverviewColumns | null; tabIndex: number | null;
      onChanged: (next: OverviewColumns) => void; onUserDirty: () => void;
      /** Set so the shell's Ctrl+F reaches THIS box — the group filter, over a
       *  list of 649 rows, which is the size that most wants a shortcut.
       *  OverviewView owns the sub-tab switch that has to happen first. */
      focusSearch?: () => void } = $props();

  let filterBox: HTMLInputElement | HTMLSelectElement | undefined = $state();
  focusSearch = () => revealAndFocus(filterBox);

  const tab = $derived(data?.tabs.find((t) => t.index === tabIndex) ?? null);

  /** Which control a failure belongs to. Seven call sites, six controls — one
   *  live message each, replaced rather than stacked. */
  type Where = "groups" | "exception" | "rename" | "preset" | "duplicate" | "delete";
  let error = $state<{ where: Where; text: string; detail: string } | null>(null);

  function fail(where: Where, text: string, e: unknown): void {
    error = { where, text, detail: errMessage(e) };
  }
  const at = (where: Where) => (error?.where === where ? error : null);

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
    error = null;
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
    } catch (e) { fail("groups", `Those groups weren't changed — ${errText(e)}`, e); }
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
    error = null;
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
    } catch (e) { fail("exception", `That exception wasn't changed — ${errText(e)}`, e); }
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
  // Was a `use:` action, which Svelte cannot apply to a component. Field hands
  // back its control node instead, and this focuses it when the rename box
  // appears — same moment, same effect.
  let renameInput: HTMLInputElement | HTMLSelectElement | undefined = $state();
  $effect(() => {
    if (!renameInput) return;
    renameInput.focus();
    if (renameInput instanceof HTMLInputElement) renameInput.select();
  });

  function startRenamePreset() {
    if (!tab) return;
    pending = { value: labelFor(tab.preset), old: tab.preset };
  }
  async function submitPending() {
    error = null;
    if (!pending) return;
    const p = pending;
    const name = p.value.trim();
    if (!name) {
      pending = null;
      return;
    }
    try {
      // Compare against the shown label: the rename box is prefilled with
      // labelFor(old), so an unedited submit on a DefaultPreset_<id> (label
      // "Carriers") must be a no-op, not a rename of the raw key to "Carriers".
      if (name !== labelFor(p.old)) {
        onChanged(await api.presetRename(p.old, name));
        onUserDirty();
      }
      // The field closes on SUCCESS only. It used to close first and report the
      // failure in a modal, so a refused rename threw away what the user typed
      // and made them start again — which is the retry cost that made the modal
      // feel like a punishment rather than a report.
      pending = null;
    } catch (e) { fail("rename", `The preset wasn't renamed — ${errText(e)}`, e); }
  }
  async function setTabPreset(preset: string) {
    error = null;
    if (!tab || preset === tab.preset) return;
    try { onChanged(await api.tabSetPreset(tab.index, preset)); onUserDirty(); }
    catch (e) { fail("preset", `The tab's preset wasn't changed — ${errText(e)}`, e); }
  }
  async function duplicatePreset() {
    error = null;
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
    } catch (e) { fail("duplicate", `The preset wasn't duplicated — ${errText(e)}`, e); }
  }
  async function deletePreset() {
    error = null;
    if (!tab || !data) return;
    const name = tab.preset;
    const list = data.presets.map((p) => p.name);
    const pos = list.indexOf(name);
    if (pos < 0 || list.length <= 1) return;
    const neighbour = pos > 0 ? list[pos - 1] : list[pos + 1];
    // No confirmation: this is an OVERVIEW preset, stored inside the account
    // document, and Discard reverses it exactly. (The other thing called a
    // preset — a settings-preset folder on disk — keeps its confirm, because
    // deleting one is `remove_dir_all`. Two unrelated things, one word, opposite
    // sides of the reversibility line.)
    //
    // The toast is strictly MORE informative than the dialog it replaces: it can
    // count the tabs that moved, and the dialog could only name the neighbour.
    const moved = data.tabs.filter((t) => t.preset === name).length;
    try {
      onChanged(await api.presetDelete(name));
      onUserDirty();
      toast(
        `Deleted “${labelFor(name)}”. ${moved} tab${moved === 1 ? "" : "s"} now use${moved === 1 ? "s" : ""} “${labelFor(neighbour)}”.`,
        { action: undoAction() },
      );
    } catch (e) { fail("delete", `The preset wasn't deleted — ${errText(e)}`, e); }
  }
</script>

{#if tab}
  <div class="filters-controls">
    <Field
      kind="select"
      label="Preset"
      value={tab.preset}
      onchange={(e) => setTabPreset((e.currentTarget as HTMLSelectElement).value)}
      options={[
        ...(grouped.defaults.includes(tab.preset) || grouped.user.includes(tab.preset)
          ? []
          : [{ value: tab.preset, label: labelFor(tab.preset) }]),
        ...grouped.defaults.map((k) => ({ value: k, label: labelFor(k), group: "Default profiles" })),
        ...grouped.user.map((k) => ({ value: k, label: labelFor(k), group: "Your profiles" })),
      ]} />
    {#if at("preset")}
      <InlineMessage variant="error" detail={error!.detail}>{error!.text}</InlineMessage>
    {/if}
    <div class="preset-actions">
      <Button onclick={duplicatePreset} disabled={!editable}
              disabledReason="This preset cannot be duplicated"
              title="Duplicate this preset">Duplicate preset</Button>
      <Button onclick={startRenamePreset} disabled={!storedPreset || isDefaultKey(tab.preset)}
              disabledReason="A built-in profile cannot be renamed"
              title="Rename this preset">Rename preset…</Button>
      <Button variant="danger" onclick={deletePreset}
              disabled={!storedPreset || isDefaultKey(tab.preset) || (data?.presets.length ?? 0) <= 1}
              disabledReason={isDefaultKey(tab.preset)
                ? "A built-in profile cannot be deleted"
                : "The last preset cannot be deleted"}
              title="Delete this preset">Delete preset</Button>
    </div>
    <!-- At the three buttons, which is where the click was. -->
    {#if at("duplicate") || at("delete")}
      <InlineMessage variant="error" detail={error!.detail}>{error!.text}</InlineMessage>
    {/if}
    {#if editable}
      <div class="preset-contents">
        <div class="contents-head">
          <span class="contents-title">Shows: {labelFor(tab.preset)}</span>
          <!-- controlClass, not class: the spec reads `.group-filter`'s value,
               so the hook has to land on the input. -->
          <Field
            controlClass="group-filter"
            ariaLabel="Filter groups"
            placeholder="Filter groups"
            bind:element={filterBox}
            bind:value={typedFilter} />
        </div>

        <h4 class="section-heading">Types shown</h4>

        <!-- Above the group grid: a group toggle can fail from anywhere in a
             four-hundred-row list, and the list is what it is about. -->
        {#if at("groups")}
          <InlineMessage variant="error" detail={error!.detail}>{error!.text}</InlineMessage>
        {/if}

        {#if unknownIds.length}
          <InlineMessage variant="warn" class="unknown-groups">
            Unrecognised groups — not in the catalogue
            {#each unknownIds as id}
              <Field kind="checkbox" label="#{id}" value={true} onchange={() => setPresetGroup(id, false)} />
            {/each}
          </InlineMessage>
        {/if}

        {#each visibleCategories as cat (cat.id)}
          <details
            class="group-cat"
            open={isOpen(cat.id)}
            ontoggle={(e) => noteToggle(cat.id, (e.currentTarget as HTMLDetailsElement).open)}>
            <!-- preventDefault, or the click's default action on the ancestor
                 <summary> toggles the category shut as you bulk-select it. -->
            <summary>
              <!-- Our own marker, because the grid below replaces `list-item`
                   and takes the native one with it. Rotating one glyph is the
                   whole cost of putting the name, the arrow and the bulk pair
                   on one line. -->
              <span class="marker" aria-hidden="true">▸</span>
              <span class="cat-name">{cat.name}</span>
              <Button variant="ghost" size="sm"
                      onclick={(e) => { e.preventDefault(); setCategory(cat, true); }}
                      title="Select every group shown in {cat.name}">All</Button>
              <Button variant="ghost" size="sm"
                      onclick={(e) => { e.preventDefault(); setCategory(cat, false); }}
                      title="Deselect every group shown in {cat.name}">None</Button>
            </summary>
            {#if isOpen(cat.id)}
              <div class="group-grid">
                {#each cat.groups as g (g.id)}
                  <Field
                    kind="checkbox"
                    class="group-item"
                    label={g.name}
                    value={presetGroupSet.has(g.id)}
                    onchange={(e) => setPresetGroup(g.id, (e.currentTarget as HTMLInputElement).checked)} />
                {/each}
              </div>
            {/if}
          </details>
        {/each}

        <h4 class="section-heading">Exceptions</h4>
        {#if at("exception")}
          <InlineMessage variant="error" detail={error!.detail}>{error!.text}</InlineMessage>
        {/if}
        <div class="exceptions-list">
          {#each exceptionRows as row (row.id)}
            {@const choice = exceptionOf(presetFiltered, presetAlwaysShown, row.id)}
            <div class="exception-row">
              <span class="exception-label">{row.label}</span>
              <Field kind="radio" name={`exc-${row.id}`} label="Show"
                     value={choice} radioValue="show"
                     onchange={() => setException(row.id, "show")} />
              <Field kind="radio" name={`exc-${row.id}`} label="Hide"
                     value={choice} radioValue="hide"
                     onchange={() => setException(row.id, "hide")} />
              <Field kind="radio" name={`exc-${row.id}`} label="Always show"
                     value={choice} radioValue="always"
                     onchange={() => setException(row.id, "always")} />
            </div>
          {/each}
        </div>
      </div>
    {/if}
    {#if pending}
      <div class="name-entry">
        <Field bind:value={pending.value} bind:element={renameInput} ariaLabel="Preset name" placeholder="Preset name"
               onkeydown={(e: KeyboardEvent) => {
                 if (e.key === "Enter") { e.preventDefault(); submitPending(); }
                 else if (e.key === "Escape") pending = null;
               }} />
        <Button variant="primary" onclick={submitPending}>Rename preset</Button>
        <Button onclick={() => (pending = null)}>Cancel</Button>
      </div>
      {#if at("rename")}
        <InlineMessage variant="error" detail={error!.detail}>{error!.text}</InlineMessage>
      {/if}
    {/if}
  </div>
{/if}

<style>
  /* Same flex-wrap row layout the shared tab strip used, kept local now that
     the preset controls have their own panel instead of sharing a row with it. */
  /* Both dark-native-control blocks are gone — the selects, their options and
     optgroups, the two text inputs and the radios are all Fields now. */
  .filters-controls { display: flex; gap: var(--s4); margin-bottom: var(--s2); align-items: center; flex-wrap: wrap; }
  .preset-actions { display: flex; gap: var(--s1); align-items: center; flex-wrap: wrap; }
  .name-entry { display: flex; gap: var(--s1); align-items: center; margin-bottom: var(--s2); }
  .name-entry :global(.field) { flex: 1; max-width: 16rem; }
  .name-entry :global(input) { width: 100%; }
  /* Full-width so the box below can size a real column grid — it's a flex item
     inside the wrapping .filters-controls row otherwise. */
  .preset-contents { flex-basis: 100%; margin-top: var(--s2); display: flex; flex-direction: column; gap: var(--s1); }
  /* Same reason, same fix: the two error messages in this row are bare flex
     children with auto basis, so they wrapped in BESIDE the control they belong
     to instead of onto their own line under it. `.preset-contents` got this and
     they did not. */
  .filters-controls > :global(.msg) { flex-basis: 100%; }
  .contents-head { display: flex; gap: var(--s2); align-items: center; flex-wrap: wrap; }
  .contents-title { font-weight: 600; }
  .section-heading { margin: var(--s1) 0 0; font-size: var(--t-body); }
  /* The bulk pair used to trail the category name inline, so it sat wherever
     that name happened to end — "Ship" put them near the margin and "Planetary
     Industry" put them far right, down a list of fifteen.

     The grid is ON the summary, not inside it. An inner grid is a BLOCK box, so
     it dropped onto the line below the disclosure marker and left every name
     hanging under an arrow.

     `justify-content: start` and a bounded name column are what keep All/None
     beside the names instead of flung to the panel's right edge — this column
     is as wide as the window, and a `1fr` name track put the buttons a hand's
     width from the thing they act on. */
  .group-cat > summary {
    cursor: pointer;
    padding: var(--s1) 0;
    display: grid;
    grid-template-columns: 1rem minmax(0, 13rem) auto auto;
    align-items: center;
    justify-content: start;
    gap: var(--s2);
    list-style: none;
  }
  .group-cat > summary::-webkit-details-marker { display: none; }
  .marker {
    color: var(--text-muted);
    transition: transform 0.12s ease;
  }
  .group-cat[open] > summary .marker { transform: rotate(90deg); }
  @media (prefers-reduced-motion: reduce) {
    .marker { transition: none; }
  }
  .cat-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .group-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr));
    gap: 0 var(--s3);
    padding: var(--s1) 0 var(--s1) var(--s4);
  }
  /* ONE grid for the whole list, with each row `display: contents`, so all
     thirty-odd rows share four column tracks and the three radios line up down
     the page.

     Per-row flex could not do this. `.exception-label` carried a `min-width`,
     so a label longer than it — "Pilot has a kill right on them that you can
     activate" — pushed its own radios right while "Pilot is a criminal" left
     them near the margin. Every row chose its own alignment, which is precisely
     what a column of radios must not do: the eye reads down these, not across. */
  /* A reading width, for the same reason `.ov-cols` and `.state-list` have one:
     the first column is `1fr` in a work area that is a wide pane rather than a
     20rem panel, so without a cap the three radios sit against the far right
     edge of the screen and the eye has to travel the whole way from the label.
     Sized to the widest row this list can produce rather than to the 26rem the
     other two use — that label really is "Pilot has a kill right on them that
     you can activate" (~26rem), and Show / Hide / Always show add ~17rem. */
  .exceptions-list {
    display: grid;
    max-width: 44rem;
    grid-template-columns: minmax(0, 1fr) auto auto auto;
    align-items: center;
    gap: 0 var(--s3);
    padding: var(--s1) 0 var(--s1) var(--s4);
  }
  .exception-row { display: contents; }
  .exception-label { min-width: 0; }
</style>
