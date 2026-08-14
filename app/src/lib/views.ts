//! The six views, in row order, and the one rule that says whether each is
//! reachable. `02-shell.md` §5.5.
//!
//! The conditions here are byte-for-byte the ones they replace — this phase
//! removes no feature and unlocks none. What changes is that the answer is a
//! REASON rather than a boolean, so a tab that cannot be entered can be shown
//! disabled with something actionable on it instead of vanishing from the strip.
//! Four consumers now share it: the tab's disabled state, the tab's tooltip, the
//! switcher's "Go to…" section and the post-open fallback. Two of those used to
//! be two copies of the same prose conditions, in `+page.svelte` and in the
//! template beside it.
import { subject } from "./subject.svelte";

export type View = "layout" | "overview" | "autofill" | "keybinds" | "probes" | "raw";

/**
 * Row order, and it is THE order — fixed membership is worth nothing if the
 * sequence moves.
 *
 * Raw is last, and renamed from "Tree", because it is the escape hatch. It used
 * to sit first and be the default view, which is precisely why it read as the
 * normal way to look at settings rather than as the way out.
 */
export const VIEWS: { id: View; label: string }[] = [
  { id: "layout", label: "Layout" },
  { id: "overview", label: "Overview" },
  { id: "autofill", label: "Autofill" },
  { id: "keybinds", label: "Keybinds" },
  { id: "probes", label: "Probes" },
  { id: "raw", label: "Raw" },
];

/** `null` when the view is reachable, else the actionable reason it is not. */
export function viewAvailable(v: View): string | null {
  const anySubject = subject.charId !== null || subject.slots.user?.status === "opened";
  if (v === "raw") return null;
  if (v === "layout") {
    if (subject.layoutAvailable) return null;
    return subject.slots.char === null && subject.slots.user === null
      ? "Open a character to edit its window layout."
      : "This file has no saved window layout.";
  }
  return anySubject ? null : "Open a character or an account file.";
}

/** The first reachable view in row order. Raw is always reachable, so this
 *  always resolves. */
export function firstAvailableView(): View {
  return VIEWS.find((v) => viewAvailable(v.id) === null)!.id;
}

/**
 * Where to land after a file opens: hold the tab the user was on if the new
 * document supports it, else the first available one.
 *
 * The deliberate behaviour change of this phase. The old fallback was Raw, which
 * with six visible tabs and Raw sitting last would drop a first-time user onto a
 * dict tree while a Layout canvas sat one tab away.
 */
export function resolveView(prior: View): View {
  return viewAvailable(prior) === null ? prior : firstAvailableView();
}
