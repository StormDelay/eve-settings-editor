// A pre-existing bug, fixed and pinned.
//
// `refreshToken={savedAt}` reached exactly two of the six views. Layout and
// Overview reloaded on every save, open, Discard and backup restore; Autofill,
// Keybinds and Probes keyed on `userOpen`/`userId` alone — and NEITHER changes
// across a Discard or a restore, because the same account file is still open.
//
// So all three went on showing pre-Discard data indefinitely in the shipped
// build. Undo makes that worse (the document changes under them on every
// Ctrl+Z), which is how the bug was found, but it is not undo's bug and this
// commit stands on its own.
//
// One test per view, each a direct copy of OverviewView's own token test.
import { describe, expect, test } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import AutofillView from "$lib/AutofillView.svelte";
import KeybindsView from "$lib/KeybindsView.svelte";
import ProbeFormationsView from "$lib/ProbeFormationsView.svelte";
import { calls } from "$lib/test/setup";

const noop = () => {};

/** The props every one of the three shares, so the only variable is the view. */
const props = (refreshToken: number) => ({
  userOpen: true,
  userId: 140,
  refreshToken,
  onUserDirty: noop,
  onShowAccounts: noop,
  onShowBatch: noop,
});

const CASES = [
  { name: "Autofill", view: AutofillView, command: "autofill_lists", reply: [] },
  {
    name: "Keybinds",
    view: KeybindsView,
    command: "keybinds",
    reply: { available: true, entries: [] },
  },
  {
    name: "Probes",
    view: ProbeFormationsView,
    command: "probe_formations",
    reply: { formations: [], selected: null },
  },
] as const;

describe("a refreshToken bump re-reads the file", () => {
  for (const c of CASES) {
    test(`${c.name} reloads when the token changes`, async () => {
      calls.stub(c.command, c.reply);
      const { rerender } = render(c.view as never, props(1) as never);
      await waitFor(() => expect(calls.of(c.command).length).toBe(1));

      // Nothing else moves: same account, same id. Only the token.
      //
      // There is deliberately no "and does NOT reload when nothing changed"
      // twin: `rerender` hands the component a fresh props object either way,
      // so such a test would measure Svelte's prop identity rather than this
      // effect's dependency list, and would pass or fail for reasons that have
      // nothing to do with the bug.
      await rerender(props(2) as never);
      await waitFor(() => expect(calls.of(c.command).length).toBe(2));
    });
  }
});
