// The seam Phase 5 left, now filled in.
//
// Every toast that reports an IN-MEMORY change — a deleted tab, a removed
// overview window, a reset neocom, deleted empty stack frames, a deleted filter
// preset — passes `action: undoAction()`. Those call sites are unchanged by this
// phase: `undoAction()` returned `undefined` and now returns a button.
//
// The alternative — adding the action at each call site when undo shipped —
// would be thirteen edits in nine files, each an opportunity to give a
// NON-undoable action an Undo button. That mistake is the one 05b §8 calls out
// by name, so the seam is deliberately the only place that decides.
import { api } from "./api";
import { subject } from "./subject.svelte";
import { toast, type ToastAction } from "./ui/toasts.svelte";

/** Mirrors the backend's stack, so Undo can be disabled rather than absent. */
export const undoState = $state({ canUndo: false, canRedo: false });

/**
 * Apply one step's outcome to the shell.
 *
 * `dirty` comes from the response and REPLACES whatever the frontend thought:
 * after edit → save → edit → undo the file is clean, and one more undo makes it
 * dirty again. No frontend-only scheme can tell those apart, so the frontend
 * does not try.
 */
function land(r: Awaited<ReturnType<typeof api.undo>>): boolean {
  if (r === null) return false;
  for (const [slot, tree] of [
    ["char", r.char_tree],
    ["user", r.user_tree],
  ] as const) {
    const doc = subject.slots[slot];
    // Reassign rather than mutate in place, the same rule `runMutation`
    // follows, so the derived reads refire.
    if (doc?.status === "opened" && tree) subject.slots[slot] = { ...doc, tree };
  }
  subject.dirty.char = r.dirty.char;
  subject.dirty.user = r.dirty.user;
  undoState.canUndo = r.state.can_undo;
  undoState.canRedo = r.state.can_redo;
  // Every view's reload key. Its name still says "saved", which is now half the
  // truth; the comment at its declaration carries the rest rather than a rename
  // that would collide with two other diffs for no functional gain.
  subject.savedAt += 1;
  return true;
}

/**
 * `Ctrl+Z` with an empty stack SAYS SO. Never silence — silence after a delete
 * is exactly how a user concludes the undo did something they cannot see.
 */
export async function doUndo(): Promise<void> {
  if (!land(await api.undo())) toast("Nothing to undo.");
}

export async function doRedo(): Promise<void> {
  if (!land(await api.redo())) toast("Nothing to redo.");
}

/** Re-read the stack's state after something that clears it — open, close,
 *  discard, restore. Cheap, and it keeps the button honest. */
export function forgetUndoHistory(): void {
  undoState.canUndo = false;
  undoState.canRedo = false;
}

/**
 * The toast action for an in-memory change.
 *
 * The hazard this guards is real and easy to miss: the button undoes the TOP OF
 * THE STACK, not "its own" step — there is no such thing, because a stack has
 * only a top. Make another edit inside the toast's four seconds and the button
 * now reverts the newer edit, silently, while naming the older one.
 *
 * The spec's fix is "dismiss the toast on any subsequent edit". That needs every
 * call site to mark the document dirty BEFORE it raises its toast, and they do
 * not — `NeocomButtons` cannot, because its dirty flag is set by an async
 * command it only kicks off. A rule that five of six call sites happen to follow
 * is not a rule.
 *
 * So the button carries the stack depth it was minted at and refuses rather than
 * reverting the wrong thing. Two extra round trips at human keypress speed, and
 * no ordering discipline anywhere.
 */
export function undoAction(): ToastAction | undefined {
  // Resolved now, which is after the edit that raised this toast has pushed —
  // every caller builds its toast on the far side of an awaited command.
  const mintedAt = api
    .undoState()
    .then((s) => s.depth)
    .catch(() => -1);
  return {
    label: "Undo",
    run: () =>
      void (async () => {
        const [was, now] = await Promise.all([mintedAt, api.undoState()]);
        if (was !== now.depth) {
          toast("That isn't the most recent change any more — undo again to reach it.", {
            variant: "warn",
          });
          return;
        }
        await doUndo();
      })(),
  };
}

/** Called when anything edits a document, so the Undo control lights up without
 *  waiting for a round trip. The backend's own answer overrides it on the next
 *  undo or redo. */
export function noteEdit(): void {
  undoState.canUndo = true;
  undoState.canRedo = false;
}
