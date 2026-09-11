<script lang="ts">
  // Jump to a character, or run a command. Opened by the subject block's ▾ and
  // by Ctrl+K, which are deliberately the SAME control — a palette that competes
  // with a switcher is two answers to "where is my other character".
  //
  // Phase 2 called this "the seed of Phase 5's command palette: same list, same
  // type-ahead, one extra section of commands", and that is exactly what this
  // adds. Building a second component would have been the mistake it named.
  //
  // Commands are ranked rather than filtered, because a command list is the one
  // section where the ORDER carries information: typing `overv` should put the
  // Overview view above a character whose name happens to contain those letters.
  // Characters and presets keep their substring filter and their alphabetical
  // order, which is how a name is found.
  import { COMMANDS, haystack, type Command, type Ctx } from "./commands";
  import { rank, score } from "./fuzzy";
  import { subject, accountAliasOf } from "./subject.svelte";
  import { resolvedName } from "./filesort.svelte";
  import { allPresets, summarise } from "./presetLibrary.svelte";
  import { VIEWS, viewAvailable, type View } from "./views";
  import type { PresetInfo, SettingsFile } from "./api";
  import Chip from "./ui/Chip.svelte";
  import EmptyState from "./ui/EmptyState.svelte";
  import ListRow from "./ui/ListRow.svelte";
  import Popover from "./ui/Popover.svelte";
  import SearchField from "./ui/SearchField.svelte";

  let {
    anchor,
    onclose,
    onOpen,
    onOpenPreset,
    onGoto,
    ctx,
  }: {
    anchor: HTMLElement;
    onclose: () => void;
    onOpen: (path: string) => void;
    onOpenPreset: (p: PresetInfo) => void;
    onGoto: (v: View) => void;
    ctx: Ctx;
  } = $props();

  let query = $state("");
  const q = $derived(query.trim().toLowerCase());

  const label = (f: SettingsFile) => resolvedName(f.kind, f.id) ?? f.file_name;

  // The type-ahead matches the account alias as well as the name, and that is
  // the flat list's whole answer to "switch to my other character on this
  // account" — typing `stormdelay2` filters to exactly that account's
  // characters, still alphabetical. A filter beats a grouping at this because it
  // is temporary: it costs nothing to every other opening, which is looking for
  // one name.
  const rows = $derived(
    subject.characters.filter(
      (f) =>
        q === "" ||
        label(f).toLowerCase().includes(q) ||
        (accountAliasOf(f)?.toLowerCase().includes(q) ?? false),
    ),
  );

  const presets = $derived(
    allPresets().filter((p) => q === "" || p.name.toLowerCase().includes(q)),
  );

  const views = $derived(VIEWS.filter((v) => q === "" || v.label.toLowerCase().includes(q)));

  // Enabled first, then the disabled ones with their reason. They are SHOWN
  // rather than hidden for the same reason the tabs are: a command that vanishes
  // teaches nothing, and "Save — nothing has changed" is an answer to the
  // question the user was asking.
  const commands = $derived(
    rank(COMMANDS, (c) => score(query, c.label, haystack(c))).sort(
      (a, b) => Number(a.enabled() !== true) - Number(b.enabled() !== true),
    ),
  );

  function choose(fn: () => void) {
    onclose();
    fn();
  }

  /** Close FIRST, then run — the ordering ProbeFormationsView's picker already
   *  documents, and for the same reason: a confirmation or an OS picker raised
   *  by the action would otherwise stack on a panel that is no longer
   *  reachable. */
  function runCommand(c: Command) {
    if (c.enabled() !== true) return;
    onclose();
    c.run(ctx);
  }
</script>

<Popover {anchor} placement="bottom-start" {onclose} ariaLabel="Find a character" class="switcher">
  <!-- eslint-disable-next-line -- autofocus is the point of a type-ahead -->
  <SearchField verb="search" nouns="characters, presets and commands" bind:value={query} />

  {#if rows.length === 0 && presets.length === 0 && views.length === 0 && commands.length === 0}
    <EmptyState title="No matches" description="Nothing matches “{query}”." />
  {/if}

  {#if rows.length}
    <ul>
      {#each rows as f (f.path)}
        {@const alias = accountAliasOf(f)}
        <li>
          <!-- The raw file name lives here, on the row and in its tooltip: one
               of the three places §5.2 keeps it after taking it out of the
               context bar. -->
          <ListRow onclick={() => choose(() => onOpen(f.path))} title={f.file_name}>
            {label(f)}
            {#if alias}<Chip size="sm">{alias}</Chip>{/if}
            {#snippet trailing()}<span class="file">{f.file_name}</span>{/snippet}
          </ListRow>
        </li>
      {/each}
    </ul>
  {/if}

  {#if presets.length}
    <p class="section">Presets</p>
    <ul>
      {#each presets as p (p.dir)}
        <li>
          <ListRow
            onclick={() => choose(() => onOpenPreset(p))}
            disabled={p.error !== null}
            disabledReason={p.error ?? undefined}
            title={p.error ?? p.dir}>
            {p.name}
            {#snippet trailing()}<span class="file">{summarise(p)}</span>{/snippet}
          </ListRow>
        </li>
      {/each}
    </ul>
  {/if}

  {#if commands.length}
    <p class="section">Commands</p>
    <ul>
      {#each commands as c (c.id)}
        {@const why = c.enabled()}
        <li>
          <ListRow
            onclick={() => runCommand(c)}
            disabled={why !== true}
            disabledReason={why === true ? undefined : why}
            title={why === true ? undefined : why}>
            {c.label}
            <Chip size="sm">{c.group}</Chip>
            {#snippet trailing()}
              {#if c.accel}<span class="file">{c.accel}</span>{/if}
            {/snippet}
          </ListRow>
        </li>
      {/each}
    </ul>
  {/if}

  {#if views.length}
    <p class="section">Go to…</p>
    <ul>
      {#each views as v (v.id)}
        {@const reason = viewAvailable(v.id)}
        <li>
          <ListRow
            onclick={() => choose(() => onGoto(v.id))}
            disabled={reason !== null}
            disabledReason={reason ?? undefined}
            title={reason ?? undefined}>
            {v.label}
          </ListRow>
        </li>
      {/each}
    </ul>
  {/if}
</Popover>

<style>
  :global(.popover.switcher) {
    width: min(32rem, 90vw);
    max-height: min(30rem, 80vh);
    overflow-y: auto;
    padding: var(--s2);
  }
  ul {
    list-style: none;
    margin: var(--s1) 0;
    padding: 0;
  }
  li {
    list-style: none;
  }
  .section {
    margin: var(--s3) 0 0;
    font-size: var(--t-caption);
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .file {
    color: var(--text-muted);
    font-size: var(--t-caption);
  }
</style>
