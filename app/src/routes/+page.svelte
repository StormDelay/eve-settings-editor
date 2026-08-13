<script lang="ts">
  import Sidebar from "$lib/Sidebar.svelte";
  import TreeNode from "$lib/TreeNode.svelte";
  import InsertForm from "$lib/InsertForm.svelte";
  import BackupsPanel from "$lib/BackupsPanel.svelte";
  import LayoutView from "$lib/LayoutView.svelte";
  import AccountsView from "$lib/AccountsView.svelte";
  import OverviewView from "$lib/OverviewView.svelte";
  import AutofillView from "$lib/AutofillView.svelte";
  import KeybindsView from "$lib/KeybindsView.svelte";
  import ProbeFormationsView from "$lib/ProbeFormationsView.svelte";
  import BatchView from "$lib/BatchView.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Chip from "$lib/ui/Chip.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import InlineMessage from "$lib/ui/InlineMessage.svelte";
  import SearchField from "$lib/ui/SearchField.svelte";
  import Sheet from "$lib/ui/Sheet.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import Toast from "$lib/ui/Toast.svelte";
  import { api, errMessage, type OpenOutcome, type Slot } from "$lib/api";
  import type { Mutation, NodePath, TreeNodeData, PresetInfo } from "$lib/api";
  import { searchTree } from "$lib/search";
  import { names, resolveNames } from "$lib/names.svelte";
  import { aliasFor, accountsStore } from "$lib/accounts.svelte";
  import { loadPrefs } from "$lib/prefs.svelte";
  // Everything about WHO is open and what is unsaved. See subject.svelte.ts's
  // header for why it is a module rather than more state in this file.
  import {
    subject,
    confirmDiscardIfDirty,
    discardChanges,
    loadCharacter,
    reconcileCharSlot,
    reconcileUserSlot,
    saveFile,
  } from "$lib/subject.svelte";
  import { message } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  let mainView: "file" | "accounts" | "batch" = $state("file");
  // Side panels collapse to a thin reopen rail so the center pane (esp. the
  // layout canvas) can use the full width. In-memory only; resets on reload.
  let sidebarOpen = $state(true);
  let backupsOpen = $state(true);
  // Which file the raw Tree view shows; a Tree-local switch flips it to the
  // account file when one is loaded. Reset on every open.
  let treeFile = $state<Slot>("char");
  type View = "tree" | "layout" | "overview" | "autofill" | "keybinds" | "probes";
  let view = $state<View>("tree");
  // The active document is a consequence of the current view — NOT a manual
  // toggle: Autofill edits the account file, the Tree view honors its file
  // switch, everything else (Layout, Overview, search, backups) follows the
  // character.
  const active = $derived<Slot>(
    (view === "autofill" || view === "keybinds" || view === "probes") && subject.slots.user?.status === "opened"
      ? "user"
      : view === "tree" && treeFile === "user" && subject.slots.user?.status === "opened"
        ? "user"
        : "char",
  );
  const current = $derived(subject.slots[active]);

  // Route a settings file to its slot by filename kind. Non-standard/other files
  // use the char slot (the generic editing slot).
  function slotForName(name: string): Slot {
    return /^core_user_\d+\.dat$/.test(name) ? "user" : "char";
  }

  api.discover().then((p) => (subject.profiles = p)).catch(() => {});
  void loadPrefs();

  let insertTarget: TreeNodeData | null = $state(null);
  // Whether a view has anything to show for the currently open file(s) — the same
  // conditions that gate each view's tab button below. Used to keep the user on
  // their current tab across a file switch when the new file still supports it.
  const viewAvailable = (v: View) =>
    v === "tree" ||
    (v === "layout" && subject.layoutAvailable) ||
    (v === "overview" && (subject.charId !== null || subject.slots.user?.status === "opened")) ||
    (v === "autofill" && (subject.charId !== null || subject.slots.user?.status === "opened")) ||
    (v === "keybinds" && (subject.charId !== null || subject.slots.user?.status === "opened")) ||
    (v === "probes" && (subject.charId !== null || subject.slots.user?.status === "opened"));
  // Selected canvas window, lifted here so it survives Tree/Layout switches.
  let selectedWindowId = $state<string | null>(null);
  // Bound down through LayoutView -> WindowPanel, where the filter input
  // actually lives; lets the global Ctrl+F handler focus it (see below).
  let layoutFocusFilter = $state<(() => void) | undefined>(undefined);
  // A request to reveal a node in the tree (bump `n` to re-fire on the same path).
  let reveal = $state<{ path: NodePath; n: number } | null>(null);

  // Name for the loaded char file, if resolved. `core_char_<id>.dat` -> name.
  const openCharName = $derived.by(() => {
    if (current?.status !== "opened") return null;
    const m = current.file_name.match(/^core_char_(\d+)\.dat$/);
    if (!m) return null;
    const hit = names[m[1]];
    return hit ? hit.name : null;
  });

  // Alias for the loaded user file, if named. `core_user_<id>.dat` -> alias.
  const openUserAlias = $derived.by(() => {
    if (current?.status !== "opened") return null;
    const m = current.file_name.match(/^core_user_(\d+)\.dat$/);
    return m ? aliasFor(Number(m[1])) : null;
  });

  // Best single label for the open file — character name, else user alias, else
  // the bare filename. Feeds the OS window title and the backups panel.
  const openDisplay = $derived.by(() => {
    if (subject.preset !== null) return `${subject.preset} (preset)`;
    if (current?.status !== "opened") return null;
    return openCharName ?? openUserAlias ?? current.file_name;
  });

  // The banner text the four account-scoped views each render for themselves.
  // Deleted in the shell commit, where one shell-owned `ScopeBanner` consumes
  // `subject.sharedNames` directly (`02-shell.md` §2.6, §5.4).
  const sharedLabel = $derived(
    "Shared account settings" +
      (subject.sharedNames.length ? ` — also applies to ${subject.sharedNames.join(", ")}` : ""),
  );

  const APP_TITLE = "EVE Settings Editor";
  $effect(() => {
    void getCurrentWindow().setTitle(
      openDisplay ? `${openDisplay} — ${APP_TITLE}` : APP_TITLE,
    );
  });

  // Resolve names so the width selector shows character names, not bare ids.
  $effect(() => {
    if (subject.accountCharacters.length) void resolveNames(subject.accountCharacters);
  });

  // If the open character becomes paired while its account slot is empty — e.g.
  // the user just paired it in the Accounts view — load the account file so the
  // account-scoped editors light up without a manual re-open (spec §5). Guarded
  // on an empty user slot, so it never re-loads an already-open account.
  $effect(() => {
    const o = subject.slots.char;
    void accountsStore.roster; // track roster changes
    if (o?.status === "opened" && subject.slots.user === null) void reconcileUserSlot(o);
  });

  // Jump to a value in the full tree: leave search, expand and scroll to it.
  function revealInTree(path: NodePath) {
    view = "tree";
    query = "";
    reveal = { path, n: (reveal?.n ?? 0) + 1 };
  }

  let query = $state("");
  let searchBox: HTMLInputElement | HTMLSelectElement | undefined = $state();
  const searching = $derived(query.trim() !== "");
  // Re-runs after every mutation, since the tree is replaced wholesale.
  const searchIn = (doc: OpenOutcome | null, q: string) =>
    doc?.status === "opened" ? searchTree(doc.tree, q) : null;
  const found = $derived(searchIn(current, query));

  function openSearch() {
    searchBox?.focus();
    // Field's `element` is typed for either control; only an input can select.
    if (searchBox instanceof HTMLInputElement) searchBox.select();
  }

  function closeSearch() {
    query = "";
    searchBox?.blur();
  }

  // `openFile` and `openPresetPair` stay HERE rather than moving to
  // subject.svelte.ts with the rest of the transitions (`02-shell.md` §6.1
  // nominates them). They interleave subject state with `treeFile`, `view`,
  // `mainView`, `selectedWindowId` and `reveal`, which §6.2 keeps in the shell,
  // and the interleaving is load-bearing: `treeFile = slot` must land BEFORE the
  // `savedAt` bump, or the bump fires while `active` still names the outgoing
  // slot and the backups panel refetches the wrong file. Splitting them would
  // mean either a callback into the shell or a reordering that changes
  // behaviour, and every consumer that needs to open a file (the switcher, the
  // launch empty state, the sidebar) is a descendant of this component and can
  // take it as a prop.
  async function openFile(path: string) {
    const name = path.split(/[\\/]/).pop() ?? "";
    const slot = slotForName(name);
    if (!(await confirmDiscardIfDirty())) return;
    try {
      subject.preset = null;
      const outcome = await api.open(slot, path);
      subject.slots[slot] = outcome;
      // A file opened via the dialog isn't in the sidebar scan, so its name was
      // never resolved — resolve it here so the header names it too. (A no-op if
      // it was scanned: the id is already cached.)
      if (outcome.status === "opened") {
        const m = outcome.file_name.match(/^core_char_(\d+)\.dat$/);
        if (m) void resolveNames([Number(m[1])]);
      }
      subject.dirty[slot] = false;
      treeFile = slot;
      subject.savedAt += 1;
      // Hold the tab the user was on across the load (switching between two chars
      // shouldn't bounce you out of Layout), falling back to Tree only if the new
      // file can't support it. Each view reads the already-swapped active slot and
      // is null-safe against its own mid-load fetch, so there's no flash to Tree.
      const priorView = view;
      mainView = "file";
      selectedWindowId = null;
      reveal = null;
      try {
        subject.layoutAvailable =
          outcome.status === "opened" && (await api.windowLayout(slot)).windows.length > 0;
      } catch {
        subject.layoutAvailable = false;
      }
      // Reconcile the *other* slot so the two are always a matching char/user
      // pair (or one empty) — never a stale, unrelated file the Overview editor
      // would misread.
      if (slot === "char") await reconcileUserSlot(outcome);
      else await reconcileCharSlot(outcome);
      if (!viewAvailable(priorView)) view = "tree";
    } catch (e) {
      await message(errMessage(e), { title: "Open failed", kind: "error" });
    }
  }

  // Open a preset: BOTH slots at once, so the pairing machinery never runs.
  // Deliberately not routed through openFile, whose char branch would call
  // reconcileUserSlot and replace the preset's account side with a character's.
  async function openPresetPair(p: PresetInfo) {
    if (!(await confirmDiscardIfDirty())) return;
    try {
      const [charOutcome, userOutcome] = await Promise.all([
        api.open("char", p.char_path),
        api.open("user", p.user_path),
      ]);
      subject.slots.char = charOutcome;
      subject.slots.user = userOutcome;
      subject.dirty.char = false;
      subject.dirty.user = false;
      subject.preset = p.name;
      treeFile = "char";
      subject.savedAt += 1;
      const priorView = view;
      mainView = "file";
      selectedWindowId = null;
      reveal = null;
      try {
        subject.layoutAvailable = (await api.windowLayout("char")).windows.length > 0;
      } catch {
        subject.layoutAvailable = false;
      }
      if (!viewAvailable(priorView)) view = "tree";
    } catch (e) {
      await message(errMessage(e), { title: "Open failed", kind: "error" });
    }
  }

  // `rethrow` is for callers with somewhere better to put the error than a
  // dialog — the insert form shows it inline and stays open on failure.
  async function runMutation(m: Mutation, rethrow = false) {
    const doc = subject.slots[active];
    if (doc?.status !== "opened") return;
    try {
      const tree = await api.mutate(active, m);
      // Reassign (not mutate-in-place) so the derived `current` refires.
      subject.slots[active] = { ...doc, tree };
      subject.dirty[active] = true;
    } catch (e) {
      if (rethrow) throw e;
      await message(errMessage(e), { title: "Edit failed", kind: "error" });
    }
  }

  // Batched sibling of runMutation: one backend round-trip for many mutations
  // (e.g. a layout-canvas drag fanning out to several windows' geometry).
  async function runMutations(ms: Mutation[], rethrow = false) {
    const doc = subject.slots[active];
    if (doc?.status !== "opened") return;
    if (ms.length === 0) return;
    try {
      const tree = await api.mutateMany(active, ms);
      subject.slots[active] = { ...doc, tree };
      subject.dirty[active] = true;
    } catch (e) {
      if (rethrow) throw e;
      await message(errMessage(e), { title: "Edit failed", kind: "error" });
    }
  }

  const handleEdit = (path: NodePath, text: string) =>
    runMutation({ op: "set_scalar", path, text });
  const handleRemove = (path: NodePath) =>
    runMutation({ op: "remove_entry", path });
