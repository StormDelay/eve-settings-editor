<script lang="ts">
  import { api, errMessage, type Keybinds, type KeybindEntry } from "./api";
  import { labelFor, groupFor, GROUP_ORDER, defaultFor, keysToLabel, eventToKeys } from "./keybinds";
  import { message } from "@tauri-apps/plugin-dialog";

  let { userOpen, userId = null, onUserDirty, onShowAccounts = () => {}, onShowBatch = () => {}, sharedLabel = "" }:
    { userOpen: boolean; userId?: number | null; onUserDirty: () => void;
      onShowAccounts?: () => void; onShowBatch?: () => void; sharedLabel?: string } = $props();

  let binds = $state<Keybinds | null>(null);
  let error = $state<string | null>(null);
  let query = $state("");
  /** Command currently listening for a keypress, or null. */
  let listening = $state<string | null>(null);
  /** Transient "took X from Y" notice, keyed by the command that LOST it. */
  let stolenFrom = $state<Record<string, string>>({});

  async function reload() {
    // Both are about the CURRENT table, so neither may outlive it. Command
    // names are global, so a "taken by X" note left on one account would
    // silently reattach to the same row on the next one and describe a theft
    // that never happened there.
    stolenFrom = {};
    listening = null;
    if (!userOpen) { binds = null; return; }
    error = null;
    try { binds = await api.keybinds(); }
    catch (e) { error = errMessage(e); }
  }
  $effect(() => { void userOpen; void userId; reload(); });

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const all = binds?.entries ?? [];
    if (!q) return all;
    return all.filter(
      (e) =>
        labelFor(e.command).toLowerCase().includes(q) ||
        e.command.toLowerCase().includes(q) ||
        keysToLabel(e.keys).toLowerCase().includes(q),
    );
  });

  /** Grouped for display; the projection reports file order, grouping is ours. */
  const grouped = $derived.by(() => {
    const by = new Map<string, KeybindEntry[]>();
    for (const e of filtered) {
      const g = groupFor(e.command);
      if (!by.has(g)) by.set(g, []);
      by.get(g)!.push(e);
    }
    const rank = (g: string) => { const i = GROUP_ORDER.indexOf(g); return i === -1 ? GROUP_ORDER.length : i; };
    return [...by.entries()].sort((a, b) => rank(a[0]) - rank(b[0]));
  });

  async function commit(command: string, keys: number[] | null) {
    try {
      const res = await api.setKeybind(command, keys);
      binds = res.keybinds;
      onUserDirty();
      // Name what was taken, on the row that lost it.
      const next: Record<string, string> = {};
      for (const lost of res.stolen) next[lost] = labelFor(command);
      stolenFrom = next;
    } catch (e) {
      await message(errMessage(e), { title: "Rebind failed", kind: "error" });
    } finally {
      listening = null;
    }
  }

  function onKeydown(e: KeyboardEvent, command: string) {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") { listening = null; return; }
    if (e.key === "Backspace") { void commit(command, null); return; }
    const keys = eventToKeys(e);
    if (keys === null) return; // bare modifier, or a key EVE cannot store
    void commit(command, keys);
  }
</script>

{#if !userOpen}
  <p class="empty">
    No account file open. <button class="link" onclick={onShowAccounts}>Pair this character…</button>
  </p>
{:else if error}
  <p class="error">{error}</p>
{:else if binds && !binds.available}
  <p class="empty">
    This account has no keybinding table yet. EVE only writes one once you have opened
    the in-game keybinding screen at least once on this account.
    <button class="link" onclick={onShowBatch}>Copy bindings from another account…</button>
  </p>
{:else if binds}
  {#if sharedLabel}<p class="shared-banner">{sharedLabel}</p>{/if}
  <div class="searchbar">
    <!-- No "(Ctrl+F)" hint: that shortcut still opens the Tree search from the
         page-level handler. Wiring it per-view is being done on the layout
         branch; this placeholder gains the hint when that lands. -->
    <input class="search" bind:value={query} placeholder="Search commands and keys" />
  </div>
  {#each grouped as [group, entries] (group)}
    <h3>{group}</h3>
    <table class="binds">
      <tbody>
        {#each entries as e (e.command)}
          <tr class:malformed={e.malformed}>
            <td class="label" title={e.command}>{labelFor(e.command)}</td>
            <td class="combo">
              {#if e.malformed}
                <span class="chip readonly" title="Unrecognised value; left untouched">unreadable</span>
              {:else}
                <button
                  class="chip"
                  class:listening={listening === e.command}
                  onclick={() => (listening = e.command)}
                  onkeydown={(ev) => listening === e.command && onKeydown(ev, e.command)}>
                  {listening === e.command ? "press a key…" : keysToLabel(e.keys)}
                </button>
              {/if}
              {#if stolenFrom[e.command]}
                <span class="meta">taken by {stolenFrom[e.command]}</span>
              {/if}
            </td>
            <td class="default">{keysToLabel(defaultFor(e.command))}</td>
            <td>
              <button
                class="mini"
                disabled={defaultFor(e.command) === null}
                title="Reset to EVE's default (not yet captured)"
                onclick={() => commit(e.command, defaultFor(e.command))}>↺</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/each}
  {#if listening}
    <p class="meta">Esc cancels · Backspace unbinds</p>
  {/if}
{/if}

<style>
  /* Native controls render light in the dark WebView2 shell unless told
     otherwise — see the dark-native-controls note in the repo memory. */
  .search { background: var(--bg-panel); color: var(--fg); border: 1px solid var(--border); }
  .chip { background: var(--bg-panel); color: var(--fg); border: 1px solid var(--border); min-width: 7rem; }
  .chip.listening { border-color: var(--accent); }
  .chip.readonly { opacity: 0.6; }
  .default { opacity: 0.5; }
  tr.malformed { opacity: 0.6; }
  .meta { opacity: 0.7; font-size: 0.85em; margin-left: 0.5rem; }
  .shared-banner {
    margin: 0 0 0.6rem; padding: 0.3rem 0.5rem; font-size: 0.85em;
    color: var(--fg-dim); border-left: 2px solid var(--accent); background: var(--bg-panel);
  }
</style>
