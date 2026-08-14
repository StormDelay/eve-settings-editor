<script lang="ts">
  import { api, errMessage, errText, type RememberedList } from "./api";
  import { labelFor } from "./autofill";
  import { aliasFor } from "./accounts.svelte";
  import { confirmDialog } from "./ui/confirm.svelte";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";
  import { accel } from "./keys";
  import { revealAndFocus } from "./keymap";
  import SearchField from "./ui/SearchField.svelte";

  let { userOpen, userId = null, refreshToken = 0, onUserDirty, charOpen = false, charName = null, onShowAccounts = () => {}, focusSearch = $bindable(undefined) }:
    { userOpen: boolean; userId?: number | null; onUserDirty: () => void;
    /** Bumped by every save, open, discard, backup restore and undo. Without it
     *  this view reloads only when the ACCOUNT changes, and neither Discard nor
     *  a restore changes that — so it would go on showing pre-Discard data. */
    refreshToken?: number;
      /** A character file is open. Separate from `charName`, which stays null
       * until the ESI name lookup resolves — offline, it never does. */
      charOpen?: boolean;
      charName?: string | null; onShowAccounts?: () => void;
      /** Set so the shell's Ctrl+F focuses THIS view's box while it is active,
       *  instead of being suppressed and then doing nothing. */
      focusSearch?: () => void } = $props();

  let filterInput: HTMLInputElement | HTMLSelectElement | undefined = $state();
  focusSearch = () => revealAndFocus(filterInput);

  let lists = $state<RememberedList[] | null>(null);
  let error = $state<string | null>(null);
  // One live message per owning control (§3.1). A refused edit belongs under the
  // list it was refused for — there are twenty-odd on screen and "Edit failed"
  // in a modal named none of them.
  let listError = $state<{ widget: string; text: string; detail: string } | null>(null);
  // Its own slot, because its owning control is the button rather than a list.
  let clearError = $state<{ text: string; detail: string } | null>(null);

  const alias = $derived(userId === null ? null : aliasFor(userId));

  async function reload() {
    if (!userOpen) { lists = null; return; }
    error = null;
    try { lists = await api.autofillLists(); }
    catch (e) { error = errMessage(e); }
  }
  // `refreshToken` as well as the two ids: neither `userOpen` nor `userId`
  // changes across a Discard, a backup restore or an undo, so without it this
  // view goes on showing pre-Discard data indefinitely. That is a bug in the
  // shipped build, not something undo introduced — Layout and Overview already
  // took the token and these three did not.
  $effect(() => { void userOpen; void userId; void refreshToken; reload(); });

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
    // Cleared by the operation next succeeding, not by a close button: a message
    // you dismiss trains exactly the reflex the modals did.
    listError = null;
    try { lists = await api.setAutofillList(widget, entries); onUserDirty(); }
    catch (e) {
      listError = { widget, text: `That list wasn't changed — ${errText(e)}`, detail: errMessage(e) };
    }
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

  // Survivor 5, and the one judgement call among the six. By the letter of the
  // rule this is an in-memory edit that Discard reverses, so it should be a
  // toast. It keeps its confirmation because its blast radius is EVERY list in
  // the file, most of them not on screen at the moment of the click, and Discard
  // is all-or-nothing — reversing it also throws away every other unsaved edit
  // made in the same session. The last clause below is the honest version of
  // that asymmetry. Revisit once per-step undo ships.
  async function clearAll() {
    const n = lists?.length ?? 0;
    const ok = await confirmDialog({
      title: "Clear every remembered list?",
      body:
        `${n} list${n === 1 ? "" : "s"} in ${alias ?? "this account"} ${n === 1 ? "is" : "are"} emptied. ` +
        "Nothing is written until you save, and Discard puts them back — along with any other unsaved edits.",
      confirm: "Clear everything",
      danger: true,
    });
    if (!ok) return;
    clearError = null;
    try { lists = await api.clearAllAutofill(); onUserDirty(); }
    catch (e) {
      clearError = { text: `The lists weren't cleared — ${errText(e)}`, detail: errMessage(e) };
    }
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
      <p><strong>{charName ?? "This character"}</strong>'s remembered text lives in the account file.</p>
      <Button onclick={onShowAccounts}>Pair this character…</Button>
    </div>
  {:else}
    <EmptyState
      title="No file open"
      description="Open a character to edit its account's remembered text." />
  {/if}
{:else if error}
  <InlineMessage variant="error">{error}</InlineMessage>
{:else if lists && lists.length === 0}
  <EmptyState
    title="Nothing remembered yet"
    description="EVE stores what you type into station, search and fitting boxes here." />
{:else if lists}
  <!-- The scope banner is the shell's now, rendered once for all four
       account-scoped views. -->
  <div class="af-top">
    <SearchField class="af-filter" nouns="lists" shortcut={accel("F")} bind:element={filterInput} bind:value={query} />
    <Button variant="danger" onclick={clearAll}>Clear all remembered text</Button>
  </div>
  <!-- At the button that failed, not in a modal over the whole app. -->
  {#if clearError}
    <InlineMessage variant="error" detail={clearError.detail}>{clearError.text}</InlineMessage>
  {/if}
  {#if filtered.length === 0}
    <EmptyState title="No matches" description="No lists match “{query}”." />
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
          disabledReason="This list is already empty">Clear list</Button>
      </header>
      {#if listError?.widget === l.widget}
        <InlineMessage variant="error" detail={listError.detail}>{listError.text}</InlineMessage>
      {/if}
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
                <Button variant="ghost" size="sm" iconOnly title="Remove this entry" onclick={() => removeAt(l, i)}>
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
            placeholder="Add remembered text"
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
