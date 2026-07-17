<script lang="ts">
  import { untrack } from "svelte";
  import { api, errMessage, type Profile, type Category, type BatchCandidate, type BatchTargetResult, type BatchOp } from "./api";
  import { byResolvedName, resolvedName } from "./filesort.svelte";
  import { primaryProfileDir, profileLabels } from "./profiles";

  let { openPath }: { openPath: string | null } = $props();

  // All char/user files across discovery, as source options.
  let profiles = $state<Profile[]>([]);
  api.discover().then((p) => (profiles = p)).catch(() => {});
  const sources = $derived(
    profiles
      .flatMap((p) =>
        p.files
          .filter((f) => f.kind === "char" || f.kind === "user")
          .map((f) => ({
            path: f.path,
            file_name: f.file_name,
            id: f.id,
            kind: f.kind,
            dir: p.dir,
          })),
      )
      .sort(byResolvedName),
  );

  // Discovery spans every profile, and the same character name can appear in
  // several — so the source is picked in two steps, profile then file, rather
  // than from one long ambiguous list. Only profiles holding a usable file are
  // offered.
  const folders = $derived.by(() => {
    const labels = profileLabels(profiles);
    return profiles
      .filter((p) => sources.some((s) => s.dir === p.dir))
      .map((p) => ({ dir: p.dir, label: labels.get(p.dir)! }));
  });

  // The folder follows the chosen file, so opening the view on a file lands on
  // its profile; with nothing chosen, the profile actually in use (most
  // recently touched) is the best guess. An explicit pick overrides both.
  let folderPick = $state<string | null>(null);
  const autoFolder = $derived(
    sources.find((s) => s.path === sourcePath)?.dir ?? primaryProfileDir(profiles),
  );
  const folder = $derived(folderPick ?? autoFolder);
  const filesInFolder = $derived(sources.filter((s) => s.dir === folder));

  function pickFolder(dir: string) {
    folderPick = dir;
    sourcePath = null; // the previously picked file lives in another profile
  }

  // Defaults to the file open in the editor when this view mounts; the user
  // can then pick a different source, so only the initial value is captured.
  let sourcePath = $state<string | null>(untrack(() => openPath));
  const source = $derived(sources.find((s) => s.path === sourcePath) ?? null);
  const sourceKind = $derived(source?.kind ?? null);
  // Reset op + targets whenever the source changes.
  $effect(() => {
    sourcePath; // track
    fullCopy = false;
    selectedCats = new Set();
    selectedTargets = new Set();
  });

  // Categories available for the source's type.
  const availableCats: Category[] = $derived(
    sourceKind === "char" ? ["layout"] : sourceKind === "user" ? ["autofill"] : [],
  );
  const catLabel: Record<Category, string> = { layout: "Window layout", autofill: "Autofill (remembered text)" };

  let fullCopy = $state(false);
  let selectedCats = $state<Set<Category>>(new Set());
  function toggleCat(c: Category) {
    const next = new Set(selectedCats);
    next.has(c) ? next.delete(c) : next.add(c);
    selectedCats = next;
  }

  let allowOtherFolders = $state(false);
  let candidates = $state<BatchCandidate[]>([]);
  let selectedTargets = $state<Set<string>>(new Set());
  let loadingTargets = $state(false);
  $effect(() => {
    const sp = sourcePath;
    const allow = allowOtherFolders;
    selectedTargets = new Set();
    if (!sp) { candidates = []; return; }
    loadingTargets = true;
    api.batchTargets(sp, allow)
      .then((c) => (candidates = c))
      .catch(() => (candidates = []))
      .finally(() => (loadingTargets = false));
  });
  function toggleTarget(path: string) {
    const next = new Set(selectedTargets);
    next.has(path) ? next.delete(path) : next.add(path);
    selectedTargets = next;
  }

  // Targets are all the source's kind (batch_targets filters on it), so they
  // order by the same rule the sidebar's file list uses.
  const sortedCandidates = $derived(
    [...candidates].sort((a, b) =>
      byResolvedName(
        { kind: sourceKind ?? "", id: a.id, file_name: a.file_name },
        { kind: sourceKind ?? "", id: b.id, file_name: b.file_name },
      ),
    ),
  );

  const nameOf = (id: number | null, kind: string, fileName: string) => {
    if (id == null) return fileName;
    return resolvedName(kind, id) ?? (kind === "user" ? `account ${id}` : `char ${id}`);
  };
  let busy = $state(false);
  let error = $state<string | null>(null);
  let results = $state<BatchTargetResult[] | null>(null);

  const opChosen = $derived(fullCopy || selectedCats.size > 0);
  const canApply = $derived(!!sourcePath && opChosen && selectedTargets.size > 0 && !busy);

  async function apply() {
    if (!sourcePath) return;
    busy = true; error = null; results = null;
    const op: BatchOp = fullCopy
      ? { kind: "full_copy" }
      : { kind: "categories", categories: [...selectedCats] };
    try {
      results = await api.batchApply(sourcePath, op, [...selectedTargets]);
    } catch (e) {
      error = errMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="batch">
  <h2>Batch apply</h2>

  <section>
    <label for="folder">Profile</label>
    <select id="folder" value={folder} onchange={(e) => pickFolder(e.currentTarget.value)}>
      {#each folders as f}
        <option value={f.dir}>{f.label}</option>
      {/each}
    </select>

    <label for="src">Source file</label>
    <select id="src" bind:value={sourcePath}>
      <option value={null} disabled>Choose a file…</option>
      {#each filesInFolder as s}
        <option value={s.path}>{nameOf(s.id, s.kind, s.file_name)} — {s.kind} — {s.file_name}</option>
      {/each}
    </select>
  </section>

  {#if source}
    <section>
      <div class="head">What to copy</div>
      <label><input type="checkbox" bind:checked={fullCopy} /> Full copy (entire file — overrides categories)</label>
      {#each availableCats as c}
        <label class:disabled={fullCopy}>
          <input type="checkbox" disabled={fullCopy} checked={selectedCats.has(c)} onchange={() => toggleCat(c)} />
          {catLabel[c]}
        </label>
      {/each}
    </section>

    <section>
      <div class="head">
        Targets
        <label class="inline"><input type="checkbox" bind:checked={allowOtherFolders} /> Show other folders</label>
      </div>
      {#if loadingTargets}
        <p class="muted">Loading…</p>
      {:else if candidates.length === 0}
        <p class="muted">No other {sourceKind} files found.</p>
      {:else}
        {#each sortedCandidates as c}
          <label>
            <input type="checkbox" checked={selectedTargets.has(c.path)} onchange={() => toggleTarget(c.path)} />
            {nameOf(c.id, sourceKind ?? "", c.file_name)}
            <span class="muted">{c.file_name}{c.same_folder ? "" : ` · ${c.folder}`}</span>
          </label>
        {/each}
      {/if}
    </section>

    <section>
      {#if selectedTargets.size > 0 && opChosen}
        <p class="preview">Will overwrite {selectedTargets.size} file(s) — each is backed up first.</p>
      {/if}
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
  /* Dark native controls: the app runs in a dark WebView2; give the select and
     its options explicit dark colors (see the dark-native-controls memo).
     Every input in this view is a checkbox, so bare `input` is never styled —
     a background override would paint over its check state; accent-color is
     the safe way to theme a checkbox. */
  select, option { background: var(--bg-panel); color: var(--fg); border: 1px solid var(--border); border-radius: 3px; padding: 2px 4px; font: inherit; }
  input[type="checkbox"] { accent-color: var(--accent); }
  .muted { color: var(--fg-dim); }
  .preview { color: #d0a000; }
  .err, .fail { color: #e06c6c; }
  .ok { color: #6cc06c; }
  button { padding: 0.35rem 0.9rem; }
</style>
