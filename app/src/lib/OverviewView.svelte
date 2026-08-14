<script lang="ts">
  import { api, errMessage, type OverviewColumns } from "./api";
  import type { MenuItem } from "./ContextMenu.svelte";
  import { message, confirm, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { documentDir } from "@tauri-apps/api/path";
  import { plainTabName, formatTabName, type TabName } from "./tabName";
  import OverviewColumnsTab from "./OverviewColumnsTab.svelte";
  import OverviewFiltersTab from "./OverviewFiltersTab.svelte";
  import OverviewAppearanceTab from "./OverviewAppearanceTab.svelte";
  import OverviewTabList from "./OverviewTabList.svelte";
  import Button from "./ui/Button.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import InlineMessage from "./ui/InlineMessage.svelte";
  import MenuButton from "./ui/MenuButton.svelte";
  import ScopeBanner from "./ui/ScopeBanner.svelte";
  import Tabs from "./ui/Tabs.svelte";
  import { toast } from "./ui/toasts.svelte";

  let { userOpen, userId, charId, charOpen, refreshToken, scopeLabel = "",
        onUserDirty, onCharDirty, onWindowAdded, onShowAccounts }:
    { userOpen: boolean; userId: number | null; charId: number | null; charOpen: boolean; refreshToken: number;
      scopeLabel?: string;
      onUserDirty: () => void; onCharDirty: () => void;
      onWindowAdded: (windowId: string) => void; onShowAccounts: () => void } = $props();

  let data = $state<OverviewColumns | null>(null);
  let tabIndex = $state<number | null>(null);
  let error = $state<string | null>(null);
  // Sub-tab selector added in the Columns/Filters/Appearance split; each child
  // stays mounted (hidden via the `hidden` attribute, not `{#if}`) so switching
  // sub-tabs doesn't re-run a child's effects or reset its local state.
  let sub = $state("Columns");

  async function reload() {
    if (!userOpen) { data = null; return; }
    error = null;
    try {
      data = await api.overviewColumns();
      // Keep the selected tab if it still exists in the (possibly just-switched)
      // file; otherwise fall back to the first tab.
      if (tabIndex === null || !data.tabs.some((t) => t.index === tabIndex)) {
        tabIndex = data.tabs[0]?.index ?? null;
      }
    } catch (e) { error = errMessage(e); }
  }
  // Reload when the slot's file changes (refreshToken bumps on every open/save),
  // not only when userOpen/charId flip — switching between two account files
  // leaves both unchanged and would otherwise show the previous file's overview.
  $effect(() => { void userOpen; void userId; void charId; void refreshToken; reload(); });

  const tab = $derived(data?.tabs.find((t) => t.index === tabIndex) ?? null);
  // The window strip whose tab_indices contains the selected tab (null for an
  // orphan tab that isn't listed under any window).
  const currentWindow = $derived(data?.windows.find((w) => w.tab_indices.includes(tabIndex ?? -1)) ?? null);
  const currentWindowIndex = $derived(currentWindow?.index ?? null);

  // The tab a create just added, which is always the highest index: the backend
  // allocates max+1, and the gap-compaction that follows keeps it last. Diffing
  // the index set against a snapshot taken before the call does NOT survive that
  // compaction — on a table that had gaps, renumbering frees up low indices that
  // were not in the snapshot either, and the first of those wins the diff.
  function newestTab(): number | null {
    const highest = Math.max(...(data?.tabs.map((t) => t.index) ?? []));
    return Number.isFinite(highest) ? highest : null;
  }

  async function edit(run: () => Promise<OverviewColumns>, title = "Edit failed"): Promise<boolean> {
    try { data = await run(); return true; }
    catch (e) { await message(errMessage(e), { title, kind: "error" }); return false; }
  }

  async function createTab(name: string, windowIdx: number | null) {
    // `windowIdx` is null in two different situations, and 0 is the right answer
    // to both. A windowless account ignores the argument entirely — the backend
    // refuses to fabricate a mapping and EVE distributes tabs itself. A tab in
    // the "Other" group gets the new tab in window 0: arbitrary, but visible and
    // movable, where refusing would leave the command dead for a selection that
    // looks ordinary.
    if (await edit(() => api.tabCreate(windowIdx ?? 0, name, tabIndex))) {
      tabIndex = newestTab() ?? tabIndex;
      onUserDirty();
    }
  }

  async function addWindow(name: string) {
    // Add window writes the user grouping AND the char-file geometry, so mark
    // BOTH slots dirty — otherwise saveFile skips the char slot and the new
    // window's position never persists. Then hand the new window's id up so the
    // Layout editor selects it: it defaults offset on top of window 0, so
    // without selecting it it's easy to miss.
    if (!(await edit(() => api.overviewWindowAdd(name, tabIndex)))) return;
    tabIndex = newestTab() ?? tabIndex;
    onUserDirty();
    onCharDirty();
    const w = data?.windows[data.windows.length - 1];
    if (w) onWindowAdded(w.index === 0 ? "overview" : `overview_${w.index}`);
  }

  // Name, colour and bold all rewrite the same markup-bearing string, so they
  // share one command — see tabName.ts. The inspector composes the string for
  // colour and bold; this only writes it.
  async function renameTab(idx: number, next: string) {
    if (await edit(() => api.tabRename(idx, next))) onUserDirty();
  }

  // The row editor edits the decomposed name — text, colour, weight — and hands
  // back all three at once, because in the file they are one markup-bearing
  // string. A name that comes back unchanged is not an edit, and an unparseable
  // one re-emits as itself, which is what keeps `parseTabName`'s give-up case
  // from being rewritten by the mere act of opening the editor on it.
  function renameTabName(idx: number, name: TabName) {
    const current = data?.tabs.find((t) => t.index === idx)?.name ?? "";
    const next = formatTabName(name);
    if (next === current) return;
    void renameTab(idx, next);
  }

  async function deleteTab(idx: number) {
    const target = data?.tabs.find((t) => t.index === idx);
    if (!target) return;
    const ok = await confirm(`Delete tab "${plainTabName(target.name)}"? This can't be undone.`, { title: "Delete tab", kind: "warning" });
    if (!ok) return;
    if (!(await edit(() => api.tabDelete(idx)))) return;
    tabIndex = data?.tabs[0]?.index ?? null;
    onUserDirty();
    // A delete renumbers the account's tabs, and the backend carries the open
    // character's per-tab column widths and sort setting across with them — so
    // that slot has unsaved work too whenever a character is open.
    if (charOpen) onCharDirty();
  }

  async function removeWindow(windowIdx: number) {
    if (!data || data.windows.length <= 1) return;
    const ok = await confirm(
      `Remove Overview ${windowIdx + 1}? Its tabs move to Overview 1.`,
      { title: "Remove overview window", kind: "warning" },
    );
    if (!ok) return;
    // Edits both slots (grouping + geometry) — mark both dirty so saveFile
    // doesn't skip the char slot.
    if (!(await edit(() => api.overviewWindowRemove(windowIdx)))) return;
    tabIndex = data.tabs[0]?.index ?? null;
    onUserDirty();
    onCharDirty();
  }

  // Reordering or moving a tab RENUMBERS the tab table — EVE draws a window's
  // tabs in ascending tab index, so that is the only way an order reaches the
  // game. The selected tab's index therefore changes under us, and left alone
  // `tabIndex` would silently come to name a different tab. Re-point it by
  // POSITION instead: whatever the backend now lists where the tab landed is
  // that tab, without this file repeating the backend's renumbering arithmetic.
  function keepSelection(windowIdx: number, pos: number): number | null {
    const strip = data?.windows.find((w) => w.index === windowIdx)?.tab_indices ?? [];
    return strip[pos] ?? tabIndex;
  }

  // Only the tabs of the window that was renumbered move — `renumber_to_strip_order`
  // redistributes that window's own index slots and leaves every other window
  // alone. So a selection outside it needs nothing, and a selection inside it is
  // re-pointed from the position it will hold once the operation lands.
  async function reorder(windowIdx: number, order: number[]) {
    const before = data?.windows.find((w) => w.index === windowIdx)?.tab_indices ?? [];
    // "Actually changed the order" — a drop in place is not an edit, and must
    // not dirty the file or fire the width warning.
    if (order.length === before.length && order.every((v, i) => v === before[i])) return;
    const selectedAt = tabIndex === null ? -1 : order.indexOf(tabIndex);
    if (!(await edit(() => api.tabReorder(windowIdx, order)))) return;
    if (selectedAt >= 0) tabIndex = keepSelection(windowIdx, selectedAt);
    onUserDirty();
    warnWidthSwap();
  }

  async function moveTab(tabIdx: number, from: number, to: number, pos: number) {
    const dst = data?.windows.find((w) => w.index === to)?.tab_indices ?? [];
    const after = [...dst];
    after.splice(Math.min(pos, after.length), 0, tabIdx);
    const selectedAt = tabIndex === null ? -1 : after.indexOf(tabIndex);
    if (!(await edit(() => api.tabMove(tabIdx, from, to, pos)))) return;
    if (selectedAt >= 0) tabIndex = keepSelection(to, selectedAt);
    onUserDirty();
    warnWidthSwap();
  }

  // The shipped ceiling, surfaced at the one moment it is actionable. Per-tab
  // column widths are keyed (overviewScroll2, tabIndex) in the CHARACTER file,
  // so renumbering leaves them on the slot rather than on the tab. The remap is
  // its own branch (docs/small-tasks.md); with no character open there are no
  // widths on screen to be wrong, so there is nothing to say.
  function warnWidthSwap() {
    if (!charOpen) return;
    toast("Tabs renumbered. Column widths stay with the position — check widths on the tabs you moved.", { variant: "warn" });
  }

  // A windowless account is normal: EVE's own overview importer deletes the
  // tab-to-window mapping, so anyone who has imported a pack lands here. Writing
  // one REPLACES the client's default distribution and pins every tab into a
  // single window, so it is offered rather than done — and the confirm says so.
  async function setUpWindowMapping() {
    if (!data || data.windows.length > 0) return;
    const n = data.tabs.length;
    const ok = await confirm(
      `Put all ${n} tab${n === 1 ? "" : "s"} in one overview window?\n\n` +
        `This account currently lets EVE decide which of your overview windows each tab ` +
        `appears in. Setting this up replaces that with an explicit list, so every tab ` +
        `starts in one window and you arrange them from there.\n\n` +
        `The editor can't undo this — it can't remove the last overview window. If you ` +
        `save and change your mind, importing an overview pack through the client removes ` +
        `the list again.`,
      { title: "Set up per-window tabs", kind: "warning" },
    );
    if (!ok) return;
    if (await edit(() => api.overviewCreateWindowMapping())) onUserDirty();
  }

  // Pack import/export is account-wide, so it lives in the view's ⋯ rather than
  // inside one sub-tab. Import marks the slot dirty; the user still saves.
  let packBusy = $state(false);

  // The three account-wide, rare commands. Visible — unlike a right-click — and
  // present-and-disabled rather than absent, so their position never moves.
  function viewMenu(): MenuItem[] {
    const mapped = (data?.windows.length ?? 0) > 0;
    return [
      { label: "Import overview pack…", run: importPack, disabled: packBusy, hint: packBusy ? "A pack command is in flight" : undefined },
      { label: "Export overview pack…", run: exportPack, disabled: packBusy, hint: packBusy ? "A pack command is in flight" : undefined },
      { label: "Set up per-window tabs…", run: setUpWindowMapping, disabled: mapped,
        hint: mapped ? "This account already assigns tabs to windows" : undefined },
    ];
  }

  // EVE's own export lands in Documents/EVE/Overview, so start the picker there.
  // Best-effort: if the path can't be resolved the dialog just opens wherever it
  // last was.
  async function overviewFolder(): Promise<string | undefined> {
    try {
      return `${await documentDir()}EVE/Overview`;
    } catch {
      return undefined;
    }
  }

  async function importPack() {
    const picked = await openDialog({
      multiple: false,
      defaultPath: await overviewFolder(),
      filters: [{ name: "Overview pack", extensions: ["yaml", "yml"] }],
    });
    if (typeof picked !== "string") return;
    packBusy = true;
    try {
      const summary = await api.packPreview(picked);
      const what = summary.sections
        .map(([name, count]) => (count > 0 ? `${name} (${count})` : name))
        .join(", ");
      const ignored = summary.ignored.length
        ? `\n\nIgnored unknown sections: ${summary.ignored.join(", ")}`
        : "";
      // Per-tab column overrides are only ever stripped inside apply_tabs,
      // which only runs when the pack defines a non-empty tabSetup section —
      // a preset-only pack never touches them, so don't claim it does.
      const dropsColumns = summary.sections.some(([name, count]) => name === "tabSetup" && count > 0);
      const columnsNote = dropsColumns ? " Per-tab column overrides are discarded." : "";
      const ok = await confirm(
        `This pack contains: ${what}.\n\nEach of those replaces your account's current overview settings.${columnsNote}${ignored}`,
        { title: "Import overview pack", kind: "warning" },
      );
      if (!ok) return;
      const result = await api.packImport(picked);
      data = result.columns;
      // A pack can replace the tab set wholesale (or the account may have had
      // none at all); if the previously-selected tab no longer exists, fall
      // back to the first tab rather than leaving tabIndex dangling — same
      // rule deleteTab already follows.
      if (!data.tabs.some((t) => t.index === tabIndex)) tabIndex = data.tabs[0]?.index ?? null;
      onUserDirty();
      const warnings = result.report.warnings.length ? `\n\n${result.report.warnings.join("\n")}` : "";
      await message(`Pack imported. Save to write it to the account file.${warnings}`, { title: "Import overview pack" });
    } catch (e) {
      await message(errMessage(e), { title: "Import failed", kind: "error" });
    } finally {
      packBusy = false;
    }
  }

  async function exportPack() {
    const picked = await saveDialog({
      defaultPath: "overview.yaml",
      filters: [{ name: "Overview pack", extensions: ["yaml"] }],
    });
    if (typeof picked !== "string") return;
    packBusy = true;
    try {
      const report = await api.packExport(picked);
      const warnings = report.warnings.length ? `\n\n${report.warnings.join("\n")}` : "";
      await message(`Exported ${report.applied.length} section(s).${warnings}`, { title: "Export overview pack" });
    } catch (e) {
      await message(errMessage(e), { title: "Export failed", kind: "error" });
    } finally {
      packBusy = false;
    }
  }
</script>

<!-- ONE child, spanning the work column and the column an inspector would sit
     in. `display: contents` on the root is what lets it reach across: the root
     stops participating in layout and `.work` becomes the grid item.
     A tab's properties are docked under the list that selects it (§13), so
     there is no third column to leave for the shell to fill. -->
<div class="overview-view">
  <div class="work wide">
    <ScopeBanner label={scopeLabel} compact />
    {#if !userOpen && charId !== null}
      <div class="scroll">
        <EmptyState
          title="Link this character to an account to edit shared settings"
          description="Overview columns live in the account file.">
          {#snippet action()}<Button onclick={onShowAccounts}>Pair…</Button>{/snippet}
        </EmptyState>
      </div>
    {:else if !userOpen}
      <div class="scroll">
        <EmptyState title="Open a character or account file to edit overview columns." />
      </div>
    {:else if error}
      <div class="scroll"><InlineMessage variant="error">{error}</InlineMessage></div>
    {:else if data}
      <div class="sub-row">
        {#if data.tabs.length > 0}
          <Tabs
            variant="underline"
            class="subtabs"
            ariaLabel="Overview section"
            tabs={[
              { id: "Columns", label: "Columns" },
              { id: "Filters", label: "Filters" },
              { id: "Appearance", label: "Appearance" },
            ]}
            bind:value={sub} />
        {/if}
        <!-- A real button, beside the tabs rather than pinned to the far edge
             of the work column: as a small ghost at the right margin of a wide
             window it was several hundred pixels from anything and read as
             decoration. It is the only home for three account-wide commands. -->
        <MenuButton items={viewMenu} title="Overview actions" variant="default" size="md" />
      </div>
      <div class="panes">
        <!-- Just the list. Everything a tab has — its text, its colour, its
             weight, which window it is in, and deleting it — is on the row
             itself, so the properties pane §5 specced has nothing left to hold.
             See §13: the two fields that outlived the move both turned out to
             be duplicates of controls elsewhere. -->
        <div class="side">
          <OverviewTabList
            {data}
            {tabIndex}
            onSelect={(i) => (tabIndex = i)}
            onCreateTab={createTab}
            onAddWindow={addWindow}
            onRemoveWindow={removeWindow}
            onDeleteTab={deleteTab}
            onRenameTab={renameTabName}
            onReorder={reorder}
            onMove={moveTab}
            onSetUpWindowMapping={setUpWindowMapping} />
        </div>
        <div class="scroll">
          {#if data.tabs.length > 0}
            <div hidden={sub !== "Columns"}>
              <OverviewColumnsTab {data} {tabIndex} {charOpen} onChanged={(next) => (data = next)} {onUserDirty} {onCharDirty} />
            </div>
            <div hidden={sub !== "Filters"}>
              <OverviewFiltersTab {data} {tabIndex} onChanged={(next) => (data = next)} {onUserDirty} />
            </div>
            <div hidden={sub !== "Appearance"}>
              <OverviewAppearanceTab {data} onChanged={(next) => (data = next)} {onUserDirty} />
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .overview-view { display: contents; }
  /* Spans the work column AND the column an inspector would occupy. This view's
     inspector is docked under the list that selects the tab, so there is no
     third column and nothing for the shell to draw in it. */
  .wide { grid-column: 2 / 4; }
  .sub-row { display: flex; align-items: flex-end; gap: var(--s3); margin: var(--s2) var(--s3); }
  /* Bounded side column, unbounded centre — the same reasoning LayoutView's
     grid uses, and the reason the tab list cannot eat the content. Wider than
     a bare list needs, because the side now carries the selected tab's fields
     as well. */
  .panes {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(15rem, 20rem) minmax(0, 1fr);
  }
  .side {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border);
  }
  .side :global(.tablist) { flex: 1 1 auto; min-height: 4rem; }
</style>
