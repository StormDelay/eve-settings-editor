// The six surviving confirmations, plus the pack-import disclosure. Seventy-three
// blocking native dialogs went to inline messages and toasts; what is left is the
// set that guards something the backup chain cannot walk back inside one session,
// and those become ONE in-app dialog rather than staying native.
//
// The reason is reuse, not taste. `Sheet` already ships the focus trap, Escape,
// and focus-restore-to-the-opener that a native dialog gives free; keeping
// `ask()` for six cases would leave the app with two modal mechanisms where one
// already suffices, which is the fault this redesign is retiring.
//
// `Promise<boolean>` deliberately — the same shape `confirm()`/`ask()` return, so
// every call site is an import swap and no surrounding control flow moves. It is
// also the rollback: if the focus trap ever misbehaves in the field, this file
// can delegate straight back to `ask()` and nothing else changes.
export type ConfirmRequest = {
  /** A question or a statement of consequence. Sentence case, names the object. */
  title: string;
  /** One sentence: what happens, and why it cannot be walked back. */
  body: string;
  /** Raw file names, paths and diagnostic codes go HERE, on `title=`, and never
   *  into the sentence — R5. */
  detail?: string;
  /** Names the verb and the object. Never "OK". */
  confirm: string;
  cancel?: string;
  /** Paints the confirming button red. Off for a disclosure like the pack
   *  import, which asks you to read rather than to weigh a loss. */
  danger?: boolean;
};

type Pending = ConfirmRequest & { id: number; resolve: (ok: boolean) => void };

/** A queue rather than a single slot, so two overlapping asks cannot drop one
 *  another's promise on the floor. The host renders `[0]`; in practice there is
 *  never more than one. */
export const pending = $state<Pending[]>([]);

let next = 0;

export function confirmDialog(req: ConfirmRequest): Promise<boolean> {
  return new Promise((resolve) => pending.push({ ...req, id: ++next, resolve }));
}

export function answer(id: number, ok: boolean): void {
  const i = pending.findIndex((p) => p.id === id);
  if (i < 0) return;
  const [p] = pending.splice(i, 1);
  p.resolve(ok);
}
