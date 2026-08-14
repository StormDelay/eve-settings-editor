<script lang="ts">
  // The complete, mouse-only route to everything global. Someone who never
  // learns Ctrl+K loses nothing — that is discovery rule 1, and this menu is
  // what makes it true, which is why it ships BEFORE the palette rather than
  // beside it.
  //
  // Its rows are the registry filtered by `homes`, not a hand-written list, so
  // the menu cannot drift from the commands — it IS them. The two rows that are
  // not commands (Rescan, Refresh names) are here because they act on the
  // profile scan rather than on a document, and neither has a subject the
  // palette could name.
  //
  // "Open file…" is NOT here. It is a file-list operation and the only route to
  // an account file directly, so it stays in the sidebar, at the bottom.
  import { api, type Proposal } from "./api";
  import { rescanProfiles, allCharIds } from "./subject.svelte";
  import { refreshNames } from "./names.svelte";
  import { COMMANDS, type Command, type Ctx } from "./commands";
  import { toast } from "./ui/toasts.svelte";
  import Chip from "./ui/Chip.svelte";
  import Popover from "./ui/Popover.svelte";

  let {
    anchor,
    onclose,
    ctx,
  }: {
    anchor: HTMLElement;
    onclose: () => void;
    ctx: Ctx;
  } = $props();

  // Computed WHEN THE MENU OPENS — this component is only mounted while it is
  // open — and never at app start. A count briefly absent while the scan runs is
  // correct: it is not yet known.
  //
  // It COUNTS, it does not name. Naming characters is `Accept all`'s job inside
  // the sheet, where there is room and the objects are on screen.
  let waiting = $state<number | null>(null);
  api
    .launcherProposals()
    .then((ps: Proposal[]) => (waiting = ps.filter((p) => p.conflict === null).length))
    .catch(() => (waiting = null));

  let namesBusy = $state(false);

  /** The menu's shape, as ids. A `null` is a divider. Grouped by what the row
   *  acts on — the open file, then the places you go, then the profile scan. */
  const LAYOUT: (string | null)[] = [
    "file.save",
    "file.discard",
    "file.history",
    null,
    "go.accounts",
    "go.copySettings",
    null,
    "help.shortcuts",
    "file.about",
  ];

  const rows = $derived(
    LAYOUT.map((id) => (id === null ? null : COMMANDS.find((c) => c.id === id)!)),
  );

  function pick(c: Command) {
    // Close FIRST, then run. A command that raises a confirmation or an OS
    // picker would otherwise stack it on a menu that is no longer reachable.
    onclose();
    c.run(ctx);
  }

  async function refreshNamesClick() {
    if (namesBusy) return;
    namesBusy = true;
    try {
      await refreshNames(allCharIds());
    } finally {
      namesBusy = false;
    }
    toast("Names refreshed", { variant: "success" });
  }

  async function rescan() {
    const n = await rescanProfiles();
    toast(
      n === null ? "Rescan failed" : `Refreshed — ${n} profile${n === 1 ? "" : "s"}`,
      { variant: n === null ? "error" : "success" },
    );
  }
</script>

<Popover {anchor} placement="bottom-start" {onclose} role="menu" ariaLabel="App menu" class="app-menu">
  {#each rows as row, i (i)}
    {#if row === null}
      <hr />
    {:else}
      {@const why = row.enabled()}
      <!-- Disabled with a REASON, never hidden. A row that vanishes when the
           backend would refuse it teaches nothing and moves the rows under the
           cursor; "Save — nothing has changed" is an answer. The reason comes
           from the same predicate the palette renders. -->
      <button
        role="menuitem"
        disabled={why !== true}
        title={why === true ? undefined : why}
        onclick={() => pick(row)}>
        <span>{row.label}</span>
        <span class="right">
          {#if row.id === "go.accounts" && waiting}
            <Chip state="proposed" size="sm" title="Pairings your EVE launcher log proposes">{waiting}</Chip>
          {/if}
          <!-- Every item shows its accelerator, per platform. That is discovery
               rule 3: people learn the shortcut at the moment they use the slow
               path. -->
          {#if row.accel}<kbd>{row.accel}</kbd>{/if}
        </span>
      </button>
    {/if}
  {/each}
  <hr />
  <button role="menuitem" disabled={namesBusy} onclick={() => void refreshNamesClick()}>
    <span>{namesBusy ? "Refreshing character names…" : "Refresh character names"}</span>
  </button>
  <button role="menuitem" onclick={() => void rescan()}><span>Rescan profiles</span></button>
</Popover>

<style>
  /* :global because the class lands on Popover's root, which is in Popover's
     scope. The buttons below are authored here, so they scope normally —
     matching ContextMenu, which solved this first. */
  :global(.popover.app-menu) {
    min-width: 16rem;
    display: flex;
    flex-direction: column;
  }
  button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s3);
    background: none;
    border: none;
    border-radius: var(--r-sm);
    color: var(--text);
    font: inherit;
    font-size: var(--t-ui);
    text-align: left;
    padding: var(--s1) var(--s2);
    cursor: pointer;
  }
  .right {
    display: flex;
    align-items: center;
    gap: var(--s2);
    flex-shrink: 0;
  }
  kbd {
    color: var(--text-muted);
    font-family: inherit;
    font-size: var(--t-caption);
    white-space: nowrap;
  }
  button:hover:not(:disabled) {
    background: var(--surface-raised);
  }
  button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  button:disabled {
    opacity: var(--o-disabled);
    cursor: default;
  }
  hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: var(--s1) 0;
    width: 100%;
  }
</style>
