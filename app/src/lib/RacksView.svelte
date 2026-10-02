<script lang="ts">
  import { api, errMessage, errText, type SlotEntry } from "./api";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import Field from "./ui/Field.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import ListRow from "./ui/ListRow.svelte";
  import SearchField from "./ui/SearchField.svelte";

  let { userOpen, userId = null, refreshToken = 0, onUserDirty }: {
    userOpen: boolean;
    userId?: number | null;
    /** Bumped by every save, open, discard, restore and undo (see FleetView). */
    refreshToken?: number;
    onUserDirty: () => void;
  } = $props();

  /** Inventory flag → the name players use: high H1–H8, mid M1–M8, low L1–L8. */
  const slotLabel = (f: number) => (f >= 27 ? `H${f - 26}` : f >= 19 ? `M${f - 18}` : `L${f - 10}`);
  const rackOf = (f: number) => (f >= 27 ? "high" : f >= 19 ? "mid" : "low");
  // Position p is row p/8: the client's InitDrawSlots, spec §2.2.
  const ROWS = [
    { name: "Top rack", from: 0 },
    { name: "Middle rack", from: 8 },
    { name: "Bottom rack", from: 16 },
  ];

  let ships = $state<SlotEntry[] | null>(null);
  let loadError = $state<string | null>(null);
  let actionError = $state<{ text: string; detail: string } | null>(null);
  let selectedId = $state<number | null>(null);
  let filter = $state("");
  /** The slot clicked first, waiting for the slot to swap it with. */
  let picked = $state<number | null>(null);
  let newId = $state("");
  let copyTo = $state<Set<number>>(new Set());

  async function reload() {
    if (!userOpen) { ships = null; return; }
    loadError = null;
    try { ships = await api.slotOrders(); } catch (e) { loadError = errMessage(e); }
  }
  $effect(() => { void userOpen; void userId; void refreshToken; reload(); });

  const shown = $derived((ships ?? []).filter((s) => String(s.ship_id).includes(filter.trim())));
  const current = $derived(ships?.find((s) => s.ship_id === selectedId) ?? null);
  // A different ship starts with nothing picked and nothing ticked.
  $effect(() => { void selectedId; picked = null; copyTo = new Set(); });

  async function write(subject: string, fn: () => Promise<SlotEntry[]>): Promise<boolean> {
    actionError = null;
    try {
      ships = await fn();
      onUserDirty();
      return true;
    } catch (e) {
      actionError = { text: `${subject} — ${errText(e)}`, detail: errMessage(e) };
      return false;
    }
  }

  function swap(a: number, b: number) {
    picked = null;
    const ship = current;
    if (ship && a !== b) void write("Those slots weren't swapped", () => api.swapSlots(ship.ship_id, a, b));
  }
  function pick(flag: number) {
    if (picked === null) picked = flag;
    else swap(picked, flag);
  }

  const parsedNewId = $derived(/^\d+$/.test(newId.trim()) ? Number(newId.trim()) : null);
  const newIdTaken = $derived(parsedNewId !== null && !!ships?.some((s) => s.ship_id === parsedNewId));
  async function add() {
    if (parsedNewId === null || newIdTaken) return;
    const id = parsedNewId;
    const from = current?.order ? current.ship_id : null;
    if (await write(`Ship ${id} wasn't added`, () => api.addSlotOrder(id, from))) {
      selectedId = id;
      newId = "";
    }
  }
  async function remove() {
    if (!current) return;
    const id = current.ship_id;
    if (await write(`Ship ${id} wasn't removed`, () => api.removeSlotOrder(id))) selectedId = null;
  }
  function toggleCopy(id: number) {
    const next = new Set(copyTo);
    next.has(id) ? next.delete(id) : next.add(id);
    copyTo = next;
  }
  async function copy() {
    if (!current || copyTo.size === 0) return;
    const from = current.ship_id;
    if (await write("That layout wasn't copied", () => api.copySlotOrder(from, [...copyTo]))) copyTo = new Set();
  }
</script>

