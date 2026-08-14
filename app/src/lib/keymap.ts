// One keyboard map, dispatching through the command registry.
//
// It replaces an inline handler in `<svelte:window>` that knew four shortcuts by
// hand. Two other components still bind `onkeydown` — LayoutView for the nudge
// arrows and ProbeFormationsView for Ctrl+C/V — and they keep theirs, because
// those are genuinely LOCAL and positional: they act on a selection inside one
// canvas, and the guards that stop them firing inside a text field are already
// right there beside the state they guard.
//
// What is global lives here, and it dispatches by id rather than by branch, so
// the shortcut a menu prints and the shortcut this fires cannot disagree — they
// are the same `accel` field on the same command.
import { byId, type Command, type Ctx } from "./commands";

/** True when the event came from somewhere the browser's own key handling must
 *  keep working. A global accelerator that steals Ctrl+F from a text field is a
 *  worse bug than not having the accelerator. */
export function inAField(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA" || el.isContentEditable === true;
}

/** `Ctrl` on Windows and Linux, `⌘` on macOS — matching what `accel()` PRINTS.
 *  The app already tested both here; only the rendering half was missing. */
const primary = (e: KeyboardEvent) => e.ctrlKey || e.metaKey;

/** Command id per key, for the primary-modifier chords. The view digits are
 *  built from the same `VIEWS` order the tab strip uses. */
const CHORDS: Record<string, string> = {
  o: "file.open",
  s: "file.save",
  h: "file.history",
  k: "palette.open",
  f: "view.find",
  "/": "help.shortcuts",
  "1": "go.layout",
  "2": "go.overview",
  "3": "go.autofill",
  "4": "go.keybinds",
  "5": "go.probes",
  "6": "go.raw",
};

/**
 * Returns the command a keystroke means, or null.
 *
 * Split out from running it so the whole map is testable without a DOM: the one
 * thing worth pinning about a keyboard map is which key means what.
 */
export function commandFor(e: KeyboardEvent): Command | null {
  if (!primary(e)) return null;
  const id = CHORDS[e.key.toLowerCase()];
  if (!id) return null;
  return byId(id) ?? null;
}

/**
 * Handle one keydown. Returns true if it was consumed.
 *
 * A DISABLED command is consumed and does nothing — not an error, and not a
 * fall-through to the webview. `Ctrl+2` on a file with no window layout should
 * be a no-op, because the alternative is the browser doing something arbitrary
 * with a key the app has claimed.
 */
export function handleKey(e: KeyboardEvent, ctx: Ctx): boolean {
  // Ctrl+F and Ctrl+K are wanted even from inside a field — they are how you
  // GET to a field, and both replace the field's own content wholesale rather
  // than editing it. Everything else defers to the field.
  const key = e.key.toLowerCase();
  if (inAField(e.target) && key !== "f" && key !== "k" && key !== "s") return false;

  const cmd = commandFor(e);
  if (!cmd) return false;
  e.preventDefault();
  if (cmd.enabled() !== true) return true;
  cmd.run(ctx);
  return true;
}
