// The modifier key this platform actually uses, so a shortcut printed in the UI
// matches the one the user presses.
//
// `@tauri-apps/plugin-os` would also answer this. It is not a dependency today
// (`app/package.json` carries only Tauri plugins the app genuinely calls) and
// two lines of `navigator` do not justify making it one. `ssr = false`
// (`routes/+layout.ts`), so `navigator` is always there.
//
// `userAgentData.platform` first because `navigator.platform` is deprecated and
// frozen to "MacIntel" on Apple silicon — which happens to still be correct
// here, but only by accident.
export const MOD: "⌘" | "Ctrl" = /mac/i.test(
  (navigator as { userAgentData?: { platform?: string } }).userAgentData?.platform ??
    navigator.platform ??
    "",
)
  ? "⌘"
  : "Ctrl";

/** `accel("K")` -> "Ctrl+K" or "⌘K". Phase 5 uses it for every menu item's
 *  shortcut column; Phase 2 has two call sites. */
export const accel = (key: string): string => (MOD === "⌘" ? `${MOD}${key}` : `${MOD}+${key}`);
