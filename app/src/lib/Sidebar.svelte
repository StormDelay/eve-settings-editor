<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, errMessage, type Profile } from "./api";

  let { onOpen }: { onOpen: (path: string) => void } = $props();

  let profiles: Profile[] = $state([]);
  let error: string | null = $state(null);

  async function refresh() {
    try {
      profiles = await api.discover();
      error = null;
    } catch (e) {
      error = errMessage(e);
    }
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
    <button onclick={refresh} title="Rescan standard EVE locations">⟳</button>
  </div>
  {#if error}<p class="error">{error}</p>{/if}
  {#if profiles.length === 0}
    <p class="hint">No EVE profiles found in standard locations. Use “Open file…”.</p>
  {/if}
  {#each profiles as p (p.dir)}
    <details open>
      <summary>{p.server} / {p.profile}</summary>
      <ul>
        {#each p.files as f (f.path)}
          <li>
            <button class="file" onclick={() => onOpen(f.path)}>
              {f.file_name}
              <span class="meta">{Math.round(f.size / 1024)} KB</span>
            </button>
          </li>
        {/each}
      </ul>
    </details>
  {/each}
</aside>
