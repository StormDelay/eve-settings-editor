<script lang="ts">
  import Sidebar from "$lib/Sidebar.svelte";
  import TreeNode from "$lib/TreeNode.svelte";
  import RawInspector from "$lib/RawInspector.svelte";
  import InsertForm from "$lib/InsertForm.svelte";
  import ContextBar from "$lib/ContextBar.svelte";
  import ViewTabs from "$lib/ViewTabs.svelte";
  import AboutPanel from "$lib/AboutPanel.svelte";
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
  import ListRow from "$lib/ui/ListRow.svelte";
  import ScopeBanner from "$lib/ui/ScopeBanner.svelte";
  import SearchField from "$lib/ui/SearchField.svelte";
  import Sheet from "$lib/ui/Sheet.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import Toast from "$lib/ui/Toast.svelte";
  import { api, errMessage, type OpenOutcome, type Slot } from "$lib/api";
  import type { Mutation, NodePath, TreeNodeData, PresetInfo } from "$lib/api";
  import { searchTree } from "$lib/search";
  import { resolveNames } from "$lib/names.svelte";
  import { accountsStore } from "$lib/accounts.svelte";
  import { loadPrefs } from "$lib/prefs.svelte";
  import { resolvedName } from "$lib/filesort.svelte";
  import { accel } from "$lib/keys";
  import { resolveView, type View } from "$lib/views";
  import {
    subject,
    accountAliasOf,
    confirmDiscardIfDirty,
    loadCharacter,
    noCharactersHint,
    reconcileCharSlot,
    reconcileUserSlot,
    rescanProfiles,
    saveFile,
  } from "$lib/subject.svelte";
  import { open as openDialog, message } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  // One sheet at a time; `null` is the editor. Replaces `mainView`, which had no
  // third value's worth of meaning — "file" was only ever "no sheet".
  //
  // Not a stack: there is exactly one place in the app where one sheet refers to
  // the other, and it is prose. Nothing can open Accounts from inside Copy
  // settings, so a stack has no way to form — and a single variable makes `Esc`
  // unambiguous and gives focus-restore one saved element to hold.
  let sheet = $state<"accounts" | "batch" | null>(null);
  let sidebarOpen = $state(true);
  // Renamed from `backupsOpen`. The right column is no longer "backups" — it is
  // properties of the current selection in the current view, on every tab.
  let inspectorOpen = $state(true);
  // The Raw tree's selection, held as a PATH rather than as the node: an edit
  // rebuilds the tree, and a held node would go on showing its old value.
  let selectedPath = $state<NodePath | null>(null);
  // A path is an index route, not a name, so the SAME path names a different
  // node in a different file. Dropped whenever the tree underneath changes.
  $effect(() => {
    void treeFile;
    void current;
    selectedPath = null;
  });
  let aboutOpen = $state(false);
  let switcherOpen = $state(false);
  // Which file the Raw view shows; a Raw-local switch flips it to the account
  // file when one is loaded. Reset on every open.
  let treeFile = $state<Slot>("char");
  // Starts on Layout, not Raw, and is resolved through the same fallback on the
  // first open. Raw is the escape hatch and sits last.
  let view = $state<View>("layout");

  // What the editing commands write to. The clause that made this a function of
  // the VIEW is gone: it existed to serve a display — the file bar's name, the
  // badges, the backups panel — and none of those read it any more. Autofill,
  // Keybinds and Probes never mutated through `runMutation` at all; they are
  // handed `userId` and call their own commands.
  //
  // The `view === "raw"` guard has to stay: without it, a user who left the Raw
  // view on the account file would hand `slot="user"` to LayoutView.
  const editSlot = $derived<Slot>(
    view === "raw" && treeFile === "user" && subject.slots.user?.status === "opened" ? "user" : "char",
  );
  const current = $derived(subject.slots[editSlot]);

  // The folder AccountsView and BatchView resolve their scope from. It is the
  // SUBJECT's path rather than `slots[editSlot]`'s, so which tab you happened to
  // be on no longer decides which pairings "Accept all" commits — v0.34 made
  // that scope gate a WRITE, not just which cards are listed.
  const subjectPath = $derived(
    subject.slots.char?.status === "opened"
      ? subject.slots.char.path
      : subject.slots.user?.status === "opened"
        ? subject.slots.user.path
        : null,
  );

  // The four views that edit account-scoped data — the same set the four
  // copy-pasted banners covered.
  const ACCOUNT_SCOPED: View[] = ["overview", "autofill", "keybinds", "probes"];
  // The views that supply their own inspector through `display: contents`. The
  // shell draws its placeholder for every other one, because the column is a
  // promise: a column that is there on one tab and gone on the next is the same
  // fault as a tab strip that changes membership.
  const viewOwnsInspector = $derived(
    current?.status === "opened" && (view === "layout" || view === "overview"),
  );
  const scopeLabel = $derived(
    sheet === null && ACCOUNT_SCOPED.includes(view) && subject.sharedNames.length
      ? `Shared account settings — also applies to ${subject.sharedNames.join(", ")}`
      : "",
  );

  // Route a settings file to its slot by filename kind. Non-standard/other files
  // use the char slot (the generic editing slot).
  function slotForName(name: string): Slot {
    return /^core_user_\d+\.dat$/.test(name) ? "user" : "char";
  }

  void rescanProfiles();
  void loadPrefs();

  // Measured so a sheet can inset past the two content-sized rows above the work
   // area. jsdom reports 0 for both, which just makes the sheet full-window there.
  let barHeight = $state(0);
  let tabsHeight = $state(0);

  let insertTarget: TreeNodeData | null = $state(null);
  // Slots a batch copy rewrote that could not be re-read because they hold
  // unsaved edits. Cleared as soon as the slot stops being dirty, so acting on
  // the message — by Save or by Discard — is what dismisses it.
  let staleWritten = $state<Slot[]>([]);
  const staleSlots = $derived(staleWritten.filter((s) => subject.dirty[s]));
  // Selected canvas window, lifted here so it survives Raw/Layout switches.
  let selectedWindowId = $state<string | null>(null);
  // One bindable the ACTIVE view sets, generalised from `layoutFocusFilter`.
  let viewFocusSearch = $state<(() => void) | undefined>(undefined);
  // A request to reveal a node in the tree (bump `n` to re-fire on the same path).
  let reveal = $state<{ path: NodePath; n: number } | null>(null);

  const APP_TITLE = "EVE Settings Editor";
  $effect(() => {
    // Reads the SUBJECT, not `slots[editSlot]`, so switching from Overview to
    // Autofill no longer retitles the window from the character to the account.
    void getCurrentWindow().setTitle(
      subject.subjectLabel ? `${subject.subjectLabel} — ${APP_TITLE}` : APP_TITLE,
    );
  });

  // Resolve names so the width selector shows character names, not bare ids.
  $effect(() => {
    if (subject.accountCharacters.length) void resolveNames(subject.accountCharacters);
  });

  // If the open character becomes paired while its account slot is empty — e.g.
  // the user just paired it in the Accounts view — load the account file so the
  // account-scoped editors light up without a manual re-open. Guarded on an
  // empty user slot, so it never re-loads an already-open account.
  $effect(() => {
    const o = subject.slots.char;
    void accountsStore.roster; // track roster changes
    if (o?.status === "opened" && subject.slots.user === null) void reconcileUserSlot(o);
  });

  // Jump to a value in the full tree: leave search, expand and scroll to it.
  function revealInTree(path: NodePath) {
    view = "raw";
    sheet = null;
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

  // The one implementation of "pick a file from the OS dialog", shared by the
  // sidebar's button and the launch empty state's.
  async function pickFile() {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "EVE settings", extensions: ["dat"] }],
    });
    if (typeof picked === "string") await openFile(picked);
  }

  // `openFile` and `openPresetPair` stay HERE rather than in subject.svelte.ts
  // with the rest of the transitions. They interleave subject state with
  // `treeFile`, `view` and the selection, which the shell owns, and the
  // interleaving is load-bearing: `treeFile = slot` has to land before the
  // `savedAt` bump, or the bump fires while `editSlot` still names the outgoing
  // slot. Every consumer that opens a file is a descendant of this component.
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
      const priorView = view;
      sheet = null;
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
      view = resolveView(priorView);
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
      sheet = null;
      selectedWindowId = null;
      reveal = null;
      try {
        subject.layoutAvailable = (await api.windowLayout("char")).windows.length > 0;
      } catch {
        subject.layoutAvailable = false;
      }
      view = resolveView(priorView);
    } catch (e) {
      await message(errMessage(e), { title: "Open failed", kind: "error" });
    }
  }

  // `rethrow` is for callers with somewhere better to put the error than a
  // dialog — the insert form shows it inline and stays open on failure.
  async function runMutation(m: Mutation, rethrow = false) {
    const doc = subject.slots[editSlot];
    if (doc?.status !== "opened") return;
    try {
      const tree = await api.mutate(editSlot, m);
      // Reassign (not mutate-in-place) so the derived `current` refires.
      subject.slots[editSlot] = { ...doc, tree };
      subject.dirty[editSlot] = true;
    } catch (e) {
      if (rethrow) throw e;
      await message(errMessage(e), { title: "Edit failed", kind: "error" });
    }
  }

  // Batched sibling of runMutation: one backend round-trip for many mutations
  // (e.g. a layout-canvas drag fanning out to several windows' geometry).
  async function runMutations(ms: Mutation[], rethrow = false) {
    const doc = subject.slots[editSlot];
    if (doc?.status !== "opened") return;
    if (ms.length === 0) return;
    try {
      const tree = await api.mutateMany(editSlot, ms);
      subject.slots[editSlot] = { ...doc, tree };
      subject.dirty[editSlot] = true;
    } catch (e) {
      if (rethrow) throw e;
      await message(errMessage(e), { title: "Edit failed", kind: "error" });
    }
  }

  /**
   * A batch copy writes files on disk, behind the in-memory documents.
   *
   * A slot it wrote that is CLEAN is simply re-read — the same `api.open` plus
   * `savedAt` bump that opening and discarding already use, so every
   * projection-based view refreshes through the mechanism it has.
   *
   * A slot it wrote that is DIRTY is never re-read: that would silently destroy
   * unsaved edits, and no amount of warning makes that acceptable. It gets a
   * message instead, and both routes out already exist and are already correct —
   * Discard re-reads both files, and Save hits the backend's changed-on-disk
   * check and offers the overwrite confirmation. The message only moves the
   * discovery forward from save time to now.
   */
  async function onBatchApplied(written: string[]) {
    const stale: Slot[] = [];
    for (const slot of ["char", "user"] as const) {
      const o = subject.slots[slot];
      if (o?.status !== "opened" || !written.includes(o.path)) continue;
      if (subject.dirty[slot]) {
        stale.push(slot);
        continue;
      }
      try {
        subject.slots[slot] = await api.open(slot, o.path);
        subject.savedAt += 1;
      } catch {
        stale.push(slot); // couldn't re-read it — say so rather than pretend
      }
    }
    staleWritten = stale;
  }

  const handleEdit = (path: NodePath, text: string) =>
    runMutation({ op: "set_scalar", path, text });
  const handleRemove = (path: NodePath) =>
    runMutation({ op: "remove_entry", path });

  // Up to eight on the launch screen, then "… N more" which opens the subject
  // list. No recents store: it would need new persisted state for a list the app
  // can already derive, in a window where the full list fits.
  const launchRows = $derived(subject.characters.slice(0, 8));
  const launchMore = $derived(subject.characters.length - launchRows.length);
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
    if ((e.ctrlKey || e.metaKey) && e.key === "k") {
      e.preventDefault();
      switcherOpen = true;
    }
    // Take Ctrl+F off the webview: its find-on-page cannot see collapsed nodes.
    // The active view focuses its OWN search box if it has one; otherwise this
    // falls through to the Raw tree's.
    //
    // NOT `viewFocusSearch?.() ?? openSearch()` as §5.12 writes it — those
    // callbacks return void, so the `??` would fire openSearch() every time,
    // including right after the view had focused its own box.
    //
    // Suppressed entirely while a sheet is open: every box it could focus is
    // behind the scrim, and focusing an inert control would break the trap.
    if ((e.ctrlKey || e.metaKey) && e.key === "f") {
      e.preventDefault();
      if (sheet !== null) return;
      if (viewFocusSearch) viewFocusSearch();
      else openSearch();
    }
    if (e.key === "Escape" && searching) closeSearch();
  }}
