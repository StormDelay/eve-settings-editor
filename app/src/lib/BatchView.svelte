<script lang="ts">
  import { untrack } from "svelte";
  import { api, errMessage, type Profile, type Aspect, type SetupPlan, type BatchTargetResult, type BatchSource, type PresetInfo } from "./api";
  import { byResolvedName, resolvedName } from "./filesort.svelte";
  import { primaryProfileDir, profileLabels } from "./profiles";
  import { accountsStore, loadRoster } from "./accounts.svelte";
  import { allPresets, loadPresets, summarise } from "./presetLibrary.svelte";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";

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
        .map((f) => ({ path: f.path, file_name: f.file_name, id: f.id, kind: f.kind, dir: p.dir })),
    ),
  );

  const folders = $derived.by(() => {
    const labels = profileLabels(profiles);
    return profiles
      .filter((p) => chars.some((c) => c.dir === p.dir))
      .map((p) => ({ dir: p.dir, label: labels.get(p.dir)! }));
  });

  let allowOtherFolders = $state(false);
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
  //
  // A third source kind sidesteps both: "a file", copied byte-for-byte onto
  // other files of the same kind. No aspects and no pairing — the plain copy
  // the character-centric flow cannot express, because every aspect it offers
  // writes the account file and so needs to know which account that is.
  let sourceKind = $state<"character" | "preset" | "file">("character");
  let presetDir = $state<string | null>(null);
  const preset = $derived<PresetInfo | null>(allPresets().find((p) => p.dir === presetDir) ?? null);

  let sourceFile = $state<string | null>(null);
  const fileMode = $derived(sourceKind === "file");
  // Every char AND user file, backups included: this mode addresses files by
  // path, so `core_char_123 - old.dat` is a legitimate pick at either end.
  const filesInScope = $derived(
    profiles
      .filter((p) => allowOtherFolders || p.dir === folder)
      .flatMap((p) => p.files.filter((f) => f.kind !== "other").map((f) => ({ ...f, dir: p.dir })))
      .sort(byResolvedName),
  );
  // Searched across every profile, not just `filesInScope`: toggling "show other
  // folders" must not invalidate a source already picked.
  const sourceFileKind = $derived(
    profiles.flatMap((p) => p.files).find((f) => f.path === sourceFile)?.kind ?? null,
  );
  const fileTargets = $derived(
    filesInScope.filter((f) => f.kind === sourceFileKind && f.path !== sourceFile),
  );

  // What the chosen source can offer. A preset offers only what it holds, so
  // Autofill cannot be ticked on a preset that has none.
  const offered = $derived<Aspect[]>(
    sourceKind === "preset"
      ? (preset?.aspects ?? [])
      : ["layout", "overview", "autofill", "keybinds", "probe_formations", "everything"],
  );

  const batchSource = $derived<BatchSource | null>(
    sourceKind === "character"
      ? (sourcePath ? { kind: "character", path: sourcePath } : null)
      : (presetDir ? { kind: "preset", dir: presetDir, anchor_dir: folder ?? "" } : null),
  );

  // Aspects. "Everything" is exclusive.
  const ASPECTS: { key: Aspect; label: string; account: boolean }[] = [
    { key: "layout", label: "Window layout (positions, neocom, ship HUD, fighter panel, badge)", account: true },
    { key: "overview", label: "Overview (columns, tabs, presets)", account: true },
    { key: "autofill", label: "Autofill (remembered text)", account: true },
    { key: "keybinds", label: "Keybindings", account: true },
    { key: "probe_formations", label: "Probe formations (custom scan formations)", account: true },
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

  // The source dropdown lists every character in the folder (the current source
  // included), ordered like the sidebar.
  const charsInScope = $derived(
    chars
      .filter((c) => allowOtherFolders || c.dir === folder)
      .sort((a, b) =>
        byResolvedName(
          { kind: "char", id: a.id, file_name: a.file_name },
          { kind: "char", id: b.id, file_name: b.file_name },
        ),
      ),
  );
  const candidates = $derived(
    fileMode
      ? fileTargets
      // A character cannot be its own copy target — but ONLY when it is the
      // source. `sourcePath` is seeded from the open file and never cleared on
      // switching to a preset source, so filtering on it directly kept the open
      // character out of the list for the rest of the session.
      : charsInScope.filter((c) => !(batchSource?.kind === "character" && c.path === batchSource.path)),
  );
  let selectedTargets = $state<Set<string>>(new Set());
  function toggleTarget(path: string) {
    const next = new Set(selectedTargets);
    next.has(path) ? next.delete(path) : next.add(path);
    selectedTargets = next;
  }
  // Pairing only gates the character flow: a file copy writes the file you
  // picked, so it has no account to look up.
  const targetDisabled = (id: number | null) =>
    !fileMode && anyAccountAspect && !(id != null && pairedIds.has(id));

  // The targets actually sent to the backend: the selected set minus any row the
  // current aspect selection excludes (an unpaired character under an account
  // aspect). The backend independently excludes them too, but filtering here
  // keeps the UI honest — a disabled row never counts as a real target, and its
  // selection is preserved so it re-includes if the aspect choice changes back.
  const effectiveTargets = $derived(
    fileMode
      ? [...selectedTargets]
      : [...selectedTargets].filter((p) => {
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
  // Window layout is the only aspect that can REMOVE a value: a HUD field the
  // source leaves at EVE's default is deleted from the target so it falls back
  // to that same default. Every other aspect only ever overwrites.
  const resetsToDefaults = $derived(selected.has("layout"));

  const nameOf = (kind: string, id: number | null, fileName: string) =>
    id == null ? fileName : (resolvedName(kind, id) ?? `${kind === "user" ? "account" : "char"} ${id}`);
  const nameOfChar = (id: number | null, fileName: string) => nameOf("char", id, fileName);
  const accountLabel = (id: number) => {
    const alias = resolvedName("user", id);
    return alias ? `${alias} (${id})` : `${id}`;
  };
  const folderLabelOf = (dir: string) => profileLabels(profiles).get(dir) ?? dir;

  // Reset op + targets when the source changes — including a switch between
  // "character" and "preset", not just a different pick within one kind.
  //
  // `folderPick`, not `folder`: the derived one starts null and gains a value
  // when `api.discover()` resolves, which is not a user action, and an aspect
  // ticked in that window (the source seeds from the open file synchronously,
  // so the checkboxes can render before discover lands) was silently cleared.
  // Nothing is lost by watching the pick instead — `pickFolder` also nulls
  // `sourcePath`, which triggers this effect anyway.
  $effect(() => {
    sourcePath;
    presetDir;
    sourceFile;
    sourceKind;
    folderPick;
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
    // A file copy has no plan to fetch: the write list is exactly the ticked files.
    if (fileMode || !src || asp.length === 0 || tgts.length === 0) { plan = null; return; }
    const seq = ++previewSeq;
    api.setupPreview(src, tgts, asp as Aspect[], allow)
      .then((p) => { if (seq === previewSeq) plan = p; })
      .catch(() => { if (seq === previewSeq) plan = null; });
  });

  let busy = $state(false);
  let error = $state<string | null>(null);
  let results = $state<BatchTargetResult[] | null>(null);
  const canApply = $derived(
    fileMode
      ? !!sourceFile && effectiveTargets.length > 0 && !busy
      : !!batchSource && selected.size > 0 && effectiveTargets.length > 0 && !busy &&
        !!plan && !plan.source_error && (plan.char_writes.length + plan.account_writes.length > 0),
  );

  async function apply() {
    busy = true; error = null; results = null;
    try {
      if (fileMode) {
        if (!sourceFile) return;
        results = await api.copyFiles(sourceFile, effectiveTargets);
      } else {
        if (!batchSource) return;
        results = await api.setupApply(batchSource, effectiveTargets, [...selected] as Aspect[], allowOtherFolders);
      }
    } catch (e) {
      error = errMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="batch">
  <h2>{fileMode ? "Copy a file onto other files" : "Copy a setup to other characters"}</h2>

  <section>
    <Field
      kind="select"
      id="folder"
      label="Profile"
      layout="column"
      value={folder}
      onchange={(e) => pickFolder((e.currentTarget as HTMLSelectElement).value)}
      options={folders.map((f) => ({ value: f.dir, label: f.label }))} />

    <div class="head">Source</div>
    <Field kind="radio" name="sourceKind" bind:value={sourceKind} radioValue="character" label="A character" />
    <Field kind="radio" name="sourceKind" bind:value={sourceKind} radioValue="preset" label="A preset" />
    <Field kind="radio" name="sourceKind" bind:value={sourceKind} radioValue="file" label="A file, copied as-is" />

    {#if sourceKind === "character"}
      <Field
        kind="select"
        id="src"
        label="Source character"
        layout="column"
        bind:value={sourcePath}
        options={[
          { value: null, label: "Choose a character…", disabled: true },
          ...charsInScope.map((c) => ({
            value: c.path,
            label: `${nameOfChar(c.id, c.file_name)} — ${c.file_name}`,
          })),
        ]} />
    {:else if sourceKind === "preset"}
      <Field
        kind="select"
        id="srcpreset"
        label="Source preset"
        layout="column"
        bind:value={presetDir}
        options={[
          { value: null, label: "Choose a preset…", disabled: true },
          ...allPresets()
            .filter((p) => p.error === null)
            .map((p) => ({ value: p.dir, label: `${p.name} — ${summarise(p)}` })),
        ]} />
      {#if allPresets().length === 0}
        <EmptyState title="No presets yet — save one from the sidebar first." />
      {/if}
    {:else}
      <Field
        kind="select"
        id="srcfile"
        label="Source file"
        layout="column"
        bind:value={sourceFile}
        options={[
          { value: null, label: "Choose a file…", disabled: true },
          ...filesInScope.map((f) => ({
            value: f.path,
            label: `${nameOf(f.kind, f.id, f.file_name)} — ${f.file_name}`,
          })),
        ]} />
      <InlineMessage>
        The whole file is copied onto every file you tick — character files onto character files,
        account files onto account files. No pairing needed.
      </InlineMessage>
    {/if}
  </section>

  {#if fileMode ? !!sourceFile : !!batchSource}
    {#if !fileMode}
      <section>
        <div class="head">What to copy</div>
        {#each ASPECTS.filter((a) => offered.includes(a.key)) as a}
          <Field
            kind="checkbox"
            label={a.label}
            value={selected.has(a.key)}
            disabled={everything && a.key !== "everything"}
            disabledReason="Everything already covers this"
            onchange={() => toggleAspect(a.key)} />
        {/each}
      </section>
    {/if}

    <section>
      <div class="head">
        {fileMode ? "Copy onto" : "Target characters"}
        <Button variant="ghost" size="sm" class="linkbtn" type="button" onclick={selectAllTargets}>
          Select all
        </Button>
        <Button variant="ghost" size="sm" class="linkbtn" type="button" onclick={clearTargets}>Clear</Button>
        <Field kind="checkbox" label="Show other folders" bind:value={allowOtherFolders} />
      </div>
      {#if candidates.length === 0}
        <EmptyState
          title={fileMode ? "No other file of this kind in reach." : "No other character files found."} />
      {:else}
        <!-- The row stays a hand-written wrapping label rather than a Field: its
             caption is three spans, not a string, and BatchView.spec reads the
             whole label's text to check what a disabled target says. -->
        {#each candidates as c}
          <label class:disabled={targetDisabled(c.id)}>
            <input type="checkbox" checked={selectedTargets.has(c.path) && !targetDisabled(c.id)}
              disabled={targetDisabled(c.id)}
              title={targetDisabled(c.id) ? "Pair this character in the Accounts view first" : undefined}
              onchange={() => toggleTarget(c.path)} />
            {nameOf(c.kind, c.id, c.file_name)}
            <span class="muted">{c.file_name}{c.dir === folder ? "" : ` · ${folderLabelOf(c.dir)}`}</span>
            {#if targetDisabled(c.id)}<span class="muted"> — pair in the Accounts view to include</span>{/if}
          </label>
        {/each}
      {/if}
    </section>

    <!-- Applies to both modes, so it sits outside the plan: a file copy fetches
         no plan, and in the character flow this warns before the plan lands. -->
    {#if targetsOpenFile}
      <section class="preview">
        <InlineMessage variant="warn">⚠ One target is the file open in the editor. Its on-screen
          copy will be out of date after this runs — reload it before editing
          further, or your next save will collide with what this wrote.</InlineMessage>
      </section>
    {/if}

    {#if fileMode}
      {#if effectiveTargets.length > 0}
        <section class="preview">
          <p>Will write {effectiveTargets.length} file(s) — each is backed up first.</p>
          <InlineMessage variant="warn">⚠ Each target is replaced whole — every setting it holds,
            including ones this editor does not show.{#if sourceFileKind === "user"} An
            account file carries the settings of every character on that account.{/if}</InlineMessage>
        </section>
      {/if}
    {:else if plan}
      <section class="preview">
        {#if plan.source_error}
          <InlineMessage variant="error">{plan.source_error}</InlineMessage>
        {:else}
          <p>Will write {plan.char_writes.length + plan.account_writes.length} file(s) — each is backed up first.</p>
          {#each plan.char_writes.filter((w) => w.resolution_mismatch) as w}
            <InlineMessage variant="warn">⚠ {nameOfChar(w.char_id, "")}: screen resolution differs from the source — copied windows may land off-screen.</InlineMessage>
          {/each}
          {#each plan.account_writes as w}
            <InlineMessage variant="warn">⚠ {w.full_copy ? "Entire account settings replaced" : `${changedAspectNames.join(" / ")} changed${resetsToDefaults ? " — and any of those the source leaves at EVE's default is reset to that default here, not left as it is" : ""}`} for account {accountLabel(w.user_id)}{#if w.collateral_char_ids.length > 0} — also changes: {w.collateral_char_ids.map((id) => nameOfChar(id, `char ${id}`)).join(", ")}{/if}. Other characters on this account that aren't paired yet are affected too — pair them in the Accounts view to see them by name.</InlineMessage>
          {/each}
          {#each plan.excluded as ex}
            <p class="muted">Excluded {nameOfChar(ex.char_id, `char ${ex.char_id}`)} — {ex.reason}</p>
          {/each}
        {/if}
      </section>
    {/if}

    <section>
      <Button
        variant="primary"
        disabled={!canApply}
        disabledReason={busy ? "A copy is already running" : "Pick a source and at least one target"}
        onclick={apply}>{busy ? "Copying…" : "Copy"}</Button>
      {#if error}<InlineMessage variant="error">{error}</InlineMessage>{/if}
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
  /* The select/option and accent-color rules are gone — Field owns them. The
     .warn/.err/.ok colour trio is gone too: those were three more spellings of
     --warn/--danger/--ok, and the paragraphs they coloured are InlineMessages
     now, which carry the meaning in a rail rather than in the body text. */
  .batch { padding: var(--s4); max-width: 46rem; }
  section { margin: var(--s3) 0; }
  /* BatchView.spec finds each section through this class. */
  .head { font-weight: 600; margin-bottom: var(--s1); display: flex; gap: var(--s4); align-items: baseline; }
  label { display: block; padding: 0; }
  label.disabled { opacity: var(--o-disabled); }
  .batch :global(.linkbtn) { color: var(--accent); padding: 0; }
  .batch :global(.linkbtn:hover) { text-decoration: underline; background: none; }
  .muted { color: var(--text-muted); }
  .preview p { margin: var(--s1) 0; }
  .fail { color: var(--danger); }
  .ok { color: var(--ok); }
</style>
