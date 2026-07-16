<script lang="ts">
  import Sidebar from "$lib/Sidebar.svelte";
  import TreeNode from "$lib/TreeNode.svelte";
  import InsertForm from "$lib/InsertForm.svelte";
  import BackupsPanel from "$lib/BackupsPanel.svelte";
  import LayoutView from "$lib/LayoutView.svelte";
  import AccountsView from "$lib/AccountsView.svelte";
  import OverviewView from "$lib/OverviewView.svelte";
  import AutofillView from "$lib/AutofillView.svelte";
  import { api, errMessage, type OpenOutcome, type Slot } from "$lib/api";
  import type { Mutation, NodePath, TreeNodeData, ErrDto, Profile } from "$lib/api";
  import { searchTree } from "$lib/search";
  import { names, resolveNames } from "$lib/names.svelte";
  import { aliasFor, accountsStore } from "$lib/accounts.svelte";
  import {
    pairedFilePath,
    associatedCharacters,
    userSlotFor,
    charSlotFor,
  } from "$lib/overview";
  import { ask, message } from "@tauri-apps/plugin-dialog";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  let mainView: "file" | "accounts" = $state("file");
  // Two independent editing slots: a character file and an account (user) file,
  // each with its own dirty flag. `active` picks which one the UI shows/edits.
  const slots = $state<{ char: OpenOutcome | null; user: OpenOutcome | null }>({
    char: null,
    user: null,
  });
  const dirtySlots = $state<{ char: boolean; user: boolean }>({ char: false, user: false });
  let active = $state<Slot>("char");
  const current = $derived(slots[active]);

  function slotSaveable(o: OpenOutcome | null, dirty: boolean): boolean {
    return dirty && o?.status === "opened" && o.fidelity.state === "editable";
  }
  const canSave = $derived(slotSaveable(slots.char, dirtySlots.char) || slotSaveable(slots.user, dirtySlots.user));

  // Route a settings file to its slot by filename kind. Non-standard/other files
  // use the char slot (the generic editing slot).
  function slotForName(name: string): Slot {
    return /^core_user_\d+\.dat$/.test(name) ? "user" : "char";
  }

  // Discovered profiles, for resolving a char/user id to its file path within
  // the same profile folder as an already-open file (see pairedFilePath).
  let profiles = $state<Profile[]>([]);
  api.discover().then((p) => (profiles = p)).catch(() => {});

  let insertTarget: TreeNodeData | null = $state(null);
  let savedAt = $state(0); // bumped after each save; BackupsPanel refetches on change
  let view: "tree" | "layout" | "overview" | "autofill" = $state("tree");
  let layoutAvailable = $state(false);
  // Selected canvas window, lifted here so it survives Tree/Layout switches.
  let selectedWindowId = $state<string | null>(null);
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
    if (current?.status !== "opened") return null;
    return openCharName ?? openUserAlias ?? current.file_name;
  });

  const APP_TITLE = "EVE Settings Editor";
  $effect(() => {
    void getCurrentWindow().setTitle(
      openDisplay ? `${openDisplay} — ${APP_TITLE}` : APP_TITLE,
    );
  });

  // Overview editor inputs: the ids of the open char/user files, and the roster's
  // characters for the open account (the width selector loads one of these).
  const openCharId = $derived.by(() => {
    const o = slots.char;
    if (o?.status !== "opened") return null;
    const m = o.file_name.match(/^core_char_(\d+)\.dat$/);
    return m ? Number(m[1]) : null;
  });
  const openUserId = $derived.by(() => {
    const o = slots.user;
    if (o?.status !== "opened") return null;
    const m = o.file_name.match(/^core_user_(\d+)\.dat$/);
    return m ? Number(m[1]) : null;
  });
  const openAccountCharacters = $derived(
    openUserId === null ? [] : associatedCharacters(openUserId, accountsStore.roster),
  );
  // Resolve names so the width selector shows character names, not bare ids.
  $effect(() => { if (openAccountCharacters.length) void resolveNames(openAccountCharacters); });

  // Jump to a value in the full tree: leave search, expand and scroll to it.
  function revealInTree(path: NodePath) {
    view = "tree";
    query = "";
    reveal = { path, n: (reveal?.n ?? 0) + 1 };
  }

  let query = $state("");
  let searchBox: HTMLInputElement | undefined = $state();
  const searching = $derived(query.trim() !== "");
  // Re-runs after every mutation, since the tree is replaced wholesale.
  const searchIn = (doc: OpenOutcome | null, q: string) =>
    doc?.status === "opened" ? searchTree(doc.tree, q) : null;
  const found = $derived(searchIn(current, query));

  function openSearch() {
    searchBox?.focus();
    searchBox?.select();
  }

  function closeSearch() {
    query = "";
    searchBox?.blur();
  }

  // Shared unsaved-changes prompt for anything that swaps out an open file:
  // the Open-file dialog/sidebar and the (Task C4) character selector alike.
  async function confirmDiscardIfDirty(): Promise<boolean> {
    if (!dirtySlots.char && !dirtySlots.user) return true;
    const which = [dirtySlots.char && "character", dirtySlots.user && "account"]
      .filter(Boolean)
      .join(" and ");
    const noun = dirtySlots.char && dirtySlots.user ? "files" : "file";
    return ask(
      `You have unsaved changes to the ${which} ${noun}. Discard them and open another file?`,
      { title: "Unsaved changes", kind: "warning" },
    );
  }

  async function openFile(path: string) {
    const name = path.split(/[\\/]/).pop() ?? "";
    const slot = slotForName(name);
    if (!(await confirmDiscardIfDirty())) return;
    try {
      const outcome = await api.open(slot, path);
      slots[slot] = outcome;
      // A file opened via the dialog isn't in the sidebar scan, so its name was
      // never resolved — resolve it here so the header names it too. (A no-op if
      // it was scanned: the id is already cached.)
      if (outcome.status === "opened") {
        const m = outcome.file_name.match(/^core_char_(\d+)\.dat$/);
        if (m) void resolveNames([Number(m[1])]);
      }
      dirtySlots[slot] = false;
      active = slot;
      savedAt += 1;
      view = "tree";
      mainView = "file";
      selectedWindowId = null;
      reveal = null;
      try {
        layoutAvailable =
          outcome.status === "opened" && (await api.windowLayout(slot)).windows.length > 0;
      } catch {
        layoutAvailable = false;
      }
      // Reconcile the *other* slot so the two are always a matching char/user
      // pair (or one empty) — never a stale, unrelated file the Overview editor
      // would misread.
      if (slot === "char") await reconcileUserSlot(outcome);
      else await reconcileCharSlot(outcome);
    } catch (e) {
      await message(errMessage(e), { title: "Open failed", kind: "error" });
    }
  }

  // Empty a slot: close its backend document and clear the frontend state.
  async function clearSlot(slot: Slot) {
    if (slots[slot] === null) return;
    try {
      await api.close(slot);
    } catch { /* best-effort */ }
    slots[slot] = null;
    dirtySlots[slot] = false;
  }

  // After a character lands in the char slot, make the user slot its paired
  // account file — or empty it. Never keep a stale, unrelated account file (the
  // Overview view shows the Accounts nudge when the user slot is empty).
  async function reconcileUserSlot(charOutcome: OpenOutcome) {
    const charId =
      charOutcome.status === "opened"
        ? charOutcome.file_name.match(/^core_char_(\d+)\.dat$/)?.[1] ?? null
        : null;
    const action = userSlotFor(
      charOutcome.status === "opened" ? charOutcome.path : "",
      charId === null ? null : Number(charId),
      slots.user?.status === "opened" ? slots.user.path : null,
      accountsStore.roster,
      profiles,
    );
    if (action.kind === "keep") return;
    if (action.kind === "clear") return clearSlot("user");
    try {
      slots.user = await api.open("user", action.path);
      dirtySlots.user = false;
    } catch {
      await clearSlot("user"); // couldn't load the pair -> don't keep a stale one
    }
  }

  // After an account file lands in the user slot, keep the char slot only if it
  // holds one of this account's characters — otherwise empty it (the character
  // selector picks which of the account's characters to load).
  async function reconcileCharSlot(userOutcome: OpenOutcome) {
    const userId =
      userOutcome.status === "opened"
        ? userOutcome.file_name.match(/^core_user_(\d+)\.dat$/)?.[1] ?? null
        : null;
    const currentCharId =
      slots.char?.status === "opened"
        ? slots.char.file_name.match(/^core_char_(\d+)\.dat$/)?.[1] ?? null
        : null;
    const action = charSlotFor(
      userId === null ? null : Number(userId),
      currentCharId === null ? null : Number(currentCharId),
      accountsStore.roster,
    );
    if (action.kind === "clear") await clearSlot("char");
  }

  // Load a selected character into the char slot (from the OverviewView selector).
  async function loadCharacter(charId: number) {
    if (!(await confirmDiscardIfDirty())) return;
    const anchor = slots.user?.status === "opened" ? slots.user.path : "";
    const charPath = pairedFilePath(profiles, anchor, charId, "char");
    if (!charPath) return;
    try {
      slots.char = await api.open("char", charPath);
      dirtySlots.char = false;
      await resolveNames([charId]);
    } catch (e) {
      await message(errMessage(e), { title: "Open failed", kind: "error" });
    }
  }

  // `rethrow` is for callers with somewhere better to put the error than a
  // dialog — the insert form shows it inline and stays open on failure.
  async function runMutation(m: Mutation, rethrow = false) {
    const doc = slots[active];
    if (doc?.status !== "opened") return;
    try {
      const tree = await api.mutate(active, m);
      // Reassign (not mutate-in-place) so the derived `current` refires.
      slots[active] = { ...doc, tree };
      dirtySlots[active] = true;
    } catch (e) {
      if (rethrow) throw e;
      await message(errMessage(e), { title: "Edit failed", kind: "error" });
    }
  }

  const handleEdit = (path: NodePath, text: string) =>
    runMutation({ op: "set_scalar", path, text });
  const handleRemove = (path: NodePath) =>
    runMutation({ op: "remove_entry", path });

  async function saveFile(force = false) {
    for (const slot of ["char", "user"] as const) {
      const o = slots[slot];
      if (!dirtySlots[slot] || o?.status !== "opened" || o.fidelity.state !== "editable") continue;
      try {
        const report = await api.save(slot, force);
        dirtySlots[slot] = false;
        savedAt += 1;
        const note = `Saved ${report.bytes_written} bytes to ${o.file_name}.\nBackup: ${report.backup_path}`;
        await message(note, { title: "Saved", kind: "info" });
      } catch (e) {
        const err = e as ErrDto;
        if (err.code === "conflict") {
          const overwrite = await ask(
            `${o.file_name} changed on disk after it was loaded (the EVE client may have ` +
              `written it). Overwrite anyway?\n\nA backup of the on-disk file is taken first either way.`,
            { title: "File changed on disk", kind: "warning" },
          );
          if (overwrite) {
            try {
              await api.save(slot, true);
              dirtySlots[slot] = false;
              savedAt += 1;
            } catch (e2) {
              await message(errMessage(e2), { title: "Save failed", kind: "error" });
            }
          }
        } else {
          await message(errMessage(e), { title: `Save failed — ${o.file_name} untouched`, kind: "error" });
        }
      }
    }
  }
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
    if ((e.ctrlKey || e.metaKey) && e.key === "f") {
      e.preventDefault();
      openSearch();
    }
    if (e.key === "Escape" && searching) closeSearch();
  }}
