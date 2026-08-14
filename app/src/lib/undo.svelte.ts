// The seam Phase 5b attaches to, written in Phase 5 so that landing undo touches
// no toast call site.
//
// Every toast that reports an IN-MEMORY change — a deleted tab, a removed
// overview window, a reset neocom, deleted empty stack frames — passes
// `action: undoAction()`. Today that is `undefined` and the toast is
// informational, with Discard as the escape. When 5b lands, this function starts
// returning `{ label: "Undo", run }` and every one of those toasts grows the
// button without being edited.
//
// The alternative — adding the action at each call site when undo ships — is
// thirteen edits in nine files, each of which is an opportunity to give a
// NON-undoable action an Undo button. That mistake is the one 05b §8 calls out
// by name, so the seam is deliberately the only place that decides.
import type { ToastAction } from "./ui/toasts.svelte";

/** `undefined` until Phase 5b. Toasts for actions the document stack cannot
 *  reverse — a pairing, a settings-preset delete, a backup restore — must NOT
 *  call this: their remedy is named in their own words instead. */
export function undoAction(): ToastAction | undefined {
  return undefined;
}
