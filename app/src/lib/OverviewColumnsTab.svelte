<script lang="ts">
  import { api, errMessage, type OverviewColumns, type OverviewTab } from "./api";
  import { plainTabName } from "./tabName";
  import { message } from "@tauri-apps/plugin-dialog";
  import Button from "./ui/Button.svelte";
  import Field from "./ui/Field.svelte";
  import ListRow from "./ui/ListRow.svelte";
  import Sheet from "./ui/Sheet.svelte";

  let { data, tabIndex, charOpen, onChanged, onUserDirty, onCharDirty }:
    { data: OverviewColumns | null; tabIndex: number | null; charOpen: boolean;
      onChanged: (next: OverviewColumns) => void; onUserDirty: () => void; onCharDirty: () => void } = $props();

  const tab = $derived(data?.tabs.find((t) => t.index === tabIndex) ?? null);

  async function toggle(column: string, visible: boolean) {
    try { onChanged(await api.setOverviewVisible(tabIndex!, column, visible)); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }
  async function setWidth(column: string, raw: string) {
    const width = Number(raw);
    if (!charOpen || raw.trim() === "" || Number.isNaN(width)) return;
    try { onChanged(await api.setOverviewWidth(tabIndex!, column, width)); onCharDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }

  // Drag-reorder: track the dragged row index, drop reorders the token list.
  let dragFrom = $state<number | null>(null);
  async function drop(to: number) {
    if (dragFrom === null || !tab) return;
    const order = tab.columns.map((c) => c.name);
    const [moved] = order.splice(dragFrom, 1);
    order.splice(to, 0, moved);
    dragFrom = null;
    try { onChanged(await api.setOverviewOrder(tabIndex!, order)); onUserDirty(); }
    catch (e) { await message(errMessage(e), { title: "Edit failed", kind: "error" }); }
  }

  // Copy this tab's columns onto others. The panel is inline rather than a
  // modal — the app has no modal, and OverviewView's name-entry rows set the
  // precedent. Ticking the targets by hand IS the confirmation step, so there
  // is no second confirm dialog on top of it.
  let copyOpen = $state(false);
  let picked = $state<Record<number, boolean>>({});
  // Copying a PART of a layout is the unusual ask, so all three start on;
  // targets start empty, because this overwrites whatever the target had.
  let parts = $state({ order: true, visible: true, widths: true });

  // Same window grouping the tab selector shows, minus the source tab itself.
  const targetGroups = $derived.by(() => {
    const others = (data?.tabs ?? []).filter((t) => t.index !== tabIndex);
    const windows = data?.windows ?? [];
    if (windows.length === 0) return [{ label: "", tabs: others }];
    const byIndex = new Map(others.map((t) => [t.index, t]));
    const grouped = new Set(windows.flatMap((w) => w.tab_indices));
    const groups = windows
      .map((w) => ({
        label: `Overview ${w.index + 1}`,
        tabs: w.tab_indices.map((i) => byIndex.get(i)).filter((t): t is OverviewTab => !!t),
      }))
      .filter((g) => g.tabs.length > 0);
    const orphans = others.filter((t) => !grouped.has(t.index));
    if (orphans.length > 0) groups.push({ label: "Other", tabs: orphans });
    return groups;
  });

  const chosen = $derived(
    targetGroups.flatMap((g) => g.tabs).filter((t) => picked[t.index]).map((t) => t.index),
  );
  // Widths live in the character file, so there is nothing to copy without one.
  const copyWidths = $derived(parts.widths && charOpen);

  function openCopy() {
    picked = {};
    copyOpen = true;
  }
  function pickAll(on: boolean) {
    picked = on ? Object.fromEntries(targetGroups.flatMap((g) => g.tabs).map((t) => [t.index, true])) : {};
  }
  async function runCopy() {
    if (chosen.length === 0) return;
    try {
      onChanged(await api.overviewCopyColumns(tabIndex!, chosen, parts.order, parts.visible, copyWidths));
      // Two files, two dirty flags — miss the char one and the copied widths
      // are dropped on save.
      if (parts.order || parts.visible) onUserDirty();
      if (copyWidths) onCharDirty();
      copyOpen = false;
    } catch (e) { await message(errMessage(e), { title: "Copy failed", kind: "error" }); }
  }
</script>

{#if tab}
  <div class="col-actions">
    <Button onclick={openCopy} disabled={copyOpen || (data?.tabs.length ?? 0) < 2}
            disabledReason={copyOpen ? "The copy panel is already open" : "There is no other tab to copy onto"}
            title="Copy this tab's column settings onto other tabs">Copy columns…</Button>
  </div>
  {#if copyOpen}
    <!-- A Sheet rather than an inline block: the column list stays put instead
         of being pushed down the page, and the panel gets a real dismiss —
         Escape, the scrim, and focus returned to the button that opened it.
         The comment this replaces said it was inline "because the app has no
         modal"; Phase 1 gave it one. Ticking the targets by hand is still the
         confirmation step, so no confirm is added. -->
    <Sheet
      title="Copy columns"
      width="min(34rem, 92vw)"
      class="copy-panel"
      onclose={() => (copyOpen = false)}>
      <!-- What travels, then where it goes: the two questions are separate and
           the panel asks them in that order. -->
      <section>
        <h4>What to copy from <strong>{plainTabName(tab.name).trim()}</strong></h4>
        <div class="copy-parts">
          <Field kind="checkbox" label="Column order" bind:value={parts.order} />
          <Field kind="checkbox" label="Visible columns" bind:value={parts.visible} />
          <Field
            kind="checkbox"
            label="Widths{charOpen ? '' : ' (no character open)'}"
            value={copyWidths}
            disabled={!charOpen}
            disabledReason="Widths are per character — open one to copy them"
            title={charOpen ? "" : "Widths are per character — open one to copy them"}
            onchange={(e) => (parts.widths = (e.currentTarget as HTMLInputElement).checked)} />
        </div>
      </section>
      <section>
        <div class="copy-head">
          <h4>Copy it to</h4>
          <Button size="sm" onclick={() => pickAll(true)}>Select all</Button>
          <Button size="sm" onclick={() => pickAll(false)}>None</Button>
        </div>
        <div class="copy-targets">
          {#each targetGroups as g (g.label)}
            {#if g.label}<span class="copy-group">{g.label}</span>{/if}
            {#each g.tabs as t (t.index)}
              <Field kind="checkbox" label={plainTabName(t.name).trim()} bind:value={picked[t.index]} />
            {/each}
          {/each}
        </div>
      </section>
      {#snippet footer()}
        <Button
          variant="primary"
          onclick={runCopy}
          disabled={chosen.length === 0 || !(parts.order || parts.visible || copyWidths)}
          disabledReason={chosen.length === 0 ? "Tick at least one tab" : "Tick at least one thing to copy"}>
          Copy to {chosen.length} tab{chosen.length === 1 ? "" : "s"}
        </Button>
        <Button onclick={() => (copyOpen = false)}>Cancel</Button>
      {/snippet}
    </Sheet>
  {/if}
  <ul class="ov-cols">
    {#each tab.columns as col, i (col.name)}
      <li>
        <ListRow
          draggable
          ondragstart={(e) => { dragFrom = i;
            // WebView2/Chromium won't fire `drop` unless dragstart sets data.
            e.dataTransfer?.setData("text/plain", String(i));
            if (e.dataTransfer) e.dataTransfer.effectAllowed = "move"; }}
          ondragover={(e) => { e.preventDefault();
            if (e.dataTransfer) e.dataTransfer.dropEffect = "move"; }}
          ondrop={(e) => { e.preventDefault(); drop(i); }}
          ondragend={() => (dragFrom = null)}>
          <Field
            kind="checkbox"
            label={col.label}
            title={col.name}
            value={col.visible}
            onchange={(e) => toggle(col.name, (e.target as HTMLInputElement).checked)} />
          {#snippet trailing()}
            <Field
              kind="number"
              controlClass="w"
              width="5rem"
              min={0}
              disabled={!charOpen}
              disabledReason="Widths are per character — open one to set them"
              ariaLabel="{col.label} width"
              value={col.width ?? ""}
              placeholder="—"
              onchange={(e) => setWidth(col.name, (e.target as HTMLInputElement).value)} />
          {/snippet}
        </ListRow>
      </li>
    {/each}
  </ul>
  {#if tab.inherits}<p class="meta">This tab uses the account-default columns. EVE doesn't save an
    inheriting tab's exact column order, so the order shown here is the account default — editing
    gives the tab its own copy.</p>{/if}
  <!-- Under the width boxes, which is the only thing on the page it is about.
       The third sentence is the width-swap ceiling's standing disclosure: a
       reorder renumbers the tab table and these widths are keyed by that number
       in the CHARACTER file, so they stay with the position. The remap is its
       own branch (docs/small-tasks.md). -->
  <p class="meta">Column widths are stored per character. Everything else on this tab is shared by
    the whole account. Reordering tabs moves widths with the position, not with the tab.</p>
{/if}

<style>
  /* The width input's dark-native-control rule is gone — Field owns it. */
  /* A reading width, NOT the work column's width. `ListRow` pushes its trailing
     control to the container's right edge, which is right for a row in a 20rem
     panel and absurd for one in a work area that is most of a wide monitor: the
     width box ended up a screen away from the column it belongs to. Capped at
     the widest row this list can actually produce — grip, checkbox,
     "Transversalvelocity", and the 5rem number box. */
  .ov-cols { list-style: none; padding: 0; margin: 0; max-width: 26rem; }
  .ov-cols li { list-style: none; }
  .meta { color: var(--text-muted); font-size: var(--t-caption); }
  .col-actions { display: flex; gap: var(--s1); margin-bottom: var(--s1); }
  /* A rule between the two questions — what to copy, and where to. */
  :global(.copy-panel) section + section { border-top: 1px solid var(--border); padding-top: var(--s2); }
  :global(.copy-panel) h4 { margin: 0 0 var(--s1); font-size: var(--t-body); }
  .copy-head { display: flex; gap: var(--s2); align-items: baseline; flex-wrap: wrap; }
  .copy-head h4 { margin: 0 0 var(--s1); }
  .copy-targets {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr));
    gap: 0 var(--s3);
  }
  /* A window heading owns its own full row above the tabs it groups. */
  .copy-group {
    grid-column: 1 / -1;
    color: var(--text-muted);
    font-size: var(--t-caption);
    margin-top: var(--s1);
  }
  .copy-parts { display: flex; gap: var(--s4); flex-wrap: wrap; }
</style>
