<script lang="ts">
  import { api, errMessage, type RememberedList } from "./api";
  import { labelFor } from "./autofill";
  import { message, confirm } from "@tauri-apps/plugin-dialog";

  let { userOpen, onUserDirty }: { userOpen: boolean; onUserDirty: () => void } = $props();

  let lists = $state<RememberedList[] | null>(null);
  let error = $state<string | null>(null);

  async function reload() {
    if (!userOpen) { lists = null; return; }
    error = null;
    try { lists = await api.autofillLists(); }
    catch (e) { error = errMessage(e); }
  }
  $effect(() => { void userOpen; reload(); });

  // Sort by friendly label for findability; the raw path is shown per row.
  const sorted = $derived(
    lists ? [...lists].sort((a, b) => labelFor(a.widget).localeCompare(labelFor(b.widget))) : [],
  );

  async function commit(widget: string, entries: string[]) {
    try { lists = await api.setAutofillList(widget, entries); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  const removeAt = (l: RememberedList, i: number) =>
    commit(l.widget, l.entries.filter((_, j) => j !== i));
  const editAt = (l: RememberedList, i: number, text: string) =>
    commit(l.widget, l.entries.map((e, j) => (j === i ? text : e)));
  const addTo = (l: RememberedList, text: string) => {
    if (text.trim() === "") return;
    commit(l.widget, [...l.entries, text]);
  };
  const clearList = (l: RememberedList) => commit(l.widget, []);

  // Drag-reorder within one list.
  let drag = $state<{ widget: string; from: number } | null>(null);
  function drop(l: RememberedList, to: number) {
    if (!drag || drag.widget !== l.widget) { drag = null; return; }
    const next = [...l.entries];
    const [moved] = next.splice(drag.from, 1);
    next.splice(to, 0, moved);
    drag = null;
    commit(l.widget, next);
  }

  async function clearAll() {
    const ok = await confirm(
      "Clear ALL remembered text in this account file? Every autofill list will be emptied. A backup is taken on save.",
      { title: "Clear all remembered text", kind: "warning" },
    );
    if (!ok) return;
    try { lists = await api.clearAllAutofill(); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Clear all failed", kind: "error" }); }
  }
</script>

{#if !userOpen}
  <p class="hint">Open an account file to edit its remembered text.</p>
{:else if error}
  <p class="error">{error}</p>
{:else if lists && lists.length === 0}
  <p class="hint">No remembered text in this account file yet.</p>
{:else if lists}
  <div class="af-top">
    <button class="danger" onclick={clearAll}>Clear all remembered text</button>
  </div>
  {#each sorted as l (l.widget)}
    <section class="af-list">
      <header>
        <span class="title" title={l.widget}>{labelFor(l.widget)}</span>
        <span class="path">{l.widget}</span>
        <button class="mini" onclick={() => clearList(l)} disabled={l.entries.length === 0}>Clear</button>
      </header>
      <ul>
        <!-- Index-keyed: safe only because inputs below are one-way (value=,
             commit-on-change) and edits replace `lists` wholesale afterward.
             If this ever grows bind:value or autofocus, switch to a
             content-based key first or rows will steal focus on reorder/removal. -->
        {#each l.entries as entry, i (i)}
          <li draggable="true"
              ondragstart={(e) => { drag = { widget: l.widget, from: i };
                e.dataTransfer?.setData("text/plain", String(i));
                if (e.dataTransfer) e.dataTransfer.effectAllowed = "move"; }}
              ondragover={(e) => { e.preventDefault();
                if (e.dataTransfer) e.dataTransfer.dropEffect = "move"; }}
              ondrop={(e) => { e.preventDefault(); drop(l, i); }}
              ondragend={() => (drag = null)}>
            <span class="grip" title="Drag to reorder">⠿</span>
            <input value={entry}
                   onchange={(e) => editAt(l, i, (e.target as HTMLInputElement).value)} />
            <button class="mini" title="Remove" onclick={() => removeAt(l, i)}>×</button>
          </li>
        {/each}
        <li class="add">
          <input placeholder="+ add remembered text…"
                 onkeydown={(e) => { if (e.key === "Enter") {
                   const t = e.target as HTMLInputElement; addTo(l, t.value); t.value = ""; } }} />
        </li>
      </ul>
    </section>
  {/each}
{/if}

<style>
  .af-top { margin-bottom: 0.75rem; }
  .af-list { margin-bottom: 1rem; }
  .af-list header { display: flex; align-items: baseline; gap: 0.6rem; }
  .af-list .title { font-weight: 600; }
  .af-list .path { color: var(--fg-dim); font-size: 0.8em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .af-list ul { list-style: none; padding: 0; margin: 0.25rem 0 0; }
  .af-list li { display: flex; align-items: center; gap: 0.4rem; padding: 0.1rem 0; }
  .grip { cursor: grab; opacity: 0.6; }
  /* Dark native controls: the app runs in a dark WebView2 (see the memo). */
  input, button.mini, button.danger {
    background: var(--bg-panel); color: var(--fg);
    border: 1px solid var(--border); border-radius: 3px; padding: 2px 6px; font: inherit;
  }
  .af-list li input { flex: 1; }
  button.danger { border-color: #a33; }
  .hint, .error { color: var(--fg-dim); }
  .error { color: #e66; }
</style>
