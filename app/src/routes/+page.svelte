<script lang="ts">
  import Sidebar from "$lib/Sidebar.svelte";
  import TreeNode from "$lib/TreeNode.svelte";
  import InsertForm from "$lib/InsertForm.svelte";
  import BackupsPanel from "$lib/BackupsPanel.svelte";
  import LayoutView from "$lib/LayoutView.svelte";
  import AccountsView from "$lib/AccountsView.svelte";
  import { api, errMessage, type OpenOutcome } from "$lib/api";
  import type { Mutation, NodePath, TreeNodeData, ErrDto } from "$lib/api";
  import { searchTree } from "$lib/search";
  import { names } from "$lib/names.svelte";
  import { aliasFor } from "$lib/accounts.svelte";
  import { ask, message } from "@tauri-apps/plugin-dialog";

  let mainView: "file" | "accounts" = $state("file");
  let current: OpenOutcome | null = $state(null);
  let dirty = $state(false);
  let insertTarget: TreeNodeData | null = $state(null);
  let savedAt = $state(0); // bumped after each save; BackupsPanel refetches on change
  let view: "tree" | "layout" = $state("tree");
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

  async function openFile(path: string) {
    if (dirty) {
      const discard = await ask(
        "You have unsaved changes. Discard them and open the other file?",
        { title: "Unsaved changes", kind: "warning" },
      );
      if (!discard) return;
    }
    try {
      current = await api.open(path);
      dirty = false;
      savedAt += 1;
      view = "tree";
      mainView = "file";
      selectedWindowId = null;
      reveal = null;
      try {
        layoutAvailable =
          current.status === "opened" && (await api.windowLayout()).windows.length > 0;
      } catch {
        layoutAvailable = false;
      }
    } catch (e) {
      await message(errMessage(e), { title: "Open failed", kind: "error" });
    }
  }

  // `rethrow` is for callers with somewhere better to put the error than a
  // dialog — the insert form shows it inline and stays open on failure.
  async function runMutation(m: Mutation, rethrow = false) {
    if (current?.status !== "opened") return;
    try {
      current.tree = await api.mutate(m);
      dirty = true;
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
    if (!dirty || current?.status !== "opened" || current.fidelity.state !== "editable") return;
    try {
      const report = await api.save(force);
      dirty = false;
      savedAt += 1;
      let note = `Saved ${report.bytes_written} bytes.\nBackup: ${report.backup_path}`;
      if (report.recent_sibling_writes.length > 0) {
        note +=
          `\n\nWarning: other files in this profile changed in the last 5 minutes` +
          ` — the EVE client may be running and can overwrite your changes on logout:` +
          `\n${report.recent_sibling_writes.join("\n")}`;
      }
      await message(note, { title: "Saved", kind: "info" });
    } catch (e) {
      const err = e as ErrDto;
      if (err.code === "conflict") {
        const overwrite = await ask(
          "The file changed on disk after it was loaded (the EVE client may have " +
            "written it). Overwrite anyway?\n\nA backup of the current on-disk file " +
            "is taken first either way, so nothing is lost.",
          { title: "File changed on disk", kind: "warning" },
        );
        if (overwrite) await saveFile(true);
      } else {
        await message(errMessage(e), { title: "Save failed — file untouched", kind: "error" });
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
    <AccountsView />
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
        {#if dirty}<span class="badge dirty">unsaved changes</span>{/if}
        {#if layoutAvailable}
          <span class="viewtabs">
            <button class:active={view === "tree"} onclick={() => (view = "tree")}>Tree</button>
            <button class:active={view === "layout"} onclick={() => (view = "layout")}>Layout</button>
          </span>
        {/if}
        <span class="spacer"></span>
        <button
          class="save"
          disabled={!dirty || current.fidelity.state !== "editable"}
          onclick={() => saveFile()}>Save</button>
      </header>
      {#if view === "layout"}
        <div class="tree-area">
          <LayoutView
            {runMutation}
            readOnly={current.fidelity.state !== "editable"}
            refreshToken={savedAt}
            bind:selectedId={selectedWindowId}
            onReveal={revealInTree} />
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
      {savedAt}
      onRestored={(outcome) => {
        current = outcome;
        dirty = false;
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
