<script lang="ts">
  import { api, errMessage, type RememberedList } from "./api";
  import { labelFor } from "./autofill";
  import { message, confirm } from "@tauri-apps/plugin-dialog";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";
  import ScopeBanner from "./ui/ScopeBanner.svelte";
  import SearchField from "./ui/SearchField.svelte";

  let { userOpen, userId = null, onUserDirty, charOpen = false, charName = null, sharedLabel = "", onShowAccounts = () => {} }:
    { userOpen: boolean; userId?: number | null; onUserDirty: () => void;
      /** A character file is open. Separate from `charName`, which stays null
       * until the ESI name lookup resolves — offline, it never does. */
      charOpen?: boolean;
      charName?: string | null; sharedLabel?: string; onShowAccounts?: () => void } = $props();

  let lists = $state<RememberedList[] | null>(null);
  let error = $state<string | null>(null);

  async function reload() {
    if (!userOpen) { lists = null; return; }
    error = null;
    try { lists = await api.autofillLists(); }
    catch (e) { error = errMessage(e); }
  }
  $effect(() => { void userOpen; void userId; reload(); });

  // Sort by friendly label for findability; the raw path is shown per row.
  const sorted = $derived(
    lists ? [...lists].sort((a, b) => labelFor(a.widget).localeCompare(labelFor(b.widget))) : [],
  );

  // Filter box: narrow to lists whose label, raw widget path, or any remembered
  // entry contains the query ("which list has that station name?"). Empty shows all.
  let query = $state("");
  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return sorted;
    return sorted.filter(
      (l) =>
        labelFor(l.widget).toLowerCase().includes(q) ||
        l.widget.toLowerCase().includes(q) ||
        l.entries.some((e) => e.toLowerCase().includes(q)),
    );
  });

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
  {#if charOpen}
    <!-- Keyed off the file being open, not off knowing whose it is: this used
         to test `charName`, so a character whose name had not resolved (an
         unnamed file, or any character at all with no ESI lookup) was told to
         "open a character" while one was already open. -->
    <!-- Not EmptyState, though §5.7 nominates it: the sentence carries a
         <strong> around the character's name, which EmptyState's plain-string
         title and description cannot hold, and AutofillView.spec reads this
         paragraph's own text. The complaint §5.7 actually names — that this
         prompt and OverviewView's render the same thing with two different
         button treatments — is fixed by both now being a Button. -->
    <div class="pair">
      <p>Link <strong>{charName ?? "this character"}</strong> to an account to edit shared settings.</p>
      <Button onclick={onShowAccounts}>Pair…</Button>
    </div>
  {:else}
    <EmptyState title="Open a character to edit its account's remembered text." />
  {/if}
{:else if error}
  <InlineMessage variant="error">{error}</InlineMessage>
{:else if lists && lists.length === 0}
  <EmptyState title="No remembered text in this account file yet." />
{:else if lists}
  <ScopeBanner label={sharedLabel ?? ""} />
  <div class="af-top">
    <SearchField class="af-filter" nouns="lists" bind:value={query} />
    <Button variant="danger" onclick={clearAll}>Clear all remembered text</Button>
  </div>
  {#if filtered.length === 0}
    <EmptyState title="No lists match “{query}”." />
  {/if}
  {#each filtered as l (l.widget)}
    <section class="af-list">
      <header>
        <span class="title" title={l.widget}>{labelFor(l.widget)}</span>
        <span class="path">{l.widget}</span>
        <!-- Was `.mini`, and therefore invisible: it sits outside any `.row`,
             so `.row:hover .mini { opacity: 1 }` never fired. It clears a whole
             list. -->
        <Button
          variant="ghost"
          size="sm"
          onclick={() => clearList(l)}
          disabled={l.entries.length === 0}
          disabledReason="This list is already empty">Clear</Button>
      </header>
      <ul>
        <!-- Index-keyed: safe only because inputs below are one-way (value=,
             commit-on-change) and edits replace `lists` wholesale afterward.
             If this ever grows bind:value or autofocus, switch to a
             content-based key first or rows will steal focus on reorder/removal. -->
        {#each l.entries as entry, i (i)}
          <li>
            <ListRow
              draggable
              ondragstart={(e) => { drag = { widget: l.widget, from: i };
                e.dataTransfer?.setData("text/plain", String(i));
                if (e.dataTransfer) e.dataTransfer.effectAllowed = "move"; }}
              ondragover={(e) => { e.preventDefault();
                if (e.dataTransfer) e.dataTransfer.dropEffect = "move"; }}
              ondrop={(e) => { e.preventDefault(); drop(l, i); }}
              ondragend={() => (drag = null)}>
              <Field
                class="entry"
                ariaLabel="Remembered text {i + 1}"
                value={entry}
                onchange={(e) => editAt(l, i, (e.target as HTMLInputElement).value)} />
              {#snippet trailing()}
                <!-- Also `.mini`, also invisible, and this one deletes an entry. -->
                <Button variant="ghost" size="sm" iconOnly title="Remove" onclick={() => removeAt(l, i)}>
                  ×
                </Button>
              {/snippet}
            </ListRow>
          </li>
        {/each}
        <li class="add">
          <Field
            class="entry"
            ariaLabel="Add remembered text"
            placeholder="+ add remembered text…"
            onkeydown={(e: KeyboardEvent & { currentTarget: HTMLInputElement }) => {
              if (e.key === "Enter") { addTo(l, e.currentTarget.value); e.currentTarget.value = ""; } }} />
        </li>
      </ul>
    </section>
  {/each}
{/if}

<style>
  /* The whole "give the native control explicit dark colours" block is gone —
     Field is the only thing in the app that styles an input now. */
  .af-top { display: flex; gap: var(--s2); align-items: center; margin-bottom: var(--s3); }
  .af-top :global(.af-filter) { flex: 1; max-width: 20rem; }
  .af-list { margin-bottom: var(--s4); }
  .af-list header { display: flex; align-items: baseline; gap: var(--s2); }
  .af-list .title { font-weight: 600; }
  .af-list .path {
    color: var(--text-muted);
    font-size: var(--t-caption);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .af-list ul { list-style: none; padding: 0; margin: var(--s1) 0 0; }
  .af-list li { list-style: none; }
  .af-list :global(.entry) { flex: 1; }
  .af-list :global(.entry input) { width: 100%; }
  .af-list li.add { padding: var(--s1) var(--s2); }
  .pair { display: flex; align-items: center; gap: var(--s2); }
</style>
