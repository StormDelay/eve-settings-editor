<script lang="ts">
  import { api, errMessage, errText, type Keybinds, type KeybindEntry } from "./api";
  import { labelFor, groupFor, GROUP_ORDER, defaultFor, keysToLabel, eventToKeys } from "./keybinds";
  import Button from "./ui/Button.svelte";
  import Chip from "./ui/Chip.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import { accel } from "./keys";
  import SearchField from "./ui/SearchField.svelte";

  let { userOpen, userId = null, refreshToken = 0, onUserDirty, onShowAccounts = () => {}, onShowBatch = () => {}, focusSearch = $bindable(undefined) }:
    { userOpen: boolean; userId?: number | null; onUserDirty: () => void;
    /** Bumped by every save, open, discard, backup restore and undo. Without it
     *  this view reloads only when the ACCOUNT changes, and neither Discard nor
     *  a restore changes that — so it would go on showing pre-Discard data. */
    refreshToken?: number;
      onShowAccounts?: () => void; onShowBatch?: () => void;
      /** Set so the shell's Ctrl+F focuses THIS view's box while it is active,
       *  instead of being suppressed and then doing nothing. */
      focusSearch?: () => void } = $props();

  let searchInput: HTMLInputElement | HTMLSelectElement | undefined = $state();
  focusSearch = () => {
    searchInput?.focus();
    if (searchInput instanceof HTMLInputElement) searchInput.select();
  };

  let binds = $state<Keybinds | null>(null);
  let error = $state<string | null>(null);
  let query = $state("");
  /** Command currently listening for a keypress, or null. */
  let listening = $state<string | null>(null);
  /** Transient "took X from Y" notice, keyed by the command that LOST it. */
  let stolenFrom = $state<Record<string, string>>({});
  /** A refused rebind, on the row that refused it. The `stolenFrom` notice
   *  beside it already proves a per-row message slot renders there, so this
   *  needs no new layout — which is the whole reason the error can leave the
   *  modal and land on the control. */
  let rowError = $state<{ command: string; text: string; detail: string } | null>(null);

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
  // See AutofillView: `userOpen`/`userId` do not change across a Discard, a
  // restore or an undo, so the token is what makes this view reload for any of
  // the three.
  $effect(() => { void userOpen; void userId; void refreshToken; reload(); });

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
    rowError = null;
    try {
      const res = await api.setKeybind(command, keys);
      binds = res.keybinds;
      onUserDirty();
      // Name what was taken, on the row that lost it.
      const next: Record<string, string> = {};
      for (const lost of res.stolen) next[lost] = labelFor(command);
      stolenFrom = next;
    } catch (e) {
      rowError = { command, text: `That binding wasn't changed — ${errText(e)}`, detail: errMessage(e) };
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

<!-- Both of these used `class="empty"`, which NO stylesheet in the repo ever
     declared, so two shipped empty states rendered as bare unstyled paragraphs
     and nobody noticed. That is the sharpest single piece of evidence for this
     phase: there was no shared thing whose absence would show. -->
{#if !userOpen}
  <EmptyState title="No account paired" description="Keybindings live in the account file.">
    {#snippet action()}
      <Button onclick={onShowAccounts}>Pair this character…</Button>
    {/snippet}
  </EmptyState>
{:else if error}
  <InlineMessage variant="error">{error}</InlineMessage>
{:else if binds && !binds.available}
  <EmptyState
    title="No keybindings yet"
    description="EVE only writes one once you have opened the in-game keybinding screen at least once on this account.">
    {#snippet action()}
      <Button onclick={onShowBatch}>Copy bindings from another account…</Button>
    {/snippet}
  </EmptyState>
{:else if binds}
  <!-- The scope banner is the shell's now, rendered once for all four
       account-scoped views. -->
  <!-- Its own class, not the global `.searchbar`: that rule belonged to the
       tree's search bar and is renamed `.raw-search` with it. -->
  <div class="search-row">
    <!-- The Ctrl+F hint this placeholder was waiting for. `focusSearch` is one
         bindable the ACTIVE view sets, so the shortcut now reaches this box
         instead of being suppressed and doing nothing. -->
    <!-- Filter, not Search: the table is on screen, so this narrows what you can
         already see. "Commands" now means palette commands. -->
    <SearchField nouns="keybindings" shortcut={accel("F")} bind:element={searchInput} bind:value={query} class="search" />
    <span class="meta">Click a binding, then press the combination you want.</span>
  </div>
  {#each grouped as [group, entries] (group)}
    <h3>{group}</h3>
    <table class="binds">
      <!-- Fixed widths so nothing appearing inside a cell (the theft notice)
           can reflow the Default and reset columns mid-interaction. -->
      <colgroup>
        <col class="c-label" /><col class="c-combo" /><col class="c-default" /><col class="c-reset" />
      </colgroup>
      <tbody>
        {#each entries as e (e.command)}
          <tr class:malformed={e.malformed}>
            <td class="label" title={e.command}>{labelFor(e.command)}</td>
            <td class="combo">
              {#if e.malformed}
                <Chip class="readonly" title="Unrecognised value; left untouched">unreadable</Chip>
              {:else}
                <!-- Keeps `class="chip"`: KeybindsView.spec finds this control
                     by that class. It is a toggle, so it says so with
                     aria-pressed rather than only a border colour. -->
                <Button
                  class="chip"
                  pressed={listening === e.command}
                  onclick={() => (listening = e.command)}
                  onkeydown={(ev: KeyboardEvent) => listening === e.command && onKeydown(ev, e.command)}>
                  {listening === e.command ? "press a key…" : keysToLabel(e.keys)}
                </Button>
              {/if}
              {#if stolenFrom[e.command]}
                <span class="meta" title={stolenFrom[e.command]}
                  >taken by {stolenFrom[e.command]}</span>
              {/if}
              {#if rowError?.command === e.command}
                <InlineMessage variant="error" detail={rowError.detail}>{rowError.text}</InlineMessage>
              {/if}
            </td>
            <td class="default">{keysToLabel(defaultFor(e.command))}</td>
            <td>
              <!-- Was `.mini`, and so invisible: it sits outside any `.row`. -->
              <Button
                variant="ghost"
                size="sm"
                iconOnly
                disabled={defaultFor(e.command) === null}
                disabledReason="EVE's default for this command hasn't been captured yet"
                title="Reset to EVE's default ({keysToLabel(defaultFor(e.command))})"
                onclick={() => commit(e.command, defaultFor(e.command))}>↺</Button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/each}
  {#if listening}
    <!-- Pinned rather than inline: inside the row it widened the cell and shoved
         the Default column sideways on every click, and at the foot of a
         hundred-row list it was several screens from the chip being captured. -->
    <p class="capture-bar">press a key… <span class="meta">Esc cancels · Backspace unbinds</span></p>
  {/if}
{/if}

<style>
  /* The dark-native-control rules are gone; Field owns that once, and Button
     owns the capture control's own colours and its pressed state. */
  /* Scoped through .combo, which is authored here — a bare :global(.chip) would
     reach every Chip in the app. */
  .combo :global(.chip) { min-width: 7rem; }
  /* A binding EVE wrote in a form we cannot read is genuinely unavailable, so
     it takes the one disabled treatment rather than a bespoke dimness. */
  .combo :global(.chip.readonly) { opacity: var(--o-disabled); }
  /* Rank by colour weight, not by dimming: the default column is reference
     information beside the live value, and at opacity .5 it was unreadable. */
  .default { color: var(--text-muted); }
  tr.malformed { color: var(--text-muted); }
  .meta { color: var(--text-muted); font-size: var(--t-caption); margin-left: var(--s2); }
  /* Ellipsised, not wrapped: the combo column is a fixed 16rem in a
     `table-layout: fixed` table, so a long command name ("Activate High Power
     Slot 4") used to spill out of the row and overlap the one beneath. The full
     name is on the `title`. Scoped to the combo cell — `.meta` is shared with
     the searchbar and capture-bar hints, which are not width-constrained and
     would lose their text to the same ellipsis. */
  .combo .meta {
    display: inline-block;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    vertical-align: bottom;
  }
  .binds { table-layout: fixed; width: 100%; }
  .c-combo { width: 16rem; }
  .c-default { width: 9rem; }
  .c-reset { width: 3rem; }
  .capture-bar {
    position: sticky; bottom: 0; margin: var(--s2) 0 0;
    padding: var(--s1) var(--s2); background: var(--surface);
    border-top: 1px solid var(--accent); color: var(--text);
  }
  .search-row {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin-bottom: var(--s2);
  }
  .search-row :global(.search) { flex: 1; max-width: 20rem; }
</style>