/>

<main
  class="shell"
  class:subjects-railed={!sidebarOpen}
  class:inspector-railed={!inspectorOpen}
  style="--shell-inset-top: {barHeight + tabsHeight}px">
  <ContextBar
    bind:switcherOpen
    bind:height={barHeight}
    onOpen={openFile}
    onOpenPreset={openPresetPair}
    onGoto={(v) => {
      view = v;
      sheet = null;
    }}
    onShowAccounts={() => (sheet = "accounts")}
    onShowBatch={() => (sheet = "batch")}
    onShowAbout={() => (aboutOpen = true)}
    onRestored={(slot, outcome) => {
      subject.slots[slot] = outcome;
      subject.dirty[slot] = false;
      subject.savedAt += 1;
    }} />

  {#if sidebarOpen}
    <Sidebar
      onOpen={openFile}
      onPickFile={pickFile}
      onCollapse={() => (sidebarOpen = false)}
      onOpenPreset={openPresetPair}
      onShowAccounts={() => (sheet = "accounts")} />
  {:else}
    <button class="rail rail-left" onclick={() => (sidebarOpen = true)}
      title="Show file list" aria-label="Show file list">&raquo;</button>
  {/if}

  <ViewTabs bind:value={view} bind:height={tabsHeight} onpick={() => (sheet = null)} />

  <!-- §7.2(c). A slot Copy settings rewrote that could not be re-read, because
       it holds unsaved edits. Both routes out already exist and both are
       correct; this only moves the discovery forward from save time to now.
       Dismissed by acting — Discard clears the dirty flags, Save writes. -->
  {#if staleSlots.length > 0}
    <div class="stale">
      <InlineMessage variant="warn">
        {staleSlots.length === 1 ? "The" : "Both the"}
        {staleSlots.map((s) => (s === "char" ? "character" : "account")).join(" and ")}
        file {staleSlots.length === 1 ? "was" : "were"} rewritten on disk by Copy settings. Your
        unsaved edits are still here — saving will overwrite what was just copied. Discard to take
        the copied version instead.
      </InlineMessage>
    </div>
  {/if}

  {#if current === null}
    <div class="work">
      <div class="scroll">
        <EmptyState
          title="Open a character to begin"
          description="EVE Settings Editor edits the settings files EVE writes for each of your characters and accounts." />
        <div class="launch">
          {#if subject.profiles.length === 0}
            <InlineMessage>No EVE profiles found in standard locations. Use “Open file…”.</InlineMessage>
          {:else if launchRows.length === 0}
            <!-- The same words the sidebar uses, from the same function, so the
                 two cannot drift — including the one that names the filter as
                 the cause. -->
            <InlineMessage>{noCharactersHint()}</InlineMessage>
          {:else}
            <ul>
              {#each launchRows as f (f.path)}
                {@const alias = accountAliasOf(f)}
                <li>
                  <ListRow onclick={() => openFile(f.path)} title={f.file_name}>
                    {resolvedName(f.kind, f.id) ?? f.file_name}
                    {#if alias}<Chip size="sm">{alias}</Chip>{/if}
                  </ListRow>
                </li>
              {/each}
            </ul>
            {#if launchMore > 0}
              <Button variant="ghost" onclick={() => (sidebarOpen = true)}>… {launchMore} more</Button>
            {/if}
          {/if}
          <p class="launch-foot">
            <Button onclick={pickFile}>Open file…</Button>
            <span>or press {accel("K")} to search</span>
          </p>
        </div>
      </div>
    </div>
  {:else if current.status !== "opened"}
    <div class="work">
      <div class="scroll">
        <InlineMessage variant="error">Cannot edit: {current.message} (offset {current.offset})</InlineMessage>
        <pre class="hex">{current.hex_preview}</pre>
      </div>
    </div>
  {:else if view === "layout"}
    <!-- The ONE view that fills both columns. Its root is `display: contents`,
         so its two children become grid items of the shell and land in columns 2
         and 3 — no portal, no prop hoisting, no dependency. -->
    <LayoutView
      slot={editSlot}
      {runMutations}
      readOnly={current.fidelity.state !== "editable"}
      accountReadOnly={subject.slots.user?.status === "opened" && subject.slots.user.fidelity.state !== "editable"}
      refreshToken={subject.savedAt}
      userOpen={subject.slots.user?.status === "opened"}
      bind:selectedId={selectedWindowId}
      onCollapseInspector={() => (inspectorOpen = false)}
      onReveal={revealInTree}
      onDirty={(slot) => (subject.dirty[slot] = true)}
      sharedNames={subject.sharedNames}
      bind:focusSearch={viewFocusSearch} />
  {:else if view === "overview"}
    <!-- The second view that fills both columns, by the same `display: contents`
         route: its tab list is the work area and its inspector is the tab's
         properties. It renders the scope banner itself for that reason. -->
    <OverviewView
      {scopeLabel}
      onCollapseInspector={() => (inspectorOpen = false)}
      userOpen={subject.slots.user?.status === "opened"}
      userId={subject.userId}
      charId={subject.charId}
      charOpen={subject.slots.char?.status === "opened"}
      characters={subject.accountCharacters}
      refreshToken={subject.savedAt}
      onLoadCharacter={loadCharacter}
      onUserDirty={() => (subject.dirty.user = true)}
      onCharDirty={() => (subject.dirty.char = true)}
      onWindowAdded={(id) => { if (subject.layoutAvailable) { selectedWindowId = id; view = "layout"; } }}
      onShowAccounts={() => (sheet = "accounts")} />
  {:else}
    <div class="work">
      <!-- One of ScopeBanner's two shell-owned call sites. Scope has to be
           legible BEFORE the edit, not only at save time; the other is inside
           the save disclosure. What went away is the duplication — four
           components taking a `sharedLabel` prop and each rendering its own
           byte-identical paragraph and CSS block. -->
      <ScopeBanner label={scopeLabel} compact />
      {#if view === "autofill"}
        <div class="scroll">
          <AutofillView
            userOpen={subject.slots.user?.status === "opened"}
            userId={subject.userId}
            charOpen={subject.slots.char?.status === "opened"}
            charName={subject.charName}
            onShowAccounts={() => (sheet = "accounts")}
            onUserDirty={() => (subject.dirty.user = true)}
            bind:focusSearch={viewFocusSearch} />
        </div>
      {:else if view === "keybinds"}
        <div class="scroll">
          <KeybindsView
            userOpen={subject.slots.user?.status === "opened"}
            userId={subject.userId}
            onShowAccounts={() => (sheet = "accounts")}
            onShowBatch={() => (sheet = "batch")}
            onUserDirty={() => (subject.dirty.user = true)}
            bind:focusSearch={viewFocusSearch} />
        </div>
      {:else if view === "probes"}
        <div class="scroll">
          <ProbeFormationsView
            userOpen={subject.slots.user?.status === "opened"}
            userId={subject.userId}
            onShowAccounts={() => (sheet = "accounts")}
            onUserDirty={() => (subject.dirty.user = true)} />
        </div>
      {:else}
        {#if subject.slots.user?.status === "opened"}
          <Tabs
            class="tree-file"
            ariaLabel="Raw file"
            tabs={[
              { id: "char", label: "Character file" },
              { id: "user", label: "Account file" },
            ]}
            bind:value={treeFile} />
        {/if}
        <div class="raw-search">
          <SearchField
            verb="search"
            nouns="labels and values"
            shortcut={accel("F")}
            bind:element={searchBox}
            bind:value={query}
            count={searching ? (found?.count ?? 0) : undefined}
            onclear={closeSearch} />
        </div>
        <div class="scroll">
          {#if found?.tree}
            <TreeNode
              node={found.tree}
              autoExpand={searching}
              {searching}
              revealPath={reveal?.path ?? null}
              revealNonce={reveal?.n ?? 0}
              {selectedPath}
              onSelect={(n) => (selectedPath = n.path)}
              onReveal={revealInTree}
              onEdit={handleEdit}
              onRemove={handleRemove}
              onInsertRequest={(n) => (insertTarget = n)} />
          {:else}
            <EmptyState title="Nothing in this file matches &ldquo;{query}&rdquo;." />
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  <!-- The inspector column exists on EVERY tab. A column that is there on one
       tab and gone on the others is the same class of fault as a tab strip that
       changes membership, so this is a visible promise rather than a collapsed
       column — and anyone who wants the width back rails it. Layout supplies its
       own through `display: contents`, so the shell does not draw one over it. -->
  {#if !inspectorOpen}
    <button class="rail rail-right" onclick={() => (inspectorOpen = true)}
      title="Show properties" aria-label="Show properties">&laquo;</button>
  <!-- No longer conditioned on `sheet`: the editor is never unmounted now, so
       Layout still supplies its own inspector underneath an open sheet. -->
  {:else if !viewOwnsInspector}
    <aside class="inspector">
      <div class="inspector-head">
        <Button variant="ghost" size="sm" iconOnly title="Hide properties"
          onclick={() => (inspectorOpen = false)}>&raquo;</Button>
      </div>
      <!-- Raw is the third view with an inspector, and the strongest case in the
           app: every one of these fields was invisible, hover-only, or encoded
           as a colour. Autofill, Keybinds and Probes declare none — there is
           nothing to select in the first, the second's selection is transient
           and its capture bar is already in the right place, and the third
           already has a selection rail and an editor. -->
      {#if view === "raw" && current?.status === "opened"}
        <RawInspector
          root={found?.tree ?? null}
          path={selectedPath}
          file={treeFile}
          onReveal={revealInTree}
          onRemove={handleRemove}
          onInsertRequest={(n) => (insertTarget = n)} />
      {:else}
        <EmptyState title="Select something to see its properties." />
      {/if}
    </aside>
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

  <!-- Both sheets are fixed-positioned, so neither takes a grid track. The
       editor above them is unconditional: nothing is restored when a sheet
       closes because nothing was destroyed — the view tab, the canvas selection,
       the tree search and the scroll position are all still exactly where they
       were, and the scroll position is the one that no amount of
       snapshot-and-restore code could have given back. -->
  {#if sheet === "accounts"}
    <AccountsView openPath={subjectPath} onClose={() => (sheet = null)} />
  {:else if sheet === "batch"}
    <BatchView
      openCharPath={subject.slots.char?.status === "opened" ? subject.slots.char.path : null}
      openUserPath={subject.slots.user?.status === "opened" ? subject.slots.user.path : null}
      onClose={() => (sheet = null)}
      onApplied={onBatchApplied} />
  {/if}
</main>

{#if aboutOpen}<AboutPanel onClose={() => (aboutOpen = false)} />{/if}

<!-- Mounted once, here. Every transient confirmation in the app renders through
     it, so it has to outlive whichever view raised it. -->
<Toast />

<style>
  .launch {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s2);
    max-width: 44ch;
    margin: 0 auto;
  }
  .launch ul {
    list-style: none;
    margin: 0;
    padding: 0;
    width: 100%;
  }
  .launch li {
    list-style: none;
  }
  .launch-foot {
    display: flex;
    align-items: center;
    gap: var(--s2);
    margin: var(--s3) 0 0;
    color: var(--text-muted);
    font-size: var(--t-body);
  }
  /* Its own row between the tabs and the work area, spanning both the work
     column and the inspector — it is about the documents, not about a view. */
  .stale {
    grid-column: 2 / 4;
    grid-row: 3;
    align-self: start;
    padding: var(--s2) var(--s3) 0;
    z-index: 1;
  }
</style>