</script>

<!-- The webview's stock context menu (Back/Reload/…) means nothing here. Tree
     actions take its place when we add them. -->
<svelte:window
  oncontextmenu={(e) => e.preventDefault()}
  onkeydown={(e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      saveFile();
    }
    // Take Ctrl+F off the webview: its find-on-page cannot see collapsed nodes.
    // On the Layout view the tree search box isn't even rendered, so focus its
    // own window filter instead — otherwise Ctrl+F silently does nothing there.
    if ((e.ctrlKey || e.metaKey) && e.key === "f") {
      e.preventDefault();
      if (view === "layout") {
        layoutFocusFilter?.();
      } else {
        openSearch();
      }
    }
    if (e.key === "Escape" && searching) closeSearch();
  }}
/>

<main class="layout" class:sidebar-collapsed={!sidebarOpen} class:backups-collapsed={!backupsOpen}>
  {#if sidebarOpen}
    <Sidebar
      onOpen={openFile}
      onShowAccounts={() => (mainView = "accounts")}
      onShowBatch={() => (mainView = "batch")}
      onCollapse={() => (sidebarOpen = false)}
      onOpenPreset={openPresetPair}
      charOpen={subject.slots.char?.status === "opened"}
      userOpen={subject.slots.user?.status === "opened"}
      openPresetName={subject.preset} />
  {:else}
    <button class="rail rail-left" onclick={() => (sidebarOpen = true)}
      title="Show file list" aria-label="Show file list">&raquo;</button>
  {/if}
  {#if mainView === "accounts"}
    <AccountsView openPath={current?.status === "opened" ? current.path : null} />
  {:else if mainView === "batch"}
    <BatchView openPath={current?.status === "opened" ? current.path : null} />
  {:else}
  <section class="editor">
    {#if current === null}
      <EmptyState title="Open a settings file to begin." />
    {:else if current.status === "opened"}
      <header class="filebar">
        <span class="filename">
          {#if openCharName}{openCharName} — {/if}{#if openUserAlias}{openUserAlias} — {/if}{current.file_name}
        </span>
        <!-- A light role tone on its own dim ground. These three badges used to
             be dark text on a saturated fill, measuring Lc 51 / 55 / 43 at 12px
             where APCA wants 75; they measure ~69 now. -->
        {#if current.fidelity.state === "read_only"}
          <Chip tone="danger" size="sm" class="badge read-only" title={current.fidelity.reason}>read-only</Chip>
        {:else}
          <Chip tone="ok" size="sm" class="badge editable">editable</Chip>
        {/if}
        {#if subject.preset !== null}
          {#if subject.dirty.char || subject.dirty.user}
            <Chip tone="warn" size="sm" class="badge dirty">preset: unsaved</Chip>
          {/if}
        {:else}
          {#if subject.dirty.char}<Chip tone="warn" size="sm" class="badge dirty">character: unsaved</Chip>{/if}
          {#if subject.dirty.user}<Chip tone="warn" size="sm" class="badge dirty">account: unsaved</Chip>{/if}
        {/if}
        {#if subject.dirty.char || subject.dirty.user}
          <Button
            variant="danger"
            size="sm"
            onclick={discardChanges}
            title="Throw the unsaved changes away and reload both files from disk. Backups are untouched."
            >Discard</Button>
        {/if}
        <!-- The strip keeps its {#if} guards, so it still shows exactly the tabs
             it shows today. Phase 2 drops the guards and passes `disabled`
             instead, which is what stops it rearranging under the cursor —
             Tabs already carries the capability. What it gains here is the ARIA
             it never had: this was a bare <span> of buttons with no roles. -->
        {#if subject.layoutAvailable || subject.charId !== null || subject.slots.user?.status === "opened"}
          <Tabs
            class="viewtabs"
            ariaLabel="Editor view"
            tabs={[
              { id: "tree", label: "Tree" },
              ...(subject.layoutAvailable ? [{ id: "layout", label: "Layout" }] : []),
              ...(subject.charId !== null || subject.slots.user?.status === "opened"
                ? [
                    { id: "overview", label: "Overview" },
                    { id: "autofill", label: "Autofill" },
                    { id: "keybinds", label: "Keybinds" },
                    { id: "probes", label: "Probes" },
                  ]
                : []),
            ]}
            bind:value={view} />
        {/if}
        <span class="spacer"></span>
        <Button
          variant="primary"
          disabled={!subject.canSave}
          disabledReason={current.fidelity.state !== "editable"
            ? "This file is read-only"
            : "There is nothing to save"}
          onclick={() => saveFile()}>Save</Button>
      </header>
      {#if view === "layout"}
        <div class="tree-area">
          <LayoutView
            slot={active}
            {runMutations}
            readOnly={current.fidelity.state !== "editable"}
            accountReadOnly={subject.slots.user?.status === "opened" && subject.slots.user.fidelity.state !== "editable"}
            refreshToken={subject.savedAt}
            userOpen={subject.slots.user?.status === "opened"}
            bind:selectedId={selectedWindowId}
            onReveal={revealInTree}
            onDirty={(slot) => (subject.dirty[slot] = true)}
            sharedNames={subject.sharedNames}
            bind:focusFilter={layoutFocusFilter} />
        </div>
      {:else if view === "overview"}
        <div class="tree-area">
          <OverviewView
            userOpen={subject.slots.user?.status === "opened"}
            userId={subject.userId}
            charId={subject.charId}
            charOpen={subject.slots.char?.status === "opened"}
            characters={subject.accountCharacters}
            refreshToken={subject.savedAt}
            sharedLabel={sharedLabel}
            onLoadCharacter={loadCharacter}
            onUserDirty={() => (subject.dirty.user = true)}
            onCharDirty={() => (subject.dirty.char = true)}
            onWindowAdded={(id) => { if (subject.layoutAvailable) { selectedWindowId = id; view = "layout"; } }}
            onShowAccounts={() => (mainView = "accounts")} />
        </div>
      {:else if view === "autofill"}
        <div class="tree-area">
          <AutofillView
            userOpen={subject.slots.user?.status === "opened"}
            userId={subject.userId}
            charOpen={subject.slots.char?.status === "opened"}
            charName={openCharName}
            sharedLabel={sharedLabel}
            onShowAccounts={() => (mainView = "accounts")}
            onUserDirty={() => (subject.dirty.user = true)} />
        </div>
      {:else if view === "keybinds"}
        <div class="tree-area">
          <KeybindsView
            userOpen={subject.slots.user?.status === "opened"}
            userId={subject.userId}
            sharedLabel={sharedLabel}
            onShowAccounts={() => (mainView = "accounts")}
            onShowBatch={() => (mainView = "batch")}
            onUserDirty={() => (subject.dirty.user = true)} />
        </div>
      {:else if view === "probes"}
        <div class="tree-area">
          <ProbeFormationsView
            userOpen={subject.slots.user?.status === "opened"}
            userId={subject.userId}
            sharedLabel={sharedLabel}
            onShowAccounts={() => (mainView = "accounts")}
            onUserDirty={() => (subject.dirty.user = true)} />
        </div>
      {:else}
        {#if subject.slots.user?.status === "opened"}
          <Tabs
            class="tree-file"
            ariaLabel="Tree file"
            tabs={[
              { id: "char", label: "Character file" },
              { id: "user", label: "Account file" },
            ]}
            bind:value={treeFile} />
        {/if}
        <!-- The last of the four permanently-invisible buttons: the clear "×"
             was `.mini`, hidden at opacity 0 and revealed only inside a `.row`,
             which the search bar is not. SearchField's clear button is a ghost
             Button and is actually visible when the box has text. -->
        <div class="searchbar">
          <SearchField
            verb="search"
            nouns="labels and values"
            shortcut="Ctrl+F"
            bind:element={searchBox}
            bind:value={query}
            count={searching ? (found?.count ?? 0) : undefined}
            onclear={closeSearch} />
        </div>
        <div class="tree-area">
          {#if found?.tree}
            <TreeNode
              node={found.tree}
              autoExpand={searching}
              {searching}
              revealPath={reveal?.path ?? null}
              revealNonce={reveal?.n ?? 0}
              onReveal={revealInTree}
              onEdit={handleEdit}
              onRemove={handleRemove}
              onInsertRequest={(n) => (insertTarget = n)} />
          {:else}
            <EmptyState title="Nothing in this file matches &ldquo;{query}&rdquo;." />
          {/if}
        </div>
      {/if}
    {:else}
      <InlineMessage variant="error">Cannot edit: {current.message} (offset {current.offset})</InlineMessage>
      <pre class="hex">{current.hex_preview}</pre>
    {/if}
  </section>
  {#if current?.status === "opened"}
    {#if backupsOpen}
      <BackupsPanel
        slot={active}
        savedAt={subject.savedAt}
        subtitle={openDisplay}
        onCollapse={() => (backupsOpen = false)}
        onRestored={(outcome) => {
          subject.slots[active] = outcome;
          subject.dirty[active] = false;
          subject.savedAt += 1;
        }}
      />
    {:else}
      <button class="rail rail-right" onclick={() => (backupsOpen = true)}
        title="Show backups" aria-label="Show backups">«</button>
    {/if}
  {/if}
  {#if insertTarget !== null}
    <!-- A Sheet, so it traps focus, restores it to the opener and closes on
         Escape. The `.modal` it replaces did none of the three. -->
    <Sheet title="Add entry" width="min(420px, 92vw)" onclose={() => (insertTarget = null)}>
      <InsertForm
        target={insertTarget}
        onSubmit={async (m) => {
          await runMutation(m, true); // throws => the form keeps itself open
          insertTarget = null;
        }}
        onCancel={() => (insertTarget = null)} />
    </Sheet>
  {/if}
  {/if}
</main>

<!-- Mounted once, here. Every transient confirmation in the app renders through
     it, so it has to outlive whichever view raised it. -->
<Toast />
