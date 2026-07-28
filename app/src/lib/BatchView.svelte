<script lang="ts">
  import { untrack } from "svelte";
  import { api, errMessage, type Profile, type Aspect, type SetupPlan, type BatchTargetResult, type BatchSource, type PresetInfo } from "./api";
  import { byResolvedName, resolvedName } from "./filesort.svelte";
  import { primaryProfileDir, profileLabels } from "./profiles";
  import { accountsStore, loadRoster } from "./accounts.svelte";
  import { allPresets, loadPresets, summarise } from "./presetLibrary.svelte";

  let { openPath }: { openPath: string | null } = $props();

  loadRoster();
  loadPresets();

  // Character (char) files only — the source and every target is a character.
  let profiles = $state<Profile[]>([]);
  api.discover().then((p) => (profiles = p)).catch(() => {});
  const chars = $derived(
    profiles.flatMap((p) =>
      p.files
        .filter((f) => f.kind === "char")
        .map((f) => ({ path: f.path, file_name: f.file_name, id: f.id, dir: p.dir })),
    ),
  );

  const folders = $derived.by(() => {
    const labels = profileLabels(profiles);
    return profiles
      .filter((p) => chars.some((c) => c.dir === p.dir))
      .map((p) => ({ dir: p.dir, label: labels.get(p.dir)! }));
  });

  let folderPick = $state<string | null>(null);
  const autoFolder = $derived(
    chars.find((c) => c.path === sourcePath)?.dir ?? primaryProfileDir(profiles),
  );
  const folder = $derived(folderPick ?? autoFolder);

  let sourcePath = $state<string | null>(
    untrack(() => (openPath && openPath.includes("core_char_") ? openPath : null)),
  );

  function pickFolder(dir: string) {
    folderPick = dir;
    sourcePath = null;
  }

  // The batch source is either a character (as before) or a saved preset. A
  // preset belongs to no profile folder, so it borrows the Profile dropdown's
  // `folder` as its anchor — the folder whose characters populate the target list.
  let sourceKind = $state<"character" | "preset">("character");
  let presetDir = $state<string | null>(null);
  const preset = $derived<PresetInfo | null>(allPresets().find((p) => p.dir === presetDir) ?? null);

  // What the chosen source can offer. A preset offers only what it holds, so
  // Autofill cannot be ticked on a preset that has none.
  const offered = $derived<Aspect[]>(
    sourceKind === "preset"
      ? (preset?.aspects ?? [])
      : ["layout", "overview", "autofill", "keybinds", "everything"],
  );

  const batchSource = $derived<BatchSource | null>(
    sourceKind === "character"
      ? (sourcePath ? { kind: "character", path: sourcePath } : null)
      : (presetDir ? { kind: "preset", dir: presetDir, anchor_dir: folder ?? "" } : null),
  );

  // Aspects. "Everything" is exclusive.
  const ASPECTS: { key: Aspect; label: string; account: boolean }[] = [
    { key: "layout", label: "Window layout (positions, neocom, ship HUD — not the fighter panel or badge)", account: false },
    { key: "overview", label: "Overview (columns, tabs, presets)", account: true },
    { key: "autofill", label: "Autofill (remembered text)", account: true },
    { key: "keybinds", label: "Keybindings", account: true },
    { key: "everything", label: "Everything (full clone of both files)", account: true },
  ];
  let selected = $state<Set<Aspect>>(new Set());
  const everything = $derived(selected.has("everything"));
  const anyAccountAspect = $derived([...selected].some((a) => ASPECTS.find((x) => x.key === a)?.account));
  function toggleAspect(a: Aspect) {
    const next = new Set(selected);
    if (a === "everything") {
      next.has(a) ? next.delete(a) : (next.clear(), next.add(a));
    } else {
      next.delete("everything");
      next.has(a) ? next.delete(a) : next.add(a);
    }
    selected = next;
  }

  // Which char ids are paired (member of some account) — unpaired chars can't
  // receive an account aspect.
  const pairedIds = $derived(
    new Set(accountsStore.roster.accounts.flatMap((acc) => acc.characters)),
  );

  let allowOtherFolders = $state(false);
  const candidates = $derived(
    chars
      // A character cannot be its own copy target — but ONLY when it is the
      // source. `sourcePath` is seeded from the open file and never cleared on
      // switching to a preset source, so filtering on it directly kept the open
      // character out of the list for the rest of the session.
      .filter((c) => !(batchSource?.kind === "character" && c.path === batchSource.path))
      .filter((c) => allowOtherFolders || c.dir === folder)
      .slice()
      .sort((a, b) =>
        byResolvedName(
          { kind: "char", id: a.id, file_name: a.file_name },
          { kind: "char", id: b.id, file_name: b.file_name },
        ),
      ),
  );
  // The source dropdown lists every character in the folder (the current source
  // included), ordered like the sidebar.
  const sourceOptions = $derived(
    chars
      .filter((c) => allowOtherFolders || c.dir === folder)
      .slice()
      .sort((a, b) =>
        byResolvedName(
          { kind: "char", id: a.id, file_name: a.file_name },
          { kind: "char", id: b.id, file_name: b.file_name },
        ),
      ),
  );
  let selectedTargets = $state<Set<string>>(new Set());
  function toggleTarget(path: string) {
    const next = new Set(selectedTargets);
    next.has(path) ? next.delete(path) : next.add(path);
    selectedTargets = next;
  }
  const targetDisabled = (id: number | null) => anyAccountAspect && !(id != null && pairedIds.has(id));

  // The targets actually sent to the backend: the selected set minus any row the
  // current aspect selection excludes (an unpaired character under an account
  // aspect). The backend independently excludes them too, but filtering here
  // keeps the UI honest — a disabled row never counts as a real target, and its
  // selection is preserved so it re-includes if the aspect choice changes back.
  const effectiveTargets = $derived(
    [...selectedTargets].filter((p) => {
      const c = chars.find((x) => x.path === p);
      return c ? !targetDisabled(c.id) : false;
    }),
  );

  // Applying onto the open document writes behind it: the in-memory copy goes
  // stale and the only thing that notices is the save-time on-disk check, two
  // steps later. Warn at the point of decision instead. Dirty state doesn't
  // matter — the on-screen copy is out of date either way.
  const targetsOpenFile = $derived(openPath !== null && effectiveTargets.includes(openPath));

  function selectAllTargets() {
    const next = new Set(selectedTargets);
    for (const c of candidates) if (!targetDisabled(c.id)) next.add(c.path);
    selectedTargets = next;
  }
  function clearTargets() {
    selectedTargets = new Set();
  }

  // Short names for the account-write warning, in the order checked in ASPECTS
  // (excluding "everything", which is reported as a full copy instead).
  const changedAspectNames = $derived(
    ASPECTS.filter((a) => a.account && a.key !== "everything" && selected.has(a.key)).map((a) => a.label),
  );

  const nameOfChar = (id: number | null, fileName: string) =>
    id == null ? fileName : (resolvedName("char", id) ?? `char ${id}`);
  const accountLabel = (id: number) => {
    const alias = resolvedName("user", id);
    return alias ? `${alias} (${id})` : `${id}`;
  };
  const folderLabelOf = (dir: string) => profileLabels(profiles).get(dir) ?? dir;

  // Reset op + targets when the source changes — including a switch between
  // "character" and "preset", not just a different pick within one kind.
  $effect(() => {
    sourcePath;
    presetDir;
    sourceKind;
    folder;
    selected = new Set();
    selectedTargets = new Set();
  });

  // Preview from the backend whenever source/aspects/targets settle. Guarded
  // by a request token so a slow, stale response can't clobber a newer plan.
  let plan = $state<SetupPlan | null>(null);
  let previewSeq = 0;
  $effect(() => {
    const src = batchSource;
    const asp = [...selected];
    const tgts = effectiveTargets;
    const allow = allowOtherFolders;
    if (!src || asp.length === 0 || tgts.length === 0) { plan = null; return; }
    const seq = ++previewSeq;
    api.setupPreview(src, tgts, asp as Aspect[], allow)
      .then((p) => { if (seq === previewSeq) plan = p; })
      .catch(() => { if (seq === previewSeq) plan = null; });
  });

  let busy = $state(false);
  let error = $state<string | null>(null);
  let results = $state<BatchTargetResult[] | null>(null);
  const canApply = $derived(
    !!batchSource && selected.size > 0 && effectiveTargets.length > 0 && !busy &&
    !!plan && !plan.source_error && (plan.char_writes.length + plan.account_writes.length > 0),
  );

  async function apply() {
    const src = batchSource;
    if (!src) return;
    busy = true; error = null; results = null;
    try {
      results = await api.setupApply(src, effectiveTargets, [...selected] as Aspect[], allowOtherFolders);
    } catch (e) {
      error = errMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="batch">
  <h2>Copy a setup to other characters</h2>

  <section>
    <label for="folder">Profile</label>
    <select id="folder" value={folder} onchange={(e) => pickFolder(e.currentTarget.value)}>
      {#each folders as f}<option value={f.dir}>{f.label}</option>{/each}
    </select>

    <div class="head">Source</div>
    <label class="inline">
      <input type="radio" name="sourceKind" bind:group={sourceKind} value="character" /> A character
    </label>
    <label class="inline">
      <input type="radio" name="sourceKind" bind:group={sourceKind} value="preset" /> A preset
    </label>

    {#if sourceKind === "character"}
      <label for="src">Source character</label>
      <select id="src" bind:value={sourcePath}>
        <option value={null} disabled>Choose a character…</option>
        {#each sourceOptions as c}
          <option value={c.path}>{nameOfChar(c.id, c.file_name)} — {c.file_name}</option>
        {/each}
      </select>
    {:else}
      <label for="srcpreset">Source preset</label>
      <select id="srcpreset" bind:value={presetDir}>
        <option value={null} disabled>Choose a preset…</option>
        {#each allPresets().filter((p) => p.error === null) as p}
          <option value={p.dir}>{p.name} — {summarise(p)}</option>
        {/each}
      </select>
      {#if allPresets().length === 0}
        <p class="muted">No presets yet — save one from the sidebar first.</p>
      {/if}
    {/if}
  </section>

  {#if batchSource}
    <section>
      <div class="head">What to copy</div>
      {#each ASPECTS.filter((a) => offered.includes(a.key)) as a}
        <label class:disabled={everything && a.key !== "everything"}>
          <input type="checkbox" checked={selected.has(a.key)}
            disabled={everything && a.key !== "everything"}
            onchange={() => toggleAspect(a.key)} />
          {a.label}
        </label>
      {/each}
    </section>

    <section>
      <div class="head">
        Target characters
        <button type="button" class="linkbtn" onclick={selectAllTargets}>Select all</button>
        <button type="button" class="linkbtn" onclick={clearTargets}>Clear</button>
        <label class="inline"><input type="checkbox" bind:checked={allowOtherFolders} /> Show other folders</label>
      </div>
      {#if candidates.length === 0}
        <p class="muted">No other character files found.</p>
      {:else}
        {#each candidates as c}
          <label class:disabled={targetDisabled(c.id)}>
            <input type="checkbox" checked={selectedTargets.has(c.path) && !targetDisabled(c.id)}
              disabled={targetDisabled(c.id)} onchange={() => toggleTarget(c.path)} />
            {nameOfChar(c.id, c.file_name)}
            <span class="muted">{c.file_name}{c.dir === folder ? "" : ` · ${folderLabelOf(c.dir)}`}</span>
            {#if targetDisabled(c.id)}<span class="muted"> — pair in the Accounts view to include</span>{/if}
          </label>
        {/each}
      {/if}
    </section>

    {#if plan}
      <section class="preview">
        {#if plan.source_error}
          <p class="err">{plan.source_error}</p>
        {:else}
          <p>Will write {plan.char_writes.length + plan.account_writes.length} file(s) — each is backed up first.</p>
          {#each plan.char_writes.filter((w) => w.resolution_mismatch) as w}
            <p class="warn">⚠ {nameOfChar(w.char_id, "")}: screen resolution differs from the source — copied windows may land off-screen.</p>
          {/each}
          {#if targetsOpenFile}
            <p class="warn">⚠ One target is the file open in the editor. Its on-screen
              copy will be out of date after this runs — reload it before editing
              further, or your next save will collide with what this wrote.</p>
          {/if}
          {#each plan.account_writes as w}
            <p class="warn">⚠ {w.full_copy ? "Entire account settings replaced" : `${changedAspectNames.join(" / ")} changed`} for account {accountLabel(w.user_id)}{#if w.collateral_char_ids.length > 0} — also changes: {w.collateral_char_ids.map((id) => nameOfChar(id, `char ${id}`)).join(", ")}{/if}. Other characters on this account that aren't paired yet are affected too — pair them in the Accounts view to see them by name.</p>
          {/each}
          {#each plan.excluded as ex}
            <p class="muted">Excluded {nameOfChar(ex.char_id, `char ${ex.char_id}`)} — {ex.reason}</p>
          {/each}
        {/if}
      </section>
    {/if}

    <section>
      <button disabled={!canApply} onclick={apply}>{busy ? "Applying…" : "Apply"}</button>
      {#if error}<p class="err">{error}</p>{/if}
    </section>

    {#if results}
      <section class="results">
        <div class="head">Result</div>
        {#each results as r}
          <div class:ok={r.ok} class:fail={!r.ok}>
            {r.ok ? "✓" : "✗"} {r.path.split(/[\\/]/).pop()}
            {#if r.error}<span class="muted"> — {r.error}</span>{/if}
          </div>
        {/each}
      </section>
    {/if}
  {/if}
</div>

<style>
  .batch { padding: 1rem; max-width: 46rem; }
  section { margin: 0.75rem 0; }
  .head { font-weight: 600; margin-bottom: 0.25rem; display: flex; gap: 1rem; align-items: baseline; }
  label { display: block; padding: 0.15rem 0; }
  label.disabled { opacity: 0.5; }
  label.inline { display: inline; font-weight: 400; }
  .linkbtn { background: none; border: none; color: var(--accent); cursor: pointer; font: inherit; padding: 0; }
  .linkbtn:hover { text-decoration: underline; }
  select, option { background: var(--bg-panel); color: var(--fg); border: 1px solid var(--border); border-radius: 3px; padding: 2px 4px; font: inherit; }
  input[type="checkbox"], input[type="radio"] { accent-color: var(--accent); }
  .muted { color: var(--fg-dim); }
  .preview p { margin: 0.15rem 0; }
  .warn { color: #d0a000; }
  .err, .fail { color: #e06c6c; }
  .ok { color: #6cc06c; }
  button { padding: 0.35rem 0.9rem; }
</style>