/>

<main class="layout">
  <Sidebar onOpen={openFile} onShowAccounts={() => (mainView = "accounts")} />
  {#if mainView === "accounts"}
    <AccountsView openPath={current?.status === "opened" ? current.path : null} />
  {:else}
  <section class="editor">
    {#if current === null}
      <p class="hint">Open a settings file to begin.</p>
    {:else if current.status === "opened"}
      <header class="filebar">
        <span class="filename">
          {#if openCharName}{openCharName} — {/if}{#if openUserAlias}{openUserAlias} — {/if}{current.file_name}
        </span>
        {#if current.fidelity.state === "read_only"}
          <span class="badge read-only" title={current.fidelity.reason}>read-only</span>
        {:else}
          <span class="badge editable">editable</span>
        {/if}
        {#if dirtySlots.char}<span class="badge dirty">character: unsaved</span>{/if}
        {#if dirtySlots.user}<span class="badge dirty">account: unsaved</span>{/if}
        {#if slots.char && slots.user}
          <span class="viewtabs">
            <button class:active={active === "char"} onclick={() => (active = "char")}>Character</button>
            <button class:active={active === "user"} onclick={() => (active = "user")}>Account</button>
          </span>
        {/if}
        {#if layoutAvailable || openCharId !== null || slots.user?.status === "opened"}
          <span class="viewtabs">
            <button class:active={view === "tree"} onclick={() => (view = "tree")}>Tree</button>
            {#if layoutAvailable}<button class:active={view === "layout"} onclick={() => (view = "layout")}>Layout</button>{/if}
            {#if openCharId !== null || slots.user?.status === "opened"}<button class:active={view === "overview"} onclick={() => (view = "overview")}>Overview</button>{/if}
            {#if slots.user?.status === "opened"}<button class:active={view === "autofill"} onclick={() => (view = "autofill")}>Autofill</button>{/if}
          </span>
        {/if}
        <span class="spacer"></span>
        <button
          class="save"
          disabled={!canSave}
          onclick={() => saveFile()}>Save</button>
      </header>
      {#if view === "layout"}
        <div class="tree-area">
          <LayoutView
            slot={active}
            {runMutation}
            readOnly={current.fidelity.state !== "editable"}
            refreshToken={savedAt}
            bind:selectedId={selectedWindowId}
            onReveal={revealInTree} />
        </div>
      {:else if view === "overview"}
        <div class="tree-area">
          <OverviewView
            userOpen={slots.user?.status === "opened"}
            charId={openCharId}
            characters={openAccountCharacters}
            onLoadCharacter={loadCharacter}
            onUserDirty={() => (dirtySlots.user = true)}
            onCharDirty={() => (dirtySlots.char = true)}
            onShowAccounts={() => (mainView = "accounts")} />
        </div>
      {:else if view === "autofill"}
        <div class="tree-area">
          <AutofillView
            userOpen={slots.user?.status === "opened"}
            onUserDirty={() => (dirtySlots.user = true)} />
        </div>
      {:else}
        <div class="searchbar">
          <input
            class="search"
            bind:this={searchBox}
            bind:value={query}
            placeholder="Search labels and values (Ctrl+F)" />
          {#if searching}
            <span class="meta">
              {found?.count ?? 0} match{found?.count === 1 ? "" : "es"}
            </span>
            <button class="mini" title="Clear search (Esc)" onclick={closeSearch}>×</button>
          {/if}
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
            <p class="hint">Nothing in this file matches “{query}”.</p>
          {/if}
        </div>
      {/if}
    {:else}
      <p class="error">Cannot edit: {current.message} (offset {current.offset})</p>
      <pre class="hex">{current.hex_preview}</pre>
    {/if}
  </section>
  {#if current?.status === "opened"}
    <BackupsPanel
      slot={active}
      {savedAt}
      subtitle={openDisplay}
      onRestored={(outcome) => {
        slots[active] = outcome;
        dirtySlots[active] = false;
        savedAt += 1;
      }}
    />
  {/if}
  {#if insertTarget !== null}
    <div class="overlay" role="none" onclick={() => (insertTarget = null)}>
      <div class="modal" role="none" onclick={(e) => e.stopPropagation()}>
        <InsertForm
          target={insertTarget}
          onSubmit={async (m) => {
            await runMutation(m, true); // throws => the form keeps itself open
            insertTarget = null;
          }}
          onCancel={() => (insertTarget = null)}
        />
      </div>
    </div>
  {/if}
  {/if}
</main>
