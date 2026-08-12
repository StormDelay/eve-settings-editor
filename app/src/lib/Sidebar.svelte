<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, type Profile, type PresetInfo } from "./api";
  import { resolveNames, refreshNames } from "./names.svelte";
  import { loadRoster, aliasFor, accountsStore } from "./accounts.svelte";
  import { accountOf } from "./overview";
  import { byResolvedName, resolvedName } from "./filesort.svelte";
  import { primaryProfileDir, profileLabels, profileNote } from "./profiles";
  import PresetGroup from "./PresetGroup.svelte";
  import AboutPanel from "./AboutPanel.svelte";

  let {
    onOpen,
    onShowAccounts,
    onShowBatch,
    onCollapse,
    onOpenPreset,
    charOpen,
    userOpen,
    openPresetName,
  }: {
    onOpen: (path: string) => void;
    onShowAccounts: () => void;
    onShowBatch: () => void;
    onCollapse: () => void;
    onOpenPreset: (p: PresetInfo) => void;
    charOpen: boolean;
    userOpen: boolean;
    openPresetName: string | null;
  } = $props();

  // Naming and ordering come from filesort, shared with the batch-apply target
  // list so the two cannot drift apart.
  const byName = byResolvedName;

  let profiles: Profile[] = $state([]);
  let error: string | null = $state(null);
  let flash: string | null = $state(null);
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  let aboutOpen = $state(false);
  let namesBusy = $state(false);
  // Hide user-made backups / anomalous names, keeping only EVE's own working
  // file names (core_char_<id>.dat / core_user_<id>.dat). On by default.
  let hideNonStandard = $state(true);
  const isStandardName = (name: string) => /^core_(char|user)_\d+\.dat$/.test(name);

  // Profile labels come from profiles.ts, shared with the batch-apply source
  // picker (which faces the same ambiguity). Full path is on the tooltip.
  // discover() returns them alphabetically; the profile EVE itself wrote last
  // is the one actually in use, so it gets pinned on top and opened. Array.sort
  // is stable, so the rest keep their alphabetical run.
  const rows = $derived.by(() => {
    const labels = profileLabels(profiles);
    const primaryDir = primaryProfileDir(profiles);
    return profiles
      .map((p) => ({ p, label: labels.get(p.dir)!, primary: p.dir === primaryDir }))
      .sort((a, b) => Number(b.primary) - Number(a.primary));
  });

  // Mirrors the per-row filter below: a profile with no character file renders
  // no header at all (its account files are reachable via "Open file..."), so
  // when EVERY profile is in that state the sidebar came up blank with nothing
  // saying why. The filter lives in the template because each row needs its own
  // sorted list; this only needs to know whether any row will draw.
  const anyCharVisible = $derived(
    profiles.some((p) =>
      p.files.some((f) => f.kind === "char" && (!hideNonStandard || isStandardName(f.file_name))),
    ),
  );

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
        >Copy settings</button>
      <button onclick={() => (aboutOpen = true)} title="Version and licence">About</button>
    </div>
    <button class="collapse" onclick={onCollapse} title="Hide file list" aria-label="Hide file list"
      >«</button>
  </div>
  <label class="toggle" title="Show only EVE's own core_char_<id>.dat files">
    <input type="checkbox" bind:checked={hideNonStandard} />
    Hide non-standard files
  </label>
  {#if flash}<p class="flash" aria-live="polite">{flash}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
  {#if profiles.length === 0}
    <p class="hint">No EVE profiles found in standard locations. Use “Open file…”.</p>
  {:else if !anyCharVisible}
    <p class="hint">
      {hideNonStandard
        ? "No character files with EVE's own names in these profiles. Untick “Hide non-standard files”, or use “Open file…”."
        : "These profiles hold no character files. Use “Open file…” to open an account file directly."}
    </p>
  {/if}
  <PresetGroup {onOpenPreset} {charOpen} {userOpen} {openPresetName} />
  {#each rows as { p, label, primary } (p.dir)}
    {@const chars = p.files
      .filter((f) => f.kind === "char" && (!hideNonStandard || isStandardName(f.file_name)))
      .sort(byName)}
    {#if chars.length > 0}
      <details open={primary}>
        <summary title={p.dir}>
          {label}
          <span class="meta" class:not-live={!primary}>{profileNote(primary)}</span>
        </summary>
        <ul>
          {#each chars as f (f.path)}
            {@const userId = f.id === null ? null : accountOf(f.id, accountsStore.roster)}
            {@const alias = userId === null ? null : aliasFor(userId)}
            <li>
              <button class="file" onclick={() => onOpen(f.path)} title={f.file_name}>
                {resolvedName(f.kind, f.id) ?? f.file_name}
                {#if alias}<span class="acct">· {alias}</span>{/if}
                <span class="meta">{Math.round(f.size / 1024)} KB</span>
              </button>
            </li>
          {/each}
        </ul>
      </details>
    {/if}
  {/each}
</aside>

{#if aboutOpen}<AboutPanel onClose={() => (aboutOpen = false)} />{/if}

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
  .acct { color: var(--fg-dim); font-size: 0.85em; margin: 0 0.3em; }
  /* A non-live profile is a real hazard, not a detail: editing one looks like
     it worked and changes nothing the game reads. */
  .meta.not-live { color: var(--warn, #d08770); }
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
