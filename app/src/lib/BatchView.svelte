<script lang="ts">
  import { untrack } from "svelte";
  import { api, errMessage, type Profile, type Aspect, type SetupPlan, type BatchTargetResult } from "./api";
  import { byResolvedName, resolvedName } from "./filesort.svelte";
  import { primaryProfileDir, profileLabels } from "./profiles";
  import { accountsStore, loadRoster } from "./accounts.svelte";

  let { openPath }: { openPath: string | null } = $props();

  loadRoster();

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
  const source = $derived(chars.find((c) => c.path === sourcePath) ?? null);

  function pickFolder(dir: string) {
    folderPick = dir;
    sourcePath = null;
  }

  // Aspects. "Everything" is exclusive.
  const ASPECTS: { key: Aspect; label: string; account: boolean }[] = [
    { key: "layout", label: "Window layout", account: false },
    { key: "overview", label: "Overview (columns, tabs, presets)", account: true },
    { key: "autofill", label: "Autofill (remembered text)", account: true },
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
      .filter((c) => c.path !== sourcePath)
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

  const nameOfChar = (id: number | null, fileName: string) =>
    id == null ? fileName : (resolvedName("char", id) ?? `char ${id}`);
  const folderLabelOf = (dir: string) => profileLabels(profiles).get(dir) ?? dir;

  // Reset op + targets when the source changes.
  $effect(() => {
    sourcePath;
    selected = new Set();
    selectedTargets = new Set();
  });

  // Preview from the backend whenever source/aspects/targets settle.
  let plan = $state<SetupPlan | null>(null);
  $effect(() => {
    const sp = sourcePath;
    const asp = [...selected];
    const tgts = [...selectedTargets];
    const allow = allowOtherFolders;
    if (!sp || asp.length === 0 || tgts.length === 0) { plan = null; return; }
    api.setupPreview(sp, tgts, asp as Aspect[], allow).then((p) => (plan = p)).catch(() => (plan = null));
  });

  let busy = $state(false);
  let error = $state<string | null>(null);
  let results = $state<BatchTargetResult[] | null>(null);
  const canApply = $derived(
    !!sourcePath && selected.size > 0 && selectedTargets.size > 0 && !busy &&
    !!plan && !plan.source_error && (plan.char_writes.length + plan.account_writes.length > 0),
  );

  async function apply() {
    if (!sourcePath) return;
    busy = true; error = null; results = null;
    try {
      results = await api.setupApply(sourcePath, [...selectedTargets], [...selected] as Aspect[], allowOtherFolders);
    } catch (e) {
      error = errMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="batch">
  <h2>Copy a character's setup</h2>

  <section>
    <label for="folder">Profile</label>
    <select id="folder" value={folder} onchange={(e) => pickFolder(e.currentTarget.value)}>
      {#each folders as f}<option value={f.dir}>{f.label}</option>{/each}
    </select>

    <label for="src">Source character</label>
    <select id="src" bind:value={sourcePath}>
      <option value={null} disabled>Choose a character…</option>
      {#each sourceOptions as c}
        <option value={c.path}>{nameOfChar(c.id, c.file_name)} — {c.file_name}</option>
      {/each}
    </select>
  </section>

  {#if source}
    <section>
      <div class="head">What to copy</div>
      {#each ASPECTS as a}
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
        <label class="inline"><input type="checkbox" bind:checked={allowOtherFolders} /> Show other folders</label>
      </div>
      {#if candidates.length === 0}
        <p class="muted">No other character files found.</p>
      {:else}
        {#each candidates as c}
          <label class:disabled={targetDisabled(c.id)}>
            <input type="checkbox" checked={selectedTargets.has(c.path)}
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
          {#each plan.account_writes.filter((w) => w.collateral_char_ids.length > 0) as w}
            <p class="warn">⚠ {w.full_copy ? "Entire account settings replaced" : "Overview / autofill changed"} for account {w.user_id} — also changes: {w.collateral_char_ids.map((id) => nameOfChar(id, `char ${id}`)).join(", ")}.</p>
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
  select, option { background: var(--bg-panel); color: var(--fg); border: 1px solid var(--border); border-radius: 3px; padding: 2px 4px; font: inherit; }
  input[type="checkbox"] { accent-color: var(--accent); }
  .muted { color: var(--fg-dim); }
  .preview p { margin: 0.15rem 0; }
  .warn { color: #d0a000; }
  .err, .fail { color: #e06c6c; }
  .ok { color: #6cc06c; }
  button { padding: 0.35rem 0.9rem; }
</style>
