<script lang="ts">
  // One static table, and the visible home for the shortcuts that are NOT
  // commands — Escape, the nudge arrows, Enter to commit. Those have no registry
  // entry because they act on whatever is under the cursor or the caret, so
  // without this sheet they would be undocumented anywhere in the app.
  //
  // Every accelerator here is rendered through `accel()`, never written out, so
  // the table is correct on macOS for free.
  import { COMMANDS } from "./commands";
  import { accel, MOD } from "./keys";
  import Sheet from "./ui/Sheet.svelte";

  let { onClose }: { onClose: () => void } = $props();

  const chords = $derived(COMMANDS.filter((c) => c.accel));

  // The positional keys. They are here rather than in the registry because none
  // of them names a subject the palette could show — "nudge the selected window"
  // is not a row in a command list.
  const LOCAL: { keys: string; does: string; where: string }[] = [
    // Undo and redo are not registry commands: they act on the document stack
    // rather than on a subject the palette could name. This table is their only
    // written home.
    { keys: accel("Z"), does: "Undo the last change", where: "Everywhere" },
    { keys: `${accel("Z")} with Shift`, does: "Redo", where: "Everywhere" },
    { keys: accel("Y"), does: "Redo — the Windows spelling of the same thing", where: "Everywhere" },
    { keys: "Esc", does: "Close the topmost thing — menu, sheet, or the search box", where: "Everywhere" },
    { keys: "Enter", does: "Commit an inline name entry", where: "Overview, Presets, Autofill, Accounts, Raw" },
    { keys: "Esc", does: "Cancel an inline name entry", where: "the same six" },
    { keys: "← ↑ → ↓", does: "Nudge the selected window", where: "Layout" },
    { keys: `Shift / ${MOD} / Alt + arrows`, does: "Nudge by a different step", where: "Layout" },
    { keys: accel("C"), does: "Copy the selected formation", where: "Probes" },
    { keys: accel("V"), does: "Add a formation from the clipboard", where: "Probes" },
    { keys: "any key", does: "Capture a binding", where: "Keybinds, while a chip is listening" },
    { keys: "Backspace", does: "Unbind", where: "Keybinds, while listening" },
  ];
</script>

<Sheet title="Keyboard shortcuts" titled width="min(38rem, 92vw)" onclose={onClose}>
  <table>
    <tbody>
      {#each chords as c (c.id)}
        <tr>
          <td class="keys"><kbd>{c.accel}</kbd></td>
          <td>{c.label}</td>
          <td class="where">{c.group}</td>
        </tr>
      {/each}
      {#each LOCAL as l (l.keys + l.does)}
        <tr>
          <td class="keys"><kbd>{l.keys}</kbd></td>
          <td>{l.does}</td>
          <td class="where">{l.where}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</Sheet>

<style>
  table {
    border-collapse: collapse;
    width: 100%;
    font-size: var(--t-body);
  }
  td {
    padding: var(--s1) var(--s2);
    vertical-align: baseline;
  }
  .keys {
    white-space: nowrap;
    width: 1%;
  }
  .where {
    color: var(--text-muted);
    white-space: nowrap;
    text-align: right;
  }
  kbd {
    font-family: inherit;
    font-size: var(--t-caption);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 0 var(--s1);
    white-space: nowrap;
  }
</style>
