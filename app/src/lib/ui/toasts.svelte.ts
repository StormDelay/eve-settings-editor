// The transient-confirmation channel. Three hand-rolled `.flash` timer pairs
// existed, all at 2000 ms — under the usual 5 s reading-time guidance for a
// message that names a file. Four seconds is the compromise for a three-word
// confirmation; anything you must read in order to act is sticky instead.
export type ToastVariant = "info" | "success" | "warn" | "error";

export type ToastAction = { label: string; run: () => void };

export type Toast = {
  id: number;
  message: string;
  variant: ToastVariant;
  action?: ToastAction;
  /** 0 = sticky. The host animates the fade over exactly this long. */
  duration: number;
};

export const toasts = $state<Toast[]>([]);

let next = 0;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export function dismiss(id: number): void {
  const t = timers.get(id);
  if (t !== undefined) {
    clearTimeout(t);
    timers.delete(id);
  }
  const i = toasts.findIndex((x) => x.id === id);
  if (i >= 0) toasts.splice(i, 1);
}

export function toast(
  message: string,
  opts?: { variant?: ToastVariant; duration?: number; action?: ToastAction },
): void {
  const variant = opts?.variant ?? "info";
  // An error you did not read is an error you cannot act on, so an error stays
  // until it is dismissed unless the caller says otherwise.
  const duration = opts?.duration ?? (variant === "error" ? 0 : 4000);
  const id = ++next;
  toasts.push({ id, message, variant, action: opts?.action, duration });
  if (duration > 0) timers.set(id, setTimeout(() => dismiss(id), duration));
}
