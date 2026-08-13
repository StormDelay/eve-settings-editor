<script lang="ts">
  // The five global actions that used to sit in the sidebar's top block — six
  // buttons of four different kinds inside a `flex-wrap: wrap` container, above
  // a list they had nothing to do with. The sidebar becomes a subject browser
  // and nothing else; these become a menu.
  //
  // "Open file…" is NOT here. It is a file-list operation and the only route to
  // an account file directly, so it stays in the sidebar, at the bottom.
  import { api, type Proposal } from "./api";
  import { rescanProfiles, allCharIds } from "./subject.svelte";
  import { refreshNames } from "./names.svelte";
  import { toast } from "./ui/toasts.svelte";
  import Chip from "./ui/Chip.svelte";
  import Popover from "./ui/Popover.svelte";

  let {
    anchor,
    onclose,
    onShowAccounts,
    onShowBatch,
    onShowAbout,
  }: {
    anchor: HTMLElement;
    onclose: () => void;
    onShowAccounts: () => void;
    onShowBatch: () => void;
    onShowAbout: () => void;
  } = $props();

  // Computed WHEN THE MENU OPENS — this component is only mounted while it is
  // open — and never at app start. §5.7.1 refuses a per-character proposal chip
  // in the sidebar partly because it would move `read_roster_from`'s scan of
  // every launcher `.log` onto startup, for a signal that changes nothing the
  // user can do from there. A menu is opened on demand, and opening Accounts
  // already pays that cost, so this adds no work to a path that did not already
  // do it. A count briefly absent while the scan runs is correct: it is not yet
  // known.
  //
  // It COUNTS, it does not name. Naming characters is `Accept all`'s job inside
  // the sheet, where there is room and the objects are on screen.
  let waiting = $state<number | null>(null);
  api
    .launcherProposals()
    .then((ps: Proposal[]) => (waiting = ps.filter((p) => p.conflict === null).length))
    .catch(() => (waiting = null));

  let namesBusy = $state(false);

  function pick(run: () => void) {
    onclose();
    run();
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
  <button role="menuitem" onclick={() => pick(onShowAccounts)}>
    <span>Accounts…</span>
    <!-- The one signpost that proposals are waiting. It attaches to no
         character's row and names nobody, so §5.7.2 rule 6 stands: no list gains
         an account chip from a `Proposal`. -->
    {#if waiting}
      <Chip state="proposed" size="sm" title="Pairings your EVE launcher log proposes">{waiting}</Chip>
    {/if}
  </button>
  <button role="menuitem" onclick={() => pick(onShowBatch)}>Copy settings…</button>
  <hr />
  <button role="menuitem" disabled={namesBusy} onclick={() => void refreshNamesClick()}>
    {namesBusy ? "Refreshing…" : "Refresh names"}
  </button>
  <button role="menuitem" onclick={() => void rescan()}>Rescan profiles</button>
  <hr />
  <button role="menuitem" onclick={() => pick(onShowAbout)}>About</button>
</Popover>

<style>
  /* :global because the class lands on Popover's root, which is in Popover's
     scope. The buttons below are authored here, so they scope normally —
     matching ContextMenu, which solved this first. */
  :global(.popover.app-menu) {
    min-width: 14rem;
    display: flex;
    flex-direction: column;
  }
  button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s2);
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
  button:hover {
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
  button:disabled:hover {
    background: none;
  }
  hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: var(--s1) 0;
    width: 100%;
  }
</style>
