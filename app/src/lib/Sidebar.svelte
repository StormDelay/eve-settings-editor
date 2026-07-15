<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, type Profile } from "./api";
  import { names, resolveNames, refreshNames } from "./names.svelte";

  let { onOpen, onShowAccounts }: { onOpen: (path: string) => void; onShowAccounts: () => void } =
    $props();

  let profiles: Profile[] = $state([]);
  let error: string | null = $state(null);
  let flash: string | null = $state(null);
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  let namesBusy = $state(false);
  // Hide user-made backups / anomalous names, keeping only EVE's own working
  // file names (core_char_<id>.dat / core_user_<id>.dat). On by default.
  let hideNonStandard = $state(true);
  const isStandardName = (name: string) => /^core_(char|user)_\d+\.dat$/.test(name);

  // Two installs can hold the same server and profile name (a SharedCache dir
  // and a legacy one both with settings_Default) — then, and only then, the
  // install name is what tells them apart. Full path is on the tooltip.
  // discover() returns them alphabetically; the profile whose files were
  // touched most recently is the one actually in use, so it gets pinned on top
  // and opened. Array.sort is stable, so the rest keep their alphabetical run.
  const rows = $derived.by(() => {
    const seen = new Map<string, number>();
    const key = (p: Profile) => `${p.server} / ${p.profile}`;
    for (const p of profiles) seen.set(key(p), (seen.get(key(p)) ?? 0) + 1);

    const touched = (p: Profile) =>
      p.files.reduce((max, f) => Math.max(max, f.modified_unix ?? 0), 0);
    const times = profiles.map(touched);
    const newest = times.reduce((best, t, i) => (t > times[best] ? i : best), 0);

    return profiles
      .map((p, i) => ({
        p,
        label: seen.get(key(p))! > 1 ? `${key(p)} · ${p.install}` : key(p),
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
  <div class="sidebar-actions">
    <button onclick={pickFile}>Open file…</button>
    <button onclick={() => refresh(true)} title="Rescan standard EVE locations">⟳</button>
    <button
      onclick={refreshNamesClick}
      disabled={namesBusy}
      title="Re-fetch character names from ESI">{namesBusy ? "Refreshing…" : "Refresh names"}</button>
    <button onclick={onShowAccounts} title="Manage account names and character associations"
      >Accounts</button>
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
    <details open={primary}>
      <summary title={p.dir}>
        {label}
        {#if primary}<span class="meta">most recent</span>{/if}
      </summary>
      <ul>
        {#each p.files.filter((f) => !hideNonStandard || isStandardName(f.file_name)) as f (f.path)}
          {@const hit = f.kind === "char" && f.id != null ? names[f.id] : undefined}
          <li>
            <button class="file" onclick={() => onOpen(f.path)} title={f.file_name}>
              {hit ? hit.name : f.file_name}
              <span class="meta">{Math.round(f.size / 1024)} KB</span>
            </button>
          </li>
        {/each}
      </ul>
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
</style>
