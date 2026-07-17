<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, type Profile } from "./api";
  import { resolveNames, refreshNames } from "./names.svelte";
  import { loadRoster } from "./accounts.svelte";
  import { byResolvedName, resolvedName } from "./filesort.svelte";
  import { profileLabels } from "./profiles";

  let {
    onOpen,
    onShowAccounts,
    onShowBatch,
    onCollapse,
  }: {
    onOpen: (path: string) => void;
    onShowAccounts: () => void;
    onShowBatch: () => void;
    onCollapse: () => void;
  } = $props();

  // Naming and ordering come from filesort, shared with the batch-apply target
  // list so the two cannot drift apart.
  const byName = byResolvedName;

  let profiles: Profile[] = $state([]);
  let error: string | null = $state(null);
  let flash: string | null = $state(null);
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  let namesBusy = $state(false);
  // Hide user-made backups / anomalous names, keeping only EVE's own working
  // file names (core_char_<id>.dat / core_user_<id>.dat). On by default.
  let hideNonStandard = $state(true);
  const isStandardName = (name: string) => /^core_(char|user)_\d+\.dat$/.test(name);

  // Profile labels come from profiles.ts, shared with the batch-apply source
  // picker (which faces the same ambiguity). Full path is on the tooltip.
  // discover() returns them alphabetically; the profile whose files were
  // touched most recently is the one actually in use, so it gets pinned on top
  // and opened. Array.sort is stable, so the rest keep their alphabetical run.
  const rows = $derived.by(() => {
    const labels = profileLabels(profiles);

    const touched = (p: Profile) =>
      p.files.reduce((max, f) => Math.max(max, f.modified_unix ?? 0), 0);
    const times = profiles.map(touched);
    const newest = times.reduce((best, t, i) => (t > times[best] ? i : best), 0);

    return profiles
      .map((p, i) => ({
        p,
        label: labels.get(p.dir)!,
        primary: i === newest && times[i] > 0,
      }))
      .sort((a, b) => Number(b.primary) - Number(a.primary));
  });

  const charIds = (ps: Profile[]) =>
    ps
      .flatMap((p) => p.files)
      .filter((f) => f.kind === "char" && f.id != null)
      .map((f) => f.id as number);

  async function refresh(announce = false) {
    try {
      profiles = await api.discover();
      void resolveNames(charIds(profiles));
      void loadRoster();
      error = null;
      if (announce) {
        const n = profiles.length;
        flash = `Refreshed — ${n} profile${n === 1 ? "" : "s"}`;
        clearTimeout(flashTimer);
        flashTimer = setTimeout(() => (flash = null), 2000);
      }
    } catch (e) {
      error = errMessage(e);
    }
  }

  async function refreshNamesClick() {
    if (namesBusy) return;
    namesBusy = true;
    try {
      await refreshNames(charIds(profiles));
    } finally {
      namesBusy = false;
    }
    flash = "Names refreshed";
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flash = null), 2000);
  }

  async function pickFile() {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "EVE settings", extensions: ["dat"] }],
    });
    if (typeof picked === "string") onOpen(picked);
  }

  refresh();
</script>

<aside class="sidebar">
  <div class="sidebar-top">
    <div class="sidebar-actions">
      <button onclick={pickFile}>Open file…</button>
      <button onclick={() => refresh(true)} title="Rescan standard EVE locations">⟳</button>
      <button
        onclick={refreshNamesClick}
        disabled={namesBusy}
        title="Re-fetch character names from ESI">{namesBusy ? "Refreshing…" : "Refresh names"}</button>
      <button onclick={onShowAccounts} title="Manage account names and character associations"
        >Accounts</button>
      <button onclick={onShowBatch} title="Copy settings from one file to many, backing up each target first"
        >Batch apply</button>
    </div>
    <button class="collapse" onclick={onCollapse} title="Hide file list" aria-label="Hide file list"
      >«</button>
  </div>
  <label class="toggle" title="Show only EVE's own core_char_<id>.dat / core_user_<id>.dat files">
    <input type="checkbox" bind:checked={hideNonStandard} />
    Hide non-standard files
  </label>
  {#if flash}<p class="flash" aria-live="polite">{flash}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
  {#if profiles.length === 0}
    <p class="hint">No EVE profiles found in standard locations. Use “Open file…”.</p>
  {/if}
  {#each rows as { p, label, primary } (p.dir)}
    {@const visible = p.files.filter((f) => !hideNonStandard || isStandardName(f.file_name))}
    {@const groups = [
      { title: "Characters", files: visible.filter((f) => f.kind === "char").sort(byName) },
      { title: "Accounts", files: visible.filter((f) => f.kind === "user").sort(byName) },
      { title: "Other", files: visible.filter((f) => f.kind === "other").sort(byName) },
    ]}
    <details open={primary}>
      <summary title={p.dir}>
        {label}
        {#if primary}<span class="meta">most recent</span>{/if}
      </summary>
      <!-- Group by file kind so an account alias and a character with the same
           displayed name are never ambiguous. -->
      {#each groups as g (g.title)}
        {#if g.files.length > 0}
          <details class="group-fold" open>
            <summary class="group">{g.title}</summary>
            <ul>
              {#each g.files as f (f.path)}
                <li>
                  <button class="file" onclick={() => onOpen(f.path)} title={f.file_name}>
                    {resolvedName(f.kind, f.id) ?? f.file_name}
                    <span class="meta">{Math.round(f.size / 1024)} KB</span>
                  </button>
                </li>
              {/each}
            </ul>
          </details>
        {/if}
      {/each}
    </details>
  {/each}
</aside>

<style>
  .toggle {
    display: flex;
    align-items: center;
    gap: 0.4em;
    padding: 0.25rem 0.1rem 0.5rem;
    font-size: 0.85em;
    opacity: 0.75;
    cursor: pointer;
  }
  .toggle input {
    cursor: pointer;
  }
  .group {
    margin: 0.4rem 0 0.1rem;
    font-size: 0.72em;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--fg-dim);
    opacity: 0.85;
    cursor: pointer;
  }
  /* Collapse chevron pinned to the sidebar's inner (right) edge; the toolbar
     takes the remaining width and wraps within it. */
  .sidebar-top {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin-bottom: 8px;
  }
  .sidebar-top .sidebar-actions {
    flex: 1;
    margin-bottom: 0;
  }
  .collapse {
    padding: 0 6px;
  }
</style>