<div class="racks">
  {#if !userOpen}
    <EmptyState title="Open an account file to edit its ship layouts." />
  {:else if loadError}
    <InlineMessage variant="error">{loadError}</InlineMessage>
  {:else if ships}
    <div class="cols">
      <div class="list">
        <SearchField verb="filter" nouns="ship ids" bind:value={filter} count={shown.length} total={ships.length} />
        {#if ships.length === 0}
          <EmptyState
            title="No ship has a saved layout yet."
            description="EVE saves one the first time you drag a module on a ship's HUD." />
        {/if}
        {#each shown as s (s.ship_id)}
          <ListRow selected={s.ship_id === selectedId} onclick={() => (selectedId = s.ship_id)}>
            {s.ship_id}{#if !s.order}<span class="muted"> · unreadable</span>{/if}
          </ListRow>
        {/each}
        <div class="add">
          <Field kind="text" label="Ship id" layout="column" bind:value={newId} />
          <Button
            size="sm"
            disabled={parsedNewId === null || newIdTaken}
            disabledReason={newIdTaken ? "That ship is already listed" : "Type a ship's item id"}
            onclick={add}>Add ship</Button>
        </div>
      </div>

      <div class="editor">
        {#if !current}
          <EmptyState title="Pick a ship to see its racks." />
        {:else}
          <h3>Ship {current.ship_id}</h3>
          {#if current.order}
            {@const order = current.order}
            {#each ROWS as row, r}
              <div class="row" class:middle={r === 1} role="group" aria-label={row.name}>
                {#each order.slice(row.from, row.from + 8) as flag (flag)}
                  <button
                    type="button"
                    class="slot {rackOf(flag)}"
                    class:picked={picked === flag}
                    aria-pressed={picked === flag}
                    draggable="true"
                    ondragstart={(e) => e.dataTransfer?.setData("text/plain", String(flag))}
                    ondragover={(e) => e.preventDefault()}
                    ondrop={(e) => {
                      e.preventDefault();
                      const from = Number(e.dataTransfer?.getData("text/plain"));
                      if (from) swap(from, flag);
                    }}
                    onclick={() => pick(flag)}>{slotLabel(flag)}</button>
                {/each}
              </div>
            {/each}
            <p class="muted">Click two slots, or drag one onto another, to swap them, as you would on the HUD in game.</p>
          {:else}
            <InlineMessage variant="warn">This ship's layout is in a shape the editor doesn't read. Remove it to give the ship EVE's default layout.</InlineMessage>
          {/if}
          <div class="actions">
            <Button variant="danger" size="sm" onclick={remove}>Remove ship</Button>
          </div>
          {#if current.order && ships.length > 1}
            <div class="copy">
              <div class="head">Copy this layout to</div>
              {#each shown.filter((s) => s.ship_id !== current.ship_id) as s (s.ship_id)}
                <Field
                  kind="checkbox"
                  label={`Copy to ${s.ship_id}`}
                  value={copyTo.has(s.ship_id)}
                  onchange={() => toggleCopy(s.ship_id)} />
              {/each}
              <Button
                size="sm"
                disabled={copyTo.size === 0}
                disabledReason="Tick at least one ship"
                onclick={copy}>Copy layout</Button>
            </div>
          {/if}
        {/if}
        {#if actionError}
          <InlineMessage variant="error" detail={actionError.detail}>{actionError.text}</InlineMessage>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .racks { max-width: 64rem; --slot: 2.75rem; }
  .cols { display: grid; grid-template-columns: minmax(12rem, 18rem) 1fr; gap: var(--s4); }
  .list, .editor { display: flex; flex-direction: column; gap: var(--s2); min-width: 0; }
  .add { display: flex; gap: var(--s2); align-items: end; }
  .row { display: flex; gap: var(--s1); }
  /* The client draws the middle rack half a button to the right (grid x 1.5). */
  .row.middle { transform: translateX(calc((var(--slot) + var(--s1)) / 2)); }
  .slot {
    width: var(--slot); height: var(--slot); border-radius: var(--r-pill);
    border: 1px solid var(--border); background: var(--surface-raised); color: var(--text);
    font: inherit; cursor: grab;
  }
  .slot.high { border-color: var(--accent); }
  .slot.low { border-style: dashed; }
  .slot.picked { outline: 2px solid var(--accent); outline-offset: 2px; }
  .actions, .copy { display: flex; flex-direction: column; gap: var(--s1); align-items: start; }
  .head { font-weight: 600; }
  .muted { color: var(--text-muted); }
  @media (max-width: 40rem) { .cols { grid-template-columns: 1fr; } }
</style>
